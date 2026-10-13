//! Explicit, user-triggered updates from this project's stable GitHub releases.
//! No API token is shipped. Downloaded executables are verified and read-locked.
use anyhow::{Context, Result, bail, ensure};
use reqwest::blocking::Client;
use semver::Version;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{fs::{self, File, OpenOptions}, io::{Read, Write}, os::windows::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf}, process::Command, sync::{Arc, atomic::{AtomicU64, Ordering}, mpsc}, time::Duration};
use windows::{core::w, Win32::{Foundation::ERROR_FILE_NOT_FOUND, System::Registry::{
    HKEY_CURRENT_USER, RRF_RT_REG_SZ, RegGetValueW}}};

pub const RELEASES: &str="https://github.com/LucasCamy/HyperCast/releases/latest";
const API: &str="https://api.github.com/repos/LucasCamy/HyperCast/releases/latest";
const LIMIT: u64=256*1024*1024;
const REPARSE_POINT: u32=0x400;

#[derive(Clone, Debug)]
pub struct Release {pub version:Version, pub page:String, asset:String, size:u64, digest:[u8;32]}
#[derive(Clone, Debug)]
pub enum Installation {Installed(PathBuf), Portable, Unknown(String)}
pub enum Check {Available(Release), Current, NoRelease}
pub enum Event {Checked(Result<Check>), Downloaded(Result<VerifiedInstaller>)}
pub struct VerifiedInstaller {path:PathBuf, directory:PathBuf, _lock:Option<File>}
impl Drop for VerifiedInstaller {
    fn drop(&mut self){
        self._lock.take();
        // Only exact files/directories created by this worker; no recursive deletion.
        let _=fs::remove_file(&self.path);let _=fs::remove_dir(&self.directory);
    }
}
impl VerifiedInstaller {
    pub fn launch(&self,installation:&Installation)->Result<()> {
        let Installation::Installed(directory)=installation else {bail!("Use o download pelo navegador para a versão portátil");};
        // Recheck registry association before replacing an installation.
        let Installation::Installed(current)=detect_installation()? else {bail!("Instalação não encontrada");};
        ensure!(&current==directory,"O diretório instalado mudou; verifique novamente");
        // _lock denies write/delete sharing until the loader has opened the image.
        Command::new(&self.path).args(["/SILENT","/SUPPRESSMSGBOXES","/NORESTART","/HYPERCASTUPDATE=1"])
            .arg(format!("/DIR={}",directory.display())).spawn().context("Não foi possível iniciar a atualização")?;
        Ok(())
    }
}

pub struct Updater {
    pub open:bool, pub busy:bool, pub status:String, pub release:Option<Release>,
    pub installer:Option<VerifiedInstaller>, pub installation:Installation,
    pub bytes:Arc<AtomicU64>, tx:mpsc::Sender<Event>, rx:mpsc::Receiver<Event>,
}
impl Default for Updater {
    fn default()->Self{
        let(tx,rx)=mpsc::channel();
        Self{open:false,busy:false,status:String::new(),release:None,installer:None,
            installation:detect_installation().unwrap_or_else(|e|Installation::Unknown(e.to_string())),
            bytes:Arc::new(AtomicU64::new(0)),tx,rx}
    }
}
impl Updater {
    pub fn poll(&mut self){
        while let Ok(event)=self.rx.try_recv(){
            self.busy=false;
            match event{
                Event::Checked(Ok(Check::Available(release)))=>{self.status=format!("Versão {} disponível",release.version);self.release=Some(release);}
                Event::Checked(Ok(Check::Current))=>self.status="Você está na versão mais recente".into(),
                Event::Checked(Ok(Check::NoRelease))=>self.status="Ainda não há versões publicadas para baixar".into(),
                Event::Downloaded(Ok(installer))=>{self.installer=Some(installer);self.status="Download verificado. Pronto para instalar".into();}
                Event::Checked(Err(e))|Event::Downloaded(Err(e))=>self.status=format!("Atualização: {e:#}"),
            }
        }
    }
    pub fn check(&mut self,ctx:eframe::egui::Context){
        if self.busy{return;}
        self.busy=true;self.status="Verificando atualização…".into();self.release=None;self.installer=None;
        let tx=self.tx.clone();std::thread::spawn(move||{
            let result=latest().and_then(|release|match release{
                Some(release) if release.version>Version::parse(env!("CARGO_PKG_VERSION"))?=>Ok(Check::Available(release)),
                Some(_)=>Ok(Check::Current),None=>Ok(Check::NoRelease),
            });let _=tx.send(Event::Checked(result));ctx.request_repaint();
        });
    }
    pub fn download(&mut self,ctx:eframe::egui::Context){
        if self.busy{return;}
        let Some(release)=self.release.clone() else{return;};
        if !matches!(self.installation,Installation::Installed(_)){return;}
        self.busy=true;self.bytes.store(0,Ordering::Relaxed);self.status="Baixando atualização…".into();
        let tx=self.tx.clone();let progress=self.bytes.clone();std::thread::spawn(move||{
            let result=download(&release,&progress);let _=tx.send(Event::Downloaded(result));ctx.request_repaint();
        });
    }
    pub fn progress(&self)->Option<(f32,String)>{
        if !self.busy{return None;}let release=self.release.as_ref()?;
        let bytes=self.bytes.load(Ordering::Relaxed);
        Some(((bytes as f32/release.size as f32).clamp(0.0,1.0),format!("{:.1} / {:.1} MB",bytes as f64/1e6,release.size as f64/1e6)))
    }
}

