use crate::{
    desktop_engine::{Command, Engine, Layout, ShareQuality, VideoFrame},
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

fn compact_action(ui: &mut egui::Ui, label: &str, enabled: bool) -> egui::Response {
    crate::desktop_ui::compact(ui, label, enabled)
}

fn update_action(ui: &mut egui::Ui, label: &str, primary: bool) -> egui::Response {
    crate::desktop_ui::action(ui, label, true, primary)
}

fn exit_fullscreen(ui: &mut egui::Ui) -> egui::Response {
    crate::desktop_ui::exit_fullscreen(ui)
}

fn monitor_symbol(painter: &egui::Painter, center: egui::Pos2, color: Color32, size: f32) {
    let stroke = egui::Stroke::new(1.6, color);
    let screen = egui::Rect::from_center_size(
        center - Vec2::new(0.0, size * 0.08),
        Vec2::new(size, size * 0.62),
    );
    painter.rect_stroke(screen, 3.0, stroke, egui::StrokeKind::Inside);
    painter.line_segment(
        [
            center + Vec2::new(0.0, size * 0.24),
            center + Vec2::new(0.0, size * 0.43),
        ],
        stroke,
    );
    painter.line_segment(
        [
            center + Vec2::new(-size * 0.25, size * 0.43),
            center + Vec2::new(size * 0.25, size * 0.43),
        ],
        stroke,
    );
}

/// Native button keeps keyboard focus/activation and accessibility semantics;
/// painter supplies the visual hierarchy without image assets or CPU capture.
fn choice_card(
    ui: &mut egui::Ui,
    title: &str,
    subtitle: &str,
    selected: bool,
    enabled: bool,
    avatar: Option<char>,
) -> egui::Response {
    let response = ui.add_enabled(
        enabled,
        egui::Button::new("")
            .min_size(Vec2::new(ui.available_width(), 66.0))
            .corner_radius(9)
            .fill(if selected {
                Color32::from_rgb(40, 51, 40)
            } else {
                SURFACE
            })
            .stroke(egui::Stroke::new(
                1.0,
                if selected {
                    ACCENT.gamma_multiply(0.65)
                } else {
                    BORDER
                },
            )),
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            enabled,
            format!("{title}: {subtitle}"),
        )
    });
    let compact_avatar = avatar.is_some() && response.rect.width() < 180.0;
    let center =
        response.rect.left_center() + Vec2::new(if compact_avatar { 20.0 } else { 30.0 }, 0.0);
    let tint = if selected { ACCENT } else { MUTED };
    if let Some(initial) = avatar {
        ui.painter().circle_filled(
            center,
            if compact_avatar { 12.0 } else { 18.0 },
            tint.gamma_multiply(0.14),
        );
        ui.painter().text(
            center,
            egui::Align2::CENTER_CENTER,
            initial.to_uppercase().collect::<String>(),
            egui::FontId::proportional(if compact_avatar { 13.0 } else { 17.0 }),
            tint,
        );
        if subtitle.contains("compartilhando")
            || subtitle.contains(crate::i18n::label("Compartilhando", "Sharing"))
            || subtitle.contains(crate::i18n::label("Transmitindo", "Sharing"))
            || subtitle.contains("transmitindo")
        {
            let dot = center + Vec2::splat(if compact_avatar { 9.0 } else { 13.0 });
            ui.painter().circle_filled(dot, 5.0, PANEL);
            ui.painter().circle_filled(dot, 3.5, ACCENT);
        }
    } else {
        monitor_symbol(ui.painter(), center, tint, 38.0);
    }
    let painter = ui.painter().with_clip_rect(response.rect.shrink(8.0));
    let left = response.rect.left() + if compact_avatar { 38.0 } else { 60.0 };
    let right = response.rect.right() - 8.0;
    let top = response.rect.top();
    crate::desktop_ui::paint_video_label(
        ui,
        egui::Rect::from_min_max(egui::pos2(left, top + 12.0), egui::pos2(right, top + 34.0)),
        title,
        14.0,
        TEXT,
    );
    crate::desktop_ui::paint_video_label(
        ui,
        egui::Rect::from_min_max(egui::pos2(left, top + 34.0), egui::pos2(right, top + 56.0)),
        subtitle,
        11.0,
        if selected { ACCENT } else { MUTED },
    );
    if response.has_focus() {
        painter.rect_stroke(
            response.rect.shrink(2.0),
            8.0,
            egui::Stroke::new(1.5, ACCENT),
            egui::StrokeKind::Inside,
        );
    }
    response.on_hover_text(format!("{title}\n{subtitle}"))
}

fn compact_window_title(title: &str) -> String {
    let clean: String = title.chars().filter(|c| !c.is_control()).collect();
    let mut chars = clean.chars();
    let short: String = chars.by_ref().take(30).collect();
    if chars.next().is_some() {
        format!("{short}…")
    } else {
        short
    }
}

fn text_input(
    ui: &mut egui::Ui,
    text: &mut String,
    hint: &str,
    password: bool,
    limit: usize,
    enabled: bool,
) -> egui::Response {
    crate::desktop_ui::input(ui, text, hint, password, limit, enabled)
}

#[derive(Clone, Copy)]
struct SourceDraft {
    active: Option<crate::video_capture::CaptureTarget>,
    audio: bool,
    source: (bool, usize, Option<usize>),
    quality: ShareQuality,
    tab: crate::desktop_ui::ShareTab,
}

struct NativeVideo<'a> {
    identity: &'a str,
    frame: &'a Arc<VideoFrame>,
    canvas: egui::Rect,
}

fn canvas_bounds(canvas: egui::Rect, scale: f32) -> [i32; 4] {
    let x = (canvas.left() * scale).round() as i32;
    let y = (canvas.top() * scale).round() as i32;
    [
        x,
        y,
        (canvas.right() * scale).round() as i32 - x,
        (canvas.bottom() * scale).round() as i32 - y,
    ]
}

fn canvas_cutouts(bounds: [i32; 4], scale: f32, overlays: &[egui::Rect]) -> Vec<[i32; 4]> {
    let [x, y, width, height] = bounds;
    overlays
        .iter()
        .filter_map(|overlay| {
            let overlay = overlay.expand(2.0);
            let left = ((overlay.left() * scale).floor() as i32 - x).clamp(0, width);
            let top = ((overlay.top() * scale).floor() as i32 - y).clamp(0, height);
            let right = ((overlay.right() * scale).ceil() as i32 - x).clamp(0, width);
            let bottom = ((overlay.bottom() * scale).ceil() as i32 - y).clamp(0, height);
            (left < right && top < bottom).then_some([left, top, right, bottom])
        })
        .collect()
}

#[test]
fn native_canvas_clips_the_dialog_and_its_open_menu_at_high_scale() {
    let rect = |left, top, right, bottom| {
        egui::Rect::from_min_max(egui::pos2(left, top), egui::pos2(right, bottom))
    };
    let bounds = canvas_bounds(rect(10.0, 20.0, 210.0, 120.0), 1.5);
    assert_eq!(bounds, [15, 30, 300, 150]);
    assert_eq!(
        canvas_cutouts(
            bounds,
            1.5,
            &[
                rect(40.0, 0.0, 150.0, 60.0),
                rect(130.0, 50.0, 250.0, 110.0),
                rect(0.0, 130.0, 50.0, 140.0),
            ],
        ),
        [[42, 0, 213, 63], [177, 42, 300, 138]],
    );
}

