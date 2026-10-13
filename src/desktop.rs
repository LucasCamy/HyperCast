use crate::{
    desktop_engine::{Command, Engine, Layout},
    presenter::Presenter,
    subscription::ViewMode,
};
use eframe::egui::{self, Color32, RichText, Vec2};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

const BG: Color32 = Color32::from_rgb(14, 18, 19);
const PANEL: Color32 = Color32::from_rgb(21, 27, 28);
const BORDER: Color32 = Color32::from_rgb(42, 50, 50);
const TEXT: Color32 = Color32::from_rgb(234, 239, 234);
const MUTED: Color32 = Color32::from_rgb(154, 167, 161);
const ACCENT: Color32 = Color32::from_rgb(191, 239, 121);
const SURFACE: Color32 = Color32::from_rgb(28, 35, 36);
const DANGER: Color32 = Color32::from_rgb(235, 127, 119);

fn monitor_symbol(painter: &egui::Painter, center: egui::Pos2, color: Color32, size: f32) {
    let stroke = egui::Stroke::new(1.6, color);
    let screen = egui::Rect::from_center_size(center - Vec2::new(0.0, size * 0.08), Vec2::new(size, size * 0.62));
    painter.rect_stroke(screen, 3.0, stroke, egui::StrokeKind::Inside);
    painter.line_segment([center + Vec2::new(0.0, size * 0.24), center + Vec2::new(0.0, size * 0.43)], stroke);
    painter.line_segment([center + Vec2::new(-size * 0.25, size * 0.43), center + Vec2::new(size * 0.25, size * 0.43)], stroke);
}

/// Native button keeps keyboard focus/activation and accessibility semantics;
/// painter supplies the visual hierarchy without image assets or CPU capture.
fn choice_card(ui: &mut egui::Ui, title: &str, subtitle: &str, selected: bool, enabled: bool, avatar: Option<char>) -> egui::Response {
    let response = ui.add_enabled(enabled, egui::Button::new("")
        .min_size(Vec2::new(ui.available_width(), 66.0)).corner_radius(9)
        .fill(if selected { Color32::from_rgb(40, 51, 40) } else { SURFACE })
        .stroke(egui::Stroke::new(1.0, if selected { ACCENT.gamma_multiply(0.65) } else { BORDER })));
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, format!("{title}: {subtitle}")));
    let center = response.rect.left_center() + Vec2::new(30.0, 0.0);
    let tint = if selected { ACCENT } else { MUTED };
    if let Some(initial) = avatar {
        ui.painter().circle_filled(center, 18.0, tint.gamma_multiply(0.14));
        ui.painter().text(center, egui::Align2::CENTER_CENTER, initial.to_uppercase().collect::<String>(), egui::FontId::proportional(17.0), tint);
    } else { monitor_symbol(ui.painter(), center, tint, 27.0); }
    let painter = ui.painter().with_clip_rect(response.rect.shrink(8.0));
    painter.text(response.rect.left_top() + Vec2::new(60.0, 15.0), egui::Align2::LEFT_TOP,
        title, egui::FontId::proportional(14.0), TEXT);
    painter.text(response.rect.left_top() + Vec2::new(60.0, 37.0), egui::Align2::LEFT_TOP,
        subtitle, egui::FontId::proportional(11.0), if selected { ACCENT } else { MUTED });
    if response.has_focus() { painter.rect_stroke(response.rect.shrink(2.0), 8.0, egui::Stroke::new(1.5, ACCENT), egui::StrokeKind::Inside); }
    response.on_hover_text(format!("{title}\n{subtitle}"))
}

fn section_title(ui: &mut egui::Ui, label: &str, detail: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).size(11.0).strong().color(MUTED));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| { ui.label(RichText::new(detail).size(11.0).color(MUTED)); });
    });
}

