//! Room creation without sharing administrator credentials with the desktop.
use anyhow::{Result, ensure};
use std::{io::Read, sync::mpsc};
use eframe::egui;
use crate::desktop_ui as controls;

pub struct SavedRoom { pub room:String, pub title:String, pub persistent:bool, pub expired:bool, pub participants:u64 }
#[derive(Default)]
pub struct Catalog {
    pub open:bool, busy:bool, status:String, rooms:Vec<SavedRoom>, origin:String,
    last_refresh:Option<std::time::Instant>,
    receiver:Option<mpsc::Receiver<(String,String,Result<Vec<SavedRoom>>)>>,
}
impl Catalog {
    pub fn open(&mut self,address:String,session:String,ctx:egui::Context) {
        self.open=true;self.request(address,session,None,ctx);
    }
    fn request(&mut self,address:String,session:String,keep:Option<(String,bool)>,ctx:egui::Context) {
        if self.busy{return;}
        if self.origin!=address {self.rooms.clear();}
        self.origin=address.clone();self.last_refresh=Some(std::time::Instant::now());self.busy=true;self.status.clear();
        let(tx,rx)=mpsc::channel();self.receiver=Some(rx);
        std::thread::spawn(move|| {
            let result=(||->Result<Vec<SavedRoom>> {
                if let Some((room,persistent))=keep {catalog_post(&address,&session,"/api/rooms/keep",serde_json::json!({"room":room,"persistent":persistent}))?;}
                let value=catalog_post(&address,&session,"/api/rooms/list",serde_json::json!({}))?;
                let rows=value["rooms"].as_array().ok_or_else(||anyhow::anyhow!("Lista de salas inválida"))?;
                ensure!(rows.len()<=32,"Lista de salas excedeu o limite");
                let now=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs();
                rows.iter().map(|v| {
                    let text=|key:&str|->Result<String>{Ok(v[key].as_str().ok_or_else(||anyhow::anyhow!("Sala inválida"))?.to_owned())};
                    let room=text("room")?;let title=text("room_name")?;
                    ensure!(room.len()<=64 && !title.is_empty() && title.chars().count()<=64 && !title.chars().any(char::is_control),"Sala inválida");
                    let persistent=v["persistent"].as_bool().ok_or_else(||anyhow::anyhow!("Sala inválida"))?;
                    let expires=v["expires_at"].as_u64().ok_or_else(||anyhow::anyhow!("Sala inválida"))?;
                    Ok(SavedRoom{room,title,persistent,expired:!persistent && expires<=now,participants:v["participants"].as_u64().unwrap_or(0).min(12)})
                }).collect()
            })();
            let _=tx.send((address,session,result));ctx.request_repaint();
        });
    }
    pub fn show(&mut self,ctx:&egui::Context,address:&str,session:String,name:&mut String,current_room:Option<&str>,transitioning:bool)->Option<CreatedRoom> {
        if let Some(rx)=&self.receiver {
            match rx.try_recv() {
                Ok((origin,requested_session,result))=>{self.receiver=None;self.busy=false;if origin==address && requested_session==session && !session.is_empty(){match result {Ok(rooms)=>self.rooms=rooms,Err(e)=>self.status=e.to_string()}}},
                Err(mpsc::TryRecvError::Disconnected)=>{self.receiver=None;self.busy=false;self.status="Consulta interrompida; tente novamente".into();},
                Err(mpsc::TryRecvError::Empty)=>{}
            }
        }
        if self.origin!=address || session.is_empty(){self.open=false;self.rooms.clear();return None;}
        if !self.open{return None;}
        if !self.busy && self.last_refresh.is_none_or(|at|at.elapsed()>=std::time::Duration::from_secs(30)) {self.request(address.to_owned(),session.clone(),None,ctx.clone());}
        let mut open=true;let mut selected=None;let mut keep=None;let mut refresh=false;
        egui::Window::new("Escolher sala").id(egui::Id::new("saved_rooms_window")).open(&mut open)
            .frame(egui::Frame::window(&ctx.style_of(egui::Theme::Dark)).fill(controls::PANEL).stroke(egui::Stroke::new(1.0,controls::BORDER)).corner_radius(12).inner_margin(16))
            .fixed_pos(egui::pos2(16.0,16.0)).default_width(440.0).max_width(440.0).resizable(false).collapsible(false).show(ctx,|ui|{
                ui.style_mut().wrap_mode=Some(egui::TextWrapMode::Wrap);
                ui.spacing_mut().item_spacing=egui::vec2(10.0,10.0);
                ui.horizontal(|ui|{
                    ui.label(egui::RichText::new(if self.busy {"Atualizando salas…".into()}else{format!("{} sala(s) disponível(is)",self.rooms.len())}).size(12.0).color(controls::MUTED));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{
                        if controls::compact(ui,"Atualizar lista",!self.busy).clicked(){refresh=true;}
                    });
                });
                let name_label=ui.label("Seu nome").id;
                controls::input(ui,name,"Como quer aparecer na sala",false,64,current_room.is_none() && !transitioning).labelled_by(name_label);
                if !self.status.is_empty(){ui.label(&self.status);}
                if !self.busy && self.rooms.is_empty(){ui.label("Nenhuma sala salva. Crie uma para começar.");}
                let list_height=(ctx.content_rect().bottom()-ui.cursor().top()-64.0).max(160.0);
                egui::ScrollArea::vertical().min_scrolled_height(list_height).max_height(list_height).auto_shrink([false,false]).show(ui,|ui|{
                    let card_width=ui.available_width();
                    for entry in &self.rooms {
                        ui.push_id(&entry.room,|ui|{
                            egui::Frame::new().fill(controls::SURFACE).stroke(egui::Stroke::new(1.0,controls::BORDER))
                                .corner_radius(10).inner_margin(12).show(ui,|ui|{
                                    ui.set_width((card_width-26.0).max(200.0));
                                    ui.set_min_width((card_width-26.0).max(200.0));
                                    let title_width=(ui.available_width()-98.0).max(120.0);
                                    ui.horizontal_top(|ui|{
                                        ui.allocate_ui_with_layout(egui::vec2(title_width,0.0),egui::Layout::top_down(egui::Align::Min),|ui|{
                                            ui.set_min_width(title_width);
                                            ui.label(egui::RichText::new(&entry.title).strong().size(15.0));
                                            ui.label(egui::RichText::new(format!("{} · {} pessoa(s)",if entry.expired{"Expirada"}else if entry.persistent{"Permanente"}else{"Temporária · 24 h"},entry.participants)).size(12.0).color(controls::MUTED));
                                        });
                                        let valid_name=!name.is_empty() && name.len()<=64 && name.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'-'||b==b'_');
                                        let current=current_room==Some(entry.room.as_str());
                                        let enabled=!self.busy && !transitioning && !entry.expired && !current && valid_name;
                                        let enter=controls::primary(ui,if current{"Nesta sala"}else{"Entrar"},enabled,egui::vec2(88.0,36.0));
                                        enter.widget_info(||egui::WidgetInfo::labeled(egui::WidgetType::Button,enabled,format!("Entrar em {}",entry.title)));
                                        if enter.clicked(){selected=Some(CreatedRoom{room:entry.room.clone(),title:entry.title.clone()});}
                                    });
                                    let mut persistent=entry.persistent;
                                    let checkbox=ui.add_enabled(!self.busy,egui::Checkbox::new(&mut persistent,"Manter esta sala"));
                                    checkbox.widget_info(||egui::WidgetInfo::selected(egui::WidgetType::Checkbox,!self.busy,persistent,format!("Manter sala {}",entry.title)));
                                    if checkbox.changed(){keep=Some((entry.room.clone(),persistent));}
                                    if entry.expired{ui.label(egui::RichText::new(if entry.participants>0{"A limpeza aguarda os participantes saírem."}else{"Aguardando limpeza automática."}).size(11.0).color(controls::MUTED));}
                                });
                        });
                    }
                });
                if transitioning{ui.small("Aguarde a troca de sala…");}
                else if current_room.is_some(){ui.small("Ao entrar em outra sala, seu compartilhamento atual será encerrado.");}
            });
        self.open=open && selected.is_none();
        if refresh || keep.is_some(){self.request(address.to_owned(),session,keep,ctx.clone());}
        selected
    }
}
fn catalog_post(address:&str,session:&str,path:&str,body:serde_json::Value)->Result<serde_json::Value> {
    ensure!(!session.is_empty() && session.len()<=4096,"Entre no servidor novamente");
    let response=reqwest::blocking::Client::builder().redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(15)).build()?.post(crate::invitation::endpoint(address,path)?)
        .bearer_auth(session).json(&body).send().map_err(|_|anyhow::anyhow!("Servidor indisponível; tente novamente"))?;
    match response.status().as_u16(){200=>{},401|403=>anyhow::bail!("Sessão expirada; entre no servidor novamente"),_=>anyhow::bail!("Não foi possível atualizar a lista de salas")}
    let mut bytes=Vec::new();response.take(65537).read_to_end(&mut bytes)?;ensure!(bytes.len()<=65536,"Resposta excedeu o limite");
    Ok(serde_json::from_slice(&bytes)?)
}