fn trusted_transport(url:&url::Url)->bool{
    url.scheme()=="https" && url.port_or_known_default()==Some(443)
        && url.username().is_empty() && url.password().is_none()
        && matches!(url.host_str(),Some("api.github.com"|"github.com"|"release-assets.githubusercontent.com"|"objects.githubusercontent.com"))
}
fn client(seconds:u64)->Result<Client>{
    Ok(Client::builder().user_agent(concat!("HyperCast/",env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(10)).timeout(Duration::from_secs(seconds))
        .redirect(reqwest::redirect::Policy::custom(|attempt|{
            if attempt.previous().len()<4 && trusted_transport(attempt.url()){attempt.follow()}
            else{attempt.error("Redirecionamento de atualização não autorizado")}
        })).build()?)
}
fn digest(text:&str)->Result<[u8;32]>{
    let hex=text.strip_prefix("sha256:").context("Release não tem digest SHA-256")?;
    ensure!(hex.len()==64 && hex.bytes().all(|c|c.is_ascii_hexdigit()),"Digest SHA-256 inválido");
    let mut output=[0;32];for(i,byte)in output.iter_mut().enumerate(){*byte=u8::from_str_radix(&hex[i*2..i*2+2],16)?;}Ok(output)
}
fn parse_release(value:&Value)->Result<Release>{
    ensure!(value["draft"]==false && value["prerelease"]==false,"Somente versões estáveis são aceitas");
    let tag=value["tag_name"].as_str().context("Release sem versão")?;
    let version=Version::parse(tag.strip_prefix('v').unwrap_or(tag))?;
    ensure!(version.pre.is_empty() && version.build.is_empty(),"Somente versões estáveis são aceitas");
    let prefix=format!("https://github.com/LucasCamy/HyperCast/releases/");
    let page=format!("{prefix}tag/{tag}");
    ensure!(value["html_url"].as_str()==Some(page.as_str()),"Release não pertence ao HyperCast");
    let name=format!("HyperCast-{version}-windows-x64-setup.exe");
    let assets=value["assets"].as_array().context("Release sem assets")?;
    let mut found=assets.iter().filter(|a|a["name"].as_str()==Some(name.as_str()));
    let asset=found.next().context("Instalador Windows não publicado nesta versão")?;
    ensure!(found.next().is_none(),"Instaladores duplicados na release");
    let expected=format!("{prefix}download/{tag}/{name}");
    ensure!(asset["browser_download_url"].as_str()==Some(expected.as_str()) && asset["state"]=="uploaded","URL do instalador inválida");
    let size=asset["size"].as_u64().context("Tamanho de instalador inválido")?;
    ensure!(size>0 && size<=LIMIT,"Instalador excede o limite permitido");
    Ok(Release{version,page,asset:expected,size,digest:digest(asset["digest"].as_str().context("Digest do instalador indisponível")?)?})
}
pub fn latest()->Result<Option<Release>>{
    let mut response=client(20)?.get(API).header("Accept","application/vnd.github+json")
        .header("X-GitHub-Api-Version","2022-11-28").send()?;
    if response.status()==reqwest::StatusCode::NOT_FOUND{return Ok(None);}
    if response.status()==reqwest::StatusCode::FORBIDDEN || response.status()==reqwest::StatusCode::TOO_MANY_REQUESTS {
        bail!("GitHub limitou as consultas; tente novamente mais tarde");
    }
    response.error_for_status_ref()?;
    let mut bytes=Vec::new();Read::by_ref(&mut response).take(1024*1024+1).read_to_end(&mut bytes)?;
    ensure!(bytes.len()<=1024*1024,"Resposta de atualização excede o limite");
    Ok(Some(parse_release(&serde_json::from_slice(&bytes)?)?))
}
fn safe_directory(path:&Path)->Result<()> {
    let metadata=fs::symlink_metadata(path)?;
    ensure!(metadata.is_dir() && metadata.file_attributes()&REPARSE_POINT==0,"Diretório de atualização não pode ser link/reparse point");Ok(())
}
fn cache_root()->Result<PathBuf>{
    let local=PathBuf::from(std::env::var_os("LOCALAPPDATA").context("LOCALAPPDATA indisponível")?).canonicalize()?;
    let base=local.join("HyperCast");if !base.exists(){fs::create_dir(&base)?;}safe_directory(&base)?;
    let updates=base.join("updates");if !updates.exists(){fs::create_dir(&updates)?;}safe_directory(&updates)?;Ok(updates)
}
pub fn clean_old_downloads(){
    // Best effort, fixed allowlist, no recursion. Active downloads/installers are locked.
    let Ok(root)=cache_root() else{return;};let Ok(entries)=fs::read_dir(&root) else{return;};
    for entry in entries.flatten().take(128){
        let directory=entry.path();
        if uuid::Uuid::parse_str(&entry.file_name().to_string_lossy()).is_err() || safe_directory(&directory).is_err(){continue;}
        let Ok(files)=fs::read_dir(&directory) else{continue;};
        for file in files.flatten().take(4){
            let name=file.file_name();let name=name.to_string_lossy();
            let Some(version)=name.strip_prefix("HyperCast-").and_then(|s|s.strip_suffix("-windows-x64-setup.exe")) else{continue;};
            if Version::parse(version).is_err(){continue;}
            let Ok(metadata)=fs::symlink_metadata(file.path()) else{continue;};
            if !metadata.is_file() || metadata.file_attributes()&REPARSE_POINT!=0 {continue;}
            if metadata.modified().ok().and_then(|t|t.elapsed().ok()).is_some_and(|age|age>Duration::from_secs(86400)){
                let _=fs::remove_file(file.path());
            }
        }
        let _=fs::remove_dir(directory);
    }
}
fn verify_file(path:&Path,expected:&[u8;32],size:u64)->Result<File>{
    let mut file=OpenOptions::new().read(true).share_mode(1).open(path)?; // FILE_SHARE_READ: no write/delete.
    ensure!(file.metadata()?.file_attributes()&REPARSE_POINT==0,"Instalador não pode ser link/reparse point");
    let mut hash=Sha256::new();let mut buffer=[0;65536];let mut total=0;
    loop{let n=file.read(&mut buffer)?;if n==0{break;}total+=n as u64;ensure!(total<=size,"Tamanho do instalador não confere");hash.update(&buffer[..n]);}
    ensure!(total==size && hash.finalize().as_slice()==expected,"SHA-256 ou tamanho do instalador não confere");Ok(file)
}
pub fn download(release:&Release,progress:&AtomicU64)->Result<VerifiedInstaller>{
    ensure!(trusted_transport(&url::Url::parse(&release.asset)?),"Transporte de atualização inválido");
    let directory=cache_root()?.join(uuid::Uuid::new_v4().to_string());fs::create_dir(&directory)?;
    let path=directory.join(format!("HyperCast-{}-windows-x64-setup.exe",release.version));
    let mut prepared=VerifiedInstaller{path,directory,_lock:None};
    let mut response=client(300)?.get(&release.asset).send()
        .map_err(|_|anyhow::anyhow!("Não foi possível baixar o instalador; verifique sua conexão"))?;
    ensure!(response.status().is_success(),"Download recusado pelo GitHub (HTTP {})",response.status().as_u16());
    if let Some(length)=response.content_length(){ensure!(length==release.size,"Tamanho de download não confere");}
    let mut file=OpenOptions::new().write(true).create_new(true).share_mode(0).open(&prepared.path)?;
    let mut buffer=[0;65536];let mut total=0;
    loop{let n=response.read(&mut buffer)?;if n==0{break;}total+=n as u64;
        ensure!(total<=release.size && total<=LIMIT,"Download excede o tamanho esperado");
        file.write_all(&buffer[..n])?;progress.store(total,Ordering::Relaxed);
    }
    file.sync_all()?;drop(file);
    prepared._lock=Some(verify_file(&prepared.path,&release.digest,release.size)?);
    Ok(prepared)
}
pub fn detect_installation()->Result<Installation>{
    let mut text=[0u16;8192];let mut bytes=(text.len()*2) as u32;
    // SAFETY: fixed HKCU key, initialized bounded UTF-16 output. No registry writes.
    let result=unsafe{RegGetValueW(HKEY_CURRENT_USER,w!("Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{34FA3F08-63D0-4A64-9E61-F79E7078B39D}_is1"),
        w!("InstallLocation"),RRF_RT_REG_SZ,None,Some(text.as_mut_ptr().cast()),Some(&mut bytes))};
    if result==ERROR_FILE_NOT_FOUND{return Ok(Installation::Portable);}
    result.ok().context("Não foi possível identificar a instalação")?;
    let len=text.iter().position(|c|*c==0).context("Registro de instalação inválido")?;
    let directory=PathBuf::from(String::from_utf16(&text[..len])?).canonicalize()?;
    let executable=std::env::current_exe()?.canonicalize()?;
    Ok(if executable.parent()==Some(directory.as_path()){Installation::Installed(directory)}else{Installation::Portable})
}

#[cfg(test)]
mod tests{
    use super::*;
    fn manifest()->Value{serde_json::json!({"draft":false,"prerelease":false,"tag_name":"v0.5.0","html_url":"https://github.com/LucasCamy/HyperCast/releases/tag/v0.5.0",
        "assets":[{"name":"HyperCast-0.5.0-windows-x64-setup.exe","state":"uploaded","size":3,"digest":"sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        "browser_download_url":"https://github.com/LucasCamy/HyperCast/releases/download/v0.5.0/HyperCast-0.5.0-windows-x64-setup.exe"}]})}
    #[test]fn only_trusts_project_stable_assets(){
        assert!(parse_release(&manifest()).is_ok());
        for field in ["draft","prerelease"]{let mut v=manifest();v[field]=true.into();assert!(parse_release(&v).is_err());}
        for(field,value)in [("browser_download_url",Value::from("https://evil.example/setup.exe")),("digest",Value::Null),("size",Value::from(LIMIT+1))]{
            let mut v=manifest();v["assets"][0][field]=value;assert!(parse_release(&v).is_err());}
        let mut v=manifest();v["tag_name"]="v0.5.0-beta.1".into();assert!(parse_release(&v).is_err());
        assert!(Version::parse("0.10.0").unwrap()>Version::parse("0.9.0").unwrap());
    }
    #[test]fn rejects_untrusted_redirects(){
        for url in ["http://github.com/x","https://github.com.evil.example/x","https://localhost/x","https://user@github.com/x","https://github.com:444/x"]{
            assert!(!trusted_transport(&url::Url::parse(url).unwrap()));}
        assert!(trusted_transport(&url::Url::parse("https://release-assets.githubusercontent.com/x").unwrap()));
    }
    #[test]fn verifies_bytes_and_holds_write_delete_lock(){
        let directory=std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());fs::create_dir(&directory).unwrap();let path=directory.join("setup.exe");
        fs::write(&path,b"abc").unwrap();let hash=digest("sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad").unwrap();
        let lock=verify_file(&path,&hash,3).unwrap();assert!(OpenOptions::new().write(true).open(&path).is_err());assert!(fs::remove_file(&path).is_err());drop(lock);
        fs::write(&path,b"bad").unwrap();assert!(verify_file(&path,&hash,3).is_err());assert!(verify_file(&path,&hash,2).is_err());
        fs::remove_file(path).unwrap();fs::remove_dir(directory).unwrap();
    }
    #[test]
    #[ignore="Operator test: isolated registered installation and published stable release required"]
    fn install_live_release_in_isolated_registration(){
        let target=detect_installation().unwrap();
        let Installation::Installed(directory)=&target else{panic!("Run this test binary from the isolated registered directory");};
        assert_eq!(directory, &Path::new(env!("CARGO_MANIFEST_DIR")).join(".tools/update-install-test").canonicalize().unwrap(),
            "Refusing to modify a normal installation");
        let release=latest().unwrap().expect("Stable release required");
        let mut updater=Updater::default();updater.release=Some(release);
        updater.download(eframe::egui::Context::default());
        let deadline=std::time::Instant::now()+Duration::from_secs(330);
        while updater.busy {assert!(std::time::Instant::now()<deadline);std::thread::sleep(Duration::from_millis(100));updater.poll();}
        assert!(updater.bytes.load(Ordering::Relaxed)>0);
        updater.installer.as_ref().expect(&updater.status).launch(&target).unwrap();
        println!("Verified stable installer launched against isolated registration");
    }
}
