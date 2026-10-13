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

struct App {
    engine: Option<Engine>,
    address: String,
    token: String,
    mode: ViewMode,
    focus: Option<String>,
    paused: bool,
    displays: Vec<crate::screen_capture::Display>,
    selected_display: usize,
    codec: crate::encoder::Codec,
    parent: usize,
    presenters: HashMap<String, Presenter>,
    error: String,
    auto_connect: bool,
    started: Instant,
    smoke_seconds: Option<u64>,
    rendered: u64,
    last_presented: HashMap<String, (Arc<crate::desktop_engine::VideoFrame>, [i32; 4])>,
    smoke_reconnected: bool,
    smoke_layout_stage: u8,
    smoke_share_stage: u8,
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
                .unwrap_or_else(|_| "ws://127.0.0.1:8100".into()),
            token: std::env::var("HYPERCAST_ROOM_TOKEN").unwrap_or_default(),
            mode: if std::env::var("HYPERCAST_VIEW_MODE").as_deref() == Ok("grid") {
                ViewMode::Grid
            } else {
                ViewMode::Spotlight
            },
            focus: std::env::var("HYPERCAST_FOCUS").ok(),
            paused: false,
            displays: crate::screen_capture::displays().unwrap_or_default(),
            selected_display: 0,
            codec: if std::env::var("HYPERCAST_CODEC").as_deref() == Ok("av1") {
                crate::encoder::Codec::Av1
            } else {
                crate::encoder::Codec::H265
            },
            parent,
            presenters: HashMap::new(),
            error: String::new(),
            auto_connect: std::env::var("HYPERCAST_AUTO_CONNECT").as_deref() == Ok("1"),
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
        }
    }
    fn send_layout(&self) {
        if let Some(engine) = &self.engine {
            let _ = engine.commands.send(Command::Layout(Layout {
                mode: self.mode,
                focus: self.focus.clone(),
                paused: self.paused,
            }));
        }
    }
}
impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
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
        if self.auto_connect && self.engine.is_none() && !self.token.is_empty() {
            self.auto_connect = false;
            self.engine = Some(Engine::connect(
                self.address.clone(),
                self.token.clone(),
                ctx.clone(),
                self.parent,
            ));
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
            ));
            println!("GUI lifecycle: disconnected and restarted media engine");
        }
        let mut connected = false;
        let mut publishing = false;
        let mut can_publish = false;
        let mut identity = String::new();
        let mut room = "Sua sala de trabalho".to_owned();
        let mut status = "Pronto para conectar".to_owned();
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
            if self.smoke_share_stage == 0 || (self.smoke_share_stage == 2 && elapsed >= 8) {
                if let (Some(engine), Some(display)) = (&self.engine, self.displays.first()) {
                    let _ = engine.commands.send(Command::Share {
                        adapter: display.adapter_index,
                        output: display.output_index,
                        codec: self.codec,
                    });
                    self.smoke_share_stage += 1;
                    println!("GUI publisher: start");
                }
            } else if self.smoke_share_stage == 1 && elapsed >= 5 {
                if let Some(engine) = &self.engine {
                    let _ = engine.commands.send(Command::StopShare);
                }
                self.smoke_share_stage = 2;
                println!("GUI publisher: stopped");
            }
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
                status.replace(&self.token, "[token]")
            );
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

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
        egui::Panel::bottom("controls").exact_size(80.0).frame(egui::Frame::new().fill(PANEL).inner_margin(egui::Margin::symmetric(28,18))).show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.add_enabled(connected && can_publish, egui::Button::new(RichText::new(if publishing { "Parar compartilhamento" } else { "Compartilhar minha tela" }).color(if publishing { TEXT } else { BG })).fill(if publishing { Color32::from_rgb(92,43,42) } else { ACCENT }).stroke(egui::Stroke::NONE)).on_hover_text(if can_publish { "Enviar o monitor escolhido para os amigos" } else { "Seu token permite assistir. Use um acesso de publicador para compartilhar." }).clicked() {
                    if let Some(engine) = &self.engine {
                        if publishing { let _ = engine.commands.send(Command::StopShare); }
                        else if let Some(display) = self.displays.get(self.selected_display) { let _ = engine.commands.send(Command::Share { adapter: display.adapter_index, output: display.output_index, codec: self.codec }); }
                    }
                }
                if ui.add_enabled(connected, egui::Button::new(if self.paused { "Retomar telas" } else { "Pausar recepção" })).clicked() { self.paused = !self.paused; self.send_layout(); }
                if self.engine.is_some() && ui.button("Sair da sala").clicked() { self.engine = None; self.presenters.clear(); self.last_presented.clear(); }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(format!("{received} recebidos  /  {sent} codificados")).size(12.0).color(MUTED));
                });
            });
        });
        egui::Panel::left("room_sidebar")
            .exact_size(256.0)
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(PANEL)
                    .inner_margin(egui::Margin::same(24)),
            )
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.label(RichText::new("CONEXÃO").size(11.0).color(MUTED));
                        ui.add_space(12.0);
                        ui.label("Servidor");
                            ui.add_enabled(self.engine.is_none(),
                            egui::TextEdit::singleline(&mut self.address)
                                .desired_width(208.0)
                                .char_limit(2048)
                                .hint_text("wss://seu-servidor"),
                        );
                        ui.label("Token da sala");
                            ui.add_enabled(self.engine.is_none(),
                            egui::TextEdit::singleline(&mut self.token)
                                .password(true)
                                .char_limit(8192)
                                .desired_width(208.0)
                                .hint_text("Cole seu token de acesso"),
                        );
                        if self.engine.is_none()
                            && ui
                                .add_enabled(
                                    !self.token.trim().is_empty(),
                                    egui::Button::new("Entrar na sala")
                                        .min_size(Vec2::new(208.0, 44.0)),
                                )
                                .clicked()
                        {
                            self.error.clear();
                            self.engine = Some(Engine::connect(
                                self.address.clone(),
                                self.token.trim().to_owned(),
                                ctx.clone(),
                                self.parent,
                            ));
                        }
                        ui.add_space(20.0);
                        ui.separator();
                        ui.add_space(12.0);
                        ui.label(RichText::new("DIAGNÓSTICO").size(11.0).color(MUTED));
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
                        ui.add_space(12.0);
                        ui.label(RichText::new("PARTICIPANTES").size(11.0).color(MUTED));
                        if people.is_empty() {
                            ui.label(
                                RichText::new("Os amigos aparecem aqui ao entrar.")
                                    .size(13.0)
                                    .color(MUTED),
                            );
                        }
                        for (identity, name) in &people {
                            if ui.add_enabled_ui(screens.contains(identity), |ui| ui
                                .selectable_label(
                                    self.focus.as_ref() == Some(identity),
                                    if name.is_empty() { identity } else { name },
                                )).inner.clicked()
                            {
                                self.focus = Some(identity.clone());
                                self.mode = ViewMode::Spotlight;
                                self.send_layout();
                            }
                        }
                        if publishing
                            && ui
                                .selectable_label(
                                    self.focus.as_ref() == Some(&identity),
                                    "Minha tela",
                                )
                                .clicked()
                        {
                            self.focus = Some(identity.clone());
                            self.mode = ViewMode::Spotlight;
                            self.send_layout();
                        }
                        ui.add_space(24.0);
                        ui.separator();
                        ui.add_space(12.0);
                        ui.label(RichText::new("SEU MONITOR").size(11.0).color(MUTED));
                        egui::ComboBox::from_id_salt("display_picker")
                            .width(200.0)
                            .selected_text(format!("Monitor {}", self.selected_display + 1))
                            .show_ui(ui, |ui| {
                                for (index, display) in self.displays.iter().enumerate() {
                                    ui.selectable_value(
                                        &mut self.selected_display,
                                        index,
                                        format!(
                                            "{} • {} × {}",
                                            display.display_name, display.width, display.height
                                        ),
                                    );
                                }
                            });
                        ui.add_space(12.0);
                        ui.label(RichText::new("FORMATO DE VÍDEO").size(11.0).color(MUTED));
                        ui.add_enabled_ui(!publishing, |ui| {
                            egui::ComboBox::from_id_salt("codec_picker").width(200.0)
                                .selected_text(if self.codec == crate::encoder::Codec::Av1 { "AV1" } else { "HEVC / H.265" })
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut self.codec,crate::encoder::Codec::Av1,"AV1");
                                    ui.selectable_value(&mut self.codec,crate::encoder::Codec::H265,"HEVC / H.265");
                                });
                        }).response.on_hover_text("Escolha um formato compatível com as GPUs da sala. Pare o compartilhamento antes de mudar.");
                        ui.add_space(16.0);
                        ui.label(RichText::new(&status).size(12.0).color(MUTED));
                        if !self.error.is_empty() {
                            ui.colored_label(Color32::LIGHT_RED, &self.error);
                        }
                    });
            });
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(BG)
                    .inner_margin(egui::Margin::same(28)),
            )
            .show(ui, |ui| {
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
                    if self.mode == ViewMode::Spotlight {
                        selected.extend(
                            videos
                                .iter()
                                .filter(|(id, _)| Some(id) != focus.as_ref())
                                .take(2),
                        );
                    }
                    let unseen: Vec<_> = if self.mode == ViewMode::Spotlight {
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
