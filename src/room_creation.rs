//! Room creation without sharing administrator credentials with the desktop.
use anyhow::{Result, ensure};
use std::{io::Read, sync::mpsc};
use eframe::egui;

pub struct CreatedRoom { pub invite: String, pub title: String }
pub struct Creator {
    pub open: bool, pub title: String, pub busy: bool, pub status: String,
    request_id: String, requested_title: String,
    receiver: Option<mpsc::Receiver<Result<CreatedRoom>>>,
}
impl Default for Creator {
    fn default()->Self {Self {open:false,title:String::new(),busy:false,status:String::new(),request_id:String::new(),requested_title:String::new(),receiver:None}}
}
impl Creator {
    pub fn open(&mut self,_address:&str) {self.open=true;self.status.clear();self.request_id=uuid::Uuid::new_v4().simple().to_string();self.requested_title.clear();}
    pub fn start(&mut self,address:String,session:String,ctx:egui::Context) {
        if self.busy {return;}
        let title=self.title.trim().to_owned();
        if self.requested_title!=title {self.request_id=uuid::Uuid::new_v4().simple().to_string();self.requested_title=title.clone();}
        let request=self.request_id.clone();let(tx,rx)=mpsc::channel();self.busy=true;self.status="Criando sala…".into();
        std::thread::spawn(move||{let _=tx.send(create(&address,&session,&title,&request));ctx.request_repaint();});self.receiver=Some(rx);
    }
    pub fn poll(&mut self)->Option<CreatedRoom> {
        let result=match self.receiver.as_ref()?.try_recv(){Ok(r)=>r,Err(mpsc::TryRecvError::Empty)=>return None,Err(_)=>Err(anyhow::anyhow!("Criação interrompida; tente novamente"))};
        self.receiver=None;self.busy=false;
        match result {Ok(room)=>{self.open=false;self.status.clear();Some(room)},Err(e)=>{self.status=e.to_string();None}}
    }
}

pub struct ServerAccess {
    pub code:String, pub remember:bool, pub busy:bool, pub status:String,
    token:String, origin:String, loaded:bool, refresh:std::time::Instant,
    receiver:Option<mpsc::Receiver<Result<(String,bool)>>>,
}
impl Default for ServerAccess {
    fn default()->Self {Self {code:String::new(),remember:cfg!(windows),busy:false,status:String::new(),token:String::new(),origin:String::new(),loaded:false,refresh:std::time::Instant::now(),receiver:None}}
}
impl ServerAccess {
    pub fn token(&self)->String {if self.authenticated(){self.token.clone()}else{String::new()}}
    pub fn authenticated(&self)->bool {!self.token.is_empty() && self.refresh.elapsed()<std::time::Duration::from_secs(840)}
    pub fn logout(&mut self,address:&str) {let _=forget_code(address);self.code.clear();self.token.clear();self.status.clear();self.loaded=true;}
    pub fn start(&mut self,address:String,device:String,ctx:egui::Context) {
        if self.busy{return;}self.busy=true;self.status="Autenticando…".into();let code=self.code.trim().to_owned();let remember=self.remember;
        let(tx,rx)=mpsc::channel();self.receiver=Some(rx);
        std::thread::spawn(move||{
            let result=login(&address,&code,&device).map(|token|{let failed=(if remember{save_code(&address,&code)}else{forget_code(&address)}).is_err();(token,failed)});
            let _=tx.send(result);ctx.request_repaint();
        });
    }
    pub fn poll(&mut self,address:&str,device:&str,ctx:egui::Context) {
        if self.origin!=address && !self.busy {self.token.clear();self.code.clear();self.loaded=false;self.origin=address.to_owned();self.status.clear();}
        if !self.loaded {self.loaded=true;if let Ok(code)=load_code(address){self.code=code;self.start(address.to_owned(),device.to_owned(),ctx.clone());}}
        if let Some(rx)=&self.receiver {
            match rx.try_recv(){
                Ok(result)=>{self.receiver=None;self.busy=false;match result {Ok((token,failed))=>{self.token=token;self.refresh=std::time::Instant::now();self.status=if failed{"Acesso liberado; não foi possível atualizar a chave salva neste dispositivo".into()}else{String::new()};},Err(e)=>{self.token.clear();self.status=e.to_string();}}},
                Err(mpsc::TryRecvError::Disconnected)=>{self.receiver=None;self.busy=false;self.token.clear();self.status="Acesso interrompido; tente novamente".into();},
                Err(mpsc::TryRecvError::Empty)=>{}
            }
        }
        if !self.token.is_empty() && self.refresh.elapsed()>=std::time::Duration::from_secs(780) && !self.busy {self.start(address.to_owned(),device.to_owned(),ctx);}
    }
}
fn login(address:&str,code:&str,device:&str)->Result<String> {
    ensure!((32..=64).contains(&code.len()) && code.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'-'||b==b'_'),"Informe uma chave de acesso válida");
    let response=reqwest::blocking::Client::builder().redirect(reqwest::redirect::Policy::none()).timeout(std::time::Duration::from_secs(12)).user_agent("HyperCast-Desktop/0.6").build()?
        .post(crate::invitation::endpoint(address,"/api/login")?).bearer_auth(code).json(&serde_json::json!({"device_id":device})).send().map_err(|_|anyhow::anyhow!("Servidor indisponível; tente novamente"))?;
    match response.status().as_u16(){200=>{},401=>anyhow::bail!("Chave inválida, expirada ou revogada"),status=>anyhow::bail!("Acesso indisponível (HTTP {status}); tente novamente")}
    let mut bytes=Vec::new();response.take(16385).read_to_end(&mut bytes)?;ensure!(bytes.len()<=16384,"Resposta de acesso excedeu o limite");
    let value:serde_json::Value=serde_json::from_slice(&bytes)?;
    let token=value["token"].as_str().ok_or_else(||anyhow::anyhow!("Resposta de acesso inválida"))?;
    ensure!(!token.is_empty() && token.len()<=4096 && value["expires_in"].as_u64()==Some(900),"Resposta de acesso inválida");Ok(token.to_owned())
}

