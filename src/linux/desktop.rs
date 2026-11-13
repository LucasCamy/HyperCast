use super::{
    engine::{Command, Engine, Layout, Screen, ShareOptions, State},
    media::{Codec, Encoder, Source},
};
use crate::{
    camera_preview::LOCAL_CAMERA,
    desktop_ui,
    i18n::label,
    room_creation::{Creator, ServerAccess},
    stage::{self, Anchor, RoomVideos, Show},
    stage_ui::{self, ControlAction, StageOverlay, ViewChoice},
    subscription::ViewMode,
};
use eframe::egui::{self, Color32, RichText, Vec2};
#[cfg(test)]
use std::sync::mpsc;
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
const BG: Color32 = Color32::from_rgb(14, 18, 19);
const PANEL: Color32 = Color32::from_rgb(21, 27, 28);
const MUTED: Color32 = Color32::from_rgb(154, 167, 161);
const ACCENT: Color32 = Color32::from_rgb(191, 239, 121);
fn button(ui: &mut egui::Ui, label: &str, enabled: bool, primary: bool) -> egui::Response {
    crate::desktop_ui::action(ui, label, enabled, primary)
}
fn input(
    ui: &mut egui::Ui,
    value: &mut String,
    hint: &str,
    password: bool,
    limit: usize,
) -> egui::Response {
    crate::desktop_ui::input(ui, value, hint, password, limit, true)
}

type ShareQuality = (
    Codec,
    Encoder,
    crate::resolution::StreamSettings,
    crate::bandwidth::UploadProfile,
    crate::adaptive_upload::Mode,
);

#[derive(Clone, Copy)]
struct ShareDraft {
    editing: bool,
    audio: bool,
    source: Option<Source>,
    tab: crate::desktop_ui::ShareTab,
    quality: ShareQuality,
}