struct App {
    engine: Option<Engine>,
    address: String,
    token: String,
    invite: String,
    name: String,
    client_id: String,
    token_mode: bool,
    mode: ViewMode,
    focus: Option<String>,
    paused: bool,
    system_audio: bool,
    volume: u32,
    sound_muted: bool,
    displays: Vec<crate::screen_capture::Display>,
    selected_display: usize,
    window_mode:bool,
    windows:Vec<crate::video_capture::ApplicationWindow>,
    selected_window:Option<usize>,
    fullscreen:bool,
    before_fullscreen_mode:ViewMode,
    codec: crate::encoder::Codec,
    parent: usize,
    presenters: HashMap<String, Presenter>,
    error: String,
    auto_connect: bool,
    relay_only: bool,
    started: Instant,
    smoke_seconds: Option<u64>,
    rendered: u64,
    last_presented: HashMap<String, (Arc<crate::desktop_engine::VideoFrame>, [i32; 4])>,
    smoke_reconnected: bool,
    smoke_layout_stage: u8,
    smoke_share_stage: u8,
    smoke_sound_stage: u8,
    smoke_source_stage:u8,
}
impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_theme(egui::ThemePreference::Dark);
        let mut style = (*cc.egui_ctx.style_of(egui::Theme::Dark)).clone();
        style.visuals = egui::Visuals::dark();
        style.visuals.panel_fill = BG;
        style.visuals.window_fill = PANEL;
        style.visuals.override_text_color = Some(TEXT);
        style.visuals.selection.bg_fill = ACCENT.gamma_multiply(0.25);
        style.visuals.widgets.inactive.bg_fill = PANEL;
        style.visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, BORDER);
        style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(39, 48, 44);
        style.visuals.widgets.active.bg_fill = ACCENT.gamma_multiply(0.4);
        style.spacing.item_spacing = Vec2::new(12.0, 12.0);
        style.spacing.button_padding = Vec2::new(16.0, 12.0);
        style.animation_time = 0.0; // High-frequency controls and reduced motion.
        style
            .text_styles
            .insert(egui::TextStyle::Body, egui::FontId::proportional(15.0));
        style
            .text_styles
            .insert(egui::TextStyle::Button, egui::FontId::proportional(14.0));
        cc.egui_ctx.set_style_of(egui::Theme::Dark, style);
        let parent = match cc.window_handle().ok().map(|handle| handle.as_raw()) {
            Some(RawWindowHandle::Win32(window)) => window.hwnd.get() as usize,
            _ => 0,
        };
        Self {
            engine: None,
            address: std::env::var("HYPERCAST_URL")
                .unwrap_or_else(|_| "wss://hypercast.cloudlane.com.br".into()),
            token: std::env::var("HYPERCAST_ROOM_TOKEN").unwrap_or_default(),
            invite: std::env::var("HYPERCAST_INVITE").unwrap_or_default(),
            name: std::env::var("HYPERCAST_NAME").unwrap_or_default(),
            client_id: crate::invitation::client_id(),
            token_mode: std::env::var("HYPERCAST_ROOM_TOKEN").is_ok(),
            mode: if std::env::var("HYPERCAST_VIEW_MODE").as_deref() == Ok("grid") {
                ViewMode::Grid
            } else {
                ViewMode::Spotlight
            },
            focus: std::env::var("HYPERCAST_FOCUS").ok(),
            paused: false,
            system_audio: std::env::var("HYPERCAST_SYSTEM_AUDIO").as_deref() == Ok("1"),
            volume: 80,
            sound_muted: false,
            displays: crate::screen_capture::displays().unwrap_or_default(),
            selected_display: 0,
            window_mode:false,windows:crate::video_capture::windows(),selected_window:None,fullscreen:false,before_fullscreen_mode:ViewMode::Spotlight,
            codec: if std::env::var("HYPERCAST_CODEC").as_deref() == Ok("av1") {
                crate::encoder::Codec::Av1
            } else {
                crate::encoder::Codec::H265
            },
            parent,
            presenters: HashMap::new(),
            error: String::new(),
            auto_connect: std::env::var("HYPERCAST_AUTO_CONNECT").as_deref() == Ok("1"),
            relay_only: std::env::var("HYPERCAST_RELAY_ONLY").as_deref() == Ok("1"),
            started: Instant::now(),
            smoke_seconds: std::env::var("HYPERCAST_SMOKE_SECONDS")
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
                .filter(|v| (1..=300).contains(v)),
            rendered: 0,
            last_presented: HashMap::new(),
            smoke_reconnected: false,
            smoke_layout_stage: 0,
            smoke_share_stage: 0,
            smoke_sound_stage: 0,smoke_source_stage:0,
        }
    }
    fn target(&self)->Option<crate::video_capture::CaptureTarget>{
        let display=self.displays.get(self.selected_display)?;
        Some(if self.window_mode{
            let window=self.windows.iter().find(|w|Some(w.hwnd)==self.selected_window)?;
            crate::video_capture::CaptureTarget::Window{adapter:display.adapter_index,hwnd:window.hwnd,pid:window.pid}
        }else{crate::video_capture::CaptureTarget::Monitor{adapter:display.adapter_index,output:display.output_index}})
    }
    fn set_fullscreen(&mut self,ctx:&egui::Context,enabled:bool){
        if self.fullscreen==enabled{return;}
        self.fullscreen=enabled;
        if enabled {self.before_fullscreen_mode=self.mode;self.mode=ViewMode::Spotlight;}
        else {self.mode=self.before_fullscreen_mode;}
        self.send_layout();
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(enabled));
        self.last_presented.clear();
    }
    fn send_layout(&self) {
        if let Some(engine) = &self.engine {
            let _ = engine.commands.send(Command::Layout(Layout {
                volume: if self.sound_muted { 0 } else { self.volume },
                mode: self.mode,
                focus: self.focus.clone(),
                paused: self.paused,
            }));
        }
    }
    fn join_room(&self, ctx: egui::Context) -> Engine {
        let engine = if self.token_mode {
            Engine::connect(self.address.trim().to_owned(), self.token.trim().to_owned(), ctx, self.parent, self.relay_only)
        } else {
            Engine::connect_invite(self.address.trim().to_owned(), self.invite.trim().to_owned(), self.name.trim().to_owned(), self.client_id.clone(), ctx, self.parent, self.relay_only)
        };
        let _ = engine.commands.send(Command::SystemAudio(self.system_audio));
        let _ = engine.commands.send(Command::Layout(Layout {
            mode: self.mode, focus: self.focus.clone(), paused: self.paused,
            volume: if self.sound_muted { 0 } else { self.volume },
        }));
        engine
    }
}
impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if ctx.input(|i|i.key_pressed(egui::Key::F11)){self.set_fullscreen(&ctx,!self.fullscreen);}
        if self.fullscreen && ctx.input(|i|i.key_pressed(egui::Key::Escape)){self.set_fullscreen(&ctx,false);}
        if self.smoke_seconds.is_some() && std::env::var("HYPERCAST_SMOKE_SOUND").as_deref() == Ok("1") {
            let elapsed = self.started.elapsed().as_secs();
            if self.smoke_sound_stage == 0 && elapsed >= 6 {
                self.sound_muted = true; self.send_layout(); self.smoke_sound_stage = 1;
                println!("GUI audio: muted");
            } else if self.smoke_sound_stage == 1 && elapsed >= 12 {
                self.sound_muted = false; self.send_layout(); self.smoke_sound_stage = 2;
                println!("GUI audio: resumed");
            }
        }
        if self.smoke_seconds.is_some()
            && std::env::var("HYPERCAST_SMOKE_LAYOUTS").as_deref() == Ok("1")
        {
            let elapsed = self.started.elapsed().as_secs();
            if self.smoke_layout_stage == 0 && elapsed >= 4 {
                self.smoke_layout_stage = 1;
                self.mode = ViewMode::Spotlight;
                self.focus = Some("hc-screen-2".into());
                self.send_layout();
                println!("GUI layout: spotlight hc-screen-2");
            } else if self.smoke_layout_stage == 1 && elapsed >= 7 {
                self.smoke_layout_stage = 2;
                self.paused = true;
                self.send_layout();
                println!("GUI layout: paused");
            } else if self.smoke_layout_stage == 2 && elapsed >= 10 {
                self.smoke_layout_stage = 3;
                self.paused = false;
                self.mode = ViewMode::Grid;
                self.focus = None;
                self.send_layout();
                println!("GUI layout: resumed grid");
            }
        }
        if self.auto_connect && self.engine.is_none() && (!self.token.is_empty() || !self.invite.is_empty()) {
            self.auto_connect = false;
            self.engine = Some(self.join_room(ctx.clone()));
        }
        if self.smoke_seconds.is_some()
            && !self.smoke_reconnected
            && self.started.elapsed().as_secs() >= 6
            && std::env::var("HYPERCAST_SMOKE_RECONNECT").as_deref() == Ok("1")
        {
            self.smoke_reconnected = true;
            self.engine = None;
            self.presenters.clear();
            self.last_presented.clear();
            println!(
                "GUI lifecycle: {} retained GPU decoder surfaces after disconnect",
                livekit::webrtc::native::d3d11_live_surfaces()
            );
            self.engine = Some(Engine::connect(
                self.address.clone(),
                self.token.clone(),
                ctx.clone(),
                self.parent,
                self.relay_only,
            ));
            println!("GUI lifecycle: disconnected and restarted media engine");
        }
        let mut connected = false;
        let mut publishing = false;
        let mut can_publish = false;
        let mut identity = String::new();
        let mut room = "Sua sala de trabalho".to_owned();
        let mut status = "Pronto para conectar".to_owned();
        let mut audio_status = String::new();
        let mut active_target=None;
        let mut source_error=String::new();
        let mut people = Vec::new();
        let mut screens = Vec::new();
        let mut videos = Vec::new();
        let mut received = 0;
        let mut sent = 0;
        if let Some(engine) = &self.engine {
            let state = engine.shared.lock().unwrap();
            connected = state.connected;
            publishing = state.publishing;
            can_publish = state.can_publish;
            identity = state.identity.clone();
            if !state.room.is_empty() {
                room = state.room.clone();
            }
            status = state.status.clone();
            audio_status = state.audio_status.clone();
            active_target=state.active_target;source_error=state.source_error.clone();
            people = state.participants.clone();
            screens = state.screens.clone();
            videos = state
                .frames
                .iter()
                .map(|(id, frame)| (id.clone(), Arc::clone(frame)))
                .collect();
            received = state.received;
            sent = state.sent;
        }
        people.sort();
        screens.sort();
        videos.sort_by_key(|(id, _)| id.clone());
        self.last_presented
            .retain(|id, _| videos.iter().any(|(owner, _)| owner == id));
        self.presenters
            .retain(|id, _| videos.iter().any(|(owner, _)| owner == id));
        if self.smoke_seconds.is_some()
            && connected
            && can_publish
            && std::env::var("HYPERCAST_SMOKE_SHARE").as_deref() == Ok("1")
        {
            let elapsed = self.started.elapsed().as_secs();
            let cycles = if std::env::var("HYPERCAST_SMOKE_SOURCE_SWITCH").as_deref()==Ok("1"){1}else{std::env::var("HYPERCAST_SMOKE_SHARE_CYCLES")
                .ok().and_then(|raw| raw.parse::<u32>().ok()).unwrap_or(2).clamp(2, 6)};
            let stage = u32::from(self.smoke_share_stage);
            if stage < cycles * 2 - 1 && stage % 2 == 0
                && (stage == 0 || elapsed >= 2 + u64::from(stage) * 3) {
                if let (Some(engine), Some(display)) = (&self.engine, self.displays.first()) {
                    let window=std::env::var("HYPERCAST_SMOKE_WINDOW_PID").ok().and_then(|s|s.parse::<u32>().ok())
                        .and_then(|pid|self.windows.iter().find(|w|w.pid==pid));
                    let command=if let Some(w)=window{Command::ShareWindow{adapter:display.adapter_index,hwnd:w.hwnd,pid:w.pid,codec:self.codec}}
                        else {Command::Share{adapter:display.adapter_index,output:display.output_index,codec:self.codec}};
                    let _ = engine.commands.send(command);
                    self.smoke_share_stage += 1;
                    println!("GUI publisher: start");
                }
            } else if stage < cycles * 2 - 1 && stage % 2 == 1 && elapsed >= 2 + u64::from(stage) * 3 {
                if let Some(engine) = &self.engine {
                    let _ = engine.commands.send(Command::StopShare);
                }
                self.smoke_share_stage += 1;
                println!("GUI publisher: stopped");
            }
        }
        if self.smoke_seconds.is_some() && connected && std::env::var("HYPERCAST_SMOKE_SOURCE_SWITCH").as_deref()==Ok("1") {
            let elapsed=self.started.elapsed().as_secs();
            let stage=self.smoke_source_stage;
            if publishing && ((stage==0 && elapsed>=5)||(stage==1 && elapsed>=9)) {
                let target=if stage==0 {self.displays.first().map(|d|crate::video_capture::CaptureTarget::Monitor{adapter:d.adapter_index,output:d.output_index})}
                    else {std::env::var("HYPERCAST_SMOKE_WINDOW_PID").ok().and_then(|s|s.parse::<u32>().ok()).and_then(|pid|self.windows.iter().find(|w|w.pid==pid))
                        .map(|w|crate::video_capture::CaptureTarget::Window{adapter:self.displays[0].adapter_index,hwnd:w.hwnd,pid:w.pid})};
                if let (Some(e),Some(t))=(&self.engine,target){let _=e.commands.send(Command::ChangeSource(t));self.smoke_source_stage+=1;}
            }
        }
        if self.smoke_seconds.is_some() && std::env::var("HYPERCAST_SMOKE_FULLSCREEN").as_deref()==Ok("1") {
            let elapsed=self.started.elapsed().as_secs();
            if !self.fullscreen && (6..16).contains(&elapsed){self.set_fullscreen(&ctx,true);}
            else if self.fullscreen && elapsed>=16{self.set_fullscreen(&ctx,false);}
        }
        if self
            .smoke_seconds
            .is_some_and(|seconds| self.started.elapsed().as_secs() >= seconds)
        {
            println!(
                "GUI smoke: {received} received, {} GPU presentations; error: {}",
                self.rendered, self.error
            );
            println!("GUI publisher: {sent} sent");
            println!(
                "GUI status: connected={connected}, can_publish={can_publish}; {}",
                if self.token.is_empty() { status.clone() } else { status.replace(&self.token, "[token]") }
            );
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        if !self.fullscreen {
        egui::Panel::top("header")
            .exact_size(76.0)
            .frame(
                egui::Frame::new()
                    .fill(PANEL)
                    .inner_margin(egui::Margin::symmetric(28, 16)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("H").size(30.0).strong().color(ACCENT));
                    ui.vertical(|ui| {
                        ui.label(RichText::new("HYPERCAST").size(17.0).strong());
                        ui.label(
                            RichText::new("Seu espaço, compartilhado.")
                                .size(12.0)
                                .color(MUTED),
                        );
                    });
                    ui.add_space(40.0);
                    ui.label(RichText::new(&room).size(16.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(if connected { "CONECTADO" } else { "OFFLINE" })
                                .size(11.0)
                                .color(if connected { ACCENT } else { MUTED }),
                        );
                    });
                });
            });
        egui::Panel::bottom("controls").exact_size(100.0).frame(egui::Frame::new().fill(PANEL).inner_margin(egui::Margin::symmetric(24,16))).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new(if publishing { "SUA TELA ESTÁ AO VIVO" } else { "PRONTO PARA COMPARTILHAR" }).size(10.0).strong().color(if publishing { ACCENT } else { MUTED }));
                    let monitor = self.displays.get(self.selected_display);
                    ui.label(RichText::new(if publishing && active_target.is_some_and(|t|t.process().is_some()) {"Janela de aplicativo ao vivo".into()}else if publishing {"Monitor ao vivo".into()}else if self.window_mode {"Janela de aplicativo".into()}else{monitor.map_or("Nenhum monitor".into(), |d| format!("Monitor {} · {} × {}", self.selected_display + 1, d.width, d.height))}).size(13.0).color(TEXT));
                });
                ui.add_space(12.0);
                if ui.add_enabled(connected && can_publish && (publishing || self.target().is_some()), egui::Button::new(RichText::new(if publishing { "Parar de compartilhar" } else { "Compartilhar tela" }).size(14.0).strong().color(if publishing { DANGER } else { BG })).min_size(Vec2::new(180.0, 46.0)).corner_radius(9).fill(if publishing { Color32::from_rgb(59,34,34) } else { ACCENT }).stroke(egui::Stroke::new(1.0, if publishing { DANGER.gamma_multiply(0.4) } else { ACCENT }))).on_hover_text(if can_publish { "Enviar o monitor ou janela escolhido para os amigos" } else { "Seu acesso permite assistir. Um convite de publicador permite compartilhar." }).clicked() {
                    if let Some(engine) = &self.engine {
                        if publishing { let _ = engine.commands.send(Command::StopShare); }
                        else if let Some(target)=self.target(){let command=match target{
                            crate::video_capture::CaptureTarget::Monitor{adapter,output}=>Command::Share{adapter,output,codec:self.codec},
                            crate::video_capture::CaptureTarget::Window{adapter,hwnd,pid}=>Command::ShareWindow{adapter,hwnd,pid,codec:self.codec},
                        };let _=engine.commands.send(command);}
                    }
                }
                if ui.add_enabled(connected, egui::Button::new(if self.paused { "Retomar recepção" } else { "Pausar recepção" }).min_size(Vec2::new(140.0,46.0)).corner_radius(9)).clicked() { self.paused = !self.paused; self.send_layout(); }
                if ui.add_enabled(!connected || can_publish, egui::Checkbox::new(&mut self.system_audio, if (publishing && active_target.is_some_and(|t|t.process().is_some())) || (!publishing && self.window_mode) {"Áudio do aplicativo"}else{"Áudio do sistema"}))
                    .on_hover_text("Monitor: som do sistema. Janela: som do aplicativo e processos filhos; pode incluir outras janelas dele. Não captura microfone.").changed() {
                    if let Some(engine) = &self.engine { let _ = engine.commands.send(Command::SystemAudio(self.system_audio)); }
                }
                if self.engine.is_some() && ui.add(egui::Button::new(RichText::new("Sair").color(DANGER)).min_size(Vec2::new(64.0,46.0)).corner_radius(9)).on_hover_text("Sair da sala e interromper compartilhamento e recepção").clicked() { self.engine = None; self.presenters.clear(); self.last_presented.clear(); }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(if publishing { "ENVIANDO" } else if connected { "NA SALA" } else { "AGUARDANDO" }).size(10.0).color(if publishing { ACCENT } else { MUTED })).on_hover_text(format!("{received} quadros recebidos / {sent} codificados"));
                });
            });
        });
        egui::Panel::left("room_sidebar")
            .exact_size(296.0)
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(PANEL)
                    .inner_margin(egui::Margin::same(20)),
            )
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        section_title(ui, if connected { "SALA" } else { "ENTRAR NA SALA" }, if connected { "Conectado" } else { "" });
                        ui.add_space(12.0);
                        if connected {
                            egui::Frame::new().fill(SURFACE).corner_radius(9).inner_margin(12).show(ui, |ui| {
                                ui.set_min_width(228.0);
                                ui.label(RichText::new(&room).size(18.0).strong().color(TEXT));
                                ui.label(RichText::new(if can_publish { "Você pode assistir e compartilhar" } else { "Você está assistindo à sala" }).size(12.0).color(MUTED));
                            });
                        } else {
                        egui::CollapsingHeader::new("Servidor e acesso avançado").default_open(self.token_mode).show(ui, |ui| {
                        ui.label("Servidor");
                            ui.add_enabled(self.engine.is_none(),
                            egui::TextEdit::singleline(&mut self.address)
                                .desired_width(208.0)
                                .char_limit(2048)
                                .hint_text("wss://seu-servidor"),
                        );
                        ui.add_enabled(self.engine.is_none(), egui::Checkbox::new(&mut self.token_mode, "Token manual (avançado)"));
                        });
                        if !self.token_mode {
                            ui.label("Seu nome");
                            ui.add_enabled(self.engine.is_none(), egui::TextEdit::singleline(&mut self.name).char_limit(64).desired_width(256.0).hint_text("Como seus amigos vão te reconhecer"));
                            ui.label("Convite da sala");
                            ui.add_enabled(self.engine.is_none(), egui::TextEdit::singleline(&mut self.invite).password(true).char_limit(64).desired_width(256.0).hint_text("Cole o convite da sala"));
                        } else {
                        ui.label("Token da sala");
                            ui.add_enabled(self.engine.is_none(),
                            egui::TextEdit::singleline(&mut self.token)
                                .password(true)
                                .char_limit(8192)
                                .desired_width(208.0)
                                .hint_text("Cole seu token de acesso"),
                        );
                        }
                        if self.engine.is_none()
                            && ui
                                .add_enabled(
                                    if self.token_mode { !self.token.trim().is_empty() } else { !self.invite.trim().is_empty() && !self.name.trim().is_empty() },
                                    egui::Button::new("Entrar na sala")
                                        .min_size(Vec2::new(256.0, 44.0)).corner_radius(9),
                                )
                                .clicked()
                        {
                            self.error.clear();
                            self.engine = Some(self.join_room(ctx.clone()));
                        }
                        }
                        ui.add_space(20.0);
                        ui.separator();
                        ui.add_space(12.0);
                        egui::CollapsingHeader::new("Diagnóstico").default_open(false).show(ui, |ui| {
                        ui.add_enabled(self.engine.is_none(), egui::Checkbox::new(&mut self.relay_only, "Forçar TURN/TLS (teste)"))
                            .on_hover_text("Use antes de entrar para testar o relay; desmarcado permite escolher a rota de menor custo");
                        if ui.button("Copiar diagnóstico").on_hover_text("Copiar o log desta sessão, sem token ou imagens, para enviar ao suporte").clicked() {
                            match crate::diagnostics::export_text() {
                                Ok(text) => ctx.copy_text(text),
                                Err(error) => self.error = format!("Diagnóstico: {error}"),
                            }
                        }
                        if let Some(path) = crate::diagnostics::path() {
                            ui.label(RichText::new("Log salvo automaticamente").size(11.0).color(MUTED))
                                .on_hover_text(path.display().to_string());
                        }
                        });
                        ui.add_space(12.0);
                        section_title(ui, "PARTICIPANTES", &format!("{} na sala", people.len() + usize::from(connected)));
                        ui.add_space(12.0);
                        if connected {
                            let label = if self.name.is_empty() { "Você".to_owned() } else { format!("{} (você)", self.name) };
                            if choice_card(ui, &label, if publishing { "Compartilhando tela" } else { "Assistindo à sala" }, self.focus.as_ref() == Some(&identity), publishing, Some(self.name.chars().next().unwrap_or('V'))).clicked() {
                                self.focus = Some(identity.clone()); self.mode = ViewMode::Spotlight; self.send_layout();
                            }
                        }
                        people.sort_by(|a,b| screens.contains(&b.0).cmp(&screens.contains(&a.0)).then_with(|| a.1.cmp(&b.1)).then_with(|| a.0.cmp(&b.0)));
                        for (identity, name) in &people {
                            let live = screens.contains(identity);
                            let focused = self.focus.as_ref() == Some(identity) || (self.focus.is_none() && screens.first() == Some(identity));
                            if choice_card(ui, if name.is_empty() { "Participante" } else { name }, if live { if focused && self.mode == ViewMode::Spotlight { "Em destaque · compartilhando" } else { "Compartilhando tela" } } else { "Assistindo à sala" }, focused && live, live, Some(name.chars().next().unwrap_or('P'))).clicked()
                            {
                                self.focus = Some(identity.clone());
                                self.mode = ViewMode::Spotlight;
                                self.send_layout();
                            }
                        }
                        if people.is_empty() {
                            egui::Frame::new().fill(SURFACE).corner_radius(9).inner_margin(12).show(ui, |ui| {
                                ui.set_min_width(228.0);
                                ui.label(RichText::new(if connected { "Aguardando seus amigos" } else { "Seus amigos aparecem aqui" }).size(13.0).color(TEXT));
                                ui.label(RichText::new(if connected { "Compartilhe o convite da sala com eles." } else { "Entre na sala para acompanhar as telas." }).size(11.0).color(MUTED));
                            });
                        }
                        ui.add_space(16.0);
                        ui.separator();
                        ui.add_space(12.0);
                        section_title(ui, "ÁUDIO RECEBIDO", "");
                        if ui.checkbox(&mut self.sound_muted, "Silenciar áudio recebido").changed() { self.send_layout(); }
                        let volume_label = ui.horizontal(|ui| {
                            let label = ui.label(RichText::new("Volume").size(13.0).color(TEXT));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(RichText::new(format!("{}%", self.volume)).size(13.0).color(ACCENT));
                            });
                            label.id
                        }).inner;
                        let changed = ui.scope(|ui| {
                            ui.spacing_mut().slider_width = ui.available_width();
                            ui.spacing_mut().slider_rail_height = 6.0;
                            // The global inactive fill matches the sidebar, hiding the rail.
                            ui.visuals_mut().widgets.inactive.bg_fill = Color32::from_rgb(65, 78, 73);
                            ui.visuals_mut().widgets.inactive.fg_stroke = egui::Stroke::new(1.5, ACCENT);
                            ui.visuals_mut().selection.bg_fill = ACCENT;
                            ui.add(egui::Slider::new(&mut self.volume, 0..=100).show_value(false)
                                .trailing_fill(true).handle_shape(egui::style::HandleShape::Circle))
                                .labelled_by(volume_label).changed()
                        }).inner;
                        if changed { self.send_layout(); }
                        ui.label(RichText::new(if self.mode == ViewMode::Grid { "Som das telas visíveis" } else { "Som da tela em destaque" }).size(11.0).color(MUTED));
                        ui.add_space(16.0);
                        ui.label(RichText::new(&status).size(12.0).color(MUTED));
                        if !audio_status.is_empty() { ui.label(RichText::new(&audio_status).size(11.0).color(MUTED)); }
                        if !source_error.is_empty(){ui.colored_label(DANGER,&source_error);}
                        if !self.error.is_empty() {
                            ui.colored_label(Color32::LIGHT_RED, &self.error);
                        }
                    });
            });
        }
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(BG)
                    .inner_margin(egui::Margin::same(28)),
            )
            .show(ui, |ui| {
                if self.fullscreen {
                    ui.horizontal(|ui|{
                        ui.label(RichText::new("Tela em destaque").strong());
                        if ui.button("Sair da tela cheia · Esc / F11").clicked(){self.set_fullscreen(&ctx,false);}
                    });
                } else {
                let old_target=self.target();
                egui::Frame::new().fill(PANEL).stroke(egui::Stroke::new(1.0,BORDER)).corner_radius(12).inner_margin(16).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.columns(2, |columns| {
                        let ui = &mut columns[0];
                        section_title(ui, "ORIGEM DO COMPARTILHAMENTO", if publishing { "AO VIVO" } else { "" });
                        ui.horizontal(|ui|{
                            ui.selectable_value(&mut self.window_mode,false,"Monitor");
                            ui.selectable_value(&mut self.window_mode,true,"Janela de aplicativo");
                        });
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            let (symbol,_) = ui.allocate_exact_size(Vec2::new(42.0,42.0),egui::Sense::hover());
                            monitor_symbol(ui.painter(),symbol.center(),ACCENT,34.0);
                            ui.vertical(|ui| {
                                ui.add_enabled_ui(true, |ui| {
                                    if self.window_mode {
                                        let title=self.windows.iter().find(|w|Some(w.hwnd)==self.selected_window).map(|w|w.title.as_str()).unwrap_or("Selecionar janela…");
                                        egui::ComboBox::from_id_salt("window_picker").width(ui.available_width().min(275.0)).selected_text(title).show_ui(ui,|ui|{
                                            for window in &self.windows {ui.selectable_value(&mut self.selected_window,Some(window.hwnd),&window.title);}
                                        });
                                        if ui.small_button("Atualizar janelas").clicked(){self.windows=crate::video_capture::windows();}
                                    }else{
                                    let text = self.displays.get(self.selected_display).map_or("Nenhum monitor".into(), |d| format!("Monitor {} · {} × {}", self.selected_display+1,d.width,d.height));
                                    egui::ComboBox::from_id_salt("display_picker").width(ui.available_width().min(275.0)).selected_text(text).show_ui(ui, |ui| {
                                        for (index,display) in self.displays.iter().enumerate() {
                                            if choice_card(ui, &format!("Monitor {}",index+1),&format!("{} × {}",display.width,display.height),index==self.selected_display,true,None).on_hover_text(format!("{}\n{}",display.display_name,display.gpu_name)).clicked() { self.selected_display=index; }
                                        }
                                    });
                                    }
                                });
                                ui.label(RichText::new(if self.window_mode {"Apenas a janela escolhida; minimizada pode congelar"}else if publishing { "Troque de monitor sem parar" } else { "Sua tela inteira será compartilhada" }).size(11.0).color(MUTED));
                            });
                        });
                        let ui = &mut columns[1];
                        section_title(ui,"FORMATO DE VÍDEO", "");
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            for (codec,title) in [(crate::encoder::Codec::H265,"HEVC / H.265"),(crate::encoder::Codec::Av1,"AV1")] {
                                let selected=self.codec==codec;
                                if ui.add_enabled(!publishing,egui::Button::new(RichText::new(title).strong().color(if selected { ACCENT } else { TEXT })).min_size(Vec2::new(110.0,42.0)).corner_radius(8).fill(if selected { Color32::from_rgb(40,51,40) } else { SURFACE }).stroke(egui::Stroke::new(1.0,if selected { ACCENT.gamma_multiply(0.65) } else { BORDER })).selected(selected)).clicked() { self.codec=codec; }
                            }
                        });
                        ui.label(RichText::new(if self.codec==crate::encoder::Codec::H265 { "Compatível com RTX 20, 30 e mais novas" } else { "Requer GPU com codificação AV1" }).size(11.0).color(MUTED));
                    });
                });
                if publishing && self.target()!=old_target {
                    if let (Some(engine),Some(target))=(&self.engine,self.target()){let _=engine.commands.send(Command::ChangeSource(target));}
                }
                ui.add_space(20.0);
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new("Telas da sala").size(26.0).strong());
                        ui.label(
                            RichText::new("Escolha uma tela para acompanhar de perto.")
                                .size(13.0)
                                .color(MUTED),
                        );
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add_enabled(!videos.is_empty(),egui::Button::new("Tela cheia · F11")).clicked(){self.set_fullscreen(&ctx,true);}
                        if ui
                            .selectable_label(self.mode == ViewMode::Grid, "Grid")
                            .clicked()
                        {
                            self.mode = ViewMode::Grid;
                            self.send_layout();
                        }
                        if ui
                            .selectable_label(self.mode == ViewMode::Spotlight, "Destaque")
                            .clicked()
                        {
                            self.mode = ViewMode::Spotlight;
                            self.send_layout();
                        }
                    });
                });
                ui.add_space(24.0);
                }
                let available = ui.available_size();
                let rect = ui.allocate_space(available).1;
                ui.painter().rect_filled(rect, 12.0, PANEL);
                if videos.is_empty() || self.paused {
                    for presenter in self.presenters.values_mut() {
                        presenter.hide();
                    }
                    let title = if self.paused {
                        "Recepção pausada"
                    } else if connected {
                        "Aguardando uma tela"
                    } else {
                        "Um lugar para trabalhar juntos."
                    };
                    let subtitle = if self.paused {
                        "Retome quando quiser acompanhar a sala."
                    } else if connected {
                        "Compartilhe seu monitor ou aguarde um amigo começar."
                    } else {
                        "Conecte-se à sala e traga sua tela para a conversa."
                    };
                    ui.painter().text(
                        rect.center() - Vec2::new(0.0, 20.0),
                        egui::Align2::CENTER_CENTER,
                        title,
                        egui::FontId::proportional(24.0),
                        TEXT,
                    );
                    ui.painter().text(
                        rect.center() + Vec2::new(0.0, 20.0),
                        egui::Align2::CENTER_CENTER,
                        subtitle,
                        egui::FontId::proportional(14.0),
                        MUTED,
                    );
                } else {
                    let focus = self
                        .focus
                        .as_ref()
                        .filter(|id|screens.contains(id)||videos.iter().any(|(owner,_)|owner==*id))
                        .or_else(|| screens.first())
                        .or_else(|| {
                            videos
                                .iter()
                                .find(|(_, frame)| !frame.local())
                                .map(|(id, _)| id)
                        })
                        .or_else(|| videos.first().map(|(id, _)| id))
                        .cloned();
                    let mut selected: Vec<_> = videos
                        .iter()
                        .filter(|(id, _)| self.mode == ViewMode::Grid || Some(id) == focus.as_ref())
                        .take(3)
                        .collect();
                    if selected.is_empty() {
                        for presenter in self.presenters.values_mut() {
                            presenter.hide();
                        }
                        ui.painter().text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            "Aguardando a tela selecionada…",
                            egui::FontId::proportional(20.0),
                            MUTED,
                        );
                        return;
                    }
                    if self.mode == ViewMode::Spotlight && !self.fullscreen {
                        selected.extend(
                            videos
                                .iter()
                                .filter(|(id, _)| Some(id) != focus.as_ref())
                                .take(2),
                        );
                    }
                    let unseen: Vec<_> = if self.mode == ViewMode::Spotlight && !self.fullscreen {
                        screens
                            .iter()
                            .filter(|id| {
                                Some(*id) != focus.as_ref()
                                    && !videos.iter().any(|(owner, _)| owner == *id)
                            })
                            .take(3 - selected.len())
                            .collect()
                    } else {
                        Vec::new()
                    };
                    let count = (selected.len() + unseen.len()).max(1);
                    let width = (rect.width() - 16.0 * (count - 1) as f32) / count as f32;
                    let scale = ctx.pixels_per_point();
                    let mut visible = Vec::new();
                    for (index, (identity, frame)) in selected.iter().enumerate() {
                        let thumbnail = self.mode == ViewMode::Spotlight && index > 0;
                        let card = if self.mode == ViewMode::Grid || count == 1 {
                            egui::Rect::from_min_size(
                                rect.min + Vec2::new(index as f32 * (width + 16.0), 0.0),
                                Vec2::new(width, rect.height()),
                            )
                        } else if index == 0 {
                            egui::Rect::from_min_size(
                                rect.min,
                                Vec2::new(rect.width() - 224.0, rect.height()),
                            )
                        } else {
                            egui::Rect::from_min_size(
                                egui::pos2(
                                    rect.right() - 208.0,
                                    rect.top() + (index - 1) as f32 * 200.0,
                                ),
                                Vec2::new(208.0, 184.0),
                            )
                        };
                        ui.painter().rect_stroke(
                            card,
                            8.0,
                            egui::Stroke::new(1.0, BORDER),
                            egui::StrokeKind::Inside,
                        );
                        let canvas = card.shrink2(Vec2::new(8.0, 8.0));
                        let canvas = egui::Rect::from_min_max(
                            canvas.min + Vec2::new(0.0, 32.0),
                            canvas.max - Vec2::new(0.0, 32.0),
                        );
                        let name = people
                            .iter()
                            .find(|(id, _)| id.as_str() == identity.as_str())
                            .map(|(_, name)| name.as_str())
                            .filter(|name| !name.is_empty())
                            .unwrap_or(identity);
                        ui.painter().text(
                            card.min + Vec2::new(16.0, 20.0),
                            egui::Align2::LEFT_CENTER,
                            if frame.local() { "Minha tela" } else { name },
                            egui::FontId::proportional(13.0),
                            TEXT,
                        );
                        if ui
                            .interact(
                                egui::Rect::from_min_size(card.min, Vec2::new(card.width(), 32.0)),
                                egui::Id::new(("focus", identity)),
                                egui::Sense::click(),
                            )
                            .clicked()
                        {
                            self.focus = Some((*identity).clone());
                            self.mode = ViewMode::Spotlight;
                            self.send_layout();
                        }
                        let (frame_width, frame_height) = frame.dimensions();
                        ui.painter().text(
                            card.left_bottom() + Vec2::new(16.0, -18.0),
                            egui::Align2::LEFT_CENTER,
                            if thumbnail && !frame.local() {
                                "Último quadro • pausada".into()
                            } else {
                                format!("{} × {} • GPU", frame_width, frame_height)
                            },
                            egui::FontId::proportional(12.0),
                            MUTED,
                        );
                        let surface = frame.surface();
                        if let Some((texture, slice)) = surface {
                            if !self.presenters.contains_key(identity) {
                                // SAFETY: parent is the live eframe HWND; this runs on its UI thread.
                                match unsafe { Presenter::new(self.parent as *mut _) } {
                                    Ok(p) => {
                                        self.presenters.insert((*identity).clone(), p);
                                    }
                                    Err(e) => self.error = e.to_string(),
                                }
                            }
                            if let Some(presenter) = self.presenters.get_mut(identity) {
                                let bounds = [
                                    (canvas.left() * scale) as i32,
                                    (canvas.top() * scale) as i32,
                                    (canvas.width() * scale) as i32,
                                    (canvas.height() * scale) as i32,
                                ];
                                if self.last_presented.get(identity).is_some_and(
                                    |(last, old_bounds)| {
                                        presenter.visible()
                                            && Arc::ptr_eq(last, frame)
                                            && *old_bounds == bounds
                                    },
                                ) {
                                    visible.push((*identity).clone());
                                    continue;
                                }
                                // SAFETY: frame Arc owns the decoder surface until the GPU fence completes.
                                if let Err(error) = unsafe {
                                    presenter.frame(
                                        texture,
                                        slice,
                                        frame_width,
                                        frame_height,
                                        bounds,
                                    )
                                } {
                                    self.error = error.to_string();
                                    presenter.hide();
                                } else {
                                    self.rendered += 1;
                                    self.last_presented
                                        .insert((*identity).clone(), (Arc::clone(frame), bounds));
                                }
                                visible.push((*identity).clone());
                            }
                        } else {
                            self.error =
                                "O decoder não entregou uma textura GPU compatível.".into();
                        }
                    }
                    for (id, presenter) in &mut self.presenters {
                        if !visible.contains(id) {
                            presenter.hide();
                        }
                    }
                    for (index, id) in unseen.iter().enumerate() {
                        let position = selected.len() - 1 + index;
                        let card = egui::Rect::from_min_size(
                            egui::pos2(rect.right() - 208.0, rect.top() + position as f32 * 200.0),
                            Vec2::new(208.0, 184.0),
                        );
                        ui.painter().rect_stroke(
                            card,
                            8.0,
                            egui::Stroke::new(1.0, BORDER),
                            egui::StrokeKind::Inside,
                        );
                        let name = people
                            .iter()
                            .find(|(owner, _)| owner == *id)
                            .map(|(_, name)| name.as_str())
                            .unwrap_or(id.as_str());
                        ui.painter().text(
                            card.center() - Vec2::new(0.0, 14.0),
                            egui::Align2::CENTER_CENTER,
                            name,
                            egui::FontId::proportional(14.0),
                            TEXT,
                        );
                        ui.painter().text(
                            card.center() + Vec2::new(0.0, 14.0),
                            egui::Align2::CENTER_CENTER,
                            "Pausada • selecione para ver",
                            egui::FontId::proportional(12.0),
                            MUTED,
                        );
                        if ui
                            .interact(card, egui::Id::new(("unseen", id)), egui::Sense::click())
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .clicked()
                        {
                            self.focus = Some((*id).clone());
                            self.send_layout();
                        }
                    }
                }
            });
        ctx.request_repaint_after(Duration::from_millis(500));
    }
}
pub fn run() -> anyhow::Result<()> {
    // Diagnostics failure must not prevent joining a room.
    if let Err(error) = crate::diagnostics::init() {
        eprintln!("Could not initialize GUI diagnostics: {error}");
    }
    let mut options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1320.0, 820.0])
            .with_min_inner_size([960.0, 620.0]),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    options.glow_options.vsync = false;
    options.viewport = options
        .viewport
        .with_icon(eframe::icon_data::from_png_bytes(include_bytes!(
            "../assets/hypercast.png"
        ))?);
    eframe::run_native(
        "HyperCast",
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
    .map_err(|error| anyhow::anyhow!(error.to_string()))
}