pub fn create(address:&str,code:&str,title:&str,request_id:&str)->Result<CreatedRoom> {
    ensure!(!code.is_empty() && code.len()<=4096,"Autentique-se no servidor");
    let endpoint=crate::invitation::endpoint(address,"/api/rooms")?;
    let response=reqwest::blocking::Client::builder().redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(20)).user_agent("HyperCast-Desktop/0.6").build()?
        .post(endpoint).bearer_auth(code).json(&serde_json::json!({"room_name":title,"request_id":request_id}))
        .send().map_err(|_|anyhow::anyhow!("Não foi possível acessar o servidor; tente novamente"))?;
    match response.status().as_u16() {
        200=>{},
        400=>anyhow::bail!("Use um nome de sala com 1 a 64 caracteres visíveis"),
        401|403=>anyhow::bail!("Sessão expirada; entre novamente no servidor"),
        409=>anyhow::bail!("Você já tem uma sala ativa ou o servidor atingiu o limite de salas. Aguarde ou volte à sala anterior"),
        429=>anyhow::bail!("Limite de criação atingido; tente novamente mais tarde"),
        _=>anyhow::bail!("Criação de salas indisponível; tente novamente"),
    }
    let mut bytes=Vec::new();response.take(16385).read_to_end(&mut bytes)?;
    ensure!(bytes.len()<=16384,"Resposta de sala excedeu o limite");
    let value:serde_json::Value=serde_json::from_slice(&bytes)?;
    let invite=value["invite"].as_str().ok_or_else(||anyhow::anyhow!("Resposta de sala inválida"))?;
    let title=value["room_name"].as_str().ok_or_else(||anyhow::anyhow!("Resposta de sala inválida"))?;
    ensure!((32..=64).contains(&invite.len()) && invite.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'-'||b==b'_') && title.chars().count()<=64,"Resposta de sala inválida");
    Ok(CreatedRoom{invite:invite.to_owned(),title:title.to_owned()})
}

