use super::{
    engine::{Command, Engine, Layout, State},
    media::{Codec, Encoder, Source},
};
use crate::{
    room_creation::{Creator, ServerAccess},
    subscription::ViewMode,
};
use eframe::egui::{self, Color32, RichText, Vec2};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
#[cfg(test)]
use std::sync::mpsc;
const BG: Color32 = Color32::from_rgb(14, 18, 19);
const PANEL: Color32 = Color32::from_rgb(21, 27, 28);
const MUTED: Color32 = Color32::from_rgb(154, 167, 161);
const ACCENT: Color32 = Color32::from_rgb(191, 239, 121);
fn button(ui: &mut egui::Ui, label: &str, enabled: bool, primary: bool) -> egui::Response {
    crate::desktop_ui::action(ui,label,enabled,primary)
}
fn small(ui:&mut egui::Ui,label:&str)->egui::Response {crate::desktop_ui::compact(ui,label,true)}
fn input(ui:&mut egui::Ui,value:&mut String,hint:&str,password:bool,limit:usize)->egui::Response {
    crate::desktop_ui::input(ui,value,hint,password,limit,true)
}

struct App {
    address: String,
    name: String,
    sidebar_open: bool,
    invite: String,
    confirm_leave: bool,
    saved_room:Option<String>,
    device: String,
    access: ServerAccess,
    server_status: crate::server_status::ServerStatus,
    creator: Creator,
    catalog:crate::room_creation::Catalog,
    room_title:String,
    engine: Option<Engine>,
    snapshot:State,
    departure:Option<crate::room_transition::Departure>,
    pending_join:bool,
    applied_layout:bool,
    joining: Option<crate::invitation::JoinAttempt>,
    status: String,
    layout: Layout,
    codec: Codec,
    encoder: Encoder,
    upload:crate::bandwidth::UploadProfile,
    rate_mode:crate::adaptive_upload::Mode,
    stream_settings:crate::resolution::StreamSettings,
    preferences:crate::preferences::Store,
    audio: bool,
    fullscreen: bool,
    debug: bool,
    report_copied:Option<Instant>,
    updates:bool,
    textures: HashMap<String, (u64, egui::TextureHandle)>,
    rgba: Vec<u8>,
    started: Instant,
}
impl App {
    fn new(ctx: &egui::Context) -> Self {
        ctx.set_theme(egui::ThemePreference::Dark);
        let mut style = (*ctx.style_of(egui::Theme::Dark)).clone();
        style.visuals.panel_fill = BG;
        style.visuals.window_fill = PANEL;
        style.visuals.selection.bg_fill = ACCENT.gamma_multiply(0.25);
        style.spacing.item_spacing = Vec2::new(12.0, 12.0);
        style.spacing.button_padding = Vec2::new(16.0, 10.0);
        style.animation_time = 0.0;
        ctx.set_style_of(egui::Theme::Dark, style);
        let prefs=crate::preferences::Settings::load();
        let mut app = Self {
            preferences:crate::preferences::Store::new(prefs),
            address: std::env::var("HYPERCAST_URL")
                .unwrap_or("wss://hypercast.cloudlane.com.br".into()),
            name: crate::preferences::load_name(),
            sidebar_open: prefs.sidebar,
            confirm_leave: false,
            invite: std::env::var("HYPERCAST_INVITE").unwrap_or_default(),saved_room:None,
            device: crate::invitation::client_id(),
            access: ServerAccess::default(),
            server_status: crate::server_status::ServerStatus::default(),
            creator: Creator::default(),
            catalog:crate::room_creation::Catalog::default(),room_title:String::new(),
            engine: None,snapshot:State::default(),departure:None,pending_join:false,applied_layout:false,
            joining: None,
            status: String::new(),
            layout: Layout{volume:if prefs.muted{0}else{prefs.volume},mode:if prefs.grid{ViewMode::Grid}else{ViewMode::Spotlight},..Default::default()},
            codec:if prefs.codec==crate::preferences::VideoCodec::Av1{Codec::Av1}else{Codec::Hevc},
            encoder: Encoder::Auto,
            rate_mode:prefs.rate_mode,
            upload:if std::env::var_os("HYPERCAST_SMOKE_SECONDS").is_some(){crate::bandwidth::UploadProfile::initial()}else{prefs.upload},stream_settings:prefs.stream,
            audio: false,
            fullscreen: false,
            debug: false,report_copied:None,
            updates:false,
            textures: HashMap::new(),
            rgba: Vec::new(),
            started: Instant::now(),
        };
        if std::env::var("HYPERCAST_AUTO_CONNECT").as_deref() == Ok("1") {
            if let Ok(token) = std::env::var("HYPERCAST_ROOM_TOKEN") {
                app.engine = Some(Engine::start(
                    app.address.clone(),
                    token,
                    ctx.clone(),
                ));
            }
        }
        app
    }
    fn save_preferences(&mut self){let previous=self.preferences.current();let muted=previous.muted&&self.layout.volume==0;self.preferences.update(crate::preferences::Settings{codec:if self.codec==Codec::Av1{crate::preferences::VideoCodec::Av1}else{crate::preferences::VideoCodec::Hevc},amd_profile:self.preferences.current().amd_profile,stream:self.stream_settings,upload:self.upload,rate_mode:self.rate_mode,volume:if muted{previous.volume}else{self.layout.volume},muted,sidebar:self.sidebar_open,self_preview:self.preferences.current().self_preview,grid:self.layout.mode==ViewMode::Grid});}
    fn send(&self, command: Command) {
        if let Some(engine) = &self.engine {
            let _ = engine.tx.send(command);
        }
    }
    fn restart_reception(&self) {self.send(Command::RestartReception);}
    fn layout(&self) {
        let mut layout = self.layout.clone();
        if self.fullscreen { layout.mode = ViewMode::Spotlight; }
        self.send(Command::Layout(layout));
    }
    fn fullscreen(&mut self, ctx: &egui::Context, enabled: bool) {
        self.fullscreen = enabled;
        self.layout();
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(enabled));
    }
    fn leave_room(&mut self,ctx:&egui::Context,join_next:bool) {
        if let Some(engine)=&self.engine{engine.request_exit();}
        self.creator.cancel_automatic_join();self.creator.open=false;
        self.joining=None;self.textures.clear();self.snapshot=State::default();self.layout.focus=None;self.layout.paused=false;
        self.applied_layout=false;self.pending_join=join_next;
        self.status=if join_next{crate::i18n::label("Trocando de sala…","Switching rooms…")}else{crate::i18n::label("Saindo da sala…","Leaving the room…")}.into();
        if self.departure.is_none(){self.departure=Some(crate::room_transition::Departure::start(self.engine.take(),ctx.clone()));}
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
        let by_room=self.saved_room.is_some();let code=self.saved_room.clone().unwrap_or(invite);
        self.joining=Some(crate::invitation::JoinAttempt::start(address,code,name,device,session,by_room,ctx.clone()));
        self.status=crate::i18n::label("Entrando na sala…","Joining the room…").into();
    }
    fn video(&mut self, ui: &mut egui::Ui, sid: &str, name: &str, card: egui::Rect) {
        let padding = if card.height() < 144.0 || card.width() < 180.0 { 4.0 } else { 8.0 };
        let header_height = 24.0_f32.min(card.height() * 0.4);
        let recovering = self.snapshot.recovering.get(sid).copied();
        let footer_height = if recovering.is_some() && card.height() >= 100.0 { 22.0 } else { padding };
        let canvas = egui::Rect::from_min_max(
            card.min + Vec2::new(padding, header_height),
            card.max - Vec2::new(padding, footer_height),
        );
        if !canvas.is_positive() || !ui.is_rect_visible(card) {
            return;
        }
        let frame = self.snapshot.frames.get(sid).cloned();
        if let Some((sequence, frame, delivered_at)) = frame {
            if self
                .textures
                .get(sid)
                .is_none_or(|entry| entry.0 != sequence)
            {
                let render_started=Instant::now();self.snapshot.quality.evidence.record(crate::evidence::Stage::ReaderToRender,render_started.saturating_duration_since(delivered_at));
                let (w, h) = (frame.buffer.width(), frame.buffer.height());
                self.rgba.resize(w as usize * h as usize * 4, 0);
                frame.buffer.to_argb(
                    livekit::webrtc::video_frame::VideoFormatType::ABGR,
                    &mut self.rgba,
                    w * 4,
                    w as i32,
                    h as i32,
                );
                let image =
                    egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &self.rgba);
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
                self.snapshot.quality.evidence.record(crate::evidence::Stage::RenderPrepare,render_started.elapsed());
            }
        }
        let painter = ui.painter().with_clip_rect(card);
        painter.rect_filled(card, 8.0, PANEL);
        painter.rect_stroke(card, 8.0, egui::Stroke::new(1.0, crate::desktop_ui::BORDER), egui::StrokeKind::Inside);
        let title = egui::Rect::from_min_max(
            card.min + Vec2::new(padding, 0.0),
            egui::pos2(card.right() - padding, canvas.top()),
        );
        crate::desktop_ui::paint_video_label(ui, title, name, 13.0, crate::desktop_ui::TEXT);
        if let Some(exhausted) = recovering.filter(|_| footer_height > padding) {
            let footer = egui::Rect::from_min_max(egui::pos2(title.left(), canvas.bottom()), egui::pos2(title.right(), card.bottom()));
            crate::desktop_ui::paint_video_label(ui, footer, crate::reception_recovery::status_label(exhausted), 12.0, MUTED);
        }
        if let Some((_, texture)) = self.textures.get(sid) {
            let size = texture.size_vec2();
            let scale = (canvas.width() / size.x).min(canvas.height() / size.y);
            painter.image(texture.id(), egui::Rect::from_center_size(canvas.center(), size * scale),
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)), Color32::WHITE);
        } else if canvas.height() >= 20.0 {
            crate::desktop_ui::paint_video_label(ui, canvas, crate::i18n::label("Aguardando vídeo.", "Waiting for video."), 12.0, MUTED);
        }
        let response = ui.interact(card, ui.id().with(sid), egui::Sense::click());
        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true,
            crate::i18n_format!("Transmissão de {}", "Stream from {}", name)));
        if response.double_clicked() || (response.clicked() && response.interact_pointer_pos().is_none()) {
            self.layout.focus = Some(sid.to_owned());
            self.layout.mode=ViewMode::Spotlight;
            self.layout();
        }
    }
}
impl Drop for App {
    fn drop(&mut self) { self.save_preferences(); #[cfg(not(test))] {let _ = crate::preferences::save_name(&self.name);} }
}
impl App {
    fn render(&mut self, root: &mut egui::Ui) {
        self.save_preferences();
        let context = root.ctx().clone();
        let ctx = &context;
        self.access.poll(&self.address, &self.device, ctx.clone());
        self.server_status.poll(&self.address,ctx);
        if self.departure.as_ref().is_some_and(|d|d.ready()) {
            self.departure=None;
            if std::mem::take(&mut self.pending_join){self.connect(ctx);} else { self.status = crate::i18n::label("Você saiu da sala", "You left the room").into(); }
        }
        if let Some(room) = self.creator.poll() {
            self.saved_room=Some(room.room);self.invite.clear();
            self.leave_room(ctx,true);
        }
        if let Some(rx) = &self.joining {
            match rx.receiver.try_recv() {
                Ok(result)=>{
                self.joining = None;
                match result {
                    Ok(joined) => {
                        self.room_title=joined.title.unwrap_or_else(||crate::i18n::label("Sala","Room").into());
                        let token=joined.token;
                        self.engine = Some(Engine::start(self.address.clone(), token, ctx.clone()));
                        self.status.clear();
                        self.textures.clear();
                    }
                    Err(e) => self.status = e.to_string(),
                }
                },
                Err(std::sync::mpsc::TryRecvError::Disconnected)=>{self.joining=None;self.status=crate::i18n::label("Conexão interrompida; tente novamente","Connection interrupted; try again").into();},
                Err(std::sync::mpsc::TryRecvError::Empty)=>{},
            }
        }
        // Media teardown, decoder releases and server callbacks may hold the
        // state briefly. Keep drawing the previous snapshot instead of blocking GL.
        if let Some(engine) = &self.engine {
            if let Ok(state) = engine.state.try_lock() {
                self.snapshot = state.clone();
            } else {
                ctx.request_repaint_after(Duration::from_millis(16));
            }
        }
        let State { connected, sharing, selecting, can_publish, reconnecting, reception_resetting: resetting, .. } = self.snapshot;
        let state_status = self.snapshot.status.clone();
        let screens = self.snapshot.screens.clone();
        self.textures.retain(|sid, _| screens.iter().any(|screen| &screen.sid == sid));
        if connected && !self.applied_layout {self.send(Command::RateMode(self.rate_mode));self.send(Command::StreamSettings(self.stream_settings));self.layout();self.send(Command::Audio(self.audio));self.applied_layout=true;}
        if connected && !resetting && ctx.input(|i|i.key_pressed(egui::Key::F5)){self.restart_reception();}
        if !self.confirm_leave && (self.fullscreen || connected && !screens.is_empty()) && ctx.input(|i| i.key_pressed(egui::Key::F11)) {
            self.fullscreen(ctx, !self.fullscreen);
        }
        if self.fullscreen && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.fullscreen(ctx, false);
        }
        if !self.fullscreen && (connected || reconnecting) {
            crate::desktop_ui::session_panel(ctx.content_rect().width())
                .show(root, |ui| {
                    let status = if connected && !can_publish {
                        crate::i18n::label("Nesta sala, você pode assistir.", "You can watch in this room.")
                    } else if self.status.is_empty() { &state_status } else { &self.status };
                    crate::desktop_ui::session_status(ui, sharing, self.snapshot.camera_active, status);
                    use crate::desktop_ui::{session_actions, session_button, session_menu, session_stop};
                    let ready = connected && can_publish && !selecting
                        && super::media::factory_available(self.codec, self.encoder).unwrap_or(true);
                    let share = |app: &Self, source| {
                        app.send(Command::StreamSettings(app.stream_settings));
                        app.send(Command::Share(source, app.codec, app.encoder, app.upload));
                    };
                    session_actions(ui, |ui| {
                        if selecting || sharing {
                            let label = if selecting { crate::i18n::label("Cancelar seleção", "Cancel selection") } else { crate::i18n::label("Parar compartilhamento", "Stop screen sharing") };
                            if session_stop(ui, label, true).clicked() {
                                self.send(if selecting { Command::CancelSelection } else { Command::Stop });
                            }
                        } else if session_button(ui, crate::i18n::label("Compartilhar tela", "Share screen"), ready, true).clicked() {
                            share(self, Source::Portal);
                        }
                        let camera = if self.snapshot.camera_starting {
                            crate::i18n::label("Abrindo câmera…", "Opening camera…")
                        } else if self.snapshot.camera_active {
                            crate::i18n::label("Desligar câmera", "Turn off camera")
                        } else {
                            crate::i18n::label("Ligar câmera", "Turn on camera")
                        };
                        if session_button(ui, camera, connected && can_publish && !self.snapshot.camera_starting, false).clicked() {
                            self.send(Command::ToggleCamera);
                        }
                        session_menu(ui, |ui| {
                            if sharing && ui.add_enabled(ready, egui::Button::new(crate::i18n::label("Trocar fonte", "Change source"))).clicked() {
                                share(self, Source::Portal); ui.close();
                            }
                            if !sharing && std::env::var_os("WAYLAND_DISPLAY").is_none()
                                && ui.add_enabled(ready, egui::Button::new(crate::i18n::label("Tela inteira (X11)", "Entire screen (X11)"))).clicked() {
                                share(self, Source::X11); ui.close();
                            }
                            if ui.add_enabled(can_publish, egui::Checkbox::new(&mut self.audio, crate::i18n::label("Áudio do sistema", "System audio"))).changed() {
                                self.send(Command::Audio(self.audio));
                            }
                            ui.separator();
                            let label = ui.label(crate::i18n::label("Volume recebido", "Incoming volume")).id;
                            if crate::desktop_ui::volume_slider(ui, &mut self.layout.volume).labelled_by(label).changed() { self.layout(); }
                            let pause = if self.layout.paused { crate::i18n::label("Retomar vídeo recebido", "Resume incoming video") } else { crate::i18n::label("Pausar vídeo recebido", "Pause incoming video") };
                            if ui.button(pause).clicked() { self.layout.paused = !self.layout.paused; self.layout(); ui.close(); }
                            if ui.add_enabled(connected && !resetting, egui::Button::new(crate::i18n::label("Reconectar vídeo recebido", "Reconnect incoming video"))).clicked() { self.restart_reception(); ui.close(); }
                            if crate::desktop_ui::compact_room_header(ctx.content_rect().width(), connected && self.sidebar_open) {
                                if !self.room_title.is_empty() { crate::desktop_ui::copy_room_name(ui, &self.room_title); }
                                if ui.add_enabled(!screens.is_empty(), egui::Button::new(crate::i18n::label("Tela cheia", "Full screen"))).clicked() {
                                    self.fullscreen(ctx, true); ui.close();
                                }
                            }
                            crate::desktop_ui::display_scale(ui);
                            if ui.button(crate::i18n::label("Diagnóstico", "Diagnostics")).clicked() { self.debug = true; ui.close(); }
                        });
                        if session_stop(ui, crate::i18n::label("Sair da sala", "Leave room"), self.engine.is_some()).clicked() {
                            if sharing || self.snapshot.camera_active { self.confirm_leave = true; } else { self.leave_room(ctx, false); }
                        }
                    });
                });
        }
        if connected && !self.fullscreen && self.sidebar_open {
            let width = ctx.content_rect().width();
            egui::Panel::left("sidebar").exact_size(crate::desktop_ui::sidebar_width(width))
                .resizable(false).frame(egui::Frame::new().fill(PANEL).inner_margin(crate::desktop_ui::room_margin(width, false))).show(root,|ui|{
                ui.style_mut().wrap_mode=Some(egui::TextWrapMode::Wrap);
                egui::ScrollArea::vertical().auto_shrink([false,false]).show(ui,|ui|{
                    ui.horizontal_wrapped(|ui|{crate::desktop_ui::brand_mark(ui,22.0);ui.label(RichText::new("HyperCast").strong());
                        if small(ui,crate::i18n::label("Atualizar","Update")).clicked(){self.updates=true;}
                    });
                    crate::desktop_ui::server_indicator(ui, &self.server_status);
                    if reconnecting {ui.label(RichText::new(crate::i18n::label("Sala: reconectando…","Room: reconnecting…")).size(11.0).color(MUTED));}
                    ui.add_space(12.0);
                    {
                        ui.label(RichText::new(crate::i18n::label("PARTICIPANTES","PARTICIPANTS")).size(11.0).color(MUTED));
                        ui.label(crate::i18n_format!("{} (você) · {}","{} (you) · {}",if self.name.is_empty(){"Linux"}else{&self.name},if sharing{crate::i18n::label("Transmitindo","Sharing")}else{crate::i18n::label("Assistindo","Watching")}));
                        for (id, name) in &self.snapshot.participants {
                            let live=screens.iter().any(|s| &s.owner==id);let title=if name.is_empty(){id}else{name};
                            let focused=!self.layout.paused && self.layout.mode==ViewMode::Spotlight && (self.layout.focus.as_ref().is_some_and(|focus| focus == id || screens.iter().any(|s| &s.sid == focus && &s.owner == id)) || (self.layout.focus.is_none() && screens.first().is_some_and(|s|&s.owner==id)));
                            let response=button(ui,&format!("{title} · {}",if live {if focused {crate::i18n::label("Em destaque · transmitindo","Focused · sharing")}else{crate::i18n::label("Transmitindo","Sharing")}}else{crate::i18n::label("Assistindo","Watching")}),live,focused);
                            if response.clicked(){self.layout.focus=Some(id.clone());if response.double_clicked(){self.layout.mode=ViewMode::Spotlight;}self.layout();}
                        }
                        if small(ui,crate::i18n::label("Escolher sala","Choose room")).clicked(){self.catalog.open(self.address.clone(),self.access.token(),ctx.clone());}
                    }
                    ui.separator();ui.label(RichText::new(crate::i18n::label("COMPARTILHAR","SHARE")).size(11.0).color(MUTED));
                    egui::CollapsingHeader::new(crate::i18n::label("Qualidade do vídeo","Video quality")).id_salt("linux_video_format").show(ui,|ui|{
                    if self.encoder==Encoder::Software{self.stream_settings.fps=30;}
                    let before=self.stream_settings;
                    crate::desktop_ui::stream_settings(ui,&mut self.stream_settings,!sharing&&!selecting,if self.encoder==Encoder::Software{30}else{120});
                    if before!=self.stream_settings{self.send(Command::StreamSettings(self.stream_settings));}
                    crate::desktop_ui::upload_selector(ui,&mut self.upload,!sharing&&!selecting);
                    if crate::desktop_ui::rate_mode_selector(ui,&mut self.rate_mode,!sharing&&!selecting,true){self.send(Command::RateMode(self.rate_mode));}
                    ui.add_enabled_ui(!sharing&&!selecting,|ui|{
                        ui.columns(2,|columns|{
                            if crate::desktop_ui::action(&mut columns[0],"HEVC / H.265",super::media::factory_available(Codec::Hevc,self.encoder).unwrap_or(true),self.codec==Codec::Hevc).clicked(){self.codec=Codec::Hevc;}
                            if crate::desktop_ui::action(&mut columns[1],"AV1",super::media::factory_available(Codec::Av1,self.encoder).unwrap_or(true),self.codec==Codec::Av1).clicked(){self.codec=Codec::Av1;}
                        });
                        egui::ComboBox::from_id_salt("encoder").width(ui.available_width()).selected_text(if self.encoder==Encoder::Auto{"GPU · NVENC / VA-API"}else{crate::i18n::label("Compatível · HEVC 30 FPS","Compatible · HEVC 30 FPS")}).show_ui(ui,|ui|{
                            ui.selectable_value(&mut self.encoder,Encoder::Auto,"GPU · NVENC / VA-API");ui.selectable_value(&mut self.encoder,Encoder::Software,crate::i18n::label("Compatível · HEVC 30 FPS","Compatible · HEVC 30 FPS"));
                        });
                        if self.encoder==Encoder::Software{self.codec=Codec::Hevc;}
                        if super::media::factory_available(self.codec,self.encoder)==Some(false){ui.add(egui::Label::new(RichText::new(crate::i18n::label("Nenhum encoder disponível para este formato. Escolha outro formato ou instale o plugin/driver.","No encoder is available for this format. Choose another format or install the plugin/driver.")).color(MUTED).size(11.0)).wrap());}
                    });
                    });
                    ui.collapsing(crate::i18n::label("Indicadores de vídeo", "Video statistics"), |ui| {
                        crate::desktop_ui::quality_strip(ui, &self.snapshot.quality, reconnecting, sharing, self.layout.paused);
                    });
                    if super::media::factory_available(self.codec,self.encoder)==Some(false){ui.add(egui::Label::new(RichText::new(crate::i18n::label("Encoder indisponível: escolha outro formato em Formato de vídeo.","Encoder unavailable: choose another format in Video format.")).color(MUTED).size(11.0)).wrap());}
                });
            });
        }
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(BG)
                    .inner_margin(crate::desktop_ui::room_margin(ctx.content_rect().width(), self.fullscreen)),
            )
            .show(root, |ui| {
                if !connected && !reconnecting && !self.fullscreen {
                    let status = if self.status.is_empty() { &state_status } else { &self.status }.clone();
                    crate::desktop_ui::home_panel(ui, &status, |ui| {
                    crate::desktop_ui::server_indicator(ui, &self.server_status);
                    if !connected && self.departure.is_none() && self.joining.is_none() && self.engine.as_ref().is_some_and(|e|e.finished()) && (self.saved_room.is_some() || !self.invite.is_empty()) {
                        if button(ui,crate::i18n::label("Tentar novamente","Try again"),true,true).clicked(){self.leave_room(ctx,true);}
                    }
                    if !self.access.authenticated(){
                        ui.label(RichText::new(crate::i18n::label("ACESSO AO SERVIDOR","SERVER ACCESS")).size(11.0).color(MUTED));
                        ui.collapsing(crate::i18n::label("Servidor","Server"),|ui|{input(ui,&mut self.address,"wss://servidor",false,512);});
                        let key_label = ui.label(crate::i18n::label("Chave de acesso", "Access key")).id;
                        input(ui, &mut self.access.code, "", true, 64).labelled_by(key_label);
                        if button(ui,crate::i18n::label("Continuar com a chave","Continue with access key"),!self.access.busy,true).clicked(){self.access.start(self.address.clone(),self.device.clone(),ctx.clone());}
                        ui.label(RichText::new(&self.access.status).size(12.0).color(MUTED));
                    }else{
                        if input(ui,&mut self.name,crate::i18n::label("Seu nome","Your name"),false,32).lost_focus(){let _=crate::preferences::save_name(&self.name);}
                        if button(ui,crate::i18n::label("Entrar em uma sala","Join a room"),self.departure.is_none()&&self.joining.is_none()&&!self.creator.busy,true).clicked(){self.creator.open=false;self.catalog.open(self.address.clone(),self.access.token(),ctx.clone());}
                        if button(ui,crate::i18n::label("Criar sala","Create room"),self.departure.is_none()&&self.joining.is_none()&&!self.creator.busy,false).clicked(){self.creator.open(&self.address);}
                        ui.collapsing(crate::i18n::label("Usar um convite", "Use an invitation"), |ui| {
                            let label = ui.label(crate::i18n::label("Código do convite", "Invitation code")).id;
                            input(ui, &mut self.invite, "", false, 256).labelled_by(label);
                            if button(ui, crate::i18n::label("Entrar com convite", "Join with invitation"),
                                !self.invite.trim().is_empty() && self.departure.is_none() && self.joining.is_none(), true).clicked() {
                                self.saved_room = None;
                                self.leave_room(ctx, true);
                            }
                        });
                        if small(ui,crate::i18n::label("Trocar acesso ao servidor","Change server access")).clicked(){self.leave_room(ctx,false);self.access.logout(&self.address);}
                    }

                        crate::desktop_ui::session_menu(ui, |ui| {
                            crate::desktop_ui::display_scale(ui);
                            if ui.button(crate::i18n::label("Atualizar", "Update")).clicked() { self.updates = true; ui.close(); }
                            if ui.button(crate::i18n::label("Diagnóstico", "Diagnostics")).clicked() { self.debug = true; ui.close(); }
                        });
                    });
                    return;
                }

                if self.fullscreen {
                    ui.horizontal_wrapped(|ui| {
                        if crate::desktop_ui::exit_fullscreen(ui).clicked() { self.fullscreen(ctx, false); }
                        ui.menu_button(crate::i18n::label("Som recebido", "Received audio"), |ui| {
                            let label = ui.label(crate::i18n::label("Volume recebido", "Incoming volume")).id;
                            if crate::desktop_ui::volume_slider(ui, &mut self.layout.volume).labelled_by(label).changed() { self.layout(); }
                            if ui.button(crate::i18n::label("Silenciar", "Mute")).clicked() {
                                self.layout.volume = 0;
                                self.layout();
                            }
                        });
                    });
                } else {
                    let title = if self.room_title.is_empty() { crate::i18n::label("Sala", "Room").to_owned() } else { self.room_title.clone() };
                    let status = if resetting {
                        crate::i18n::label("Reconectando vídeo recebido…", "Reconnecting incoming video…")
                    } else {
                        crate::desktop_ui::room_state(connected, reconnecting)
                    };
                    crate::desktop_ui::room_header(ui, &title, status, |ui, compact| {
                        let (panel, modes) = crate::desktop_ui::room_view_controls(ui, self.sidebar_open, self.layout.mode);
                        if panel.clicked() { self.sidebar_open = !self.sidebar_open; }
                        for (mode, response) in modes {
                            if response.clicked() {
                                self.layout.mode = mode;
                                self.layout();
                            }
                        }
                        if !compact && !self.room_title.is_empty() { crate::desktop_ui::copy_room_name(ui, &title); }
                        if !compact && crate::desktop_ui::header_button(ui, crate::i18n::label("Tela cheia", "Full screen"), !screens.is_empty(), None).on_hover_text("F11").clicked() {
                            self.fullscreen(ctx, true);
                        }
                    });
                }
                if self.layout.paused || screens.is_empty() {
                    let area = ui.allocate_space(ui.available_size()).1;
                    ui.painter().rect_filled(area, 12.0, PANEL);
                    if crate::desktop_ui::playback_empty(ui, area, self.layout.paused, reconnecting, resetting, can_publish) {
                        self.layout.paused = false;
                        self.layout();
                    }
                } else if self.layout.mode == ViewMode::Grid && !self.fullscreen {
                    let area = ui.allocate_space(ui.available_size()).1;
                    for (index, screen) in screens.iter().enumerate() {
                        let card = crate::desktop_ui::video_grid_card(area, index, screens.len(), 10.0);
                        self.video(ui, &screen.sid, &screen.name, card);
                    }
                } else {
                    let focus = self
                        .layout
                        .focus
                        .as_deref()
                        .and_then(|id| screens.iter().find(|s| s.sid == id || s.owner == id))
                        .unwrap_or(&screens[0]);
                    let area = ui.allocate_space(ui.available_size()).1;
                    self.video(ui, &focus.sid, &focus.name, area);
                }
            });
        if self.confirm_leave {
            match crate::desktop_ui::confirm_leave(ctx) {
                Some(true) => { self.confirm_leave = false; self.leave_room(ctx, false); }
                Some(false) => self.confirm_leave = false,
                None => {}
            }
        }
        if self.creator.open {
            let mut open = true;
            crate::desktop_ui::utility_window(ctx, crate::i18n::label("Criar sala", "Create room"), "create_room_window", 360.0)
                .open(&mut open)
                .show(ctx, |ui| {
                    crate::desktop_ui::modal_header(ui,crate::i18n::label("Criar sala","Create room"));
                    ui.style_mut().wrap_mode=Some(egui::TextWrapMode::Wrap);
                    input(ui, &mut self.creator.title, crate::i18n::label("Nome da sala","Room name"), false, 64);
                    ui.checkbox(&mut self.creator.persistent,crate::i18n::label("Manter sala indefinidamente","Keep room indefinitely"));
                    if button(ui, crate::i18n::label("Criar sala","Create room"), !self.creator.busy, true).clicked() {
                        self.creator
                            .start(self.address.clone(), self.access.token(), ctx.clone());
                    }
                    ui.label(&self.creator.status);
                });
            self.creator.open = open;if !open{self.creator.cancel_automatic_join();}
        }
        if !self.fullscreen {
            if let Some(selected)=self.catalog.show(ctx,&self.address,self.access.token(),&mut self.name,if connected{self.saved_room.as_deref()}else{None},false) {
                self.saved_room=Some(selected.room);self.invite.clear();self.leave_room(ctx,true);
            }
        }
        if self.debug {
            let mut reset_preferences=false;
            crate::desktop_ui::utility_window(ctx, crate::i18n::label("Diagnóstico", "Diagnostics"), "debug_window", 360.0)
                .open(&mut self.debug).show(ctx,|ui|{
                crate::desktop_ui::modal_header(ui,crate::i18n::label("Diagnóstico","Diagnostics"));
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                ui.label(format!("HyperCast {} · Linux",env!("CARGO_PKG_VERSION")));
                ui.label(crate::i18n_format!("Decodificadores ativos: {}","Active decoders: {}",super::media::decoder_count()));
                ui.label(crate::i18n_format!("Quadros enviados: {} · recebidos: {}","Frames sent: {} · received: {}",self.snapshot.sent,self.snapshot.received));
                crate::desktop_ui::network_summary(ui,&self.snapshot.quality);
                let idle=self.engine.as_ref().is_none_or(|e|e.finished())&&self.joining.is_none()&&self.departure.is_none()&&!self.pending_join&&!self.creator.busy;
                reset_preferences=crate::desktop_ui::preferences_controls(ui,&mut self.preferences,idle);

                if crate::desktop_ui::action(ui,if self.report_copied.is_some_and(|at|at.elapsed()<Duration::from_secs(2)){crate::i18n::label("Relatório copiado","Report copied")}else{crate::i18n::label("Copiar relatório técnico","Copy technical report")},true,false).clicked(){
                    let s=&self.snapshot;ctx.copy_text(crate::video_metrics::technical_report(&s.quality,crate::video_metrics::ReportState{connected:s.connected,reconnecting:s.reconnecting,publishing:s.sharing,sent:s.sent,received:s.received,restarts:s.reception_restarts},self.stream_settings,self.upload));self.report_copied=Some(Instant::now());ctx.request_repaint_after(Duration::from_secs(2));
                }
                ui.label(crate::i18n::label("A chave de acesso fica somente na memória desta sessão.","The access key is kept only in memory for this session."));
                ui.label(crate::i18n::label("Renderização portátil com transferência para RAM; desempenho QHD 120 FPS ainda requer validação.","Portable rendering with RAM transfer; QHD 120 FPS performance still requires validation."));
            });
            if reset_preferences {let p=crate::preferences::Settings::default();self.stream_settings=p.stream;self.upload=p.upload;self.rate_mode=p.rate_mode;self.sidebar_open=p.sidebar;self.layout.volume=p.volume;self.layout.mode=ViewMode::Spotlight;self.layout.focus=None;self.layout.paused=false;self.codec=Codec::Hevc;}
        }
        if self.updates {
            crate::desktop_ui::utility_window(ctx, crate::i18n::label("Atualização", "Update"), "update_window", 320.0)
                .open(&mut self.updates).show(ctx,|ui|{
                    crate::desktop_ui::modal_header(ui,crate::i18n::label("Atualização","Update"));
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                    ui.label(crate::i18n_format!("Versão atual: {}","Current version: {}",env!("CARGO_PKG_VERSION")));
                    ui.label(RichText::new(crate::i18n::label("Linux · AppImage portátil","Linux · portable AppImage")).size(11.0).color(MUTED));
                    ui.label(crate::i18n::label("Confira a versão disponível na página de downloads. Para atualizar, substitua o AppImage.","Check the downloads page for the latest version. Replace the AppImage to update."));
                    if crate::desktop_ui::action(ui,crate::i18n::label("Abrir página de downloads","Open downloads page"),true,true).clicked(){ctx.open_url(egui::OpenUrl::new_tab("https://github.com/LucasCamy/HyperCast/releases"));}
                });
        }
        if let Ok(seconds) = std::env::var("HYPERCAST_SMOKE_SECONDS")
            .unwrap_or_default()
            .parse::<u64>()
        {
            if (1..=300).contains(&seconds) && self.started.elapsed() > Duration::from_secs(seconds)
            {
                if self.engine.is_some() {
                    let s = &self.snapshot;
                    println!(
                        "Linux GUI smoke: {} received, {} textures, connected={}, audio={}",
                        s.received,
                        self.textures.len(),
                        s.connected,
                        s.audio_received
                    );
                    println!("Linux throughput: sent_fps={:.1} sent_mbps={:.3} received_fps={:.1}",s.quality.send.map_or(0.0,|r|r.fps),s.quality.send.map_or(0.0,|r|r.mbps),s.quality.receive.map_or(0.0,|r|r.fps));
                    println!("GUI technical report: {}",crate::video_metrics::technical_report(&s.quality,crate::video_metrics::ReportState{connected:s.connected,reconnecting:s.reconnecting,publishing:s.sharing,sent:s.sent,received:s.received,restarts:s.reception_restarts},self.stream_settings,self.upload).replace('\n',""));
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        let busy = self.access.busy || self.creator.busy || self.departure.is_some() || self.joining.is_some();
        ctx.request_repaint_after(Duration::from_millis(if busy { 100 } else { 500 }));
    }
}
impl eframe::App for App {
    fn ui(&mut self,root:&mut egui::Ui,_:&mut eframe::Frame){self.render(root);}
}
pub fn run() -> anyhow::Result<()> {
    eprintln!("HyperCast Linux: version={} pid={}",env!("CARGO_PKG_VERSION"),std::process::id());
    gstreamer::init()?;
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_app_id("com.cloudlane.HyperCast")
            .with_icon(eframe::icon_data::from_png_bytes(include_bytes!(
                "../../assets/hypercast.png"
            ))?)
            .with_inner_size([1320.0, 820.0])
            .with_min_inner_size([460.0, 360.0]),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        "HyperCast",
        options,
        Box::new(|cc| Ok(Box::new(App::new(&cc.egui_ctx)))),
    )
    .map_err(|e| anyhow::anyhow!(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read,Write};
    fn draw(app:&mut App,ctx:&egui::Context,step:usize) {
        let output=ctx.run_ui(egui::RawInput{
            screen_rect:Some(egui::Rect::from_min_size(Default::default(),egui::vec2(960.0,620.0))),
            time:Some(step as f64*0.1),..Default::default()
        },|ui|app.render(ui));
        let painted = |needle: &str| output.shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text().contains(needle))
        });
        if app.snapshot.connected {
            assert!(painted("Sair da sala"), "Session controls must remain painted");
        } else {
            assert!(painted("HyperCast"), "Home must remain painted");
            assert!(painted("Chave") || painted("Entrar em uma sala"), "Room access must remain painted");
        }
    }
    #[test]
    #[ignore = "Requires an isolated server access key and two authorized test rooms"]
    fn room_switch_with_server() {
        gstreamer::init().unwrap();
        let ctx=egui::Context::default();let mut app=App::new(&ctx);
        app.address=std::env::var("HYPERCAST_CATALOG_TEST_URL").unwrap();
        app.device=uuid::Uuid::new_v4().simple().to_string();app.name="LinuxSwitchTester".into();
        app.access.poll(&app.address,&app.device,ctx.clone());
        app.access.code=std::env::var("HYPERCAST_CATALOG_TEST_KEY").unwrap();
        app.access.start(app.address.clone(),app.device.clone(),ctx.clone());
        let mut step=0;
        fn until(app:&mut App,ctx:&egui::Context,step:&mut usize,ready:impl Fn(&App)->bool) {
            let deadline=Instant::now()+Duration::from_secs(35);
            while !ready(app) {assert!(Instant::now()<deadline,"Linux room transition timed out");draw(app,ctx,*step);*step+=1;std::thread::sleep(Duration::from_millis(10));}
        }
        until(&mut app,&ctx,&mut step,|a|!a.access.busy);
        assert!(app.access.authenticated(),"Isolated test key was rejected");
        let first=std::env::var("HYPERCAST_CATALOG_TEST_FIRST").unwrap();
        let second=std::env::var("HYPERCAST_CATALOG_TEST_SECOND").unwrap();
        for (room,title) in [(&first,"Linux primeira"),(&second,"Linux segunda"),(&first,"Linux primeira")] {
            app.saved_room=Some(room.clone());app.leave_room(&ctx,true);
            assert!(app.engine.is_none());assert!(!app.snapshot.connected);
            until(&mut app,&ctx,&mut step,|a|a.snapshot.connected);
            assert_eq!(app.room_title,title);
        }
        app.leave_room(&ctx,false);
        until(&mut app,&ctx,&mut step,|a|a.departure.is_none());
        assert!(app.engine.is_none());assert!(!app.snapshot.connected);
        println!("PASS: Linux UI switched rooms twice and left without retaining the previous engine");
    }
    #[test]
    fn occupied_media_state_does_not_block_the_ui() {
        let ctx=egui::Context::default();let mut app=App::new(&ctx);
        app.address="ws://127.0.0.1:9".into();
        let shared=std::sync::Arc::new(crate::resilience::StateMutex::new(State::default()));
        app.engine=Some(Engine::fixture(shared.clone()));
        let(held,started)=mpsc::channel();let(release,wait)=mpsc::channel();
        let worker=std::thread::spawn(move||{let _guard=shared.lock().unwrap();held.send(()).unwrap();let _=wait.recv_timeout(Duration::from_secs(2));});
        started.recv_timeout(Duration::from_secs(2)).unwrap();
        let before=Instant::now();draw(&mut app,&ctx,0);
        let elapsed=before.elapsed();release.send(()).unwrap();worker.join().unwrap();
        assert!(elapsed<Duration::from_millis(500),"Media lock blocked the UI: {elapsed:?}");
    }
    #[test]
    fn exit_wins_over_queued_room_changes(){
        let ctx=egui::Context::default();let mut app=App::new(&ctx);
        struct SlowDrop(mpsc::Receiver<()>);impl Drop for SlowDrop{fn drop(&mut self){let _=self.0.recv();}}
        let (release,wait)=mpsc::channel();app.departure=Some(crate::room_transition::Departure::start(Some(SlowDrop(wait)),ctx.clone()));
        app.pending_join=true;let started=Instant::now();
        app.leave_room(&ctx,true);app.leave_room(&ctx,true);app.leave_room(&ctx,false);
        assert!(started.elapsed()<Duration::from_millis(100));assert!(!app.pending_join);assert!(!app.departure.as_ref().unwrap().ready());
        release.send(()).unwrap();let until=Instant::now()+Duration::from_secs(2);
        while !app.departure.as_ref().unwrap().ready(){assert!(Instant::now()<until);std::thread::yield_now();}
    }
    #[test]
    fn delayed_server_rejection_keeps_painting_and_displays_the_error() {
        let listener=std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address=format!("ws://{}",listener.local_addr().unwrap());
        let(release,wait)=mpsc::channel();let(accepted,started)=mpsc::channel();
        let server=std::thread::spawn(move||{
            let(mut socket,_)=listener.accept().unwrap();socket.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
            let mut data=[0;4096];let _=socket.read(&mut data);accepted.send(()).unwrap();
            wait.recv_timeout(Duration::from_secs(5)).unwrap();
            socket.write_all(b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
        });
        let ctx=egui::Context::default();let mut app=App::new(&ctx);
        app.address=address;app.access.poll(&app.address,&app.device,ctx.clone());
        app.access.code="A".repeat(32);app.access.start(app.address.clone(),app.device.clone(),ctx.clone());
        started.recv_timeout(Duration::from_secs(3)).unwrap();
        for step in 0..5 {draw(&mut app,&ctx,step);assert!(app.access.busy);}
        release.send(()).unwrap();server.join().unwrap();
        let until=Instant::now()+Duration::from_secs(3);let mut step=5;
        while app.access.busy {assert!(Instant::now()<until);draw(&mut app,&ctx,step);step+=1;std::thread::sleep(Duration::from_millis(5));}
        assert_eq!(app.access.status,"Chave inválida, expirada ou revogada");
        draw(&mut app,&ctx,step);
        app.creator.open=true;app.creator.status="Erro de retorno do servidor ".repeat(40);
        app.status="engine: connection error: wait_pc_connection timed out".into();
        for frame in step+1..step+6 {draw(&mut app,&ctx,frame);}
    }
}