struct App {
    address: String,
    name: String,
    sidebar_open: bool,
    invite: String,
    confirm_leave: bool,
    confirm_server_logout: bool,
    share_draft: Option<ShareDraft>,
    pending_share: Option<(ShareDraft, tokio::sync::oneshot::Receiver<()>)>,
    share_source: Source,
    saved_room: Option<String>,
    device: String,
    access: ServerAccess,
    server_status: crate::server_status::ServerStatus,
    creator: Creator,
    catalog: crate::room_creation::Catalog,
    room_title: String,
    engine: Option<Engine>,
    snapshot: State,
    departure: Option<crate::room_transition::Departure>,
    pending_join: bool,
    applied_layout: bool,
    joining: Option<crate::invitation::JoinAttempt>,
    status: String,
    layout: Layout,
    room_videos: RoomVideos,
    stage_overlay: StageOverlay,
    show: Show,
    anchor: Anchor,
    codec: Codec,
    encoder: Encoder,
    upload: crate::bandwidth::UploadProfile,
    rate_mode: crate::adaptive_upload::Mode,
    stream_settings: crate::resolution::StreamSettings,
    preferences: crate::preferences::Store,
    audio: bool,
    volume: u32,
    sound_muted: bool,
    fullscreen: bool,
    debug: bool,
    report_copied: Option<Instant>,
    updates: bool,
    textures: HashMap<String, (Instant, egui::TextureHandle)>,
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
        let prefs = crate::preferences::Settings::load();
        let mut app = Self {
            preferences: crate::preferences::Store::new(prefs),
            address: std::env::var("HYPERCAST_URL")
                .unwrap_or("wss://hypercast.cloudlane.com.br".into()),
            name: crate::preferences::load_name(),
            sidebar_open: prefs.sidebar,
            confirm_leave: false,
            confirm_server_logout: false,
            share_draft: None,
            pending_share: None,
            share_source: Source::Portal,
            invite: std::env::var("HYPERCAST_INVITE").unwrap_or_default(),
            saved_room: None,
            device: crate::invitation::client_id(),
            access: ServerAccess::default(),
            server_status: crate::server_status::ServerStatus::default(),
            creator: Creator::default(),
            catalog: crate::room_creation::Catalog::default(),
            room_title: String::new(),
            engine: None,
            snapshot: State::default(),
            departure: None,
            pending_join: false,
            applied_layout: false,
            joining: None,
            status: String::new(),
            layout: Layout {
                self_preview: prefs.self_preview,
                mode: if std::env::var("HYPERCAST_VIEW_MODE")
                    .map_or(prefs.grid, |mode| mode == "grid")
                {
                    ViewMode::Grid
                } else {
                    ViewMode::Spotlight
                },
                ..Default::default()
            },
            room_videos: RoomVideos::default(),
            stage_overlay: StageOverlay::default(),
            show: prefs.show,
            anchor: prefs.anchor,
            codec: if prefs.codec == crate::preferences::VideoCodec::Av1 {
                Codec::Av1
            } else {
                Codec::Hevc
            },
            encoder: Encoder::Auto,
            rate_mode: prefs.rate_mode,
            upload: if std::env::var_os("HYPERCAST_SMOKE_SECONDS").is_some() {
                crate::bandwidth::UploadProfile::initial()
            } else {
                prefs.upload
            },
            stream_settings: prefs.stream,
            audio: false,
            volume: prefs.volume,
            sound_muted: prefs.muted,
            fullscreen: false,
            debug: false,
            report_copied: None,
            updates: false,
            textures: HashMap::new(),
            rgba: Vec::new(),
            started: Instant::now(),
        };
        if std::env::var("HYPERCAST_AUTO_CONNECT").as_deref() == Ok("1") {
            if let Ok(token) = std::env::var("HYPERCAST_ROOM_TOKEN") {
                app.engine = Some(Engine::start(app.address.clone(), token, ctx.clone()));
            }
        }
        app
    }
    fn save_preferences(&mut self) {
        if self.share_draft.is_some() || self.pending_share.is_some() {
            return;
        }
        self.preferences.update(crate::preferences::Settings {
            codec: if self.codec == Codec::Av1 {
                crate::preferences::VideoCodec::Av1
            } else {
                crate::preferences::VideoCodec::Hevc
            },
            amd_profile: self.preferences.current().amd_profile,
            stream: self.stream_settings,
            upload: self.upload,
            rate_mode: self.rate_mode,
            volume: self.volume,
            muted: self.sound_muted,
            sidebar: self.sidebar_open,
            self_preview: self.layout.self_preview,
            grid: self.layout.mode == ViewMode::Grid,
            show: self.show,
            anchor: self.anchor,
        });
    }
    fn send(&self, command: Command) {
        if let Some(engine) = &self.engine {
            let _ = engine.tx.send(command);
        }
    }
    fn restart_reception(&self) {
        self.send(Command::RestartReception);
    }
    fn layout(&self) {
        let mut layout = self.layout.clone();
        layout.volume = if self.sound_muted { 0 } else { self.volume };
        self.send(Command::Layout(layout));
    }
    fn fullscreen(&mut self, ctx: &egui::Context, enabled: bool) {
        self.fullscreen = enabled;
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(enabled));
    }
    fn leave_room(&mut self, ctx: &egui::Context, join_next: bool) {
        if let Some(draft) = self
            .share_draft
            .take()
            .or_else(|| self.pending_share.take().map(|(draft, _)| draft))
        {
            self.restore_share_quality(draft.quality);
        }
        if let Some(engine) = &self.engine {
            engine.request_exit();
        }
        self.creator.cancel_automatic_join();
        self.creator.open = false;
        self.joining = None;
        self.textures.clear();
        self.snapshot = State::default();
        self.layout.focus = None;
        self.layout.paused = false;
        self.layout.playback = None;
        self.room_videos = RoomVideos::default();
        self.stage_overlay = StageOverlay::default();
        if self.fullscreen {
            self.fullscreen(ctx, false);
        }
        self.applied_layout = false;
        self.pending_join = join_next;
        self.status = if join_next {
            crate::i18n::label("Trocando de sala…", "Switching rooms…")
        } else {
            crate::i18n::label("Saindo da sala…", "Leaving the room…")
        }
        .into();
        if self.departure.is_none() {
            self.departure = Some(crate::room_transition::Departure::start(
                self.engine.take(),
                ctx.clone(),
            ));
        }
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
        let by_room = self.saved_room.is_some();
        let code = self.saved_room.clone().unwrap_or(invite);
        self.joining = Some(crate::invitation::JoinAttempt::start(
            address,
            code,
            name,
            device,
            session,
            by_room,
            ctx.clone(),
        ));
        self.status = crate::i18n::label("Entrando na sala…", "Joining the room…").into();
    }
    fn draw_video(&mut self, ui: &egui::Ui, sid: &str, canvas: egui::Rect) {
        if !canvas.is_positive() || !ui.is_rect_visible(canvas) {
            return;
        }
        let frame = if sid == LOCAL_CAMERA {
            self.snapshot
                .camera_preview
                .as_ref()
                .and_then(|preview| preview.latest())
        } else {
            self.snapshot
                .frames
                .get(sid)
                .map(|(_, frame, at)| (frame.clone(), *at))
        };
        if let Some((frame, delivered_at)) = frame {
            if self
                .textures
                .get(sid)
                .is_none_or(|entry| entry.0 != delivered_at)
            {
                let render_started = Instant::now();
                if sid != LOCAL_CAMERA {
                    self.snapshot.quality.evidence.record(
                        crate::evidence::Stage::ReaderToRender,
                        render_started.saturating_duration_since(delivered_at),
                    );
                }
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
                    entry.0 = delivered_at;
                } else {
                    self.textures.insert(
                        sid.into(),
                        (
                            delivered_at,
                            ui.ctx()
                                .load_texture(sid, image, egui::TextureOptions::LINEAR),
                        ),
                    );
                }
                if sid != LOCAL_CAMERA {
                    self.snapshot.quality.evidence.record(
                        crate::evidence::Stage::RenderPrepare,
                        render_started.elapsed(),
                    );
                }
            }
        }
        if let Some((_, texture)) = self.textures.get(sid) {
            let size = texture.size_vec2();
            let scale = (canvas.width() / size.x).min(canvas.height() / size.y);
            ui.painter().with_clip_rect(canvas).image(
                texture.id(),
                egui::Rect::from_center_size(canvas.center(), size * scale),
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        } else {
            stage_ui::video_loading(ui, canvas);
        }
    }

    fn grid_card(&mut self, ui: &egui::Ui, video: &Screen, card: egui::Rect) -> egui::Response {
        let padding = if card.height() < 144.0 || card.width() < 180.0 {
            4.0
        } else {
            8.0
        };
        let header = 24.0_f32.min(card.height() * 0.4);
        let recovery = self
            .snapshot
            .recovering
            .get(&video.sid)
            .copied()
            .filter(|_| card.height() >= 100.0);
        let footer = recovery.map_or(padding, |_| 22.0);
        let canvas = egui::Rect::from_min_max(
            card.min + Vec2::new(padding, header),
            card.max - Vec2::new(padding, footer),
        );
        ui.painter().rect_stroke(
            card,
            8.0,
            egui::Stroke::new(1.0, desktop_ui::BORDER),
            egui::StrokeKind::Inside,
        );
        let title = egui::Rect::from_min_max(
            egui::pos2(canvas.left(), card.top()),
            egui::pos2(canvas.right(), canvas.top()),
        );
        desktop_ui::paint_video_label(ui, title, &video.name, 13.0, desktop_ui::TEXT);
        if let Some(exhausted) = recovery {
            let footer = egui::Rect::from_min_max(
                egui::pos2(canvas.left(), canvas.bottom()),
                egui::pos2(canvas.right(), card.bottom()),
            );
            desktop_ui::paint_video_label(
                ui,
                footer,
                crate::reception_recovery::status_label(exhausted),
                12.0,
                MUTED,
            );
        }
        self.draw_video(ui, &video.sid, canvas);
        stage_ui::video_click(ui, card, &video.sid, &video.name, false)
    }

    fn room_sidebar(
        &mut self,
        root: &mut egui::Ui,
        width: f32,
        room: &str,
        screens: &[Screen],
        arrangement: &stage::Arrangement,
    ) {
        let rail = !self.sidebar_open || width < stage_ui::RAIL_BELOW;
        let mut people = self.snapshot.participants.clone();
        let target = |owner: &str| {
            screens
                .iter()
                .find(|video| video.owner == owner && video.sid != LOCAL_CAMERA)
        };
        people.sort_by(|a, b| {
            target(&b.0)
                .is_some()
                .cmp(&target(&a.0).is_some())
                .then_with(|| a.1.cmp(&b.1))
        });
        let people: Vec<_> = people
            .iter()
            .map(|(id, name)| {
                let target = target(id);
                let watched = (self.layout.mode == ViewMode::Spotlight && !self.layout.paused)
                    .then(|| {
                        screens.iter().find(|video| {
                            video.owner == *id && arrangement.stage.contains(&video.sid)
                        })
                    })
                    .flatten();
                stage_ui::RailPerson {
                    name: if name.is_empty() {
                        label("Participante", "Participant")
                    } else {
                        name
                    },
                    status: stage_ui::participant_status(
                        target.map(|video| video.camera),
                        watched.map(|video| video.camera),
                    ),
                    live: target.is_some(),
                    focused: watched.is_some(),
                }
            })
            .zip(people.iter().map(|(id, _)| target(id)))
            .collect();
        stage_ui::sidebar(width, rail).show(root, |ui| {
            stage_ui::sidebar_actions(ui, |ui| self.me_panel(ui, rail, room));
            let chosen = if rail {
                let avatars: Vec<_> = people.iter().map(|(person, _)| *person).collect();
                stage_ui::rail_people(ui, &avatars)
            } else {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(8.0, 8.0);
                        if desktop_ui::sidebar_update(ui).clicked() {
                            self.updates = true;
                        }
                        desktop_ui::participant_heading(ui, people.len() + 1);
                        let mut chosen = None;
                        for (index, (person, _)) in people.iter().enumerate() {
                            if stage_ui::participant_card(ui, person).clicked() {
                                chosen = Some(index);
                            }
                        }
                        chosen
                    })
                    .inner
            };
            if let Some(index) = chosen
                && let Some(video) = people[index].1
            {
                if !self.show.allows(video.camera) {
                    self.show = Show::All;
                }
                self.layout.focus = Some(video.sid.clone());
                self.layout.mode = ViewMode::Spotlight;
                ui.ctx().request_repaint();
            }
        });
    }

    fn me_panel(&mut self, ui: &mut egui::Ui, rail: bool, room: &str) {
        let State {
            connected,
            can_publish,
            sharing,
            selecting,
            camera_active,
            camera_starting,
            reception_resetting,
            ..
        } = self.snapshot;
        let status = stage_ui::me_status(connected, can_publish, sharing, camera_active);
        let outcome = stage_ui::me_panel(
            ui,
            &stage_ui::Me {
                name: &self.name,
                status,
                room,
                rail,
                connected,
                can_publish,
                publishing: sharing,
                selecting,
                share_pending: self.pending_share.is_some(),
                self_video: camera_active.then(|| label("minha câmera", "my camera")),
                camera_active,
                camera_starting,
                preview: self.layout.self_preview,
                paused: self.layout.paused,
                resetting: reception_resetting,
                can_switch_room: self.access.authenticated(),
            },
        );
        if outcome.share.is_some() {
            if selecting {
                self.send(Command::CancelSelection);
            } else if sharing {
                self.send(Command::Stop);
            } else {
                self.open_share_editor(false);
            }
        }
        if outcome.gear.is_some() {
            self.open_share_editor(true);
        }
        if outcome.camera {
            self.send(Command::ToggleCamera);
        }
        if outcome.preview {
            self.layout.self_preview = !self.layout.self_preview;
            ui.ctx().request_repaint();
        }
        if outcome.focus_self {
            if !self.show.allows(true) {
                self.show = Show::All;
            }
            self.layout.focus = Some(LOCAL_CAMERA.into());
            self.layout.mode = ViewMode::Spotlight;
            ui.ctx().request_repaint();
        }
        match outcome.menu {
            Some(stage_ui::RoomChoice::SwitchRoom) => {
                self.catalog
                    .open(self.address.clone(), self.access.token(), ui.ctx().clone())
            }
            Some(stage_ui::RoomChoice::Pause) => {
                self.layout.paused = !self.layout.paused;
                self.layout();
            }
            Some(stage_ui::RoomChoice::Reconnect) => self.restart_reception(),
            Some(stage_ui::RoomChoice::Update) => self.updates = true,
            Some(stage_ui::RoomChoice::Diagnostics) => self.debug = true,
            None => {}
        }
        if outcome.leave {
            if sharing || selecting || camera_active || camera_starting {
                self.confirm_leave = true;
            } else {
                self.leave_room(ui.ctx(), false);
            }
        }
    }

    fn room_video(
        &mut self,
        ui: &mut egui::Ui,
        room: &str,
        screens: &[Screen],
        sources: &[stage::Source<'_>],
        arrangement: &stage::Arrangement,
    ) {
        let ctx = ui.ctx().clone();
        let rect = ui.allocate_space(ui.available_size()).1;
        ui.painter().rect_filled(rect, 12.0, PANEL);
        let playback =
            arrangement.playback(sources, self.show, self.layout.mode, rect, self.anchor);
        if self.layout.playback.as_ref() != Some(&playback) {
            self.layout.playback = Some(playback);
            self.layout();
        }
        let video_for = |id: &str| screens.iter().find(|video| video.sid == id);
        let visible: Vec<_> = self
            .room_videos
            .arrival
            .iter()
            .filter_map(|id| video_for(id))
            .filter(|video| self.show.allows(video.camera))
            .collect();
        let empty = self.layout.paused || visible.is_empty();
        let show_controls = self
            .stage_overlay
            .controls_visible(&ctx, rect, empty, self.fullscreen);
        let mut selection = None;
        let mut tags = Vec::new();
        if empty {
            self.stage_overlay.reset_strip();
            let hidden = screens.len() - visible.len();
            if !self.layout.paused && hidden > 0 {
                if stage_ui::hidden_by_view(ui, rect, self.show, hidden) {
                    self.show = Show::All;
                    ctx.request_repaint();
                }
            } else if desktop_ui::playback_empty(
                ui,
                rect,
                self.layout.paused,
                self.snapshot.reconnecting,
                self.snapshot.reception_resetting,
                self.snapshot.can_publish,
            ) {
                self.layout.paused = false;
                self.layout();
            }
        } else if self.layout.mode == ViewMode::Grid {
            self.stage_overlay.reset_strip();
            for (index, video) in visible.iter().enumerate() {
                let card = desktop_ui::video_grid_card(rect, index, visible.len(), 10.0);
                let response = self.grid_card(ui, video, card);
                if response.double_clicked() {
                    self.fullscreen(&ctx, !self.fullscreen);
                } else if response.clicked() {
                    selection = Some(video.sid.clone());
                }
            }
        } else {
            let tiles = stage::stage_tiles(rect, arrangement.stage.len(), stage::GAP);
            for (id, tile) in arrangement.stage.iter().zip(tiles) {
                let Some(video) = video_for(id) else {
                    continue;
                };
                let response = stage_ui::video_click(ui, tile, id, &video.name, true);
                if response.double_clicked() {
                    self.fullscreen(&ctx, !self.fullscreen);
                } else if response.clicked() && arrangement.stage.len() > 1 {
                    selection = Some(id.clone());
                }
                self.draw_video(ui, id, tile);
                let mut tag = if self.fullscreen && arrangement.stage.len() == 1 {
                    format!("{room} · {}", video.name)
                } else {
                    video.name.clone()
                };
                if let Some(exhausted) = self.snapshot.recovering.get(id) {
                    tag.push_str(" · ");
                    tag.push_str(crate::reception_recovery::status_label(*exhausted));
                }
                tags.push((tile, tag));
            }
            let strip = self.stage_overlay.strip(
                ui,
                rect,
                arrangement.strip.len(),
                &mut self.anchor,
                show_controls,
            );
            for (id, slot) in arrangement.strip.iter().zip(&strip.slots) {
                let Some(video) = video_for(id) else {
                    continue;
                };
                let (canvas, response) =
                    stage_ui::strip_card(ui, *slot, id, &video.name, stage::LABEL, strip.dragging);
                self.draw_video(ui, id, canvas);
                if response.clicked() {
                    selection = Some(id.clone());
                }
            }
            if let Some(id) = stage_ui::strip_choice(ui, &strip, &arrangement.strip, |id| {
                video_for(id).map(|video| (video.name.clone(), video.camera))
            }) {
                selection = Some(id);
            }
        }
        if show_controls
            && self.share_draft.is_none()
            && !self.confirm_leave
            && !self.debug
            && !self.updates
            && !self.catalog.open
            && !self.creator.open
        {
            let entries: Vec<_> = visible
                .iter()
                .map(|video| stage_ui::StageEntry {
                    id: video.sid.clone(),
                    label: video.name.clone(),
                    on_stage: self.layout.mode == ViewMode::Spotlight
                        && arrangement.stage.contains(&video.sid),
                })
                .collect();
            let menu = stage_ui::ViewMenu {
                show: self.show,
                mode: self.layout.mode,
                entries: &entries,
            };
            let controls = stage_ui::StageControls {
                menu: &menu,
                side_by_side: self.layout.mode == ViewMode::Spotlight
                    && self.layout.focus.is_some()
                    && visible.iter().filter(|video| !video.camera).count() >= 2,
                fullscreen: self.fullscreen,
                can_reconnect: self.snapshot.connected && !self.snapshot.reception_resetting,
                can_pause: self.snapshot.connected && (!visible.is_empty() || self.layout.paused),
                paused: self.layout.paused,
            };
            let outcome = stage_ui::stage_controls(
                &ctx,
                rect,
                &tags,
                &controls,
                &mut self.volume,
                &mut self.sound_muted,
            );
            self.stage_overlay.set_controls(Some(&outcome));
            if outcome.audio_changed {
                self.layout();
            }
            if outcome.action.is_some() {
                ctx.request_repaint();
            }
            match outcome.action {
                Some(ControlAction::View(ViewChoice::Show(show))) => {
                    self.show = show;
                    self.layout.focus = None;
                }
                Some(ControlAction::View(ViewChoice::Mode(mode))) => self.layout.mode = mode,
                Some(ControlAction::View(ViewChoice::Stage(id))) => selection = Some(id),
                Some(ControlAction::SideBySide) => self.layout.focus = None,
                Some(ControlAction::Pause) => {
                    self.layout.paused = !self.layout.paused;
                    self.layout();
                }
                Some(ControlAction::Reconnect) => self.restart_reception(),
                Some(ControlAction::Fullscreen) => self.fullscreen(&ctx, !self.fullscreen),
                None => {}
            }
        } else {
            self.stage_overlay.set_controls(None);
        }
        if let Some(id) = selection {
            self.layout.focus = Some(id);
            self.layout.mode = ViewMode::Spotlight;
            ctx.request_repaint();
        }
    }
}
impl Drop for App {
    fn drop(&mut self) {
        self.save_preferences();
        #[cfg(not(test))]
        {
            let _ = crate::preferences::save_name(&self.name);
        }
    }
}
impl App {
    fn video_settings(&mut self, ui: &mut egui::Ui) {
        let software = self.encoder == Encoder::Software;
        if software {
            self.stream_settings.fps = 30;
        }
        crate::desktop_ui::stream_settings(
            ui,
            &mut self.stream_settings,
            true,
            if software { 30 } else { 120 },
        );
        crate::desktop_ui::upload_selector(ui, &mut self.upload, true);
        crate::desktop_ui::rate_mode_selector(ui, &mut self.rate_mode, true, true);
        ui.scope(|ui| {
            ui.columns(2, |columns| {
                for (column, (codec, label)) in columns
                    .iter_mut()
                    .zip([(Codec::Hevc, "HEVC / H.265"), (Codec::Av1, "AV1")])
                {
                    let available =
                        super::media::factory_available(codec, self.encoder).unwrap_or(true);
                    if crate::desktop_ui::action(column, label, available, self.codec == codec)
                        .clicked()
                    {
                        self.codec = codec;
                    }
                }
            });
            let hardware_label = "GPU · NVENC / VA-API";
            let software_label =
                crate::i18n::label("Compatível · HEVC 30 FPS", "Compatible · HEVC 30 FPS");
            crate::desktop_ui::select_menu(
                ui,
                "encoder",
                if software {
                    software_label
                } else {
                    hardware_label
                },
                |ui| {
                    crate::kit::choice(ui, &mut self.encoder, Encoder::Auto, hardware_label);
                    crate::kit::choice(ui, &mut self.encoder, Encoder::Software, software_label);
                },
            );
        });
        if self.encoder == Encoder::Software {
            self.codec = Codec::Hevc;
            self.stream_settings.fps = 30;
        }
        if super::media::factory_available(self.codec, self.encoder) == Some(false) {
            ui.label(RichText::new(crate::i18n::label(
                "Nenhum encoder disponível para este formato. Escolha outro formato ou instale o plugin/driver.",
                "No encoder is available for this format. Choose another format or install the plugin/driver.",
            )).color(MUTED).size(11.0));
        }
    }

