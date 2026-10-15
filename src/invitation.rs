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
    let endpoint = endpoint(address,"/api/join")?;
    let client = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(12))
        .user_agent("HyperCast-Desktop/0.1")
        .build()?;
    let response = client.post(endpoint).bearer_auth(session).json(&serde_json::json!({
        "invite": invite, "name": name, "request_id": request_id,
    })).send()?;
    match response.status().as_u16() {
        200 => {},
        401 | 403 => anyhow::bail!("Convite inválido, expirado ou sem vagas de uso"),
        409 => anyhow::bail!("A sala atingiu o limite de publicadores ativos ou reservados"),
        400 => anyhow::bail!("Use nome com letras sem acentos, números, hífen ou sublinhado"),
        _ => anyhow::bail!("Serviço de convites indisponível; tente novamente"),
    }
    let mut bytes = Vec::new(); response.take(16385).read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 16384, "Invitation response exceeded size limit");
    let value: serde_json::Value = serde_json::from_slice(&bytes)?;
    let token = value["token"].as_str().ok_or_else(|| anyhow::anyhow!("Missing room credential"))?;
    ensure!(!token.is_empty() && token.len() <= 8192, "Invalid room credential length");
    let title=value["room_name"].as_str().filter(|s|!s.is_empty() && s.chars().count()<=64 && !s.chars().any(char::is_control)).map(str::to_owned);
    Ok(JoinedRoom{token:token.to_owned(),title})
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
