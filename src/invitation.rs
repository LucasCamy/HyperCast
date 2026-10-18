//! Fetch short-lived room JWTs from an authenticated invitation endpoint.
use anyhow::{Result, ensure};
use std::io::Read;

pub fn client_id() -> String {
    let fresh = uuid::Uuid::new_v4().simple().to_string();
    #[cfg(windows)]
    let root=std::env::var_os("LOCALAPPDATA").map(|p|std::path::PathBuf::from(p).join("HyperCast"));
    #[cfg(not(windows))]
    let root=std::env::var_os("XDG_STATE_HOME").map(std::path::PathBuf::from)
        .filter(|p|p.is_absolute()).or_else(||std::env::var_os("HOME").map(|p|std::path::PathBuf::from(p).join(".local/state")))
        .map(|p|p.join("hypercast"));
    let Some(root)=root else{return fresh;};
    let path = root.join("client-id");
    if let Ok(file) = std::fs::File::open(&path) {
        let mut text = String::new();
        if file.take(33).read_to_string(&mut text).is_ok() && text.len() == 32 && text.bytes().all(|b| b.is_ascii_hexdigit()) {
            return text;
        }
    }
    if std::fs::create_dir_all(root).is_ok() {
        use std::io::Write;
        let mut options=std::fs::OpenOptions::new();options.write(true).create_new(true);
        #[cfg(unix)] {use std::os::unix::fs::OpenOptionsExt;options.mode(0o600);}
        if let Ok(mut file) = options.open(path) {
            let _ = file.write_all(fresh.as_bytes());
        }
    }
    fresh
}

pub struct JoinedRoom { pub token:String, pub title:Option<String> }
pub fn join(address:&str,invite:&str,name:&str,request_id:&str,session:&str)->Result<String> {
    Ok(join_details(address,invite,name,request_id,session)?.token)
}
pub fn join_details(address: &str, invite: &str, name: &str, request_id: &str, session: &str) -> Result<JoinedRoom> {
    join_request(address,session,"/api/join",serde_json::json!({"invite":invite,"name":name,"request_id":request_id}))
}
pub fn join_room(address:&str,room:&str,name:&str,request_id:&str,session:&str)->Result<JoinedRoom> {
    join_request(address,session,"/api/rooms/join",serde_json::json!({"room":room,"name":name,"request_id":request_id}))
}
fn join_request(address:&str,session:&str,path:&str,body:serde_json::Value)->Result<JoinedRoom> {
    let endpoint=endpoint(address,path)?;
    let client = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(12))
        .user_agent("HyperCast-Desktop/0.1")
        .build()?;
    let response = client.post(endpoint).bearer_auth(session).json(&body).send()?;
    check_status(response.status().as_u16())?;
    let mut bytes = Vec::new(); response.take(16385).read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 16384, "Invitation response exceeded size limit");
    decode_joined(&bytes)
}

fn check_status(status:u16)->Result<()> {
    match status {
        200 => {},
        401 | 403 => anyhow::bail!(crate::i18n::label("Sala indisponível ou sessão expirada; entre no servidor novamente","Room unavailable or session expired; connect to the server again")),
        404 => anyhow::bail!(crate::i18n::label("Sala não encontrada nesta chave de acesso","Room not found for this access key")),
        409 => anyhow::bail!(crate::i18n::label("Limite de salas de vídeo simultâneas atingido; aguarde uma ficar livre e tente novamente","Concurrent video room limit reached; wait for a room to become available and try again")),
        400 => anyhow::bail!(crate::i18n::label("Use nome com letras sem acentos, números, hífen ou sublinhado","Use unaccented letters, numbers, hyphens or underscores for your name")),
        _ => anyhow::bail!(crate::i18n::label("Não foi possível entrar na sala; tente novamente","Could not join the room; try again")),
    }
    Ok(())
}
fn decode_joined(bytes:&[u8])->Result<JoinedRoom>{
    let value: serde_json::Value = serde_json::from_slice(bytes)?;
    let token = value["token"].as_str().ok_or_else(|| anyhow::anyhow!("Missing room credential"))?;
    ensure!(!token.is_empty() && token.len() <= 8192, "Invalid room credential length");
    let title=value["room_name"].as_str().filter(|s|!s.is_empty() && s.chars().count()<=64 && !s.chars().any(char::is_control)).map(str::to_owned);
    Ok(JoinedRoom{token:token.to_owned(),title})
}

/// Owns one admission attempt; dropping it cancels its HTTP future and joins it.
pub struct JoinAttempt {pub receiver:std::sync::mpsc::Receiver<Result<JoinedRoom>>,cancel:crate::room_transition::Cancellation,worker:Option<std::thread::JoinHandle<()>>}
impl JoinAttempt {
    pub fn start(address:String,code:String,name:String,device:String,session:String,by_room:bool,ctx:eframe::egui::Context)->Self {
        let (tx,receiver)=std::sync::mpsc::channel();let cancel=crate::room_transition::Cancellation::default();let stopping=cancel.clone();
        let worker=std::thread::spawn(move||{
            let runtime=match tokio::runtime::Runtime::new(){Ok(runtime)=>runtime,Err(error)=>{let _=tx.send(Err(error.into()));ctx.request_repaint();return;}};
            if let Some(result)=runtime.block_on(stopping.run(join_async(&address,&code,&name,&device,&session,by_room))){let _=tx.send(result);ctx.request_repaint();}
            runtime.shutdown_timeout(std::time::Duration::from_secs(2));
        });Self{receiver,cancel,worker:Some(worker)}
    }
}
impl Drop for JoinAttempt {fn drop(&mut self){self.cancel.cancel();if let Some(worker)=self.worker.take(){let _=worker.join();}}}

/// Cancellable HTTP path for room admission; callers drop the future on exit.
pub async fn join_async(address:&str,code:&str,name:&str,request_id:&str,session:&str,by_room:bool)->Result<JoinedRoom>{
    let path=if by_room{"/api/rooms/join"}else{"/api/join"};
    let body=if by_room{serde_json::json!({"room":code,"name":name,"request_id":request_id})}else{serde_json::json!({"invite":code,"name":name,"request_id":request_id})};
    let client=reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).timeout(std::time::Duration::from_secs(12)).user_agent("HyperCast-Desktop").build()?;
    let mut response=client.post(endpoint(address,path)?).bearer_auth(session).json(&body).send().await?;
    check_status(response.status().as_u16())?;
    let mut bytes=Vec::new();
    while let Some(chunk)=response.chunk().await? {ensure!(bytes.len()+chunk.len()<=16384,"Room response exceeded size limit");bytes.extend_from_slice(&chunk);}
    decode_joined(&bytes)
}

pub(crate) fn endpoint(address:&str,path:&str)->Result<url::Url>{
    let mut endpoint = url::Url::parse(address)?;
    ensure!(endpoint.scheme() == "wss" || (endpoint.scheme() == "ws" && matches!(endpoint.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))), "Use WSS for invitations");
    ensure!(endpoint.username().is_empty() && endpoint.password().is_none() && endpoint.query().is_none(), "Unsupported server URL");
    let scheme = if endpoint.scheme() == "wss" { "https" } else { "http" };
    endpoint.set_scheme(scheme).map_err(|_| anyhow::anyhow!("Invalid endpoint scheme"))?;
    endpoint.set_path(path); endpoint.set_fragment(None);
    Ok(endpoint)
}
