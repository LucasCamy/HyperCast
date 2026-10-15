use super::{
    engine::{Command, Engine, Layout},
    media::{Codec, Encoder, Source},
};
use crate::{
    room_creation::{Creator, ServerAccess},
    subscription::ViewMode,
};
use eframe::egui::{self, Color32, RichText, Vec2};
use std::{
    collections::HashMap,
    sync::mpsc,
    time::{Duration, Instant},
};
const BG: Color32 = Color32::from_rgb(14, 18, 19);
const PANEL: Color32 = Color32::from_rgb(21, 27, 28);
const SURFACE: Color32 = Color32::from_rgb(28, 35, 36);
const BORDER: Color32 = Color32::from_rgb(42, 50, 50);
const MUTED: Color32 = Color32::from_rgb(154, 167, 161);
const ACCENT: Color32 = Color32::from_rgb(191, 239, 121);
fn button(ui: &mut egui::Ui, label: &str, enabled: bool, primary: bool) -> egui::Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(RichText::new(label).color(if primary {
            ACCENT
        } else {
            Color32::from_rgb(234, 239, 234)
        }))
        .min_size(Vec2::new(ui.available_width(), 40.0))
        .corner_radius(8)
        .fill(if primary {
            Color32::from_rgb(40, 51, 40)
        } else {
            SURFACE
        })
        .stroke(egui::Stroke::new(1.0, BORDER)),
    )
}
fn small(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.scope(|ui| {
        ui.spacing_mut().button_padding = Vec2::new(10.0, 4.0);
        ui.spacing_mut().interact_size.y = 26.0;
        ui.add(
            egui::Button::new(RichText::new(label).size(11.0).color(MUTED))
                .corner_radius(9)
                .fill(SURFACE)
                .stroke(egui::Stroke::new(1.0, BORDER)),
        )
    })
    .inner
}
fn input(ui: &mut egui::Ui, value: &mut String, hint: &str, password: bool, limit: usize) -> egui::Response {
    ui.add(
        egui::TextEdit::singleline(value)
            .desired_width(f32::INFINITY)
            .password(password)
            .char_limit(limit)
            .margin(egui::Margin::symmetric(12, 10))
            .background_color(SURFACE)
            .hint_text(hint),
    )
}
struct App {
    address: String,
    name: String,
    sidebar_open: bool,
    invite: String,
    saved_room:Option<String>,
    device: String,
    access: ServerAccess,
    server_status: crate::server_status::ServerStatus,
    creator: Creator,
    catalog:crate::room_creation::Catalog,
    room_title:String,
    engine: Option<Engine>,
    joining: Option<mpsc::Receiver<anyhow::Result<crate::invitation::JoinedRoom>>>,
    status: String,
    layout: Layout,
    codec: Codec,
    encoder: Encoder,
    audio: bool,
    fullscreen: bool,
    debug: bool,
    textures: HashMap<String, (u64, egui::TextureHandle)>,
    started: Instant,
}
impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_theme(egui::ThemePreference::Dark);
        let mut style = (*cc.egui_ctx.style_of(egui::Theme::Dark)).clone();
        style.visuals.panel_fill = BG;
        style.visuals.window_fill = PANEL;
        style.visuals.selection.bg_fill = ACCENT.gamma_multiply(0.25);
        style.spacing.item_spacing = Vec2::new(12.0, 12.0);
        style.spacing.button_padding = Vec2::new(16.0, 10.0);
        style.animation_time = 0.0;
        cc.egui_ctx.set_style_of(egui::Theme::Dark, style);
        let mut app = Self {
            address: std::env::var("HYPERCAST_URL")
                .unwrap_or("wss://hypercast.cloudlane.com.br".into()),
            name: crate::preferences::load_name(),
            sidebar_open: true,
            invite: std::env::var("HYPERCAST_INVITE").unwrap_or_default(),saved_room:None,
            device: crate::invitation::client_id(),
            access: ServerAccess::default(),
            server_status: crate::server_status::ServerStatus::default(),
            creator: Creator::default(),
            catalog:crate::room_creation::Catalog::default(),room_title:String::new(),
            engine: None,
            joining: None,
            status: String::new(),
            layout: Layout::default(),
            codec: Codec::Hevc,
            encoder: Encoder::Auto,
            audio: false,
            fullscreen: false,
            debug: false,
            textures: HashMap::new(),
            started: Instant::now(),
        };
        if std::env::var("HYPERCAST_AUTO_CONNECT").as_deref() == Ok("1") {
            if let Ok(token) = std::env::var("HYPERCAST_ROOM_TOKEN") {
                app.engine = Some(Engine::start(
                    app.address.clone(),
                    token,
                    cc.egui_ctx.clone(),
                ));
            }
        }
        app
    }
    fn send(&self, command: Command) {
        if let Some(engine) = &self.engine {
            let _ = engine.tx.send(command);
        }
    }
    fn restart_reception(&self) {self.send(Command::RestartReception);}
    fn layout(&self) {
        self.send(Command::Layout(self.layout.clone()));
    }
    fn fullscreen(&mut self, ctx: &egui::Context, enabled: bool) {
        self.fullscreen = enabled;
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(enabled));
    }
    fn connect(&mut self, ctx: &egui::Context) {
        let _ = crate::preferences::save_name(&self.name);
        let (address, invite, name, device, session) = (
            self.address.clone(),
            self.invite.clone(),
            self.name.clone(),
            self.device.clone(),
            self.access.token(),
        );
        let saved_room=self.saved_room.clone();
        let (tx, rx) = mpsc::channel();
        let repaint = ctx.clone();
        self.joining = Some(rx);
        self.status = "Entrando na sala…".into();
        std::thread::spawn(move || {
            let _ = tx.send(if let Some(room)=saved_room{crate::invitation::join_room(&address,&room,&name,&device,&session)}else{crate::invitation::join_details(&address, &invite, &name, &device, &session)});
            repaint.request_repaint();
        });
    }
    fn video(&mut self, ui: &mut egui::Ui, sid: &str, name: &str, size: Vec2) {
        let frame = self
            .engine
            .as_ref()
            .and_then(|engine| engine.state.lock().unwrap().frames.get(sid).cloned());
        if let Some((sequence, frame)) = frame {
            if self
                .textures
                .get(sid)
                .is_none_or(|entry| entry.0 != sequence)
            {
                let (w, h) = (frame.buffer.width(), frame.buffer.height());
                let mut rgba = vec![0; w as usize * h as usize * 4];
                frame.buffer.to_argb(
                    livekit::webrtc::video_frame::VideoFormatType::ABGR,
                    &mut rgba,
                    w * 4,
                    w as i32,
                    h as i32,
                );
                let image =
                    egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba);
                if let Some(entry) = self.textures.get_mut(sid) {
                    entry.1.set(image, egui::TextureOptions::LINEAR);
                    entry.0 = sequence;
                } else {
                    self.textures.insert(
                        sid.into(),
                        (
                            sequence,
                            ui.ctx()
                                .load_texture(sid, image, egui::TextureOptions::LINEAR),
                        ),
                    );
                }
            }
        }
        let response=egui::Frame::new()
            .fill(PANEL)
            .corner_radius(12)
            .inner_margin(10)
            .show(ui, |ui| {
                ui.set_min_size(size - Vec2::splat(20.0));
                ui.label(RichText::new(name).color(MUTED).size(12.0));
                if let Some((_, texture)) = self.textures.get(sid) {
                    ui.add(
                        egui::Image::new(texture)
                            .max_size(size - Vec2::new(20.0, 50.0))
                            .maintain_aspect_ratio(true),
                    );
                } else {
                    ui.centered_and_justified(|ui| {
                        ui.label("Aguardando vídeo…");
                    });
                }
            }).response.interact(egui::Sense::click());
        if response.double_clicked() {
            self.layout.focus=self.engine.as_ref().and_then(|e|e.state.lock().unwrap().screens.iter().find(|s|s.sid==sid).map(|s|s.owner.clone()));
            self.layout.mode=ViewMode::Spotlight;
            self.layout();
        }
    }
}
impl Drop for App {
    fn drop(&mut self) { let _ = crate::preferences::save_name(&self.name); }
}
impl eframe::App for App {
    fn ui(&mut self, root: &mut egui::Ui, _: &mut eframe::Frame) {
        let context = root.ctx().clone();
        let ctx = &context;
        self.access.poll(&self.address, &self.device, ctx.clone());
        self.server_status.poll(&self.address,ctx);
        if let Some(room) = self.creator.poll() {
            self.saved_room=Some(room.room);self.invite.clear();
            self.status = format!("Sala criada: {}", room.title);
        }
        if let Some(rx) = &self.joining {
            if let Ok(result) = rx.try_recv() {
                self.joining = None;
                match result {
                    Ok(joined) => {
                        self.room_title=joined.title.unwrap_or_else(||"Sala".into());
                        let token=joined.token;
                        self.engine = Some(Engine::start(self.address.clone(), token, ctx.clone()));
                        self.status.clear();
                        self.textures.clear();
                    }
                    Err(e) => self.status = e.to_string(),
                }
            }
        }
        let (
            connected,
            sharing,
            selecting,
            can_publish,
            reconnecting,
            participants,
            screens,
            state_status,
        ) = self
            .engine
            .as_ref()
            .map(|e| {
                let s = e.state.lock().unwrap();
                (
                    s.connected,
                    s.sharing,
                    s.selecting,
                    s.can_publish,
                    s.reconnecting,
                    s.participants.clone(),
                    s.screens.clone(),
                    s.status.clone(),
                )
            })
            .unwrap_or_default();
        let resetting=self.engine.as_ref().is_some_and(|e|e.state.lock().unwrap().reception_resetting);
        if connected && !resetting && ctx.input(|i|i.key_pressed(egui::Key::F5)){self.restart_reception();}
        if ctx.input(|i| i.key_pressed(egui::Key::F11)) {
            self.fullscreen(ctx, !self.fullscreen);
        }
        if self.fullscreen && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.fullscreen(ctx, false);
        }
        if !self.fullscreen && self.sidebar_open {
            egui::Panel::left("sidebar").exact_size(310.0).frame(egui::Frame::new().fill(PANEL).inner_margin(20)).show(root,|ui|{
                egui::ScrollArea::vertical().show(ui,|ui|{
                    ui.horizontal(|ui|{ui.label(RichText::new("HYPERCAST").size(16.0));
                        if small(ui,"Atualizar").clicked(){ctx.open_url(egui::OpenUrl::new_tab("https://github.com/LucasCamy/HyperCast/releases"));}
                    });
                    ui.label(RichText::new(self.server_status.label()).size(11.0).color(if self.server_status.status==crate::server_status::Status::Online{ACCENT}else{MUTED})).on_hover_text(format!("Servidor: {}",self.server_status.detail));
                    if reconnecting {ui.label(RichText::new("Sala: reconectando…").size(11.0).color(MUTED));}
                    ui.add_space(12.0);
                    if !self.access.authenticated(){
                        ui.label(RichText::new("ACESSO AO SERVIDOR").size(11.0).color(MUTED));
                        ui.collapsing("Servidor",|ui|{input(ui,&mut self.address,"wss://servidor",false,512);});
                        input(ui,&mut self.access.code,"Chave compartilhada do servidor",true,64);
                        if button(ui,"Entrar no servidor",!self.access.busy,true).clicked(){self.access.start(self.address.clone(),self.device.clone(),ctx.clone());}
                        ui.label(RichText::new(&self.access.status).size(12.0).color(MUTED));
                    }else if !connected{
                        if input(ui,&mut self.name,"Seu nome",false,32).lost_focus(){let _=crate::preferences::save_name(&self.name);}
                        if button(ui,"Escolher sala",self.joining.is_none()&&!self.creator.busy,true).clicked(){self.creator.open=false;self.catalog.open(self.address.clone(),self.access.token(),ctx.clone());}
                        if button(ui,"Criar sala",!self.creator.busy,false).clicked(){self.creator.open(&self.address);}
                        if small(ui,"Trocar acesso ao servidor").clicked(){self.access.logout(&self.address);}
                    }else{
                        ui.label(RichText::new(&self.room_title).strong());
                        ui.label(RichText::new("PARTICIPANTES").size(11.0).color(MUTED));
                        ui.label(format!("{} (você) · {}",if self.name.is_empty(){"Linux"}else{&self.name},if sharing{"Transmitindo"}else{"Assistindo"}));
                        for(id,name)in participants{
                            let live=screens.iter().any(|s|s.owner==id);let title=if name.is_empty(){id.clone()}else{name};
                            let focused=!self.layout.paused && self.layout.mode==ViewMode::Spotlight && (self.layout.focus.as_ref()==Some(&id) || (self.layout.focus.is_none() && screens.first().is_some_and(|s|s.owner==id)));
                            let response=button(ui,&format!("{title} · {}",if live {if focused {"Em destaque · transmitindo"}else{"Transmitindo"}}else{"Assistindo"}),live,focused);
                            if response.clicked(){self.layout.focus=Some(id);if response.double_clicked(){self.layout.mode=ViewMode::Spotlight;}self.layout();}
                        }
                        if small(ui,"Escolher sala").clicked(){self.catalog.open(self.address.clone(),self.access.token(),ctx.clone());}
                    }
                    ui.separator();ui.label(RichText::new("COMPARTILHAR").size(11.0).color(MUTED));
                    ui.add_enabled_ui(!sharing&&!selecting,|ui|{
                        egui::ComboBox::from_id_salt("codec").width(ui.available_width()).selected_text(if self.codec==Codec::Hevc{"HEVC / H.265"}else{"AV1"}).show_ui(ui,|ui|{
                            ui.selectable_value(&mut self.codec,Codec::Hevc,"HEVC / H.265");ui.selectable_value(&mut self.codec,Codec::Av1,"AV1");
                        });
                        egui::ComboBox::from_id_salt("encoder").width(ui.available_width()).selected_text(if self.encoder==Encoder::Auto{"GPU · NVENC / VA-API"}else{"Compatível · HEVC 30 FPS"}).show_ui(ui,|ui|{
                            ui.selectable_value(&mut self.encoder,Encoder::Auto,"GPU · NVENC / VA-API");ui.selectable_value(&mut self.encoder,Encoder::Software,"Compatível · HEVC 30 FPS");
                        });
                    });
                    if button(ui,if sharing{"Trocar tela ou janela"}else{"Compartilhar tela/janela"},connected&&can_publish&&!selecting,true).clicked(){self.send(Command::Share(Source::Portal,self.codec,self.encoder));}
                    if std::env::var("WAYLAND_DISPLAY").is_err()&&!sharing&&button(ui,"Tela inteira (X11)",connected&&can_publish&&!selecting,false).clicked(){self.send(Command::Share(Source::X11,self.codec,self.encoder));}
                    if (sharing||selecting)&&button(ui,if selecting{"Cancelar seleção"}else{"Parar compartilhamento"},true,false).clicked(){self.send(if selecting{Command::CancelSelection}else{Command::Stop});}
                    if ui.checkbox(&mut self.audio,"Áudio do sistema").changed(){self.send(Command::Audio(self.audio));}
                    if self.audio{ui.label(RichText::new("O áudio recebido fica silenciado durante o envio para evitar eco.").size(11.0).color(MUTED));}
                    ui.label(RichText::new(&state_status).size(12.0).color(MUTED));ui.label(RichText::new(&self.status).size(12.0).color(MUTED));
                    ui.separator();
                    if ui.add(egui::Slider::new(&mut self.layout.volume,0..=100).text("Volume")).changed(){self.layout();}
                    if button(ui,if self.layout.paused{"Retomar recepção"}else{"Pausar recepção"},connected,false).clicked(){self.layout.paused=!self.layout.paused;self.layout();}
                    ui.horizontal(|ui|{
                        if small(ui,"Debug").clicked(){self.debug=true;}
                        if self.engine.is_some()&&small(ui,"Sair").clicked(){self.engine=None;self.textures.clear();}
                    });
                });
            });
        }
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(BG)
                    .inner_margin(if self.fullscreen { 8 } else { 28 }),
            )
            .show(root, |ui| {
                ui.horizontal(|ui| {
                    let refresh=ui.add_enabled(connected&&!resetting,egui::Button::new("↻").corner_radius(6)).on_hover_text("Reconectar recepção · F5");
                    refresh.widget_info(||egui::WidgetInfo::labeled(egui::WidgetType::Button,connected&&!resetting,"Reconectar recepção"));
                    if refresh.clicked(){self.restart_reception();}
                    if resetting{ui.label("Reconectando recepção…");}
                    if !self.fullscreen {
                        if small(ui,if self.sidebar_open {"Recolher menu"}else{"Abrir menu"}).clicked(){self.sidebar_open=!self.sidebar_open;}
                        ui.heading("Telas da sala");
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if self.fullscreen {
                            if small(ui, "×").clicked() {
                                self.fullscreen(ctx, false);
                            }
                            ui.menu_button("Volume", |ui| {
                                if ui
                                    .add(egui::Slider::new(&mut self.layout.volume, 0..=100))
                                    .changed()
                                {
                                    self.layout();
                                }
                                if small(ui, "Silenciar").clicked() {
                                    self.layout.volume = 0;
                                    self.layout();
                                }
                            });
                        } else {
                            if small(ui, "Tela cheia · F11").clicked() {
                                self.fullscreen(ctx, true);
                            }
                            for (mode, name) in
                                [(ViewMode::Grid, "Grid"), (ViewMode::Spotlight, "Destaque")]
                            {
                                if ui
                                    .selectable_value(&mut self.layout.mode, mode, name)
                                    .changed()
                                {
                                    self.layout();
                                }
                            }
                        }
                    });
                });
                ui.add_space(18.0);
                if screens.is_empty() {
                    ui.centered_and_justified(|ui| {
                        ui.label(RichText::new("Um lugar para trabalhar juntos.").size(24.0));
                    });
                } else if self.layout.mode == ViewMode::Grid && !self.fullscreen {
                    let width = (ui.available_width() - (screens.len() - 1) as f32 * 12.0)
                        / screens.len() as f32;
                    let height = ui.available_height();
                    ui.horizontal(|ui| {
                        for screen in &screens {
                            self.video(ui, &screen.sid, &screen.name, Vec2::new(width, height));
                        }
                    });
                } else {
                    let focus = self
                        .layout
                        .focus
                        .as_deref()
                        .and_then(|id| screens.iter().find(|s| s.owner == id))
                        .unwrap_or(&screens[0]);
                    self.video(ui, &focus.sid, &focus.name, ui.available_size());
                }
            });
        if self.creator.open {
            let mut open = true;
            egui::Window::new("Criar sala")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .default_width(360.0)
                .show(ctx, |ui| {
                    input(ui, &mut self.creator.title, "Nome da sala", false, 64);
                    ui.checkbox(&mut self.creator.persistent,"Manter sala indefinidamente");
                    if button(ui, "Criar sala", !self.creator.busy, true).clicked() {
                        self.creator
                            .start(self.address.clone(), self.access.token(), ctx.clone());
                    }
                    ui.label(&self.creator.status);
                });
            self.creator.open = open;
        }
        if !self.fullscreen {
            if let Some(selected)=self.catalog.show(ctx,&self.address,self.access.token(),&mut self.name,connected) {
                self.saved_room=Some(selected.room);self.invite.clear();self.connect(ctx);
            }
        }
        if self.debug {
            egui::Window::new("Diagnóstico").open(&mut self.debug).collapsible(false).resizable(false).default_width(360.0).show(ctx,|ui|{
                ui.label(format!("HyperCast {} · Linux",env!("CARGO_PKG_VERSION")));
                ui.label(format!("Decodificadores ativos: {}",super::media::decoder_count()));
                if let Some(e)=&self.engine{let s=e.state.lock().unwrap();ui.label(format!("Quadros enviados: {} · recebidos: {}",s.sent,s.received));}
                ui.label("A chave de acesso fica somente na memória desta sessão.");
                ui.label("Renderização portátil com transferência para RAM; desempenho QHD 120 FPS ainda requer validação.");
            });
        }
        if let Ok(seconds) = std::env::var("HYPERCAST_SMOKE_SECONDS")
            .unwrap_or_default()
            .parse::<u64>()
        {
            if (1..=300).contains(&seconds) && self.started.elapsed() > Duration::from_secs(seconds)
            {
                if let Some(e) = &self.engine {
                    let s = e.state.lock().unwrap();
                    println!(
                        "Linux GUI smoke: {} received, {} textures, connected={}, audio={}",
                        s.received,
                        self.textures.len(),
                        s.connected,
                        s.audio_received
                    );
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        ctx.request_repaint_after(Duration::from_millis(if connected { 16 } else { 100 }));
    }
}
pub fn run() -> anyhow::Result<()> {
    gstreamer::init()?;
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_app_id("com.cloudlane.HyperCast")
            .with_icon(eframe::icon_data::from_png_bytes(include_bytes!(
                "../../assets/hypercast.png"
            ))?)
            .with_inner_size([1320.0, 820.0])
            .with_min_inner_size([960.0, 620.0]),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        "HyperCast",
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
    .map_err(|e| anyhow::anyhow!(e.to_string()))
}