#[cfg(windows)]
fn credential_path(address:&str)->Result<(std::path::PathBuf,Vec<u8>)> {
    use sha2::{Digest,Sha256};
    let origin=crate::invitation::endpoint(address,"/api/rooms")?.origin().ascii_serialization();
    let fingerprint=format!("{:x}",Sha256::digest(origin.as_bytes()));
    let root=std::path::PathBuf::from(std::env::var_os("LOCALAPPDATA").ok_or_else(||anyhow::anyhow!("Windows profile unavailable"))?).join("HyperCast");
    Ok((root.join(format!("server-access-{fingerprint}.bin")),origin.into_bytes()))
}
#[cfg(windows)]
fn crypt(data:&[u8],entropy:&[u8],protect:bool)->Result<Vec<u8>> {
    use windows::Win32::{Foundation::{HLOCAL,LocalFree},Security::Cryptography::*};
    ensure!(data.len()<=4096 && entropy.len()<=4096,"Credential size exceeded");
    let input=CRYPT_INTEGER_BLOB{cbData:data.len() as u32,pbData:data.as_ptr() as *mut u8};
    let salt=CRYPT_INTEGER_BLOB{cbData:entropy.len() as u32,pbData:entropy.as_ptr() as *mut u8};
    let mut output=CRYPT_INTEGER_BLOB::default();
    // SAFETY: input/salt slices live through the call. DPAPI allocates output;
    // it is copied before LocalFree, including validation error paths. No machine flag.
    unsafe {
        if protect {CryptProtectData(&input,windows::core::PCWSTR::null(),Some(&salt),None,None,CRYPTPROTECT_UI_FORBIDDEN,&mut output)?;}
        else {CryptUnprotectData(&input,None,Some(&salt),None,None,CRYPTPROTECT_UI_FORBIDDEN,&mut output)?;}
        let result=if !output.pbData.is_null() && output.cbData<=4096 {
            Ok(std::slice::from_raw_parts(output.pbData,output.cbData as usize).to_vec())
        }else{Err(anyhow::anyhow!("Invalid protected credential"))};
        LocalFree(Some(HLOCAL(output.pbData as *mut _)));result
    }
}
#[cfg(windows)]
pub fn load_code(address:&str)->Result<String> {
    let(path,entropy)=credential_path(address)?;
    let mut sealed=Vec::new();std::fs::File::open(path)?.take(4097).read_to_end(&mut sealed)?;
    String::from_utf8(crypt(&sealed,&entropy,false)?).map_err(Into::into)
}
#[cfg(windows)]
pub fn save_code(address:&str,code:&str)->Result<()> {
    let(path,entropy)=credential_path(address)?;
    let sealed=crypt(code.as_bytes(),&entropy,true)?;
    std::fs::create_dir_all(path.parent().unwrap())?;
    use std::io::Write;
    let staging=path.with_extension(format!("{}.tmp",uuid::Uuid::new_v4().simple()));
    let result=(||->Result<()>{
        let mut file=std::fs::OpenOptions::new().create_new(true).write(true).open(&staging)?;
        file.write_all(&sealed)?;file.sync_all()?;drop(file);
        std::fs::rename(&staging,&path)?;Ok(())
    })();
    if result.is_err(){let _=std::fs::remove_file(staging);}result
}
#[cfg(windows)]
pub fn forget_code(address:&str)->Result<()> {
    let(path,_)=credential_path(address)?;
    match std::fs::remove_file(path){Ok(())=>Ok(()),Err(e) if e.kind()==std::io::ErrorKind::NotFound=>Ok(()),Err(e)=>Err(e.into())}
}

#[cfg(all(test,windows))]
mod tests {
    use super::*;
    #[test] fn dpapi_is_bound_to_user_and_origin(){
        let sealed=crypt(b"creator-code",b"https://server-a",true).unwrap();
        assert_ne!(sealed,b"creator-code");
        assert_eq!(crypt(&sealed,b"https://server-a",false).unwrap(),b"creator-code");
        assert!(crypt(&sealed,b"https://server-b",false).is_err());
    }
}

// Linux access credentials remain in memory until a Secret Service backend is
// implemented. Never fall back to storing a shared server key in plaintext.
#[cfg(not(windows))]
pub fn load_code(_address:&str)->Result<String>{anyhow::bail!("Acesso salvo indisponível nesta plataforma")}
#[cfg(not(windows))]
pub fn save_code(_address:&str,_code:&str)->Result<()>{anyhow::bail!("A chave permanece somente nesta sessão")}
#[cfg(not(windows))]
pub fn forget_code(_address:&str)->Result<()>{Ok(())}
