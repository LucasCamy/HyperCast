//! Non-secret display name, shared by the Windows and Linux interfaces.
use std::{io::{Read, Write}, path::PathBuf};
fn path() -> Option<PathBuf> {
    #[cfg(windows)]
    let root = std::env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join("HyperCast"));
    #[cfg(not(windows))]
    let root = std::env::var_os("XDG_STATE_HOME").map(PathBuf::from).filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".local/state")))
        .map(|p| p.join("hypercast"));
    root.filter(|p| p.is_absolute()).map(|p| p.join("display-name"))
}
fn valid(name: &str) -> bool {
    !name.is_empty() && name.len() <= 256 && name.chars().count() <= 64
        && !name.chars().any(char::is_control)
}
pub fn load_name() -> String {
    if let Ok(name) = std::env::var("HYPERCAST_NAME") { return name; }
    path().and_then(|p| load(&p).ok()).unwrap_or_default()
}
fn load(path: &std::path::Path) -> std::io::Result<String> {
    let mut name = String::new();
    std::fs::File::open(path)?.take(257).read_to_string(&mut name)?;
    if !valid(&name) { return Err(std::io::Error::from(std::io::ErrorKind::InvalidData)); }
    Ok(name)
}
pub fn save_name(name: &str) -> std::io::Result<()> {
    if std::env::var_os("HYPERCAST_NAME").is_some() { return Ok(()); }
    let name = name.trim();
    if !valid(name) { return Ok(()); }
    let Some(path) = path() else { return Ok(()); };
    save(&path, name)
}
fn save(path: &std::path::Path, name: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(path.parent().unwrap())?;
    // A unique file prevents partial reads and collisions between running clients.
    let temp = path.with_extension(uuid::Uuid::new_v4().simple().to_string());
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)] { use std::os::unix::fs::OpenOptionsExt; options.mode(0o600); }
    let result = (|| {
        let mut file = options.open(&temp)?;
        file.write_all(name.as_bytes())?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temp, path)
    })();
    if result.is_err() { let _ = std::fs::remove_file(temp); }
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn name_round_trip_and_corrupt_cache() {
        let dir = std::env::temp_dir().join(format!("hypercast-name-{}", uuid::Uuid::new_v4()));
        let path = dir.join("display-name");
        save(&path, "Lucas").unwrap();
        assert_eq!(load(&path).unwrap(), "Lucas");
        save(&path, "Amigo").unwrap();
        assert_eq!(load(&path).unwrap(), "Amigo");
        std::fs::write(&path, "x".repeat(257)).unwrap();
        assert!(load(&path).is_err());
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }
    #[test]
    fn bounded_name() {
        assert!(!valid("")); assert!(!valid("Lucas\n"));
        assert!(!valid(&"a".repeat(65))); assert!(valid("Lucas"));
    }
}