struct App {
    engine: Option<Engine>,
    address: String,
    token: String,
    invite: String,
    saved_room: Option<String>,
    name: String,
    sidebar_open: bool,
    self_preview: bool,
    single_view: bool,
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
    source_draft: Option<SourceDraft>,
    pending_share: Option<(SourceDraft, tokio::sync::oneshot::Receiver<()>)>,
    source_opener: Option<egui::Id>,
    confirm_leave: bool,
    left_room: bool,
    cap_probe: crate::encoder::CapabilityProbe,
    window_mode: bool,
    windows: Vec<crate::video_capture::ApplicationWindow>,
    selected_window: Option<usize>,
    window_picker_open: bool,
    window_previews: HashMap<usize, crate::video_capture::WindowPreview>,
    fullscreen: bool,
    debug_open: bool,
    report_copied: Option<Instant>,
    updater: crate::updater::Updater,
    creator: crate::room_creation::Creator,
    catalog: crate::room_creation::Catalog,
    departure: Option<crate::room_transition::Departure>,
    pending_join: bool,
    stream_settings: crate::resolution::StreamSettings,
    preferences: crate::preferences::Store,
    server_access: crate::room_creation::ServerAccess,
    server_status: crate::server_status::ServerStatus,
    before_fullscreen_mode: ViewMode,
    codec: crate::encoder::Codec,
    amd_profile: crate::encoder::AmdProfile,
    upload: crate::bandwidth::UploadProfile,
    rate_mode: crate::adaptive_upload::Mode,
    parent: usize,
    presenters: HashMap<String, Presenter>,
    error: String,
    auto_connect: bool,
    relay_only: bool,
    started: Instant,
    smoke_seconds: Option<u64>,
    rendered: u64,
    camera_textures: HashMap<String, (Option<Instant>, egui::TextureHandle)>,
    camera_rgba: Vec<u8>,
    last_presented: HashMap<String, (Arc<crate::desktop_engine::VideoFrame>, [i32; 4])>,
    smoke_reconnected: bool,
    smoke_layout_stage: u8,
    smoke_share_stage: u8,
    smoke_sound_stage: u8,
    smoke_source_stage: u8,
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
        let mut prefs = crate::preferences::Settings::load();
        // Smoke runs intentionally ignore saved preferences; opt into the
        // automatic policy explicitly when exercising a generated-window soak.
        if std::env::var("HYPERCAST_SMOKE_SECONDS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .is_some_and(|s| (1..=300).contains(&s))
            && std::env::var("HYPERCAST_SMOKE_AUTOMATIC").as_deref() == Ok("1")
        {
            prefs.rate_mode = crate::adaptive_upload::Mode::Automatic;
        }
        Self {
            preferences: crate::preferences::Store::new(prefs),
            engine: None,
            address: std::env::var("HYPERCAST_URL")
                .unwrap_or_else(|_| "wss://hypercast.cloudlane.com.br".into()),
            token: std::env::var("HYPERCAST_ROOM_TOKEN").unwrap_or_default(),
            invite: std::env::var("HYPERCAST_INVITE").unwrap_or_default(),
            saved_room: None,
            name: crate::preferences::load_name(),
            sidebar_open: prefs.sidebar,
            self_preview: prefs.self_preview,
            single_view: false,
            client_id: crate::invitation::client_id(),
            token_mode: std::env::var("HYPERCAST_ROOM_TOKEN").is_ok(),
            mode: if std::env::var("HYPERCAST_VIEW_MODE").map_or(prefs.grid, |v| v == "grid") {
                ViewMode::Grid
            } else {
                ViewMode::Spotlight
            },
            focus: std::env::var("HYPERCAST_FOCUS").ok(),
            paused: false,
            system_audio: std::env::var("HYPERCAST_SYSTEM_AUDIO").as_deref() == Ok("1"),
            volume: prefs.volume,
            sound_muted: prefs.muted,
            displays: crate::screen_capture::displays().unwrap_or_default(),
            selected_display: 0,
            source_draft: None,
            pending_share: None,
            source_opener: None,
            confirm_leave: false,
            left_room: false,
            cap_probe: Default::default(),
            window_mode: false,
            windows: crate::video_capture::windows(),
            selected_window: None,
            window_picker_open: false,
            window_previews: HashMap::new(),
            fullscreen: false,
            debug_open: false,
            report_copied: None,
            updater: crate::updater::Updater::default(),
            creator: crate::room_creation::Creator::default(),
            catalog: crate::room_creation::Catalog::default(),
            departure: None,
            pending_join: false,
            stream_settings: prefs.stream,
            server_access: crate::room_creation::ServerAccess::default(),
            server_status: crate::server_status::ServerStatus::default(),
            before_fullscreen_mode: ViewMode::Grid,
            amd_profile: match prefs.amd_profile {
                crate::preferences::AmdPreset::Fast => crate::encoder::AmdProfile::Fast,
                crate::preferences::AmdPreset::Text => crate::encoder::AmdProfile::Text,
                crate::preferences::AmdPreset::Compatibility => {
                    crate::encoder::AmdProfile::Compatibility
                }
            },
            rate_mode: prefs.rate_mode,
            upload: if std::env::var_os("HYPERCAST_SMOKE_SECONDS").is_some() {
                crate::bandwidth::UploadProfile::initial()
            } else {
                prefs.upload
            },
            codec: if std::env::var("HYPERCAST_CODEC")
                .map_or(prefs.codec == crate::preferences::VideoCodec::Av1, |v| {
                    v == "av1"
                }) {
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
            camera_textures: HashMap::new(),
            camera_rgba: Vec::new(),
            last_presented: HashMap::new(),
            smoke_reconnected: false,
            smoke_layout_stage: 0,
            smoke_share_stage: 0,
            smoke_sound_stage: 0,
            smoke_source_stage: 0,
        }
    }
    fn save_preferences(&mut self) {
        if self.source_draft.is_some() || self.pending_share.is_some() {
            return;
        }
        self.preferences.update(crate::preferences::Settings {
            codec: if self.codec == crate::encoder::Codec::Av1 {
                crate::preferences::VideoCodec::Av1
            } else {
                crate::preferences::VideoCodec::Hevc
            },
            amd_profile: match self.amd_profile {
                crate::encoder::AmdProfile::Fast => crate::preferences::AmdPreset::Fast,
                crate::encoder::AmdProfile::Text => crate::preferences::AmdPreset::Text,
                crate::encoder::AmdProfile::Compatibility => {
                    crate::preferences::AmdPreset::Compatibility
                }
            },
            stream: self.stream_settings,
            upload: self.upload,
            rate_mode: self.rate_mode,
            volume: self.volume,
            muted: self.sound_muted,
            sidebar: self.sidebar_open,
            self_preview: self.self_preview,
            grid: if self.fullscreen {
                self.before_fullscreen_mode == ViewMode::Grid
            } else {
                self.mode == ViewMode::Grid
            },
        });
    }
    fn restore_preferences(&mut self) {
        let p = crate::preferences::Settings::default();
        self.stream_settings = p.stream;
        self.upload = p.upload;
        self.rate_mode = p.rate_mode;
        self.volume = p.volume;
        self.sound_muted = p.muted;
        self.sidebar_open = p.sidebar;
        self.self_preview = p.self_preview;
        self.mode = ViewMode::Grid;
        self.before_fullscreen_mode = ViewMode::Grid;
        self.single_view = false;
        self.codec = crate::encoder::Codec::H265;
        self.amd_profile = crate::encoder::AmdProfile::Fast;
    }
    fn target(&self) -> Option<crate::video_capture::CaptureTarget> {
        let display = self.displays.get(self.selected_display)?;
        Some(if self.window_mode {
            let window = self
                .windows
                .iter()
                .find(|w| Some(w.hwnd) == self.selected_window && w.available())?;
            crate::video_capture::CaptureTarget::Window {
                adapter: display.adapter_index,
                hwnd: window.hwnd,
                pid: window.pid,
            }
        } else {
            crate::video_capture::CaptureTarget::Monitor {
                adapter: display.adapter_index,
                output: display.output_index,
            }
        })
    }
    fn selected_encode_caps(&self, codec: crate::encoder::Codec) -> crate::encode_caps::EncodeCaps {
        self.displays
            .get(self.selected_display)
            .and_then(|d| self.cap_probe.cached(d.adapter_index))
            .map(|caps| caps[usize::from(codec == crate::encoder::Codec::Av1)])
            .unwrap_or_default()
    }
    fn poll_capture_capabilities(&mut self, ui: &egui::Ui) {
        if let Some(display) = self.displays.get(self.selected_display) {
            let adapter = display.adapter_index;
            self.cap_probe.request(adapter);
            if self.cap_probe.cached(adapter).is_none() {
                ui.ctx().request_repaint_after(Duration::from_millis(100));
            }
        }
    }
    fn capture_rejection(&self) -> Option<crate::encode_caps::Rejection> {
        let caps = self.selected_encode_caps(self.codec);
        if caps.support == 2 {
            return Some(crate::encode_caps::Rejection::Codec);
        }
        if self.rate_mode == crate::adaptive_upload::Mode::Automatic {
            if let Some(reason) = caps.automatic_rejection() {
                return Some(reason);
            }
        }
        // Application-window dimensions are validated against the actual captured texture on start.
        if self.window_mode {
            return None;
        }
        let display = self.displays.get(self.selected_display)?;
        let (max_w, max_h) = self.stream_settings.size.limits();
        let (w, h) = crate::resolution::fit(display.width, display.height, max_w, max_h)?;
        caps.rejection(w, h, self.stream_settings.fps)
    }
    fn set_fullscreen(&mut self, ctx: &egui::Context, enabled: bool) {
        if self.fullscreen == enabled {
            return;
        }
        self.fullscreen = enabled;
        if enabled {
            self.before_fullscreen_mode = self.mode;
            self.mode = ViewMode::Spotlight;
        } else {
            self.mode = self.before_fullscreen_mode;
        }
        self.send_layout();
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(enabled));
        self.last_presented.clear();
        self.camera_textures.clear();
    }
    fn window_selection(&mut self, ui: &mut egui::Ui, space: f32) {
        let mut visible = Vec::with_capacity(4);
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
        egui::ScrollArea::vertical()
            .max_height(space.clamp(78.0, 320.0))
            .auto_shrink([false, true])
            .show(ui, |ui| {
                for (index, window) in self.windows.iter().enumerate() {
                    let response = choice_card(
                        ui,
                        &crate::i18n_format!("Janela {}", "Window {}", index + 1),
                        &compact_window_title(&window.title),
                        self.selected_window == Some(window.hwnd),
                        true,
                        None,
                    );
                    // DWM draws into the native viewport, so only register fully visible
                    // screens. Keep compositor work bounded and release offscreen sources.
                    let center = response.rect.left_center() + Vec2::new(30.0, -38.0 * 0.08);
                    let screen = egui::Rect::from_center_size(center, Vec2::new(38.0, 38.0 * 0.62))
                        .shrink(2.0);
                    if visible.len() < 4
                        && ui.clip_rect().contains_rect(screen)
                        && window.available()
                    {
                        visible.push(window.hwnd);
                        if !self.window_previews.contains_key(&window.hwnd) {
                            if let Some(preview) =
                                crate::video_capture::WindowPreview::new(self.parent, window)
                            {
                                self.window_previews.insert(window.hwnd, preview);
                            }
                        }
                        let scale = ui.ctx().pixels_per_point();
                        let shown =
                            self.window_previews
                                .get_mut(&window.hwnd)
                                .is_some_and(|preview| {
                                    preview.present(
                                        (screen.left() * scale).round() as i32,
                                        (screen.top() * scale).round() as i32,
                                        (screen.width() * scale).round() as i32,
                                        (screen.height() * scale).round() as i32,
                                    )
                                });
                        if !shown {
                            self.window_previews.remove(&window.hwnd);
                        }
                    }
                    if response.clicked() && window.available() {
                        self.selected_window = Some(window.hwnd);
                        self.window_mode = true;
                    }
                }
            });
        self.window_previews
            .retain(|hwnd, _| visible.contains(hwnd));
    }
    fn source_controls(&mut self, ui: &mut egui::Ui, publishing: bool) {
        if publishing {
            ui.colored_label(ACCENT, crate::i18n::label("Ao vivo", "Live"));
        }
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().button_padding = Vec2::new(8.0, 4.0);
            let width = (ui.available_width() - ui.spacing().item_spacing.x) / 2.0;
            for (window, label) in [
                (false, "Monitor"),
                (
                    true,
                    crate::i18n::label("Janela de aplicativo", "App window"),
                ),
            ] {
                let selected = self.window_mode == window;
                if ui
                    .add(
                        egui::Button::new(RichText::new(label).color(if selected {
                            ACCENT
                        } else {
                            TEXT
                        }))
                        .min_size(Vec2::new(width, 38.0))
                        .corner_radius(8)
                        .fill(if selected {
                            Color32::from_rgb(40, 51, 40)
                        } else {
                            SURFACE
                        })
                        .selected(selected),
                    )
                    .clicked()
                {
                    if publishing && window && !self.window_mode {
                        self.selected_window = None;
                    }
                    self.window_mode = window;
                }
            }
        });
        ui.scope(|ui| {
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
            if self.window_mode {
                ui.label(crate::i18n::label("Escolha uma janela", "Choose a window"));
                if self.windows.is_empty() {
                    ui.label(crate::i18n::label(
                        "Abra um aplicativo e atualize a lista.",
                        "Open an app and refresh the list.",
                    ));
                }
                self.window_picker_open = true;
                let list_height = (ui.ctx().content_rect().height() * 0.28).clamp(78.0, 190.0);
                self.window_selection(ui, list_height);
            } else {
                let label = ui.label(crate::i18n::label("Monitor", "Display")).id;
                let title = self.displays.get(self.selected_display).map_or(
                    crate::i18n::label("Nenhum monitor", "No monitors").into(),
                    |d| {
                        format!(
                            "Monitor {} · {} × {}",
                            self.selected_display + 1,
                            d.width,
                            d.height
                        )
                    },
                );
                let response = crate::desktop_ui::select_menu(ui, "display_picker", title, |ui| {
                    for (index, display) in self.displays.iter().enumerate() {
                        if ui
                            .selectable_label(
                                index == self.selected_display,
                                format!(
                                    "Monitor {} · {} × {}",
                                    index + 1,
                                    display.width,
                                    display.height
                                ),
                            )
                            .on_hover_text(format!(
                                "{}\n{}",
                                display.display_name, display.gpu_name
                            ))
                            .clicked()
                        {
                            self.selected_display = index;
                        }
                    }
                });
                response.response.labelled_by(label);
            }
        });
        if !self.window_mode {
            self.window_picker_open = false;
            self.window_previews.clear();
        }
        if self.window_mode
            && compact_action(
                ui,
                crate::i18n::label("Atualizar janelas", "Refresh windows"),
                true,
            )
            .clicked()
        {
            self.windows = crate::video_capture::windows();
        }
        ui.label(
            RichText::new(if publishing {
                crate::i18n::label(
                    "Troque a origem sem parar o envio",
                    "Change the source without stopping the stream",
                )
            } else if self.window_mode {
                crate::i18n::label("Apenas a janela escolhida", "Only the selected window")
            } else {
                crate::i18n::label(
                    "Seu monitor inteiro será compartilhado",
                    "Your entire monitor will be shared",
                )
            })
            .size(11.0)
            .color(MUTED),
        );
    }

    fn video_settings(&mut self, ui: &mut egui::Ui) {
        self.poll_capture_capabilities(ui);
        crate::desktop_ui::stream_settings(ui, &mut self.stream_settings, true, 120);
        crate::desktop_ui::upload_selector(ui, &mut self.upload, true);
        let allow_automatic = self.selected_encode_caps(self.codec).dynamic_bitrate != 2;
        crate::desktop_ui::rate_mode_selector(ui, &mut self.rate_mode, true, allow_automatic);
        ui.horizontal(|ui| {
            let width = (ui.available_width() - ui.spacing().item_spacing.x) / 2.0;
            for (codec, title) in [
                (crate::encoder::Codec::H265, "HEVC / H.265"),
                (crate::encoder::Codec::Av1, "AV1"),
            ] {
                let selected = self.codec == codec;
                if ui
                    .add_enabled(
                        self.selected_encode_caps(codec).support != 2,
                        egui::Button::new(title)
                            .selected(selected)
                            .min_size(Vec2::new(width, 38.0)),
                    )
                    .clicked()
                {
                    self.codec = codec;
                }
            }
        });
        if self.codec == crate::encoder::Codec::H265
            && self
                .selected_encode_caps(crate::encoder::Codec::Av1)
                .support
                == 2
        {
            ui.add(
                egui::Label::new(
                    RichText::new(crate::i18n::label(
                        "AV1 indisponível no encoder desta GPU; use HEVC.",
                        "AV1 is unavailable on this GPU encoder; use HEVC.",
                    ))
                    .size(11.0)
                    .color(MUTED),
                )
                .wrap(),
            );
        }
        let caps = self.selected_encode_caps(self.codec);
        ui.add(
            egui::Label::new(
                RichText::new(
                    self.capture_rejection()
                        .map_or_else(|| caps.hint(), |r| r.label()),
                )
                .size(11.0)
                .color(MUTED),
            )
            .wrap(),
        );
        let vendor = self
            .displays
            .get(self.selected_display)
            .map(|d| d.vendor_id);
        let amd_hevc = vendor == Some(0x1002) && self.codec == crate::encoder::Codec::H265;
        ui.add_enabled_ui(amd_hevc, |ui| {
            ui.label(
                RichText::new(crate::i18n::label(
                    "Perfil AMD · HEVC",
                    "AMD profile · HEVC",
                ))
                .size(11.0)
                .color(MUTED),
            );
            let selected = if amd_hevc {
                self.amd_profile.label()
            } else if vendor == Some(0x10de) {
                crate::i18n::label("NVIDIA · preset automático", "NVIDIA · automatic preset")
            } else if vendor == Some(0x1002) {
                crate::i18n::label("AV1 · perfil automático", "AV1 · automatic profile")
            } else {
                crate::i18n::label(
                    "AMF indisponível nesta origem",
                    "AMF is unavailable for this source",
                )
            };
            crate::desktop_ui::select_menu(ui, "amd_profile", selected, |ui| {
                for profile in [
                    crate::encoder::AmdProfile::Fast,
                    crate::encoder::AmdProfile::Text,
                    crate::encoder::AmdProfile::Compatibility,
                ] {
                    ui.selectable_value(&mut self.amd_profile, profile, profile.label());
                }
            });
        });
        ui.label(
            RichText::new(if amd_hevc {
                crate::i18n::label(
                    "Texto é experimental; aplicado ao envio HEVC pela AMD.",
                    "Text is experimental; applies to AMD HEVC publishing.",
                )
            } else {
                crate::i18n::label(
                    "Perfis AMD disponíveis apenas para uma origem AMD em HEVC.",
                    "AMD profiles require an AMD capture source using HEVC.",
                )
            })
            .size(11.0)
            .color(MUTED),
        );
        ui.label(
            RichText::new(if self.codec == crate::encoder::Codec::Av1 {
                crate::i18n::label(
                    "Envio e recepção exigem GPU compatível com AV1",
                    "Publishing and playback require AV1-compatible GPUs",
                )
            } else {
                crate::i18n::label(
                    "NVIDIA e AMD com suporte HEVC por hardware",
                    "NVIDIA and AMD with hardware HEVC support",
                )
            })
            .size(11.0)
            .color(MUTED),
        );
    }

    fn share_quality(&self) -> ShareQuality {
        ShareQuality {
            codec: self.codec,
            settings: self.stream_settings,
            upload: self.upload,
            rate_mode: self.rate_mode,
            profile: self.amd_profile,
        }
    }

    fn restore_share_quality(&mut self, quality: ShareQuality) {
        self.codec = quality.codec;
        self.stream_settings = quality.settings;
        self.upload = quality.upload;
        self.rate_mode = quality.rate_mode;
        self.amd_profile = quality.profile;
    }

    fn choose_source(
        &mut self,
        opener: egui::Id,
        active: Option<crate::video_capture::CaptureTarget>,
    ) {
        if self.pending_share.is_some() {
            return;
        }
        use crate::video_capture::CaptureTarget;
        if let Some(target) = active {
            match target {
                CaptureTarget::Monitor { adapter, output } => {
                    self.window_mode = false;
                    if let Some(index) = self
                        .displays
                        .iter()
                        .position(|d| d.adapter_index == adapter && d.output_index == output)
                    {
                        self.selected_display = index;
                    }
                }
                CaptureTarget::Window { adapter, hwnd, .. } => {
                    self.window_mode = true;
                    self.selected_window = Some(hwnd);
                    if let Some(index) = self
                        .displays
                        .iter()
                        .position(|d| d.adapter_index == adapter)
                    {
                        self.selected_display = index;
                    }
                }
            }
        }
        self.source_opener = Some(opener);
        self.source_draft = Some(SourceDraft {
            active,
            audio: self.system_audio,
            source: (
                self.window_mode,
                self.selected_display,
                self.selected_window,
            ),
            quality: self.share_quality(),
            tab: crate::desktop_ui::ShareTab::Source,
        });
        self.debug_open = false;
        self.updater.open = false;
        self.windows = crate::video_capture::windows();
    }

    fn apply_source(&mut self, draft: SourceDraft) {
        let (Some(engine), Some(target)) = (&self.engine, self.target()) else {
            return;
        };
        let quality = self.share_quality();
        if let Some(active) = draft.active {
            if draft.quality == quality {
                let command = if target != active {
                    Some(Command::ChangeSource(target, Some(draft.audio)))
                } else if self.system_audio != draft.audio {
                    Some(Command::SystemAudio(draft.audio))
                } else {
                    None
                };
                if let Some(command) = command {
                    let _ = engine.commands.send(command);
                }
                self.system_audio = draft.audio;
                return;
            }
            let (applied, result) = tokio::sync::oneshot::channel();
            let _ = engine.commands.send(Command::ReplaceShare {
                target,
                quality,
                audio: draft.audio,
                applied,
            });
            self.pending_share = Some((draft, result));
            return;
        }
        self.system_audio = draft.audio;
        let command = {
            for command in [
                Command::StreamSettings(self.stream_settings),
                Command::UploadProfile(self.upload),
                Command::RateMode(self.rate_mode),
                Command::AmdProfile(self.amd_profile),
                Command::SystemAudio(self.system_audio),
            ] {
                let _ = engine.commands.send(command);
            }
            match target {
                crate::video_capture::CaptureTarget::Monitor { adapter, output } => {
                    Command::Share {
                        adapter,
                        output,
                        codec: self.codec,
                    }
                }
                crate::video_capture::CaptureTarget::Window { adapter, hwnd, pid } => {
                    Command::ShareWindow {
                        adapter,
                        hwnd,
                        pid,
                        codec: self.codec,
                    }
                }
            }
        };
        let _ = engine.commands.send(command);
    }

    fn sharing_controls(
        &mut self,
        ui: &mut egui::Ui,
        connected: bool,
        can_publish: bool,
        publishing: bool,
        active_target: Option<crate::video_capture::CaptureTarget>,
    ) -> Option<egui::Rect> {
        ui.spacing_mut().item_spacing = Vec2::new(8.0, 8.0);
        let (camera_active, camera_starting, resetting) =
            self.engine
                .as_ref()
                .map_or((false, false, false), |engine| {
                    let state = engine.shared.lock().unwrap();
                    (
                        state.camera_active,
                        state.camera_starting,
                        state.reception_resetting,
                    )
                });
        use crate::desktop_ui::{
            active_share_actions, camera_actions, session_actions, session_button, session_leave,
            session_menu, session_playback_actions, session_room_actions,
        };
        let paused = self.paused;
        let playback =
            |ui: &mut egui::Ui| session_playback_actions(ui, paused, connected, resetting);
        let mut menu_rect = None;
        session_actions(ui, |ui, compact| {
            let mut toggle_preview = false;
            let share = if publishing {
                let (stop, gear, preview) =
                    active_share_actions(ui, compact, Some(self.self_preview));
                if gear.clicked() {
                    self.choose_source(gear.id, active_target);
                }
                toggle_preview = preview.is_some_and(|button| button.clicked());
                stop
            } else {
                session_button(
                    ui,
                    crate::i18n::label("Compartilhar tela", "Share screen"),
                    connected && can_publish,
                    true,
                )
            };
            if share.clicked() {
                if publishing {
                    if let Some(engine) = &self.engine {
                        let _ = engine.commands.send(Command::StopShare);
                    }
                } else {
                    self.choose_source(share.id, None);
                }
            }
            let playback_in_first_row = (!compact).then(|| playback(ui));
            let camera = if camera_starting {
                crate::i18n::label("Abrindo câmera…", "Opening camera…")
            } else if camera_active {
                crate::i18n::label("Desligar câmera", "Turn off camera")
            } else {
                crate::i18n::label("Ligar câmera", "Turn on camera")
            };
            let (camera, preview) = camera_actions(
                ui,
                compact,
                camera,
                connected && can_publish && !camera_starting,
                (camera_active && !publishing).then_some(self.self_preview),
            );
            if camera.clicked() {
                if let Some(engine) = &self.engine {
                    let _ = engine.commands.send(Command::ToggleCamera);
                }
            }
            if toggle_preview || preview.is_some_and(|button| button.clicked()) {
                self.self_preview = !self.self_preview;
                self.send_layout();
            }
            let (pause, reconnect) = playback_in_first_row.unwrap_or_else(|| {
                ui.end_row();
                playback(ui)
            });
            if pause.clicked() {
                self.paused = !self.paused;
                self.send_layout();
            }
            if reconnect.clicked() {
                self.restart_reception();
            }
            session_room_actions(ui, compact, |ui| {
                let leave = session_leave(ui, self.engine.is_some());
                menu_rect = session_menu(ui, |ui| {
                    ui.strong(crate::i18n::label("Sala e interface", "Room and interface"));
                    let room = self
                        .engine
                        .as_ref()
                        .map(|engine| engine.shared.lock().unwrap().room.clone())
                        .unwrap_or_default();
                    if !room.is_empty() {
                        crate::desktop_ui::copy_room_name(ui, &room);
                    }
                    if self.server_access.authenticated()
                        && crate::desktop_ui::menu_action(
                            ui,
                            crate::i18n::label("Trocar de sala", "Switch room"),
                            true,
                        )
                        .clicked()
                    {
                        self.catalog.open(
                            self.address.clone(),
                            self.server_access.token(),
                            ui.ctx().clone(),
                        );
                        ui.close();
                    }
                    crate::desktop_ui::display_scale(ui);
                    if crate::desktop_ui::menu_action(
                        ui,
                        crate::i18n::label("Diagnóstico", "Diagnostics"),
                        true,
                    )
                    .clicked()
                    {
                        self.debug_open = true;
                        ui.close();
                    }
                });
                if leave.clicked() {
                    if publishing || camera_active {
                        self.confirm_leave = true;
                    } else {
                        self.leave_room(ui.ctx(), false);
                    }
                }
            });
        });
        menu_rect
    }
    fn restart_reception(&mut self) {
        if let Some(engine) = &self.engine {
            let _ = engine.commands.send(Command::RestartReception);
        }
        self.presenters.clear();
        self.last_presented.clear();
        self.camera_textures.clear();
        self.error.clear();
    }
    fn present_videos(
        &mut self,
        videos: &[NativeVideo<'_>],
        overlays: &[egui::Rect],
        scale: f32,
        evidence: &crate::evidence::Evidence,
    ) {
        let mut visible = Vec::new();
        for &NativeVideo {
            identity,
            frame,
            canvas,
        } in videos
        {
            let Some((texture, slice)) = frame.surface() else {
                continue;
            };
            let bounds = canvas_bounds(canvas, scale);
            if bounds[2] < 1 || bounds[3] < 1 {
                continue;
            }
            if !self.presenters.contains_key(identity) {
                // SAFETY: this runs on the UI thread that owns the eframe window.
                match unsafe { Presenter::new(self.parent as *mut _) } {
                    Ok(presenter) => {
                        self.presenters.insert(identity.to_owned(), presenter);
                    }
                    Err(error) => {
                        self.error = error.to_string();
                        continue;
                    }
                }
            }
            let presenter = self.presenters.get_mut(identity).unwrap();
            if !presenter.set_cutouts(
                [bounds[2], bounds[3]],
                &canvas_cutouts(bounds, scale, overlays),
            ) {
                continue;
            }
            if !self
                .last_presented
                .get(identity)
                .is_some_and(|(last, old_bounds)| {
                    presenter.visible() && Arc::ptr_eq(last, frame) && *old_bounds == bounds
                })
            {
                let started = Instant::now();
                if let Some(at) = frame.delivered_at() {
                    evidence.record(
                        crate::evidence::Stage::ReaderToRender,
                        started.saturating_duration_since(at),
                    );
                }
                let (width, height) = frame.dimensions();
                // SAFETY: the frame owns the decoder surface through the GPU fence.
                if let Err(error) =
                    unsafe { presenter.frame(texture, slice, width, height, bounds) }
                {
                    self.error = error.to_string();
                    continue;
                }
                self.rendered += 1;
                self.last_presented
                    .insert(identity.to_owned(), (Arc::clone(frame), bounds));
                evidence.record(crate::evidence::Stage::PresentCall, started.elapsed());
            }
            visible.push(identity);
        }
        for (identity, presenter) in &mut self.presenters {
            if !visible.contains(&identity.as_str()) {
                presenter.hide();
            }
        }
    }
    fn send_layout(&self) {
        if let Some(engine) = &self.engine {
            let _ = engine.commands.send(Command::Layout(Layout {
                self_preview: self.self_preview,
                volume: if self.sound_muted { 0 } else { self.volume },
                mode: self.mode,
                focus: self.focus.clone(),
                paused: self.paused,
            }));
        }
    }
    fn leave_room(&mut self, ctx: &egui::Context, join_next: bool) {
        self.left_room = !join_next && self.engine.is_some();
        if let Some((draft, _)) = self.pending_share.take() {
            self.restore_share_quality(draft.quality);
        }
        if let Some(engine) = &self.engine {
            engine.request_exit();
        }
        self.creator.cancel_automatic_join();
        self.creator.open = false;
        self.presenters.clear();
        self.last_presented.clear();
        self.camera_textures.clear();
        self.window_previews.clear();
        self.focus = None;
        self.single_view = false;
        self.paused = false;
        self.auto_connect = false;
        self.pending_join = join_next;
        if self.departure.is_none() {
            self.departure = Some(crate::room_transition::Departure::start(
                self.engine.take(),
                ctx.clone(),
            ));
        }
        self.error.clear();
    }
    fn join_room(&self, ctx: egui::Context) -> Engine {
        let _ = crate::preferences::save_name(&self.name);
        let engine = if self.token_mode {
            Engine::connect(
                self.address.trim().to_owned(),
                self.token.trim().to_owned(),
                ctx,
                self.parent,
                self.relay_only,
            )
        } else if let Some(room) = &self.saved_room {
            Engine::connect_room(
                self.address.trim().to_owned(),
                room.clone(),
                self.name.trim().to_owned(),
                self.client_id.clone(),
                self.server_access.token(),
                ctx,
                self.parent,
                self.relay_only,
            )
        } else {
            Engine::connect_invite(
                self.address.trim().to_owned(),
                self.invite.trim().to_owned(),
                self.name.trim().to_owned(),
                self.client_id.clone(),
                self.server_access.token(),
                ctx,
                self.parent,
                self.relay_only,
            )
        };
        let _ = engine
            .commands
            .send(Command::StreamSettings(self.stream_settings));
        let _ = engine.commands.send(Command::AmdProfile(self.amd_profile));
        let _ = engine.commands.send(Command::UploadProfile(self.upload));
        let _ = engine.commands.send(Command::RateMode(self.rate_mode));
        let _ = engine
            .commands
            .send(Command::SystemAudio(self.system_audio));
        let _ = engine.commands.send(Command::Layout(Layout {
            self_preview: self.self_preview,
            mode: self.mode,
            focus: self.focus.clone(),
            paused: self.paused,
            volume: if self.sound_muted { 0 } else { self.volume },
        }));
        engine
    }
}
impl Drop for App {
    fn drop(&mut self) {
        self.save_preferences();
        if let Some(engine) = &self.engine {
            engine.request_exit();
        }
        let _ = crate::preferences::save_name(&self.name);
        self.presenters.clear();
        self.last_presented.clear();
        self.camera_textures.clear();
        self.engine = None;
        if self.smoke_seconds.is_some() {
            println!(
                "GUI shutdown: {} retained GPU decoder surfaces",
                livekit::webrtc::native::d3d11_live_surfaces()
            );
        }
    }
}
impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if let Some((draft, mut result)) = self.pending_share.take() {
            match result.try_recv() {
                Ok(()) => self.system_audio = draft.audio,
                Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {
                    self.pending_share = Some((draft, result));
                    ui.ctx().request_repaint_after(Duration::from_millis(100));
                }
                Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                    self.restore_share_quality(draft.quality);
                }
            }
        }
        self.save_preferences();
        let ctx = ui.ctx().clone();
        if ctx.input(|i| i.key_pressed(egui::Key::F5))
            && self.engine.as_ref().is_some_and(|e| {
                let s = e.shared.lock().unwrap();
                s.connected && !s.reception_resetting
            })
        {
            self.restart_reception();
        }
        self.updater.poll();
        self.server_access
            .poll(&self.address, &self.client_id, ctx.clone());
        self.server_status.poll(&self.address, &ctx);
        if self.departure.as_ref().is_some_and(|d| d.ready()) {
            self.departure = None;
            if std::mem::take(&mut self.pending_join) {
                self.engine = Some(self.join_room(ctx.clone()));
            }
        }
        if let Some(created) = self.creator.poll() {
            self.saved_room = Some(created.room);
            self.invite.clear();
            self.token_mode = false;
            self.token.clear();
            self.error.clear();
            self.leave_room(&ctx, true);
        }
        if self.fullscreen && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.set_fullscreen(&ctx, false);
        }
        if self.smoke_seconds.is_some()
            && std::env::var("HYPERCAST_SMOKE_SOUND").as_deref() == Ok("1")
        {
            let elapsed = self.started.elapsed().as_secs();
            if self.smoke_sound_stage == 0 && elapsed >= 6 {
                self.sound_muted = true;
                self.send_layout();
                self.smoke_sound_stage = 1;
                println!("GUI audio: muted");
            } else if self.smoke_sound_stage == 1 && elapsed >= 12 {
                self.sound_muted = false;
                self.send_layout();
                self.smoke_sound_stage = 2;
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
        if self.auto_connect
            && self.engine.is_none()
            && (!self.token.is_empty() || !self.invite.is_empty())
        {
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
            self.camera_textures.clear();
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
        let mut quality = crate::video_metrics::QualitySnapshot::default();
        let mut reconnecting = false;
        let mut reception_resetting = false;
        let mut publishing = false;
        let mut camera_active = false;
        let mut can_publish = false;
        let mut identity = String::new();
        let mut room = crate::i18n::label("Sua sala de trabalho", "Your shared space").to_owned();
        let mut status = if self.departure.is_some() {
            if self.pending_join {
                crate::i18n::label("Trocando de sala…", "Switching rooms…")
            } else {
                crate::i18n::label("Saindo da sala…", "Leaving the room…")
            }
        } else if self.left_room {
            crate::desktop_ui::left_room_status(self.server_access.authenticated())
        } else {
            crate::i18n::label("Fora da sala", "Outside the room")
        }
        .to_owned();
        let mut audio_status = String::new();
        let mut active_target = None;
        let mut source_error = String::new();
        let mut people = Vec::new();
        let mut screens = Vec::new();
        let mut videos = Vec::new();
        let mut frozen = Vec::new();
        let mut recovering = std::collections::HashMap::new();
        let mut received = 0;
        let mut sent = 0;
        if let Some(engine) = &self.engine {
            let state = engine.shared.lock().unwrap();
            connected = state.connected;
            reception_resetting = state.reception_resetting;
            quality = state.quality.clone();
            reconnecting = state.reconnecting;
            publishing = state.publishing;
            camera_active = state.camera_active;
            if publishing && self.source_draft.is_none() && self.pending_share.is_none() {
                self.upload = state.upload;
            }
            can_publish = state.can_publish;
            identity = state.identity.clone();
            if !state.room.is_empty() {
                room = state.room.clone();
            }
            status = state.status.clone();
            audio_status = state.audio_status.clone();
            active_target = state.active_target;
            source_error = state.source_error.clone();
            people = state.participants.clone();
            screens = state.screens.clone();
            frozen = state.frozen.keys().cloned().collect();
            recovering = state.recovering.clone();
            videos = state
                .frames
                .iter()
                .filter(|(_, frame)| self.self_preview || !frame.local())
                .map(|(id, frame)| (id.clone(), Arc::clone(frame)))
                .collect();
            if self.self_preview
                && let Some(preview) = state
                    .camera_preview
                    .as_ref()
                    .and_then(|preview| preview.latest())
            {
                videos.push((
                    crate::camera_preview::LOCAL_CAMERA.into(),
                    Arc::new(crate::desktop_engine::VideoFrame::LocalCamera(preview)),
                ));
            }
            received = state.received;
            sent = state.sent;
        }
        let focused_camera_hidden = self.focus.as_deref()
            == Some(crate::camera_preview::LOCAL_CAMERA)
            && (!camera_active || !self.self_preview);
        let focused_screen_hidden = (!publishing || !self.self_preview)
            && !identity.is_empty()
            && self.focus.as_ref() == Some(&identity);
        if focused_camera_hidden || focused_screen_hidden {
            self.focus = None;
            self.send_layout();
        }
        if self.source_draft.is_none()
            && !self.confirm_leave
            && (self.fullscreen || connected && !videos.is_empty())
            && ctx.input(|i| i.key_pressed(egui::Key::F11))
        {
            self.set_fullscreen(&ctx, !self.fullscreen);
        }
        self.camera_textures
            .retain(|key, _| videos.iter().any(|(id, _)| id == key));
        screens.sort();
        videos.sort_unstable_by(|(a, _), (b, _)| a.cmp(b));
        let watched = if self.paused || self.mode != ViewMode::Spotlight {
            None
        } else {
            self.focus
                .as_ref()
                .filter(|id| screens.contains(id) || videos.iter().any(|(owner, _)| owner == *id))
                .or_else(|| screens.first())
                .or_else(|| videos.iter().find(|(_, f)| !f.local()).map(|(id, _)| id))
                .or_else(|| videos.first().map(|(id, _)| id))
                .cloned()
        };
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
            let cycles = if std::env::var("HYPERCAST_SMOKE_SOURCE_SWITCH").as_deref() == Ok("1") {
                1
            } else {
                std::env::var("HYPERCAST_SMOKE_SHARE_CYCLES")
                    .ok()
                    .and_then(|raw| raw.parse::<u32>().ok())
                    .unwrap_or(2)
                    .clamp(2, 6)
            };
            let stage = u32::from(self.smoke_share_stage);
            if stage < cycles * 2 - 1
                && stage % 2 == 0
                && (stage == 0 || elapsed >= 2 + u64::from(stage) * 3)
            {
                if let (Some(engine), Some(display)) = (&self.engine, self.displays.first()) {
                    let window = std::env::var("HYPERCAST_SMOKE_WINDOW_PID")
                        .ok()
                        .and_then(|s| s.parse::<u32>().ok())
                        .and_then(|pid| self.windows.iter().find(|w| w.pid == pid));
                    let window_adapter = std::env::var("HYPERCAST_SMOKE_WINDOW_ADAPTER")
                        .ok()
                        .and_then(|value| value.parse::<u32>().ok())
                        .unwrap_or(display.adapter_index);
                    // An explicitly requested smoke window must never fall back
                    // to the user's monitor if enumeration has not found it yet.
                    let command = if let Some(w) = window {
                        Some(Command::ShareWindow {
                            adapter: window_adapter,
                            hwnd: w.hwnd,
                            pid: w.pid,
                            codec: self.codec,
                        })
                    } else if std::env::var_os("HYPERCAST_SMOKE_WINDOW_PID").is_none() {
                        Some(Command::Share {
                            adapter: display.adapter_index,
                            output: display.output_index,
                            codec: self.codec,
                        })
                    } else {
                        None
                    };
                    if let Some(command) = command {
                        let _ = engine.commands.send(command);
                        self.smoke_share_stage += 1;
                        println!("GUI publisher: start");
                    }
                }
            } else if stage < cycles * 2 - 1
                && stage % 2 == 1
                && elapsed >= 2 + u64::from(stage) * 3
            {
                if let Some(engine) = &self.engine {
                    let _ = engine.commands.send(Command::StopShare);
                }
                self.smoke_share_stage += 1;
                println!("GUI publisher: stopped");
            }
        }
        if self.smoke_seconds.is_some()
            && connected
            && std::env::var("HYPERCAST_SMOKE_SOURCE_SWITCH").as_deref() == Ok("1")
        {
            let elapsed = self.started.elapsed().as_secs();
            let stage = self.smoke_source_stage;
            if publishing && ((stage == 0 && elapsed >= 5) || (stage == 1 && elapsed >= 9)) {
                let target = if stage == 0 {
                    self.displays
                        .first()
                        .map(|d| crate::video_capture::CaptureTarget::Monitor {
                            adapter: d.adapter_index,
                            output: d.output_index,
                        })
                } else {
                    std::env::var("HYPERCAST_SMOKE_WINDOW_PID")
                        .ok()
                        .and_then(|s| s.parse::<u32>().ok())
                        .and_then(|pid| self.windows.iter().find(|w| w.pid == pid))
                        .map(|w| crate::video_capture::CaptureTarget::Window {
                            adapter: self.displays[0].adapter_index,
                            hwnd: w.hwnd,
                            pid: w.pid,
                        })
                };
                if let (Some(e), Some(t)) = (&self.engine, target) {
                    let _ = e.commands.send(Command::ChangeSource(t, None));
                    self.smoke_source_stage += 1;
                }
            }
        }
        if self.smoke_seconds.is_some()
            && std::env::var("HYPERCAST_SMOKE_FULLSCREEN").as_deref() == Ok("1")
        {
            let elapsed = self.started.elapsed().as_secs();
            if !self.fullscreen && (6..16).contains(&elapsed) {
                self.set_fullscreen(&ctx, true);
            } else if self.fullscreen && elapsed >= 16 {
                self.set_fullscreen(&ctx, false);
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
            if let Some(e) = &self.engine {
                println!(
                    "GUI reception refreshes: {}",
                    e.shared.lock().unwrap().reception_restarts
                );
            }
            println!(
                "GUI preview: enabled={} local_surfaces={} rendered={}",
                self.self_preview,
                videos.iter().filter(|(_, f)| f.local()).count(),
                self.rendered
            );
            println!(
                "GUI status: connected={connected}, can_publish={can_publish}; {}",
                if self.token.is_empty() {
                    status.clone()
                } else {
                    status.replace(&self.token, "[token]")
                }
            );
            println!(
                "GUI throughput: sent_fps={:.1} sent_mbps={:.3} received_fps={:.1}",
                quality.send.map_or(0.0, |r| r.fps),
                quality.send.map_or(0.0, |r| r.mbps),
                quality.receive.map_or(0.0, |r| r.fps)
            );
            println!(
                "GUI technical report: {}",
                crate::video_metrics::technical_report(
                    &quality,
                    crate::video_metrics::ReportState {
                        connected,
                        reconnecting,
                        publishing,
                        sent,
                        received,
                        restarts: self.engine.as_ref().map_or(0, |e| e
                            .shared
                            .lock()
                            .unwrap()
                            .reception_restarts)
                    },
                    self.stream_settings,
                    self.upload
                )
                .replace('\n', "")
            );
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        let menu_rect = if !self.fullscreen && (connected || reconnecting) {
            crate::desktop_ui::session_panel(ctx.content_rect().width())
                .show(ui, |ui| {
                    self.sharing_controls(ui, connected, can_publish, publishing, active_target)
                })
                .inner
        } else {
            None
        };

        if connected && !self.fullscreen && self.sidebar_open {
            egui::Panel::left("room_sidebar")
                .exact_size(crate::desktop_ui::sidebar_width(ctx.content_rect().width()))
                .resizable(false)
                .frame(
                    egui::Frame::new()
                        .fill(PANEL)
                        .inner_margin(egui::Margin::same(crate::desktop_ui::room_margin(
                            ctx.content_rect().width(),
                            false,
                        ))),
                )
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                            ui.spacing_mut().item_spacing = Vec2::new(8.0, 8.0);
                            ui.spacing_mut().button_padding = Vec2::new(12.0, 6.0);
                            if crate::desktop_ui::sidebar_update(ui).clicked() {
                                self.window_picker_open = false;
                                egui::Popup::close_all(&ctx);
                                self.debug_open = false;
                                self.updater.open = true;
                                self.updater.check(ctx.clone());
                            }
                            crate::desktop_ui::participant_heading(ui, people.len() + 1);
                            if connected {
                                let label = if self.name.is_empty() {
                                    crate::i18n::label("Você", "You").to_owned()
                                } else {
                                    crate::i18n_format!("{} (você)", "{} (you)", self.name)
                                };
                                if publishing {
                                    let response = choice_card(
                                        ui,
                                        &label,
                                        if !self.self_preview {
                                            crate::i18n::label(
                                                "Transmitindo · prévia oculta",
                                                "Sharing · preview hidden",
                                            )
                                        } else if watched.as_ref() == Some(&identity) {
                                            crate::i18n::label(
                                                "Em destaque · transmitindo",
                                                "Focused · sharing",
                                            )
                                        } else {
                                            crate::i18n::label(
                                                "Transmitindo · sua tela",
                                                "Sharing · your screen",
                                            )
                                        },
                                        self.self_preview && watched.as_ref() == Some(&identity),
                                        self.self_preview,
                                        Some(self.name.chars().next().unwrap_or('V')),
                                    );
                                    if response.clicked() {
                                        self.focus = Some(identity.clone());
                                        self.mode = ViewMode::Spotlight;
                                        self.single_view = response.double_clicked();
                                        self.send_layout();
                                    }
                                } else {
                                    ui.label(
                                        RichText::new(crate::i18n_format!(
                                            "{label} · assistindo",
                                            "{label} · watching"
                                        ))
                                        .size(12.0)
                                        .color(MUTED),
                                    );
                                }
                            }
                            people.sort_by(|a, b| {
                                screens
                                    .contains(&b.0)
                                    .cmp(&screens.contains(&a.0))
                                    .then_with(|| a.1.cmp(&b.1))
                                    .then_with(|| a.0.cmp(&b.0))
                            });
                            for (identity, name) in &people {
                                let target =
                                    screens.iter().find(|key| *key == identity).or_else(|| {
                                        screens.iter().find(|key| {
                                            crate::desktop_engine::video_owner(key) == identity
                                        })
                                    });
                                let live = target.is_some();
                                let focused =
                                    target.is_some_and(|key| watched.as_ref() == Some(key));
                                let response = choice_card(
                                    ui,
                                    if name.is_empty() {
                                        crate::i18n::label("Participante", "Participant")
                                    } else {
                                        name
                                    },
                                    if live {
                                        if focused && self.mode == ViewMode::Spotlight {
                                            crate::i18n::label(
                                                "Em destaque · compartilhando",
                                                "Focused · sharing",
                                            )
                                        } else {
                                            crate::i18n::label(
                                                "Compartilhando tela",
                                                "Sharing screen",
                                            )
                                        }
                                    } else {
                                        crate::i18n::label("Assistindo à sala", "Watching the room")
                                    },
                                    focused && live && self.mode == ViewMode::Spotlight,
                                    live,
                                    Some(name.chars().next().unwrap_or('P')),
                                );
                                if response.clicked() {
                                    self.single_view = response.double_clicked();
                                    self.focus = target.cloned();
                                    self.mode = ViewMode::Spotlight;
                                    self.send_layout();
                                }
                            }
                            if people.is_empty() {
                                egui::Frame::new()
                                    .fill(SURFACE)
                                    .corner_radius(9)
                                    .inner_margin(12)
                                    .show(ui, |ui| {
                                        ui.set_min_width(ui.available_width());
                                        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                                        ui.label(
                                            RichText::new(if connected {
                                                crate::i18n::label(
                                                    "Aguardando seus amigos",
                                                    "Waiting for your friends",
                                                )
                                            } else {
                                                crate::i18n::label(
                                                    "Seus amigos aparecem aqui",
                                                    "Your friends will appear here",
                                                )
                                            })
                                            .size(13.0)
                                            .color(TEXT),
                                        );
                                        ui.label(
                                            RichText::new(if connected {
                                                crate::i18n::label(
                                                    "Seus amigos podem entrar pela lista de salas.",
                                                    "Your friends can join from the room list.",
                                                )
                                            } else {
                                                crate::i18n::label(
                                                    "Entre na sala para acompanhar as telas.",
                                                    "Join the room to watch streams.",
                                                )
                                            })
                                            .size(11.0)
                                            .color(MUTED),
                                        );
                                    });
                            }
                            if audio_status.starts_with(crate::i18n::label("Áudio:", "Audio:")) {
                                ui.colored_label(DANGER, &audio_status);
                            }
                            if !source_error.is_empty() {
                                ui.add(
                                    egui::Label::new(RichText::new(&source_error).color(DANGER))
                                        .wrap(),
                                );
                            }
                            if !self.error.is_empty() {
                                ui.colored_label(Color32::LIGHT_RED, &self.error);
                            }
                        });
                });
        }
        let mut native_videos = Vec::new();
        let mut hide_native_video = false;
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BG).inner_margin(egui::Margin::same(
                crate::desktop_ui::room_margin(ctx.content_rect().width(), self.fullscreen),
            )))
            .show(ui, |ui| {
                if !connected && !reconnecting && !self.fullscreen {
                    crate::desktop_ui::home_panel(ui, &status, |ui| {
                        crate::desktop_ui::server_indicator(ui, &self.server_status);
                        egui::CollapsingHeader::new(crate::i18n::label("Servidor", "Server")).show(
                            ui,
                            |ui| {
                                ui.label(crate::i18n::label("Servidor", "Server"));
                                text_input(
                                    ui,
                                    &mut self.address,
                                    crate::i18n::label("wss://seu-servidor", "wss://your-server"),
                                    false,
                                    2048,
                                    !self.server_access.busy
                                        && (self.engine.is_none()
                                            || self
                                                .engine
                                                .as_ref()
                                                .is_some_and(|engine| engine.finished())),
                                );
                            },
                        );
                        if !self.server_access.authenticated() {
                            let label = ui
                                .label(crate::i18n::label("Chave de acesso", "Access key"))
                                .id;
                            text_input(
                                ui,
                                &mut self.server_access.code,
                                crate::i18n::label(
                                    "Chave compartilhada do servidor",
                                    "Shared server access key",
                                ),
                                true,
                                64,
                                !self.server_access.busy,
                            )
                            .labelled_by(label);
                            ui.checkbox(
                                &mut self.server_access.remember,
                                crate::i18n::label(
                                    "Lembrar neste Windows",
                                    "Remember on this Windows device",
                                ),
                            );
                            ui.add_enabled_ui(
                                !self.server_access.busy
                                    && !self.server_access.code.trim().is_empty(),
                                |ui| {
                                    if update_action(
                                        ui,
                                        crate::i18n::label(
                                            "Continuar com a chave",
                                            "Continue with access key",
                                        ),
                                        true,
                                    )
                                    .clicked()
                                    {
                                        self.server_access.start(
                                            self.address.clone(),
                                            self.client_id.clone(),
                                            ctx.clone(),
                                        );
                                    }
                                },
                            );
                            if self.server_access.busy {
                                ui.spinner();
                            }
                            if !self.server_access.status.is_empty() {
                                ui.label(
                                    RichText::new(&self.server_access.status)
                                        .size(12.0)
                                        .color(MUTED),
                                );
                            }
                        } else {
                            ui.label(crate::i18n::label("Seu nome", "Your name"));
                            if text_input(
                                ui,
                                &mut self.name,
                                crate::i18n::label(
                                    "Como quer aparecer na sala",
                                    "Your display name in the room",
                                ),
                                false,
                                64,
                                self.engine.is_none()
                                    || self.engine.as_ref().is_some_and(|engine| engine.finished()),
                            )
                            .lost_focus()
                            {
                                let _ = crate::preferences::save_name(&self.name);
                            }
                            ui.horizontal(|ui| {
                                let width =
                                    (ui.available_width() - ui.spacing().item_spacing.x) / 2.0;
                                let ready = self.departure.is_none()
                                    && (self.engine.is_none()
                                        || self
                                            .engine
                                            .as_ref()
                                            .is_some_and(|engine| engine.finished()))
                                    && !self.creator.busy;
                                if ui
                                    .add_enabled(
                                        ready,
                                        egui::Button::new(crate::i18n::label(
                                            "Entrar em uma sala",
                                            "Join a room",
                                        ))
                                        .fill(ACCENT.gamma_multiply(0.16))
                                        .min_size(Vec2::new(width, 44.0))
                                        .corner_radius(9),
                                    )
                                    .clicked()
                                {
                                    self.creator.open = false;
                                    self.catalog.open(
                                        self.address.clone(),
                                        self.server_access.token(),
                                        ctx.clone(),
                                    );
                                }
                                if ui
                                    .add_enabled(
                                        self.departure.is_none()
                                            && self.server_access.authenticated()
                                            && !self.creator.busy
                                            && (self.engine.is_none()
                                                || !status.starts_with(crate::i18n::label(
                                                    "Conectando",
                                                    "Connecting",
                                                ))),
                                        egui::Button::new(crate::i18n::label(
                                            "Criar sala",
                                            "Create room",
                                        ))
                                        .min_size(Vec2::new(width, 44.0))
                                        .corner_radius(9),
                                    )
                                    .clicked()
                                {
                                    self.window_picker_open = false;
                                    egui::Popup::close_all(&ctx);
                                    self.creator.open(&self.address);
                                    self.debug_open = false;
                                    self.updater.open = false;
                                }
                            });
                            ui.collapsing(
                                crate::i18n::label("Usar um convite", "Use an invitation"),
                                |ui| {
                                    let label = ui
                                        .label(crate::i18n::label(
                                            "Código do convite",
                                            "Invitation code",
                                        ))
                                        .id;
                                    text_input(ui, &mut self.invite, "", false, 256, true)
                                        .labelled_by(label);
                                    if crate::desktop_ui::session_button(
                                        ui,
                                        crate::i18n::label(
                                            "Entrar com convite",
                                            "Join with invitation",
                                        ),
                                        !self.invite.trim().is_empty()
                                            && self.departure.is_none()
                                            && self.engine.as_ref().is_none_or(Engine::finished),
                                        true,
                                    )
                                    .clicked()
                                    {
                                        self.saved_room = None;
                                        self.token_mode = false;
                                        self.leave_room(&ctx, true);
                                    }
                                },
                            );
                            if !self.server_access.status.is_empty() {
                                ui.label(
                                    RichText::new(&self.server_access.status)
                                        .size(11.0)
                                        .color(MUTED),
                                );
                            }
                            if compact_action(
                                ui,
                                crate::i18n::label(
                                    "Trocar acesso ao servidor",
                                    "Change server access",
                                ),
                                !self.server_access.busy,
                            )
                            .clicked()
                            {
                                self.leave_room(&ctx, false);
                                self.server_access.logout(&self.address);
                            }
                        }
                        if self.departure.is_none()
                            && self.engine.as_ref().is_some_and(|e| e.finished())
                            && (self.saved_room.is_some()
                                || !self.invite.is_empty()
                                || !self.token.is_empty())
                            && crate::desktop_ui::session_button(
                                ui,
                                crate::i18n::label("Tentar novamente", "Try again"),
                                true,
                                true,
                            )
                            .clicked()
                        {
                            self.leave_room(&ctx, true);
                        }
                        crate::desktop_ui::session_menu(ui, |ui| {
                            crate::desktop_ui::display_scale(ui);
                            if crate::desktop_ui::menu_action(
                                ui,
                                crate::i18n::label("Atualizar", "Update"),
                                true,
                            )
                            .clicked()
                            {
                                self.updater.open = true;
                                self.updater.check(ctx.clone());
                                ui.close();
                            }
                            if crate::desktop_ui::menu_action(
                                ui,
                                crate::i18n::label("Diagnóstico", "Diagnostics"),
                                true,
                            )
                            .clicked()
                            {
                                self.debug_open = true;
                                ui.close();
                            }
                        });
                    });
                    return;
                }

                if self.fullscreen {
                    ui.spacing_mut().item_spacing = Vec2::new(8.0, 4.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(if reception_resetting {
                                crate::i18n::label(
                                    "Reconectando recepção…",
                                    "Reconnecting playback…",
                                )
                            } else {
                                crate::i18n::label("Tela em destaque", "Focused stream")
                            })
                            .size(13.0)
                            .color(MUTED),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if exit_fullscreen(ui).clicked() {
                                self.set_fullscreen(&ctx, false);
                            }
                            let resetting = self
                                .engine
                                .as_ref()
                                .is_some_and(|e| e.shared.lock().unwrap().reception_resetting);
                            if crate::desktop_ui::reconnect_button(
                                ui,
                                connected && !resetting,
                                24.0,
                            )
                            .clicked()
                            {
                                self.restart_reception();
                            }
                            if crate::desktop_ui::playback_audio(
                                ui,
                                &mut self.volume,
                                &mut self.sound_muted,
                                true,
                            ) {
                                self.send_layout();
                            }
                        });
                    });
                } else {
                    let detail = if reception_resetting {
                        crate::i18n::label(
                            "Reconectando vídeo e áudio recebidos…",
                            "Reconnecting received video and audio…",
                        )
                    } else if !source_error.is_empty() {
                        &source_error
                    } else if connected && !can_publish {
                        crate::i18n::label(
                            "Nesta sala, você pode assistir.",
                            "You can watch in this room.",
                        )
                    } else {
                        &status
                    };
                    let room_status = crate::desktop_ui::room_activity(
                        publishing,
                        camera_active,
                        detail,
                        crate::desktop_ui::room_state(connected, reconnecting),
                    );
                    let header = crate::desktop_ui::room_header(
                        ui,
                        &room,
                        &room_status,
                        self.sidebar_open,
                        self.mode,
                        !videos.is_empty(),
                        (&mut self.volume, &mut self.sound_muted),
                    );
                    if header.audio_changed {
                        self.send_layout();
                    }
                    if header.participants.clicked() {
                        self.sidebar_open = !self.sidebar_open;
                    }
                    if header.grid.clicked() {
                        self.mode = ViewMode::Grid;
                        self.single_view = false;
                        self.send_layout();
                    }
                    if header.fullscreen.clicked() {
                        self.set_fullscreen(&ctx, true);
                    }
                }
                let available = ui.available_size();
                let rect = ui.allocate_space(available).1;
                ui.painter().rect_filled(rect, 12.0, PANEL);
                // Native video child windows sit above egui, including the modal backdrop.
                hide_native_video = self.confirm_leave
                    || (!self.fullscreen
                        && (self.creator.open
                            || self.catalog.open
                            || self.debug_open
                            || self.updater.open));
                if videos.is_empty() || self.paused {
                    if crate::desktop_ui::playback_empty(
                        ui,
                        rect,
                        self.paused,
                        reconnecting,
                        reception_resetting || !screens.is_empty(),
                        can_publish,
                    ) {
                        self.paused = false;
                        self.send_layout();
                    }
                } else {
                    let mut selection = None;
                    let focus = self
                        .focus
                        .as_ref()
                        .filter(|id| {
                            screens.contains(id) || videos.iter().any(|(owner, _)| owner == *id)
                        })
                        .or(watched.as_ref())
                        .or_else(|| screens.first())
                        .or_else(|| videos.first().map(|(id, _)| id));
                    let mut selected: Vec<_> = videos
                        .iter()
                        .filter(|(id, _)| self.mode == ViewMode::Grid || Some(id) == focus)
                        .take(6 + usize::from(self.self_preview && camera_active))
                        .collect();
                    if selected.is_empty() {
                        // Keep showing the last available video until the newly
                        // selected source has delivered its first frame.
                        selected.push(&videos[0]);
                    }
                    if self.mode == ViewMode::Spotlight && !self.fullscreen && !self.single_view {
                        let primary = &selected[0].0;
                        selected.extend(videos.iter().filter(|(id, _)| id != primary).take(2));
                    }
                    let unseen: Vec<_> = if self.mode == ViewMode::Spotlight
                        && !self.fullscreen
                        && !self.single_view
                    {
                        screens
                            .iter()
                            .filter(|id| {
                                Some(*id) != focus && !videos.iter().any(|(owner, _)| owner == *id)
                            })
                            .take(3 - selected.len())
                            .collect()
                    } else {
                        Vec::new()
                    };
                    let count = (selected.len() + unseen.len()).max(1);
                    for (index, (identity, frame)) in selected.iter().enumerate() {
                        let thumbnail = self.mode == ViewMode::Spotlight && index > 0;
                        let card = if self.mode == ViewMode::Grid {
                            crate::desktop_ui::video_grid_card(rect, index, count, 10.0)
                        } else {
                            crate::desktop_ui::spotlight_card(rect, index, count)
                        };
                        ui.painter().rect_stroke(
                            card,
                            8.0,
                            egui::Stroke::new(1.0, BORDER),
                            egui::StrokeKind::Inside,
                        );
                        let compact_card =
                            card.height() < 144.0 || card.width() < 180.0 || self.fullscreen;
                        let padding = if compact_card { 4.0 } else { 8.0 };
                        let header_height =
                            (if compact_card { 20.0_f32 } else { 30.0 }).min(card.height() * 0.4);
                        let footer_height = if compact_card { padding } else { 26.0 };
                        let canvas = egui::Rect::from_min_max(
                            card.min + Vec2::new(padding, header_height),
                            card.max - Vec2::new(padding, footer_height),
                        );
                        if !canvas.is_positive() {
                            continue;
                        }
                        let owner = crate::desktop_engine::video_owner(identity);
                        let local_camera = identity == crate::camera_preview::LOCAL_CAMERA;
                        let is_camera = local_camera || owner != identity.as_str();
                        let name = people
                            .iter()
                            .find(|(id, _)| id.as_str() == owner)
                            .map(|(_, name)| name.as_str())
                            .filter(|name| !name.is_empty())
                            .unwrap_or(owner);
                        let label = if local_camera {
                            crate::i18n::label("Minha câmera", "My camera").to_owned()
                        } else if frame.local() {
                            crate::i18n::label("Minha tela", "My screen").to_owned()
                        } else {
                            let kind = if is_camera {
                                crate::i18n::label("Câmera", "Camera")
                            } else {
                                crate::i18n::label("Tela", "Screen")
                            };
                            format!("{name} · {kind}")
                        };
                        let title = egui::Rect::from_min_max(
                            card.min + Vec2::new(padding, 0.0),
                            egui::pos2(card.right() - padding, canvas.top()),
                        );
                        crate::desktop_ui::paint_video_label(ui, title, &label, 13.0, TEXT);
                        let response = ui
                            .interact(
                                card,
                                egui::Id::new(("focus", identity)),
                                egui::Sense::click(),
                            )
                            .on_hover_cursor(egui::CursorIcon::PointingHand);
                        response.widget_info(|| {
                            egui::WidgetInfo::labeled(
                                egui::WidgetType::Button,
                                true,
                                crate::i18n_format!(
                                    "Transmissão de {}",
                                    "Stream from {}",
                                    if frame.local() { &label } else { name }
                                ),
                            )
                        });
                        if response.clicked() {
                            selection = Some((identity.clone(), response.double_clicked()));
                        }
                        let (frame_width, frame_height) = frame.dimensions();
                        if !compact_card {
                            let footer = egui::Rect::from_min_max(
                                egui::pos2(title.left(), canvas.bottom()),
                                egui::pos2(title.right(), card.bottom()),
                            );
                            let details = if frozen.contains(identity) {
                                crate::i18n::label(
                                    "Reconectando • último quadro",
                                    "Reconnecting • last frame",
                                )
                                .into()
                            } else if let Some(exhausted) = recovering.get(identity) {
                                crate::reception_recovery::status_label(*exhausted).into()
                            } else if thumbnail && !frame.local() {
                                crate::i18n::label("Último quadro • pausada", "Last frame • paused")
                                    .into()
                            } else {
                                format!("{frame_width} × {frame_height}")
                            };
                            crate::desktop_ui::paint_video_label(ui, footer, &details, 12.0, MUTED);
                        }
                        if frame.surface().is_some() {
                            native_videos.push(NativeVideo {
                                identity,
                                frame,
                                canvas,
                            });
                        } else if is_camera {
                            if let Some((software, captured_at)) = frame.software_frame() {
                                let entry = self.camera_textures.entry((*identity).clone());
                                let texture = entry.or_insert_with(|| {
                                    (
                                        None,
                                        ctx.load_texture(
                                            identity.as_str(),
                                            egui::ColorImage::filled([1, 1], egui::Color32::BLACK),
                                            egui::TextureOptions::LINEAR,
                                        ),
                                    )
                                });
                                if texture.0 != Some(captured_at) {
                                    self.camera_rgba.resize(
                                        frame_width as usize * frame_height as usize * 4,
                                        0,
                                    );
                                    software.buffer.to_argb(
                                        livekit::webrtc::video_frame::VideoFormatType::ABGR,
                                        &mut self.camera_rgba,
                                        frame_width * 4,
                                        frame_width as i32,
                                        frame_height as i32,
                                    );
                                    texture.1.set(
                                        egui::ColorImage::from_rgba_unmultiplied(
                                            [frame_width as usize, frame_height as usize],
                                            &self.camera_rgba,
                                        ),
                                        egui::TextureOptions::LINEAR,
                                    );
                                    texture.0 = Some(captured_at);
                                }
                                if canvas.is_positive() && frame_width != 0 && frame_height != 0 {
                                    let size = Vec2::new(frame_width as f32, frame_height as f32);
                                    let scale =
                                        (canvas.width() / size.x).min(canvas.height() / size.y);
                                    ui.painter().image(
                                        texture.1.id(),
                                        egui::Rect::from_center_size(canvas.center(), size * scale),
                                        egui::Rect::from_min_max(
                                            egui::Pos2::ZERO,
                                            egui::pos2(1.0, 1.0),
                                        ),
                                        egui::Color32::WHITE,
                                    );
                                }
                            }
                        } else {
                            self.error = crate::i18n::label(
                                "O decoder não entregou uma textura GPU compatível.",
                                "The decoder did not return a compatible GPU texture.",
                            )
                            .into();
                        }
                    }
                    for (index, id) in unseen.iter().enumerate() {
                        let card =
                            crate::desktop_ui::spotlight_card(rect, selected.len() + index, count);
                        ui.painter().rect_stroke(
                            card,
                            8.0,
                            egui::Stroke::new(1.0, BORDER),
                            egui::StrokeKind::Inside,
                        );
                        let owner = crate::desktop_engine::video_owner(id);
                        let name = people
                            .iter()
                            .find(|(identity, _)| identity == owner)
                            .map(|(_, name)| name.as_str())
                            .unwrap_or(owner);
                        let kind = if owner == id.as_str() {
                            crate::i18n::label("Tela", "Screen")
                        } else {
                            crate::i18n::label("Câmera", "Camera")
                        };
                        let label = format!("{name} · {kind}");
                        let title = card.shrink2(Vec2::new(6.0, 0.0));
                        let title = if card.height() >= 56.0 {
                            egui::Rect::from_min_max(
                                title.min,
                                egui::pos2(title.right(), title.center().y),
                            )
                        } else {
                            title
                        };
                        crate::desktop_ui::paint_video_label(ui, title, &label, 13.0, TEXT);
                        if card.height() >= 56.0 {
                            let footer = egui::Rect::from_min_max(
                                egui::pos2(title.left(), title.bottom()),
                                egui::pos2(title.right(), card.bottom()),
                            );
                            crate::desktop_ui::paint_video_label(
                                ui,
                                footer,
                                crate::i18n::label("Selecione para ver", "Select to watch"),
                                12.0,
                                MUTED,
                            );
                        }
                        let response = ui
                            .interact(card, egui::Id::new(("unseen", id)), egui::Sense::click())
                            .on_hover_cursor(egui::CursorIcon::PointingHand);
                        if response.clicked() {
                            selection = Some(((*id).clone(), response.double_clicked()));
                        }
                    }
                    if let Some((identity, single_view)) = selection {
                        self.focus = Some(identity);
                        self.single_view = single_view;
                        self.mode = ViewMode::Spotlight;
                        self.send_layout();
                        ctx.request_repaint();
                    }
                }
            });
        if !self.window_picker_open || self.fullscreen || self.debug_open || self.updater.open {
            self.window_previews.clear();
        }
        if self.confirm_leave {
            match crate::desktop_ui::confirm_leave(&ctx) {
                Some(true) => {
                    self.confirm_leave = false;
                    self.leave_room(&ctx, false);
                }
                Some(false) => self.confirm_leave = false,
                None => {}
            }
        }
        let mut overlays: Vec<_> = menu_rect.into_iter().collect();
        if let Some(mut draft) = self.source_draft {
            use crate::desktop_ui::{
                ShareTab, dialog_body, share_confirmation, share_tabs, sharing_dialog,
            };
            let modal = sharing_dialog(
                &ctx,
                if draft.active.is_some() {
                    crate::i18n::label("Editar compartilhamento", "Edit screen share")
                } else {
                    crate::i18n::label("Escolher o que compartilhar", "Choose what to share")
                },
                460.0,
                |ui| {
                    share_tabs(ui, &mut draft.tab);
                    let rejection = dialog_body(ui, 172.0, |ui| {
                        if draft.tab == ShareTab::Quality {
                            self.window_picker_open = false;
                            self.window_previews.clear();
                            self.video_settings(ui);
                        } else {
                            self.source_controls(ui, publishing);
                            crate::desktop_ui::share_audio(ui, &mut draft.audio, self.window_mode);
                            self.poll_capture_capabilities(ui);
                        }
                        let rejection = self.capture_rejection();
                        if let Some(reason) = &rejection {
                            ui.label(reason.label());
                        }
                        rejection
                    });
                    let source = if self.window_mode {
                        self.windows
                            .iter()
                            .find(|window| Some(window.hwnd) == self.selected_window)
                            .map(|window| compact_window_title(&window.title).to_string())
                    } else {
                        self.displays.get(self.selected_display).map(|_| {
                            crate::i18n_format!(
                                "Monitor {}",
                                "Display {}",
                                self.selected_display + 1
                            )
                        })
                    };
                    let quality_changed = draft.quality != self.share_quality();
                    let label = if draft.active.is_some() {
                        if quality_changed {
                            crate::i18n::label(
                                "Aplicar e reiniciar envio",
                                "Apply and restart sharing",
                            )
                            .to_owned()
                        } else {
                            crate::i18n::label("Aplicar alterações", "Apply changes").to_owned()
                        }
                    } else {
                        source.as_deref().map_or_else(
                            || crate::i18n::label("Compartilhar", "Share").to_owned(),
                            |name| crate::i18n_format!("Compartilhar {}", "Share {}", name),
                        )
                    };
                    share_confirmation(
                        ui,
                        &label,
                        connected
                            && can_publish
                            && draft.active.is_some() == publishing
                            && self.target().is_some()
                            && rejection.is_none(),
                    )
                },
            );
            overlays.push(modal.response.rect);
            let (cancel, confirm) = modal.inner;
            if confirm {
                self.apply_source(draft);
            }
            if confirm
                || cancel
                || modal.should_close()
                || !connected
                || draft.active.is_some() != publishing
            {
                if !confirm {
                    (
                        self.window_mode,
                        self.selected_display,
                        self.selected_window,
                    ) = draft.source;
                    self.restore_share_quality(draft.quality);
                }
                self.source_draft = None;
                if let Some(id) = self.source_opener.take() {
                    ctx.memory_mut(|memory| memory.request_focus(id));
                }
                self.window_picker_open = false;
                self.window_previews.clear();
                egui::Popup::close_all(&ctx);
            } else {
                self.source_draft = Some(draft);
            }
        }
        if !self.fullscreen && self.creator.open {
            let modal = crate::desktop_ui::scrolling_dialog(
                &ctx,
                crate::i18n::label("Criar sala", "Create room"),
                "create_room_window",
                360.0,
                |ui| {
                    let title_label = ui.label(crate::i18n::label("Nome da sala", "Room name")).id;
                    text_input(
                        ui,
                        &mut self.creator.title,
                        crate::i18n::label("Jogos com amigos", "Gaming with friends"),
                        false,
                        64,
                        !self.creator.busy,
                    )
                    .labelled_by(title_label);
                    let name_label = ui.label(crate::i18n::label("Seu nome", "Your name")).id;
                    text_input(
                        ui,
                        &mut self.name,
                        crate::i18n::label("Como vão chamar você", "Your display name"),
                        false,
                        64,
                        !self.creator.busy,
                    )
                    .labelled_by(name_label);
                    ui.checkbox(
                        &mut self.creator.persistent,
                        crate::i18n::label("Manter sala indefinidamente", "Keep room indefinitely"),
                    );
                    ui.label(
                        RichText::new(if self.creator.persistent {
                            crate::i18n::label(
                                "Até 12 pessoas · 3 telas · sem expiração",
                                "Up to 12 people · 3 streams · no expiry",
                            )
                        } else {
                            crate::i18n::label(
                                "Até 12 pessoas · 3 telas · válida por 24 horas",
                                "Up to 12 people · 3 streams · valid for 24 hours",
                            )
                        })
                        .size(11.0)
                        .color(MUTED),
                    );
                    if !self.creator.status.is_empty() {
                        ui.label(RichText::new(&self.creator.status).size(12.0).color(MUTED));
                    }
                    if self.creator.busy {
                        ui.spinner();
                    }
                    let valid_name = !self.name.is_empty()
                        && self.name.len() <= 64
                        && self
                            .name
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
                    ui.add_enabled_ui(
                        !self.creator.busy
                            && valid_name
                            && !self.creator.title.trim().is_empty()
                            && self.server_access.authenticated(),
                        |ui| {
                            if update_action(
                                ui,
                                crate::i18n::label("Criar e entrar", "Create and join"),
                                true,
                            )
                            .clicked()
                            {
                                self.creator.start(
                                    self.address.clone(),
                                    self.server_access.token(),
                                    ctx.clone(),
                                );
                            }
                        },
                    );
                    if !valid_name && !self.name.is_empty() {
                        ui.label(
                            RichText::new(crate::i18n::label(
                                "Seu nome: letras sem acentos, números, hífen ou sublinhado",
                                "Your name: unaccented letters, numbers, hyphens or underscores",
                            ))
                            .size(11.0)
                            .color(MUTED),
                        );
                    }
                },
            );
            self.creator.open = !modal.should_close();
            if !self.creator.open {
                self.creator.cancel_automatic_join();
            }
        }
        if !self.fullscreen {
            if let Some(selected) = self.catalog.show(
                &ctx,
                &self.address,
                self.server_access.token(),
                &mut self.name,
                if connected {
                    self.saved_room.as_deref()
                } else {
                    None
                },
                false,
            ) {
                self.saved_room = Some(selected.room);
                self.invite.clear();
                self.token_mode = false;
                self.token.clear();
                self.error.clear();
                self.leave_room(&ctx, true);
            }
        }
        if !self.fullscreen && self.debug_open {
            let modal = crate::desktop_ui::scrolling_dialog(
                &ctx,
                crate::i18n::label("Diagnóstico", "Diagnostics"),
                "debug_window",
                360.0,
                |ui| {
                    ui.label(RichText::new(&status).size(12.0));
                    if let Some(engine) = &self.engine {
                        let stats = engine.shared.lock().unwrap().encoder_stats.clone();
                        if !stats.is_empty() {
                            ui.collapsing(
                                crate::i18n::label("Estatísticas do encoder", "Encoder statistics"),
                                |ui| {
                                    ui.small(stats);
                                },
                            );
                        }
                    }
                    if !audio_status.is_empty() {
                        ui.label(RichText::new(&audio_status).size(11.0).color(MUTED));
                    }
                    let idle = self.engine.is_none()
                        || self.engine.as_ref().is_some_and(|engine| engine.finished());
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(crate::i18n::label(
                            "CONEXÃO AVANÇADA",
                            "ADVANCED CONNECTION",
                        ))
                        .size(10.0)
                        .color(MUTED),
                    );
                    ui.add_enabled(
                        idle && !self.creator.busy,
                        egui::Checkbox::new(
                            &mut self.token_mode,
                            crate::i18n::label(
                                "Token manual (avançado)",
                                "Manual token (advanced)",
                            ),
                        ),
                    );
                    if self.token_mode {
                        ui.label(
                            RichText::new(crate::i18n::label(
                                "JWT temporário de sala, fornecido pelo administrador para testes.",
                                "Temporary room JWT supplied by the administrator for testing.",
                            ))
                            .size(11.0)
                            .color(MUTED),
                        );
                        let label = ui
                            .label(crate::i18n::label(
                                "Token de diagnóstico",
                                "Diagnostic token",
                            ))
                            .id;
                        text_input(
                            ui,
                            &mut self.token,
                            crate::i18n::label("Cole o JWT da sala", "Paste the room JWT"),
                            true,
                            8192,
                            idle,
                        )
                        .labelled_by(label);
                        ui.add_enabled_ui(
                            idle && !self.creator.busy && !self.token.trim().is_empty(),
                            |ui| {
                                if update_action(
                                    ui,
                                    crate::i18n::label("Entrar com token", "Join with token"),
                                    true,
                                )
                                .clicked()
                                {
                                    self.error.clear();
                                    self.leave_room(&ctx, true);
                                }
                            },
                        );
                        if !idle {
                            ui.label(
                                RichText::new(crate::i18n::label(
                                    "Saia da sala para alterar o acesso de diagnóstico.",
                                    "Leave the room to change diagnostic access.",
                                ))
                                .size(11.0)
                                .color(MUTED),
                            );
                        }
                    }
                    ui.add_enabled(idle,egui::Checkbox::new(&mut self.relay_only,crate::i18n::label("Forçar TURN/TLS (teste)","Force TURN/TLS (test)")))
                        .on_hover_text(crate::i18n::label("Ative antes de entrar para testar o relay; desativado escolhe a rota de menor custo","Enable before joining to test the relay; otherwise the lowest-cost route is selected"));
                    if connected || reconnecting {
                        crate::desktop_ui::quality_strip(
                            ui,
                            &quality,
                            reconnecting,
                            publishing,
                            self.paused,
                        );
                    }
                    crate::desktop_ui::network_summary(ui, &quality);
                    if crate::desktop_ui::preferences_controls(
                        ui,
                        &mut self.preferences,
                        idle && self.departure.is_none()
                            && !self.pending_join
                            && !self.creator.busy,
                    ) {
                        self.restore_preferences();
                    }

                    if update_action(
                        ui,
                        if self
                            .report_copied
                            .is_some_and(|at| at.elapsed() < Duration::from_secs(2))
                        {
                            crate::i18n::label("Relatório copiado", "Report copied")
                        } else {
                            crate::i18n::label("Copiar relatório técnico", "Copy technical report")
                        },
                        false,
                    )
                    .clicked()
                    {
                        let restarts = self
                            .engine
                            .as_ref()
                            .and_then(|e| e.shared.try_lock().ok().map(|s| s.reception_restarts))
                            .unwrap_or(0);
                        ctx.copy_text(crate::video_metrics::technical_report(
                            &quality,
                            crate::video_metrics::ReportState {
                                connected,
                                reconnecting,
                                publishing,
                                sent,
                                received,
                                restarts,
                            },
                            self.stream_settings,
                            self.upload,
                        ));
                        self.report_copied = Some(Instant::now());
                        ctx.request_repaint_after(Duration::from_secs(2));
                    }
                    if update_action(
                        ui,
                        crate::i18n::label("Copiar diagnóstico", "Copy diagnostics"),
                        false,
                    )
                    .on_hover_text(crate::i18n::label(
                        "Copiar o log da sessão, sem token ou imagens",
                        "Copy the session log without tokens or images",
                    ))
                    .clicked()
                    {
                        match crate::diagnostics::export_text() {
                            Ok(text) => ctx.copy_text(text),
                            Err(error) => {
                                self.error = crate::i18n_format!(
                                    "Diagnóstico: {error}",
                                    "Diagnostics: {error}"
                                )
                            }
                        }
                    }
                    if let Some(path) = crate::diagnostics::path() {
                        ui.label(
                            RichText::new(crate::i18n::label(
                                "Log salvo automaticamente",
                                "Log saved automatically",
                            ))
                            .size(11.0)
                            .color(MUTED),
                        )
                        .on_hover_text(path.display().to_string());
                    }
                    if !self.error.is_empty() {
                        ui.colored_label(DANGER, &self.error);
                    }
                },
            );
            self.debug_open = !modal.should_close();
        }
        if !self.fullscreen && self.updater.open {
            let mut install = false;
            let modal = crate::desktop_ui::scrolling_dialog(
                &ctx,
                crate::i18n::label("Atualização", "Update"),
                "update_window",
                320.0,
                |ui| {
                    ui.label(crate::i18n_format!(
                        "Versão atual: {}",
                        "Current version: {}",
                        env!("CARGO_PKG_VERSION")
                    ));
                    ui.label(
                        RichText::new(match &self.updater.installation {
                            crate::updater::Installation::Installed(_) => crate::i18n::label(
                                "Instalada · atualização pelo aplicativo",
                                "Installed · in-app updates",
                            ),
                            _ => crate::i18n::label(
                                "Portátil · download pelo navegador",
                                "Portable · download in browser",
                            ),
                        })
                        .size(11.0)
                        .color(MUTED),
                    );
                    ui.label(&self.updater.status);
                    if let Some((progress, label)) = self.updater.progress() {
                        ui.add(egui::ProgressBar::new(progress).text(label));
                    }
                    if let crate::updater::Installation::Unknown(message) =
                        &self.updater.installation
                    {
                        ui.label(
                            RichText::new(crate::i18n::label(
                                "Não foi possível reconhecer a instalação",
                                "Could not identify the installation",
                            ))
                            .size(11.0)
                            .color(MUTED),
                        )
                        .on_hover_text(message);
                    }
                    if !self.updater.busy {
                        if self.updater.installer.is_some() {
                            ui.label(RichText::new(crate::i18n::label("O HyperCast será fechado para instalar. A sessão atual será encerrada.","HyperCast will close to install the update. The current session will end.")).size(12.0).color(MUTED));
                            if update_action(
                                ui,
                                crate::i18n::label("Instalar e reiniciar", "Install and restart"),
                                true,
                            )
                            .clicked()
                            {
                                install = true;
                            }
                        } else if let Some(release) = self.updater.release.clone() {
                            if matches!(
                                self.updater.installation,
                                crate::updater::Installation::Installed(_)
                            ) {
                                if update_action(
                                    ui,
                                    crate::i18n::label("Baixar atualização", "Download update"),
                                    true,
                                )
                                .clicked()
                                {
                                    self.updater.download(ctx.clone());
                                }
                            } else if update_action(
                                ui,
                                crate::i18n::label("Baixar no navegador", "Download in browser"),
                                true,
                            )
                            .clicked()
                            {
                                ctx.open_url(egui::OpenUrl::new_tab(release.page));
                            }
                        }
                        if update_action(
                            ui,
                            crate::i18n::label("Verificar atualização", "Check for updates"),
                            false,
                        )
                        .clicked()
                        {
                            self.updater.check(ctx.clone());
                        }
                        if update_action(
                            ui,
                            crate::i18n::label("Abrir página de downloads", "Open downloads page"),
                            false,
                        )
                        .clicked()
                        {
                            ctx.open_url(egui::OpenUrl::new_tab(crate::updater::RELEASES));
                        }
                    }
                },
            );
            self.updater.open = !modal.should_close();
            if install {
                self.engine = None;
                self.presenters.clear();
                self.last_presented.clear();
                self.camera_textures.clear();
                let result = self
                    .updater
                    .installer
                    .as_ref()
                    .unwrap()
                    .launch(&self.updater.installation);
                match result {
                    Ok(()) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                    Err(error) => {
                        self.updater.status = crate::i18n_format!(
                            "Não foi possível instalar: {error:#}",
                            "Could not install: {error:#}"
                        )
                    }
                }
            }
        }
        overlays.extend(crate::desktop_ui::take_video_popups(&ctx));
        if hide_native_video {
            native_videos.clear();
        }
        self.present_videos(
            &native_videos,
            &overlays,
            ctx.pixels_per_point(),
            &quality.evidence,
        );
        ctx.request_repaint_after(Duration::from_millis(
            if self.updater.busy || self.creator.busy || self.server_access.busy {
                100
            } else {
                500
            },
        ));
    }
}
pub fn run() -> anyhow::Result<()> {
    crate::updater::clean_old_downloads();
    // Diagnostics failure must not prevent joining a room.
    if let Err(error) = crate::diagnostics::init() {
        eprintln!("Could not initialize GUI diagnostics: {error}");
    }
    let mut options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1320.0, 820.0])
            .with_min_inner_size([460.0, 360.0]),
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