    fn share_quality(&self) -> ShareQuality {
        (
            self.codec,
            self.encoder,
            self.stream_settings,
            self.upload,
            self.rate_mode,
        )
    }

    fn restore_share_quality(&mut self, quality: ShareQuality) {
        (
            self.codec,
            self.encoder,
            self.stream_settings,
            self.upload,
            self.rate_mode,
        ) = quality;
    }

    fn open_share_editor(&mut self, editing: bool) {
        self.share_draft = Some(ShareDraft {
            editing,
            audio: self.audio,
            source: (!editing).then_some(Source::Portal),
            tab: crate::desktop_ui::ShareTab::Source,
            quality: self.share_quality(),
        });
        self.debug = false;
        self.updates = false;
    }

    fn apply_share(&mut self, mut draft: ShareDraft) {
        if draft.editing && draft.source.is_none() && draft.quality == self.share_quality() {
            if self.audio != draft.audio {
                self.audio = draft.audio;
                self.send(Command::Audio(self.audio));
            }
            return;
        }
        let source = draft.source.unwrap_or(self.share_source);
        draft.source = Some(source);
        let (applied, result) = tokio::sync::oneshot::channel();
        self.send(Command::Share(
            source,
            self.codec,
            self.encoder,
            self.upload,
            Some(ShareOptions {
                settings: self.stream_settings,
                rate_mode: self.rate_mode,
                audio: draft.audio,
                applied,
            }),
        ));
        self.pending_share = Some((draft, result));
    }