pub struct CreatedRoom { pub room: String, pub title: String }
pub struct Creator {
    pub open: bool, pub title: String, pub persistent: bool, pub busy: bool, pub status: String,
    request_id: String, requested_title: String, requested_persistent:bool,
    receiver: Option<mpsc::Receiver<Result<CreatedRoom>>>,
}
impl Default for Creator {
    fn default()->Self {Self {open:false,title:String::new(),persistent:false,busy:false,status:String::new(),request_id:String::new(),requested_title:String::new(),requested_persistent:false,receiver:None}}
}
impl Creator {
    pub fn open(&mut self,_address:&str) {self.open=true;self.status.clear();self.request_id=uuid::Uuid::new_v4().simple().to_string();self.requested_title.clear();}
    pub fn start(&mut self,address:String,session:String,ctx:egui::Context) {
        if self.busy {return;}
        let title=self.title.trim().to_owned();
        if self.requested_title!=title || self.requested_persistent!=self.persistent {self.request_id=uuid::Uuid::new_v4().simple().to_string();self.requested_title=title.clone();self.requested_persistent=self.persistent;}
        let persistent=self.persistent;let request=self.request_id.clone();let(tx,rx)=mpsc::channel();self.busy=true;self.status="Criando sala…".into();
        std::thread::spawn(move||{let _=tx.send(create(&address,&session,&title,&request,persistent));ctx.request_repaint();});self.receiver=Some(rx);
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

pub fn create(address:&str,code:&str,title:&str,request_id:&str,persistent:bool)->Result<CreatedRoom> {
    ensure!(!code.is_empty() && code.len()<=4096,"Autentique-se no servidor");
    let endpoint=crate::invitation::endpoint(address,"/api/rooms")?;
    let response=reqwest::blocking::Client::builder().redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(20)).user_agent("HyperCast-Desktop/0.6").build()?
        .post(endpoint).bearer_auth(code).json(&serde_json::json!({"room_name":title,"request_id":request_id,"persistent":persistent}))
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
    let room=value["room"].as_str().ok_or_else(||anyhow::anyhow!("Resposta de sala inválida"))?;
    let title=value["room_name"].as_str().ok_or_else(||anyhow::anyhow!("Resposta de sala inválida"))?;
    ensure!(!room.is_empty() && room.len()<=64 && title.chars().count()<=64,"Resposta de sala inválida");
    Ok(CreatedRoom{room:room.to_owned(),title:title.to_owned()})
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