/// Non-secret, versioned UI choices. Never includes addresses, room IDs or credentials.
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct Settings {
    pub stream:crate::resolution::StreamSettings,
    pub upload:crate::bandwidth::UploadProfile,
    pub rate_mode:crate::adaptive_upload::Mode,
    pub volume:u32,pub muted:bool,pub sidebar:bool,pub self_preview:bool,pub grid:bool,
}
impl Default for Settings {fn default()->Self{Self{stream:Default::default(),upload:Default::default(),rate_mode:Default::default(),volume:80,muted:false,sidebar:true,self_preview:true,grid:false}}}
impl Settings {
    fn decode(bytes:&[u8])->Option<Self>{
        if bytes.len()>4096{return None;}
        let v:serde_json::Value=serde_json::from_slice(bytes).ok()?;
        if v["schema"].as_u64()?!=1{return None;}
        let size=match v["resolution"].as_str()?{"native"=>crate::resolution::OutputSize::Native,"1080p"=>crate::resolution::OutputSize::FullHd,"1440p"=>crate::resolution::OutputSize::QuadHd,_=>return None};
        let fps=u32::try_from(v["fps"].as_u64()?).ok()?;
        let stream=crate::resolution::StreamSettings{size,fps};if !stream.valid(){return None;}
        let upload=crate::bandwidth::UploadProfile::from_mbps(u32::try_from(v["upload_mbps"].as_u64()?).ok()?)?;
        let volume=u32::try_from(v["volume"].as_u64()?).ok()?;if volume>100{return None;}
        let rate_mode=match v.get("rate_mode"){None=>crate::adaptive_upload::Mode::Manual,Some(value)=>match value.as_str()?{"manual"=>crate::adaptive_upload::Mode::Manual,"automatic"=>crate::adaptive_upload::Mode::Automatic,_=>return None}};
        Some(Self{stream,upload,rate_mode,volume,muted:v["muted"].as_bool()?,sidebar:v["sidebar"].as_bool()?,self_preview:v["self_preview"].as_bool()?,grid:v["grid"].as_bool()?})
    }
    fn encode(self)->String{serde_json::json!({"schema":1,"resolution":match self.stream.size{crate::resolution::OutputSize::Native=>"native",crate::resolution::OutputSize::FullHd=>"1080p",crate::resolution::OutputSize::QuadHd=>"1440p"},"fps":self.stream.fps,"upload_mbps":self.upload.bps()/1_000_000,"rate_mode":match self.rate_mode{crate::adaptive_upload::Mode::Manual=>"manual",crate::adaptive_upload::Mode::Automatic=>"automatic"},"volume":self.volume,"muted":self.muted,"sidebar":self.sidebar,"self_preview":self.self_preview,"grid":self.grid}).to_string()}
    fn file()->Option<PathBuf>{path().map(|p|p.with_file_name("preferences.json"))}
    fn ephemeral()->bool{cfg!(test)||std::env::var_os("HYPERCAST_SMOKE_SECONDS").is_some()}
    pub fn load()->Self{
        if Self::ephemeral(){return Self::default();}
        Self::file().and_then(|p|{let mut b=Vec::new();std::fs::File::open(p).ok()?.take(4097).read_to_end(&mut b).ok()?;Self::decode(&b)}).unwrap_or_default()
    }
}
/// Disk writes run outside painting and are coalesced while a slider is moving.
pub struct Store {
    latest:Settings,queued:Settings,tx:Option<std::sync::mpsc::SyncSender<Settings>>,worker:Option<std::thread::JoinHandle<()>>,
}
impl Store {
    pub fn new(initial:Settings)->Self{
        let store=Self{latest:initial,queued:initial,tx:None,worker:None};
        if Settings::ephemeral(){return store;}
        let Some(file)=Settings::file()else{return store;};
        Self::at_file(initial,file)
    }
    fn at_file(initial:Settings,file:PathBuf)->Self{
        let mut store=Self{latest:initial,queued:initial,tx:None,worker:None};
        let (tx,rx)=std::sync::mpsc::sync_channel::<Settings>(1);
        if let Ok(worker)=std::thread::Builder::new().name("preferences".into()).spawn(move||{
            while let Ok(mut value)=rx.recv(){
                let deadline=std::time::Instant::now()+std::time::Duration::from_secs(2);
                loop{match rx.recv_timeout(std::time::Duration::from_millis(350)){
                    Ok(next)=>{value=next;if std::time::Instant::now()>=deadline{break;}},
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout)=>break,
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected)=>{let _=save(&file,&value.encode());return;}
                }}
                let _=save(&file,&value.encode());
            }
        }){store.tx=Some(tx);store.worker=Some(worker);}
        store
    }
    pub fn update(&mut self,value:Settings){
        self.latest=value;if self.queued==value{return;}
        if self.tx.as_ref().is_some_and(|tx|tx.try_send(value).is_ok()){self.queued=value;}
    }
}
impl Drop for Store{fn drop(&mut self){if let Some(tx)=self.tx.take(){if self.latest!=self.queued{let _=tx.send(self.latest);}drop(tx);}if let Some(worker)=self.worker.take(){let _=worker.join();}}}
#[cfg(test)]mod settings_tests{
    use super::*;
    #[test]fn writer_flushes_latest_choice_on_immediate_close(){
        let dir=std::env::temp_dir().join(format!("hypercast-settings-{}",uuid::Uuid::new_v4()));let file=dir.join("preferences.json");
        {let mut store=Store::at_file(Default::default(),file.clone());for volume in 0..=99{store.update(Settings{volume,..Default::default()});}}
        assert_eq!(Settings::decode(&std::fs::read(&file).unwrap()).unwrap().volume,99);
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(),1);
        std::fs::remove_file(file).unwrap();std::fs::remove_dir(dir).unwrap();
    }
    #[test]fn versioned_round_trip_rejects_corrupt_unsafe_and_future_values(){
        let value=Settings{rate_mode:crate::adaptive_upload::Mode::Automatic,volume:35,sidebar:false,stream:crate::resolution::StreamSettings{size:crate::resolution::OutputSize::FullHd,fps:60},..Default::default()};
        assert_eq!(Settings::decode(value.encode().as_bytes()),Some(value));
        let base:serde_json::Value=serde_json::from_str(&value.encode()).unwrap();
        let mut legacy=base.clone();legacy.as_object_mut().unwrap().remove("rate_mode");assert_eq!(Settings::decode(legacy.to_string().as_bytes()).unwrap().rate_mode,crate::adaptive_upload::Mode::Manual);
        for (field,bad) in [("schema",serde_json::json!(2)),("fps",serde_json::json!(999)),("volume",serde_json::json!(101)),("upload_mbps",serde_json::json!(500)),("rate_mode",serde_json::json!("invalid")),("resolution",serde_json::json!("../private"))]{let mut changed=base.clone();changed[field]=bad;assert!(Settings::decode(changed.to_string().as_bytes()).is_none());}
        assert!(Settings::decode(b"{broken").is_none());assert!(Settings::decode(&vec![b' ';4097]).is_none());
    }
}