    fn render(&mut self, root: &mut egui::Ui) {
        if let Some((draft, mut result)) = self.pending_share.take() {
            match result.try_recv() {
                Ok(()) => {
                    self.audio = draft.audio;
                    if let Some(source) = draft.source {
                        self.share_source = source;
                    }
                }
                Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {
                    self.pending_share = Some((draft, result))
                }
                Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                    self.restore_share_quality(draft.quality)
                }
            }
        }
        self.save_preferences();
        let context = root.ctx().clone();
        let ctx = &context;
        self.access.poll(&self.address, &self.device, ctx.clone());
        self.server_status.poll(&self.address, ctx);
        if self.departure.as_ref().is_some_and(|d| d.ready()) {
            self.departure = None;
            if std::mem::take(&mut self.pending_join) {
                self.connect(ctx);
            } else {
                self.status =
                    crate::desktop_ui::left_room_status(self.access.authenticated()).into();
            }
        }
        if let Some(room) = self.creator.poll() {
            self.saved_room = Some(room.room);
            self.invite.clear();
            self.leave_room(ctx, true);
        }
        if let Some(rx) = &self.joining {
            match rx.receiver.try_recv() {
                Ok(result) => {
                    self.joining = None;
                    match result {
                        Ok(joined) => {
                            self.room_title = joined
                                .title
                                .unwrap_or_else(|| crate::i18n::label("Sala", "Room").into());
                            let token = joined.token;
                            self.engine =
                                Some(Engine::start(self.address.clone(), token, ctx.clone()));
                            self.status.clear();
                            self.textures.clear();
                        }
                        Err(e) => self.status = e.to_string(),
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.joining = None;
                    self.status = crate::i18n::label(
                        "Conexão interrompida; tente novamente",
                        "Connection interrupted; try again",
                    )
                    .into();
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
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
        let State {
            connected,
            sharing,
            selecting,
            can_publish,
            reconnecting,
            reception_resetting: resetting,
            ..
        } = self.snapshot;
        let state_status = self.snapshot.status.clone();
        let mut screens = self.snapshot.screens.clone();
        if self.layout.self_preview && self.snapshot.camera_active {
            screens.push(Screen {
                sid: LOCAL_CAMERA.into(),
                owner: String::new(),
                name: label("Minha câmera", "My camera").into(),
                camera: true,
            });
        }
        self.textures
            .retain(|sid, _| screens.iter().any(|screen| &screen.sid == sid));
        if connected && !self.applied_layout {
            self.send(Command::RateMode(self.rate_mode));
            self.send(Command::StreamSettings(self.stream_settings));
            self.send(Command::Audio(self.audio));
            self.applied_layout = true;
        }
        if connected && !resetting && ctx.input(|i| i.key_pressed(egui::Key::F5)) {
            self.restart_reception();
        }
        if !self.confirm_leave
            && !self.confirm_server_logout
            && self.share_draft.is_none()
            && (self.fullscreen || connected && !screens.is_empty())
            && ctx.input(|i| i.key_pressed(egui::Key::F11))
        {
            self.fullscreen(ctx, !self.fullscreen);
        }
        if self.fullscreen && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.fullscreen(ctx, false);
        }
        if self.fullscreen
            && !ctx.egui_wants_keyboard_input()
            && ctx.input(|i| i.key_pressed(egui::Key::M) && i.modifiers.is_none())
        {
            desktop_ui::toggle_mute(&mut self.volume, &mut self.sound_muted);
            self.layout();
        }
        let width = ctx.content_rect().width();
        let room = if self.room_title.is_empty() {
            label("Sala", "Room").to_owned()
        } else {
            self.room_title.clone()
        };
        let sources: Vec<_> = screens
            .iter()
            .map(|screen| stage::Source {
                id: &screen.sid,
                owner: &screen.owner,
                camera: screen.camera,
                local: screen.sid == LOCAL_CAMERA,
            })
            .collect();
        if self.layout.focus.as_deref() == Some(LOCAL_CAMERA) && !self.layout.self_preview {
            self.layout.focus = None;
        }
        let arrangement = self
            .room_videos
            .arrange(&sources, self.show, &mut self.layout.focus);
        if (connected || reconnecting) && !self.fullscreen {
            self.room_sidebar(root, width, &room, &screens, &arrangement);
        }
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(BG)
                    .inner_margin(crate::desktop_ui::room_margin(
                        ctx.content_rect().width(),
                        self.fullscreen,
                    )),
            )
            .show(root, |ui| {
                if !connected && !reconnecting && !self.fullscreen {
                    let status = if self.status.is_empty() {
                        &state_status
                    } else {
                        &self.status
                    }
                    .clone();
                    crate::desktop_ui::home_panel(ui, &status, |ui| {
                        crate::desktop_ui::server_indicator(ui, &self.server_status);
                        if !connected
                            && self.departure.is_none()
                            && self.joining.is_none()
                            && self.engine.as_ref().is_some_and(|e| e.finished())
                            && (self.saved_room.is_some() || !self.invite.is_empty())
                        {
                            if button(
                                ui,
                                crate::i18n::label("Tentar novamente", "Try again"),
                                true,
                                true,
                            )
                            .clicked()
                            {
                                self.leave_room(ctx, true);
                            }
                        }
                        if !self.access.authenticated() {
                            ui.label(
                                RichText::new(crate::i18n::label(
                                    "ACESSO AO SERVIDOR",
                                    "SERVER ACCESS",
                                ))
                                .size(11.0)
                                .color(MUTED),
                            );
                            ui.collapsing(crate::i18n::label("Servidor", "Server"), |ui| {
                                input(ui, &mut self.address, "wss://servidor", false, 512);
                            });
                            let key_label = ui
                                .label(crate::i18n::label("Chave de acesso", "Access key"))
                                .id;
                            input(ui, &mut self.access.code, "", true, 64).labelled_by(key_label);
                            if button(
                                ui,
                                crate::i18n::label(
                                    "Continuar com a chave",
                                    "Continue with access key",
                                ),
                                !self.access.busy,
                                true,
                            )
                            .clicked()
                            {
                                self.access.start(
                                    self.address.clone(),
                                    self.device.clone(),
                                    ctx.clone(),
                                );
                            }
                            ui.label(RichText::new(&self.access.status).size(12.0).color(MUTED));
                        } else {
                            if input(
                                ui,
                                &mut self.name,
                                crate::i18n::label("Seu nome", "Your name"),
                                false,
                                32,
                            )
                            .lost_focus()
                            {
                                let _ = crate::preferences::save_name(&self.name);
                            }
                            if button(
                                ui,
                                crate::i18n::label("Entrar em uma sala", "Join a room"),
                                self.departure.is_none()
                                    && self.joining.is_none()
                                    && !self.creator.busy,
                                true,
                            )
                            .clicked()
                            {
                                self.creator.open = false;
                                self.catalog.open(
                                    self.address.clone(),
                                    self.access.token(),
                                    ctx.clone(),
                                );
                            }
                            if button(
                                ui,
                                crate::i18n::label("Criar sala", "Create room"),
                                self.departure.is_none()
                                    && self.joining.is_none()
                                    && !self.creator.busy,
                                false,
                            )
                            .clicked()
                            {
                                self.creator.open(&self.address);
                            }
                            ui.collapsing(
                                crate::i18n::label("Usar um convite", "Use an invitation"),
                                |ui| {
                                    let label = ui
                                        .label(crate::i18n::label(
                                            "Código do convite",
                                            "Invitation code",
                                        ))
                                        .id;
                                    input(ui, &mut self.invite, "", false, 256).labelled_by(label);
                                    if button(
                                        ui,
                                        crate::i18n::label(
                                            "Entrar com convite",
                                            "Join with invitation",
                                        ),
                                        !self.invite.trim().is_empty()
                                            && self.departure.is_none()
                                            && self.joining.is_none(),
                                        true,
                                    )
                                    .clicked()
                                    {
                                        self.saved_room = None;
                                        self.leave_room(ctx, true);
                                    }
                                },
                            );
                            if crate::desktop_ui::server_logout_button(ui, !self.access.busy)
                                .clicked()
                            {
                                self.confirm_server_logout = true;
                            }
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
                                self.updates = true;
                                ui.close();
                            }
                            if crate::desktop_ui::menu_action(
                                ui,
                                crate::i18n::label("Diagnóstico", "Diagnostics"),
                                true,
                            )
                            .clicked()
                            {
                                self.debug = true;
                                ui.close();
                            }
                        });
                    });
                    return;
                }

                if !self.fullscreen {
                    let detail = if resetting {
                        label(
                            "Reconectando vídeo recebido…",
                            "Reconnecting incoming video…",
                        )
                    } else if connected && !can_publish {
                        label(
                            "Nesta sala, você pode assistir.",
                            "You can watch in this room.",
                        )
                    } else if self.status.is_empty() {
                        &state_status
                    } else {
                        &self.status
                    };
                    let status = desktop_ui::room_activity(
                        sharing,
                        self.snapshot.camera_active,
                        detail,
                        desktop_ui::room_state(connected, reconnecting),
                    );
                    if stage_ui::stage_header(
                        ui,
                        &room,
                        &status,
                        (width >= stage_ui::RAIL_BELOW).then_some(self.sidebar_open),
                    ) {
                        self.sidebar_open = !self.sidebar_open;
                        ctx.request_repaint();
                    }
                }
                self.room_video(ui, &room, &screens, &sources, &arrangement);
            });
        if self.confirm_leave {
            match crate::desktop_ui::confirm_leave(ctx) {
                Some(true) => {
                    self.confirm_leave = false;
                    self.leave_room(ctx, false);
                }
                Some(false) => self.confirm_leave = false,
                None => {}
            }
        }
        if self.confirm_server_logout
            && let Some(confirmed) = crate::desktop_ui::confirm_server_logout(ctx)
        {
            self.confirm_server_logout = false;
            if confirmed {
                self.leave_room(ctx, false);
                self.access.logout(&self.address);
            }
        }
        if self.creator.open {
            let modal = crate::desktop_ui::scrolling_dialog(
                ctx,
                crate::i18n::label("Criar sala", "Create room"),
                "create_room_window",
                360.0,
                |ui| {
                    input(
                        ui,
                        &mut self.creator.title,
                        crate::i18n::label("Nome da sala", "Room name"),
                        false,
                        64,
                    );
                    ui.checkbox(
                        &mut self.creator.persistent,
                        crate::i18n::label("Manter sala indefinidamente", "Keep room indefinitely"),
                    );
                    if button(
                        ui,
                        crate::i18n::label("Criar sala", "Create room"),
                        !self.creator.busy,
                        true,
                    )
                    .clicked()
                    {
                        self.creator
                            .start(self.address.clone(), self.access.token(), ctx.clone());
                    }
                    ui.label(&self.creator.status);
                },
            );
            self.creator.open = !modal.should_close();
            if !self.creator.open {
                self.creator.cancel_automatic_join();
            }
        }
        if !self.fullscreen {
            if let Some(selected) = self.catalog.show(
                ctx,
                &self.address,
                self.access.token(),
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
                self.leave_room(ctx, true);
            }
        }
        if let Some(mut draft) = self.share_draft {
            use crate::desktop_ui::{
                ShareTab, dialog, dialog_body, share_confirmation, share_tabs,
            };
            let modal = dialog(
                ctx,
                if draft.editing {
                    crate::i18n::label("Editar compartilhamento", "Edit screen share")
                } else {
                    crate::i18n::label("Escolher o que compartilhar", "Choose what to share")
                },
                "choose_source",
                460.0,
                |ui| {
                    share_tabs(ui, &mut draft.tab);
                    dialog_body(ui, 172.0, |ui| {
                        if draft.tab == ShareTab::Quality {
                            self.video_settings(ui);
                        } else {
                            if draft.editing {
                                ui.radio_value(
                                    &mut draft.source,
                                    None,
                                    crate::i18n::label("Manter fonte atual", "Keep current source"),
                                );
                            }
                            ui.radio_value(
                                &mut draft.source,
                                Some(Source::Portal),
                                crate::i18n::label(
                                    "Escolher pelo sistema",
                                    "Choose with the system picker",
                                ),
                            );
                            if std::env::var_os("WAYLAND_DISPLAY").is_none() {
                                ui.radio_value(
                                    &mut draft.source,
                                    Some(Source::X11),
                                    crate::i18n::label("Tela inteira (X11)", "Entire screen (X11)"),
                                );
                            }
                            crate::desktop_ui::share_audio(ui, &mut draft.audio, false);
                        }
                    });
                    let quality_changed = draft.quality != self.share_quality();
                    let replace_capture =
                        !draft.editing || draft.source.is_some() || quality_changed;
                    let use_portal = replace_capture
                        && draft.source.unwrap_or(self.share_source) == Source::Portal;
                    if use_portal {
                        ui.small(crate::i18n::label(
                            "Confirme a tela ou janela no seletor do sistema.",
                            "Confirm the screen or window in the system picker.",
                        ));
                    }
                    let available = !replace_capture
                        || super::media::factory_available(self.codec, self.encoder)
                            .unwrap_or(true);
                    let label = if use_portal {
                        crate::i18n::label("Continuar no seletor", "Continue in picker")
                    } else if draft.editing {
                        crate::i18n::label("Aplicar alterações", "Apply changes")
                    } else {
                        crate::i18n::label("Compartilhar", "Share")
                    };
                    share_confirmation(
                        ui,
                        label,
                        connected
                            && can_publish
                            && sharing == draft.editing
                            && !selecting
                            && available,
                    )
                },
            );
            let (cancel, confirm) = modal.inner;
            if confirm {
                self.apply_share(draft);
            }
            if confirm || cancel || modal.should_close() || !connected || draft.editing != sharing {
                if !confirm {
                    self.restore_share_quality(draft.quality);
                }
                self.share_draft = None;
            } else {
                self.share_draft = Some(draft);
            }
        }
        if self.debug {
            let mut reset_preferences = false;
            let modal = crate::desktop_ui::scrolling_dialog(
                ctx,
                crate::i18n::label("Diagnóstico", "Diagnostics"),
                "debug_window",
                360.0,
                |ui| {
                    ui.label(format!("HyperCast {} · Linux", env!("CARGO_PKG_VERSION")));
                    ui.label(crate::i18n_format!(
                        "Decodificadores ativos: {}",
                        "Active decoders: {}",
                        super::media::decoder_count()
                    ));
                    ui.label(crate::i18n_format!(
                        "Quadros enviados: {} · recebidos: {}",
                        "Frames sent: {} · received: {}",
                        self.snapshot.sent,
                        self.snapshot.received
                    ));
                    if connected || reconnecting {
                        crate::desktop_ui::quality_strip(
                            ui,
                            &self.snapshot.quality,
                            reconnecting,
                            sharing,
                            self.layout.paused,
                        );
                    }
                    crate::desktop_ui::network_summary(ui, &self.snapshot.quality);
                    let idle = self.engine.as_ref().is_none_or(|e| e.finished())
                        && self.joining.is_none()
                        && self.departure.is_none()
                        && !self.pending_join
                        && !self.creator.busy;
                    reset_preferences =
                        crate::desktop_ui::preferences_controls(ui, &mut self.preferences, idle);

                    if crate::desktop_ui::action(
                        ui,
                        if self
                            .report_copied
                            .is_some_and(|at| at.elapsed() < Duration::from_secs(2))
                        {
                            crate::i18n::label("Relatório copiado", "Report copied")
                        } else {
                            crate::i18n::label("Copiar relatório técnico", "Copy technical report")
                        },
                        true,
                        false,
                    )
                    .clicked()
                    {
                        let s = &self.snapshot;
                        ctx.copy_text(crate::video_metrics::technical_report(
                            &s.quality,
                            crate::video_metrics::ReportState {
                                connected: s.connected,
                                reconnecting: s.reconnecting,
                                publishing: s.sharing,
                                sent: s.sent,
                                received: s.received,
                                restarts: s.reception_restarts,
                            },
                            self.stream_settings,
                            self.upload,
                        ));
                        self.report_copied = Some(Instant::now());
                        ctx.request_repaint_after(Duration::from_secs(2));
                    }
                    ui.label(crate::i18n::label(
                        "A chave de acesso fica somente na memória desta sessão.",
                        "The access key is kept only in memory for this session.",
                    ));
                    ui.label(crate::i18n::label("Renderização portátil com transferência para RAM; desempenho QHD 120 FPS ainda requer validação.","Portable rendering with RAM transfer; QHD 120 FPS performance still requires validation."));
                },
            );
            self.debug = !modal.should_close();
            if reset_preferences {
                let p = crate::preferences::Settings::default();
                self.stream_settings = p.stream;
                self.upload = p.upload;
                self.rate_mode = p.rate_mode;
                self.sidebar_open = p.sidebar;
                self.volume = p.volume;
                self.sound_muted = p.muted;
                self.layout.self_preview = p.self_preview;
                self.layout.mode = if p.grid {
                    ViewMode::Grid
                } else {
                    ViewMode::Spotlight
                };
                self.layout.focus = None;
                self.layout.paused = false;
                self.show = p.show;
                self.anchor = p.anchor;
                self.codec = Codec::Hevc;
                self.layout();
            }
        }
        if self.updates {
            let modal = crate::desktop_ui::update_dialog(ctx, |ui| {
                ui.label(crate::i18n_format!(
                    "Versão atual: {}",
                    "Current version: {}",
                    env!("CARGO_PKG_VERSION")
                ));
                ui.label(
                    RichText::new(crate::i18n::label(
                        "Linux · AppImage portátil",
                        "Linux · portable AppImage",
                    ))
                    .size(11.0)
                    .color(MUTED),
                );
                ui.label(crate::i18n::label("Confira a versão disponível na página de downloads. Para atualizar, substitua o AppImage.","Check the downloads page for the latest version. Replace the AppImage to update."));
                if crate::desktop_ui::action(
                    ui,
                    crate::i18n::label("Abrir página de downloads", "Open downloads page"),
                    true,
                    true,
                )
                .clicked()
                {
                    ctx.open_url(egui::OpenUrl::new_tab(
                        "https://github.com/LucasCamy/HyperCast/releases",
                    ));
                }
            });
            self.updates = !modal.should_close();
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
                    println!(
                        "Linux throughput: sent_fps={:.1} sent_mbps={:.3} received_fps={:.1}",
                        s.quality.send.map_or(0.0, |r| r.fps),
                        s.quality.send.map_or(0.0, |r| r.mbps),
                        s.quality.receive.map_or(0.0, |r| r.fps)
                    );
                    println!(
                        "GUI technical report: {}",
                        crate::video_metrics::technical_report(
                            &s.quality,
                            crate::video_metrics::ReportState {
                                connected: s.connected,
                                reconnecting: s.reconnecting,
                                publishing: s.sharing,
                                sent: s.sent,
                                received: s.received,
                                restarts: s.reception_restarts
                            },
                            self.stream_settings,
                            self.upload
                        )
                        .replace('\n', "")
                    );
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        let busy = self.access.busy
            || self.creator.busy
            || self.departure.is_some()
            || self.joining.is_some();
        ctx.request_repaint_after(Duration::from_millis(if busy { 100 } else { 500 }));
    }
}
impl eframe::App for App {
    fn ui(&mut self, root: &mut egui::Ui, _: &mut eframe::Frame) {
        self.render(root);
    }
}
pub fn run() -> anyhow::Result<()> {
    eprintln!(
        "HyperCast Linux: version={} pid={}",
        env!("CARGO_PKG_VERSION"),
        std::process::id()
    );
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
    use std::io::{Read, Write};

    #[test]
    fn camera_preview_uses_a_local_card_and_disappears_when_hidden_or_stopped() {
        use crate::camera_preview::{CameraPreview, LOCAL_CAMERA};
        use livekit::webrtc::video_frame::I420Buffer;

        let ctx = egui::Context::default();
        let mut app = App::new(&ctx);
        app.address = "ws://127.0.0.1:9".into();
        app.layout.self_preview = true;
        app.show = Show::All;
        let preview = CameraPreview::default();
        preview.set_visible(true);
        preview.capture(&mut I420Buffer::new(1280, 720), &ctx);
        let shared = std::sync::Arc::new(crate::resilience::StateMutex::new(State {
            connected: true,
            can_publish: true,
            camera_active: true,
            camera_preview: Some(preview),
            ..Default::default()
        }));
        app.engine = Some(Engine::fixture(shared.clone()));

        draw(&mut app, &ctx, 0);
        assert_eq!(app.textures[LOCAL_CAMERA].1.size(), [640, 360]);
        app.layout.focus = Some(LOCAL_CAMERA.into());
        app.layout.self_preview = false;
        draw(&mut app, &ctx, 1);
        assert!(!app.textures.contains_key(LOCAL_CAMERA));
        assert!(!app.layout.camera_preview_visible());
        assert!(app.layout.focus.is_none());
        assert!(shared.lock().unwrap().camera_active);

        app.layout.self_preview = true;
        draw(&mut app, &ctx, 2);
        assert!(app.textures.contains_key(LOCAL_CAMERA));
        shared.lock().unwrap().camera_active = false;
        draw(&mut app, &ctx, 3);
        assert!(!app.textures.contains_key(LOCAL_CAMERA));
    }

    fn draw(app: &mut App, ctx: &egui::Context, step: usize) {
        ctx.enable_accesskit();
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    Default::default(),
                    egui::vec2(960.0, 620.0),
                )),
                time: Some(step as f64 * 0.1),
                ..Default::default()
            },
            |ui| app.render(ui),
        );
        let painted = |needle: &str| {
            output.shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text().contains(needle))
        })
        };
        if app.snapshot.connected {
            assert!(
                output
                    .platform_output
                    .accesskit_update
                    .is_some_and(|update| update
                        .nodes
                        .iter()
                        .any(|(_, node)| node.label() == Some("Sair da sala"))),
                "Leave room must be available in the accessibility tree"
            );
        } else {
            assert!(painted("HyperCast"), "Home must remain painted");
            assert!(
                painted("Chave") || painted("Entrar em uma sala"),
                "Room access must remain painted"
            );
        }
    }
    #[test]
    #[ignore = "Requires an isolated server access key and two authorized test rooms"]
    fn room_switch_with_server() {
        gstreamer::init().unwrap();
        let ctx = egui::Context::default();
        let mut app = App::new(&ctx);
        app.address = std::env::var("HYPERCAST_CATALOG_TEST_URL").unwrap();
        app.device = uuid::Uuid::new_v4().simple().to_string();
        app.name = "LinuxSwitchTester".into();
        app.access.poll(&app.address, &app.device, ctx.clone());
        app.access.code = std::env::var("HYPERCAST_CATALOG_TEST_KEY").unwrap();
        app.access
            .start(app.address.clone(), app.device.clone(), ctx.clone());
        let mut step = 0;
        fn until(
            app: &mut App,
            ctx: &egui::Context,
            step: &mut usize,
            ready: impl Fn(&App) -> bool,
        ) {
            let deadline = Instant::now() + Duration::from_secs(35);
            while !ready(app) {
                assert!(Instant::now() < deadline, "Linux room transition timed out");
                draw(app, ctx, *step);
                *step += 1;
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        until(&mut app, &ctx, &mut step, |a| !a.access.busy);
        assert!(app.access.authenticated(), "Isolated test key was rejected");
        let first = std::env::var("HYPERCAST_CATALOG_TEST_FIRST").unwrap();
        let second = std::env::var("HYPERCAST_CATALOG_TEST_SECOND").unwrap();
        for (room, title) in [
            (&first, "Linux primeira"),
            (&second, "Linux segunda"),
            (&first, "Linux primeira"),
        ] {
            app.saved_room = Some(room.clone());
            app.leave_room(&ctx, true);
            assert!(app.engine.is_none());
            assert!(!app.snapshot.connected);
            until(&mut app, &ctx, &mut step, |a| a.snapshot.connected);
            assert_eq!(app.room_title, title);
        }
        app.leave_room(&ctx, false);
        until(&mut app, &ctx, &mut step, |a| a.departure.is_none());
        assert!(app.engine.is_none());
        assert!(!app.snapshot.connected);
        assert!(
            app.access.authenticated(),
            "Leaving a room cleared server access"
        );
        println!("PASS: Linux UI switched rooms twice and left while preserving server access");
    }
    #[test]
    fn occupied_media_state_does_not_block_the_ui() {
        let ctx = egui::Context::default();
        let mut app = App::new(&ctx);
        app.address = "ws://127.0.0.1:9".into();
        let shared = std::sync::Arc::new(crate::resilience::StateMutex::new(State::default()));
        app.engine = Some(Engine::fixture(shared.clone()));
        let (held, started) = mpsc::channel();
        let (release, wait) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let _guard = shared.lock().unwrap();
            held.send(()).unwrap();
            let _ = wait.recv_timeout(Duration::from_secs(2));
        });
        started.recv_timeout(Duration::from_secs(2)).unwrap();
        let before = Instant::now();
        draw(&mut app, &ctx, 0);
        let elapsed = before.elapsed();
        release.send(()).unwrap();
        worker.join().unwrap();
        assert!(
            elapsed < Duration::from_millis(500),
            "Media lock blocked the UI: {elapsed:?}"
        );
    }
    #[test]
    fn exit_wins_over_queued_room_changes() {
        let ctx = egui::Context::default();
        let mut app = App::new(&ctx);
        struct SlowDrop(mpsc::Receiver<()>);
        impl Drop for SlowDrop {
            fn drop(&mut self) {
                let _ = self.0.recv();
            }
        }
        let (release, wait) = mpsc::channel();
        app.departure = Some(crate::room_transition::Departure::start(
            Some(SlowDrop(wait)),
            ctx.clone(),
        ));
        app.pending_join = true;
        let started = Instant::now();
        app.leave_room(&ctx, true);
        app.leave_room(&ctx, true);
        app.leave_room(&ctx, false);
        assert!(started.elapsed() < Duration::from_millis(100));
        assert!(!app.pending_join);
        assert!(!app.departure.as_ref().unwrap().ready());
        release.send(()).unwrap();
        let until = Instant::now() + Duration::from_secs(2);
        while !app.departure.as_ref().unwrap().ready() {
            assert!(Instant::now() < until);
            std::thread::yield_now();
        }
    }
    #[test]
    fn delayed_server_rejection_keeps_painting_and_displays_the_error() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("ws://{}", listener.local_addr().unwrap());
        let (release, wait) = mpsc::channel();
        let (accepted, started) = mpsc::channel();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut data = [0; 4096];
            let _ = socket.read(&mut data);
            accepted.send(()).unwrap();
            wait.recv_timeout(Duration::from_secs(5)).unwrap();
            socket
                .write_all(
                    b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .unwrap();
        });
        let ctx = egui::Context::default();
        let mut app = App::new(&ctx);
        app.address = address;
        app.access.poll(&app.address, &app.device, ctx.clone());
        app.access.code = "A".repeat(32);
        app.access
            .start(app.address.clone(), app.device.clone(), ctx.clone());
        started.recv_timeout(Duration::from_secs(3)).unwrap();
        for step in 0..5 {
            draw(&mut app, &ctx, step);
            assert!(app.access.busy);
        }
        release.send(()).unwrap();
        server.join().unwrap();
        let until = Instant::now() + Duration::from_secs(3);
        let mut step = 5;
        while app.access.busy {
            assert!(Instant::now() < until);
            draw(&mut app, &ctx, step);
            step += 1;
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(app.access.status, "Chave inválida, expirada ou revogada");
        draw(&mut app, &ctx, step);
        app.creator.open = true;
        app.creator.status = "Erro de retorno do servidor ".repeat(40);
        app.status = "engine: connection error: wait_pc_connection timed out".into();
        for frame in step + 1..step + 6 {
            draw(&mut app, &ctx, frame);
        }
    }
}
