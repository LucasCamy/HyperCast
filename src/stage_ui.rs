//! Room stage controls: the view menu, the controls drawn over the video,
//! the strip's chips and the empty states.
use crate::desktop_ui::{
    BORDER, MUTED, PANEL, RoomIcon, TEXT, paint_stage_icon, room_icon_button_sized, session_button,
    speaker_toggle, volume_slider,
};
use crate::kit::{
    HEIGHT as HIT, Tone, focus_ring, keep_popup_above_video, menu_heading, menu_row, paint_frame,
    popup, popup_frame,
};
use eframe::egui::{self, Color32, RichText, Vec2};
use std::time::{Duration, Instant};

pub(crate) const RAIL_BELOW: f32 = 640.0;

pub(crate) fn sidebar(width: f32, rail: bool) -> egui::Panel {
    egui::Panel::left("room_sidebar")
        .exact_size(if rail {
            60.0
        } else {
            crate::desktop_ui::sidebar_width(width)
        })
        .resizable(false)
        .frame(
            egui::Frame::new()
                .fill(PANEL)
                .inner_margin(egui::Margin::same(if rail {
                    10
                } else {
                    crate::desktop_ui::room_margin(width, false)
                })),
        )
}

pub(crate) struct StageOverlay {
    position: Option<egui::Pos2>,
    velocity: Vec2,
    grab: Option<Vec2>,
    last_motion: Instant,
    last_pointer: Option<egui::Pos2>,
    controls: Vec<(egui::Rect, f32)>,
    bar: Option<egui::Rect>,
}

impl Default for StageOverlay {
    fn default() -> Self {
        Self {
            position: None,
            velocity: Vec2::ZERO,
            grab: None,
            last_motion: Instant::now(),
            last_pointer: None,
            controls: Vec::new(),
            bar: None,
        }
    }
}

impl StageOverlay {
    pub fn controls_visible(
        &mut self,
        ctx: &egui::Context,
        stage: egui::Rect,
        empty: bool,
        fullscreen: bool,
    ) -> bool {
        const LINGER: Duration = Duration::from_secs(2);
        let pointer = ctx.pointer_hover_pos();
        if pointer != self.last_pointer
            || ctx.input(|i| i.pointer.any_down() || !i.keys_down.is_empty())
        {
            self.last_motion = Instant::now();
            self.last_pointer = pointer;
        }
        let idle = self.last_motion.elapsed();
        let visible = empty
            || idle < LINGER
            || self.grab.is_some()
            || egui::Popup::is_any_open(ctx)
            || pointer.is_some_and(|p| self.controls.iter().any(|(rect, _)| rect.contains(p)));
        if idle < LINGER {
            ctx.request_repaint_after(LINGER - idle);
        }
        if !visible && fullscreen && pointer.is_some_and(|p| stage.contains(p)) {
            ctx.set_cursor_icon(egui::CursorIcon::None);
        }
        visible
    }

    pub fn set_controls(&mut self, outcome: Option<&ControlsOutcome>) {
        self.bar = outcome.map(|outcome| outcome.bar);
        self.controls.clear();
        if let Some(outcome) = outcome {
            self.controls.extend_from_slice(&outcome.rects);
        }
    }

    pub fn reset_strip(&mut self) {
        self.position = None;
        self.grab = None;
        self.velocity = Vec2::ZERO;
    }

    pub fn strip(
        &mut self,
        ui: &egui::Ui,
        stage: egui::Rect,
        count: usize,
        anchor: &mut crate::stage::Anchor,
        controls_visible: bool,
    ) -> Strip {
        use crate::stage::{self, GAP, MARGIN};
        if count == 0 {
            self.reset_strip();
            return Strip::default();
        }
        if stage::too_small_for_cards(stage) {
            self.reset_strip();
            let width = (stage.width() - 2.0 * MARGIN).clamp(0.0, 176.0);
            let bottom = self
                .bar
                .filter(|_| controls_visible)
                .map_or(stage.bottom(), |bar| bar.top());
            let pill = (width >= 70.0 && bottom - stage.top() >= 46.0).then(|| {
                egui::Rect::from_min_size(
                    egui::pos2(stage.right() - MARGIN - width, bottom - 46.0),
                    egui::vec2(width, 34.0),
                )
            });
            return Strip {
                pill,
                ..Default::default()
            };
        }

        let ctx = ui.ctx();
        let layout = stage::strip_layout(stage, count, *anchor);
        let size = layout.size();
        let resting = stage::anchor_origin(stage, size, *anchor, 0.0);
        let reserve = self
            .bar
            .filter(|bar| {
                controls_visible
                    && anchor.bottom()
                    && egui::Rect::from_min_size(resting, size).intersects(bar.expand(GAP))
            })
            .map_or(0.0, |bar| resting.y + size.y - (bar.top() - GAP));
        let target = stage::anchor_origin(stage, size, *anchor, reserve);
        let dt = ctx.input(|i| i.stable_dt).clamp(1.0 / 240.0, 0.1);
        let mut position = stage::clamp_origin(stage, size, self.position.unwrap_or(target));
        let handle = ui
            .interact(
                egui::Rect::from_min_size(position, size),
                ui.id().with("stage-strip"),
                egui::Sense::drag(),
            )
            .on_hover_cursor(egui::CursorIcon::Grab);
        if handle.drag_started()
            && let Some(start) = ctx.input(|i| i.pointer.press_origin())
        {
            self.grab = Some(start - position);
        }
        if let (true, Some(grab), Some(pointer)) =
            (handle.dragged(), self.grab, ctx.pointer_latest_pos())
        {
            let next = stage::clamp_origin(stage, size, pointer - grab);
            self.velocity = (next - position) / dt;
            position = next;
            ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
        }
        if handle.drag_stopped() {
            *anchor = stage::nearest_anchor(stage, position + size / 2.0);
            self.velocity *= 0.4;
            ctx.request_repaint();
        } else if !handle.dragged() {
            if stage::at_rest(position, self.velocity, target) {
                position = target;
                self.velocity = Vec2::ZERO;
            } else {
                (position, self.velocity) = stage::spring_step(position, self.velocity, target, dt);
                position = stage::clamp_origin(stage, size, position);
                ctx.request_repaint();
            }
        }
        if !handle.dragged() {
            self.grab = None;
        }
        self.position = Some(position);
        let mut slots = layout.slots_at(position);
        let overflow = if layout.overflow > 0 {
            slots.pop()
        } else {
            None
        };
        Strip {
            slots,
            overflow,
            pill: None,
            dragging: handle.dragged(),
        }
    }
}

#[derive(Default)]
pub(crate) struct Strip {
    pub slots: Vec<egui::Rect>,
    pub overflow: Option<egui::Rect>,
    pub pill: Option<egui::Rect>,
    pub dragging: bool,
}

impl Strip {
    #[cfg(windows)]
    pub fn cutouts(&self) -> Vec<(egui::Rect, f32)> {
        self.slots
            .iter()
            .map(|rect| (*rect, 8.0))
            .chain(
                self.overflow
                    .into_iter()
                    .chain(self.pill)
                    .map(|rect| (rect, rect.height().min(rect.width()) / 2.0)),
            )
            .collect()
    }
}

pub(crate) fn video_click(
    ui: &egui::Ui,
    rect: egui::Rect,
    id: &str,
    label: &str,
    on_stage: bool,
) -> egui::Response {
    let response = ui
        .interact(rect, ui.id().with(("video", id)), egui::Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            true,
            if on_stage {
                crate::i18n_format!("No palco: {}", "On stage: {}", label)
            } else {
                crate::i18n_format!("Colocar {} no palco", "Put {} on stage", label)
            },
        )
    });
    focus_ring(ui, &response, 8.0);
    response
}

/// One video the viewer can put on the main stage.
#[derive(Clone, Debug)]
pub(crate) struct StageEntry {
    pub id: String,
    pub label: String,
    pub on_stage: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ViewChoice {
    Show(crate::stage::Show),
    Mode(crate::subscription::ViewMode),
    Stage(String),
}

pub(crate) struct ViewMenu<'a> {
    pub show: crate::stage::Show,
    pub mode: crate::subscription::ViewMode,
    pub entries: &'a [StageEntry],
}

fn view_label(menu: &ViewMenu<'_>) -> String {
    if menu.mode == crate::subscription::ViewMode::Grid {
        crate::i18n_format!("Grade · {}", "Grid · {}", menu.show.label())
    } else {
        menu.show.label().to_owned()
    }
}

fn view_button(
    ui: &mut egui::Ui,
    menu: &ViewMenu<'_>,
    height: f32,
    icon_only: bool,
) -> egui::Response {
    let label = view_label(menu);
    let hint = crate::i18n_format!("Visualização · {}", "View · {}", label);
    if icon_only {
        return room_icon_button_sized(ui, RoomIcon::Stage, &hint, true, false, height);
    }
    crate::kit::dropdown_button(ui, &label, &hint, None, Some(&paint_stage_icon))
        .on_hover_text(hint)
}

/// The single view control used in the room header and in full screen.
pub(crate) fn view_menu(
    ui: &mut egui::Ui,
    menu: &ViewMenu<'_>,
    height: f32,
    icon_only: bool,
) -> (egui::Response, Option<ViewChoice>) {
    use crate::{i18n::label, stage::Show, subscription::ViewMode};
    let button = view_button(ui, menu, height, icon_only);
    let width = (ui.ctx().content_rect().width() - 24.0).clamp(160.0, 260.0);
    let choice = popup(ui, &button, width, |ui| {
        let mut choice = None;
        menu_heading(ui, label("Exibir", "Show"));
        for show in [Show::All, Show::Screens, Show::Cameras] {
            if menu_row(ui, show.label(), true, Some(menu.show == show)).clicked() {
                choice = Some(ViewChoice::Show(show));
                ui.close();
            }
        }
        ui.separator();
        menu_heading(ui, label("Organizar", "Arrange"));
        for (mode, text) in [
            (ViewMode::Spotlight, label("Palco", "Stage")),
            (ViewMode::Grid, label("Grade", "Grid")),
        ] {
            if menu_row(ui, text, true, Some(menu.mode == mode)).clicked() {
                choice = Some(ViewChoice::Mode(mode));
                ui.close();
            }
        }
        if menu.mode == ViewMode::Spotlight && !menu.entries.is_empty() {
            ui.separator();
            menu_heading(ui, label("No palco", "On stage"));
            for entry in menu.entries {
                if menu_row(ui, &entry.label, true, Some(entry.on_stage)).clicked() {
                    choice = Some(ViewChoice::Stage(entry.id.clone()));
                    ui.close();
                }
            }
        }
        choice
    })
    .flatten();
    (button, choice)
}

fn paint_exit_fullscreen(painter: &egui::Painter, center: egui::Pos2, stroke: egui::Stroke) {
    for x in [-1.0, 1.0] {
        for y in [-1.0, 1.0] {
            let corner = center + egui::vec2(x * 3.0, y * 3.0);
            painter.line_segment([corner, corner + egui::vec2(x * 5.0, 0.0)], stroke);
            painter.line_segment([corner, corner + egui::vec2(0.0, y * 5.0)], stroke);
        }
    }
}

fn group_separator(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(1.0, 20.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 0.0, BORDER);
}

fn exit_fullscreen_button(ui: &mut egui::Ui) -> egui::Response {
    crate::kit::icon_button(
        ui,
        crate::i18n::label(
            "Sair da tela cheia · Esc / F11",
            "Exit full screen · Esc / F11",
        ),
        true,
        false,
        HIT,
        paint_exit_fullscreen,
    )
}

fn volume_percent(ui: &mut egui::Ui, value: u32) {
    ui.add_sized(
        [40.0, HIT],
        egui::Label::new(RichText::new(format!("{value}%")).size(12.0).color(MUTED)),
    );
}

// ---------------------------------------------------------------------------
// Floating cards over the stage.

/// Shown where a video will appear until its first frame arrives.
pub(crate) fn video_loading(ui: &egui::Ui, area: egui::Rect) {
    if area.width() < 48.0 || area.height() < 24.0 {
        return;
    }
    ui.painter().text(
        area.center(),
        egui::Align2::CENTER_CENTER,
        crate::i18n::label("Carregando vídeo…", "Loading video…"),
        egui::FontId::proportional(13.0),
        MUTED,
    );
}

/// Empty stage because the viewer's filter hides every available video.
pub(crate) fn hidden_by_view(
    ui: &mut egui::Ui,
    area: egui::Rect,
    show: crate::stage::Show,
    hidden: usize,
) -> bool {
    use crate::stage::Show;
    if area.width() < 32.0 || area.height() < 32.0 {
        return false;
    }
    let (title, detail) = match show {
        Show::Cameras => (
            crate::i18n::label("Nenhuma câmera ligada", "No camera is on"),
            crate::i18n_format!("{} tela(s) oculta(s)", "{} screen(s) hidden", hidden),
        ),
        _ => (
            crate::i18n::label("Ninguém compartilhando tela", "No one is sharing a screen"),
            crate::i18n_format!("{} câmera(s) oculta(s)", "{} camera(s) hidden", hidden),
        ),
    };
    let mut reveal = false;
    ui.scope_builder(egui::UiBuilder::new().max_rect(area.shrink(8.0)), |ui| {
        ui.set_clip_rect(ui.clip_rect().intersect(area));
        ui.vertical_centered(|ui| {
            ui.add_space((area.height() / 2.0 - 56.0).max(0.0));
            ui.label(RichText::new(title).size(22.0).color(TEXT));
            ui.label(RichText::new(detail).size(14.0).color(MUTED));
            if area.height() >= 120.0 {
                ui.add_space(6.0);
                reveal = ui
                    .scope(|ui| {
                        ui.spacing_mut().interact_size = Vec2::new(220.0, 40.0);
                        session_button(
                            ui,
                            crate::i18n::label(
                                "Mostrar telas e câmeras",
                                "Show screens and cameras",
                            ),
                            true,
                            true,
                        )
                    })
                    .inner
                    .clicked();
            }
        });
    });
    reveal
}

fn chip_response(ui: &egui::Ui, rect: egui::Rect, id: egui::Id, label: &str) -> egui::Response {
    let response = ui
        .interact(rect, id, egui::Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    let radius = rect.height() / 2.0;
    paint_frame(ui, &response, Tone::Secondary, radius);
    focus_ring(ui, &response, radius);
    response
}

fn stage_picker(
    ui: &egui::Ui,
    response: &egui::Response,
    choices: &[(String, String)],
) -> Option<String> {
    popup(ui, response, 220.0, |ui| {
        let mut picked = None;
        menu_heading(ui, crate::i18n::label("Colocar no palco", "Put on stage"));
        for (id, label) in choices {
            if menu_row(ui, label, true, None).clicked() {
                picked = Some(id.clone());
                ui.close();
            }
        }
        picked
    })
    .flatten()
}

/// Shown instead of the strip when the stage is too small for cards: lists
/// the other videos to choose the one on the stage.
fn strip_pill(
    ui: &egui::Ui,
    rect: egui::Rect,
    text: &str,
    choices: &[(String, String)],
) -> Option<String> {
    let label = crate::i18n_format!(
        "{} · escolher para o palco",
        "{} · choose for the stage",
        text
    );
    let response = chip_response(ui, rect, egui::Id::new("strip-pill"), &label);
    let painter = ui.painter();
    let arrow = egui::pos2(rect.left() + 18.0, rect.center().y);
    let stroke = egui::Stroke::new(1.5, TEXT);
    painter.line_segment(
        [arrow + egui::vec2(-4.0, 2.0), arrow + egui::vec2(0.0, -2.0)],
        stroke,
    );
    painter.line_segment(
        [arrow + egui::vec2(0.0, -2.0), arrow + egui::vec2(4.0, 2.0)],
        stroke,
    );
    crate::desktop_ui::paint_video_label(
        ui,
        egui::Rect::from_min_max(
            egui::pos2(rect.left() + 30.0, rect.top()),
            egui::pos2(rect.right() - 12.0, rect.bottom()),
        ),
        text,
        13.0,
        TEXT,
    );
    let response = response.on_hover_text(&label);
    stage_picker(ui, &response, choices)
}

/// One strip card: the video above, the name bar below.
pub(crate) fn strip_card(
    ui: &egui::Ui,
    slot: egui::Rect,
    id: &str,
    label: &str,
    label_height: f32,
    dragging: bool,
) -> (egui::Rect, egui::Response) {
    let video = egui::Rect::from_min_max(
        slot.min + egui::vec2(2.0, 2.0),
        egui::pos2(slot.right() - 2.0, slot.bottom() - label_height),
    );
    let response = ui
        .interact(slot, ui.id().with(("strip-card", id)), egui::Sense::click())
        .on_hover_cursor(if dragging {
            egui::CursorIcon::Grabbing
        } else {
            egui::CursorIcon::Grab
        })
        .on_hover_text(crate::i18n::label(
            "Clique para pôr no palco · arraste para mover",
            "Click to put on stage · drag to move",
        ));
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            true,
            crate::i18n_format!("Colocar {} no palco", "Put {} on stage", label),
        )
    });
    let painter = ui.painter();
    painter.rect(
        slot,
        8.0,
        Color32::from_rgb(12, 16, 17),
        egui::Stroke::new(
            if response.hovered() || dragging {
                2.0
            } else {
                1.0
            },
            if response.hovered() || dragging {
                MUTED
            } else {
                BORDER
            },
        ),
        egui::StrokeKind::Inside,
    );
    let name = egui::Rect::from_min_max(
        egui::pos2(slot.left() + 8.0, slot.bottom() - label_height),
        egui::pos2(slot.right() - 8.0, slot.bottom()),
    );
    crate::desktop_ui::paint_video_label(ui, name, label, 12.0, TEXT);
    if response.has_focus() {
        painter.rect_stroke(
            slot.shrink(1.0),
            7.0,
            egui::Stroke::new(2.0, TEXT),
            egui::StrokeKind::Inside,
        );
    }
    (video, response)
}

/// "+N" next to the cards for videos that did not fit.
fn overflow_chip(ui: &egui::Ui, rect: egui::Rect, choices: &[(String, String)]) -> Option<String> {
    let text = format!("+{}", choices.len());
    let label = crate::i18n_format!("{} vídeo(s) a mais", "{} more video(s)", choices.len());
    let response = chip_response(ui, rect, egui::Id::new("cards-overflow"), &label);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        text,
        egui::FontId::proportional(13.0),
        TEXT,
    );
    let response = response.on_hover_text(label);
    stage_picker(ui, &response, choices)
}

pub(crate) fn strip_choice(
    ui: &egui::Ui,
    strip: &Strip,
    ids: &[String],
    video: impl Fn(&str) -> Option<(String, bool)>,
) -> Option<String> {
    let choices = |ids: &[String]| -> Vec<_> {
        ids.iter()
            .filter_map(|id| video(id).map(|(label, _)| (id.clone(), label)))
            .collect()
    };
    if let Some(chip) = strip.overflow {
        return overflow_chip(ui, chip, &choices(&ids[strip.slots.len()..]));
    }
    let pill = strip.pill?;
    let text = if ids
        .iter()
        .all(|id| video(id).is_some_and(|(_, camera)| camera))
    {
        crate::i18n_format!("{} câmera(s)", "{} camera(s)", ids.len())
    } else {
        crate::i18n_format!("{} vídeo(s)", "{} video(s)", ids.len())
    };
    strip_pill(ui, pill, &text, &choices(ids))
}

/// Room title, status and the participants toggle. View, audio and full
/// screen controls live over the video instead.
pub(crate) fn stage_header(
    ui: &mut egui::Ui,
    title: &str,
    status: &str,
    sidebar_open: Option<bool>,
) -> bool {
    use crate::i18n::label;
    let narrow = ui.available_width() < 340.0;
    ui.spacing_mut().item_spacing = Vec2::new(8.0, 4.0);
    let participants = ui
        .horizontal(|ui| {
            let participants = sidebar_open.is_some_and(|sidebar_open| {
                let toggle = if sidebar_open {
                    label("Ocultar participantes", "Hide participants")
                } else {
                    label("Mostrar participantes", "Show participants")
                };
                if narrow {
                    room_icon_button_sized(ui, RoomIcon::Panel, toggle, true, sidebar_open, HIT)
                        .clicked()
                } else {
                    crate::kit::labeled_button(
                        ui,
                        label("Participantes", "Participants"),
                        toggle,
                        if sidebar_open {
                            Tone::Selected
                        } else {
                            Tone::Secondary
                        },
                        true,
                        None,
                        HIT,
                    )
                    .on_hover_text(toggle)
                    .clicked()
                }
            });
            let size = if ui.available_width() < 360.0 {
                18.0
            } else {
                22.0
            };
            ui.add(egui::Label::new(RichText::new(title).size(size).strong()).truncate())
                .on_hover_text(title);
            participants
        })
        .inner;
    // Windows shows room state, errors and progress as notices instead.
    if !cfg!(windows) {
        ui.add(egui::Label::new(RichText::new(status).size(13.0).color(MUTED)).truncate())
            .on_hover_text(status);
    }
    ui.add_space(6.0);
    participants
}

/// What the controls over the video can do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ControlAction {
    View(ViewChoice),
    SideBySide,
    Pause,
    Reconnect,
    Fullscreen,
}

pub(crate) struct StageControls<'a> {
    pub menu: &'a ViewMenu<'a>,
    /// One chosen video while several screens could share the stage.
    pub side_by_side: bool,
    pub fullscreen: bool,
    pub can_reconnect: bool,
    pub can_pause: bool,
    pub paused: bool,
}

pub(crate) struct ControlsOutcome {
    pub action: Option<ControlAction>,
    pub audio_changed: bool,
    /// Everything drawn over the video with its corner radius, for hit tests
    /// and native cutouts.
    pub rects: Vec<(egui::Rect, f32)>,
    /// The bottom bar alone, so the strip can move above it.
    pub bar: egui::Rect,
}

fn seg_button(ui: &mut egui::Ui, text: &str, selected: bool) -> egui::Response {
    crate::kit::segment(ui, text, selected, None)
}

/// Small name tag over a stage tile.
fn name_tag(ctx: &egui::Context, id: egui::Id, tile: egui::Rect, text: &str) -> egui::Rect {
    egui::Area::new(id)
        .order(egui::Order::Foreground)
        .fixed_pos(tile.min + egui::vec2(10.0, 10.0))
        .constrain_to(tile)
        .interactable(false)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(PANEL)
                .stroke(egui::Stroke::new(1.0, BORDER))
                .corner_radius(crate::kit::RADIUS)
                .inner_margin(egui::Margin::symmetric(10, 5))
                .show(ui, |ui| {
                    ui.set_max_width((tile.width() - 40.0).clamp(20.0, 320.0));
                    ui.add(
                        egui::Label::new(RichText::new(text).size(13.0).color(TEXT))
                            .truncate()
                            .selectable(false),
                    );
                });
        })
        .response
        .rect
}

fn volume_popover(ui: &mut egui::Ui, volume: &mut u32, muted: &mut bool) -> bool {
    let silent = *muted || *volume == 0;
    let percent = if silent { 0 } else { *volume };
    let speaker = room_icon_button_sized(
        ui,
        RoomIcon::Speaker(silent),
        &crate::i18n_format!("Som recebido · {}%", "Incoming audio · {}%", percent),
        true,
        false,
        HIT,
    );
    let popup = egui::Popup::menu(&speaker)
        .frame(popup_frame(ui.style()))
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(8.0, 0.0);
                let toggled = speaker_toggle(ui, volume, muted, HIT).clicked();
                let changed = volume_slider(ui, volume, muted, 140.0, HIT);
                volume_percent(ui, if *muted { 0 } else { *volume });
                toggled || changed
            })
            .inner
        });
    popup.is_some_and(|popup| {
        keep_popup_above_video(ui.ctx(), popup.response.rect);
        popup.inner
    })
}

fn controls_row(
    ui: &mut egui::Ui,
    compact: bool,
    stacked: bool,
    controls: &StageControls<'_>,
    volume: &mut u32,
    muted: &mut bool,
) -> (Option<ControlAction>, bool) {
    use crate::{i18n::label, stage::Show, subscription::ViewMode};
    let mut action = None;
    ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
    if compact {
        if let (_, Some(choice)) = view_menu(ui, controls.menu, HIT, true) {
            action = Some(ControlAction::View(choice));
        }
    } else {
        for show in [Show::All, Show::Screens, Show::Cameras] {
            if seg_button(ui, show.label(), controls.menu.show == show).clicked() {
                action = Some(ControlAction::View(ViewChoice::Show(show)));
            }
        }
        group_separator(ui);
        for (mode, text) in [
            (ViewMode::Spotlight, label("Palco", "Stage")),
            (ViewMode::Grid, label("Grade", "Grid")),
        ] {
            if seg_button(ui, text, controls.menu.mode == mode).clicked() {
                action = Some(ControlAction::View(ViewChoice::Mode(mode)));
            }
        }
    }
    if controls.side_by_side {
        let side_by_side = label("Lado a lado", "Side by side");
        let response = if compact {
            room_icon_button_sized(ui, RoomIcon::Grid, side_by_side, true, false, HIT)
        } else {
            seg_button(ui, side_by_side, false)
        };
        if response.clicked() {
            action = Some(ControlAction::SideBySide);
        }
    }
    if !compact {
        group_separator(ui);
    }
    let audio_changed = if compact {
        volume_popover(ui, volume, muted)
    } else {
        let silent = *muted || *volume == 0;
        let percent = if silent { 0 } else { *volume };
        let toggled = speaker_toggle(ui, volume, muted, HIT).clicked();
        let changed = volume_slider(ui, volume, muted, 96.0, HIT);
        volume_percent(ui, percent);
        toggled || changed
    };
    if !compact {
        group_separator(ui);
    }
    if stacked {
        ui.end_row();
    }
    if room_icon_button_sized(
        ui,
        RoomIcon::Playback(controls.paused),
        if controls.paused {
            label("Retomar recepção", "Resume playback")
        } else {
            label("Pausar recepção", "Pause playback")
        },
        controls.can_pause,
        controls.paused,
        HIT,
    )
    .clicked()
    {
        action = Some(ControlAction::Pause);
    }
    if room_icon_button_sized(
        ui,
        RoomIcon::Reconnect,
        label("Reconectar recepção (F5)", "Reconnect playback (F5)"),
        controls.can_reconnect,
        false,
        HIT,
    )
    .clicked()
    {
        action = Some(ControlAction::Reconnect);
    }
    let fullscreen = if controls.fullscreen {
        exit_fullscreen_button(ui)
    } else {
        room_icon_button_sized(
            ui,
            RoomIcon::Fullscreen,
            label("Tela cheia (F11)", "Full screen (F11)"),
            true,
            false,
            HIT,
        )
    };
    if fullscreen.clicked() {
        action = Some(ControlAction::Fullscreen);
    }
    (action, audio_changed)
}

/// Controls drawn over the video: a name tag per stage tile and one bar at
/// the bottom center. Narrow stages fold the view choices into a menu and
/// the volume into a popover.
pub(crate) fn stage_controls(
    ctx: &egui::Context,
    stage: egui::Rect,
    tags: &[(egui::Rect, String)],
    controls: &StageControls<'_>,
    volume: &mut u32,
    muted: &mut bool,
) -> ControlsOutcome {
    let mut rects = Vec::new();
    for (index, (tile, text)) in tags.iter().enumerate() {
        rects.push((
            name_tag(ctx, egui::Id::new(("stage-tag", index)), *tile, text),
            8.0,
        ));
    }
    let compact = stage.width() < 760.0;
    let stacked = stage.width() < 276.0;
    let shown = egui::Area::new(egui::Id::new("stage-controls"))
        .order(egui::Order::Foreground)
        .pivot(egui::Align2::CENTER_BOTTOM)
        .fixed_pos(stage.center_bottom() - egui::vec2(0.0, 12.0))
        .constrain_to(stage)
        .show(ctx, |ui| {
            ui.set_max_width(stage.width());
            egui::Frame::new()
                .fill(PANEL)
                .stroke(egui::Stroke::new(1.0, crate::kit::OUTLINE))
                .corner_radius(crate::kit::PANEL_RADIUS)
                .inner_margin(egui::Margin::same(6))
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        controls_row(ui, compact, stacked, controls, volume, muted)
                    })
                    .inner
                })
                .inner
        });
    let bar = shown.response.rect;
    rects.push((bar, 12.0));
    let (action, audio_changed) = shown.inner;
    ControlsOutcome {
        action,
        audio_changed,
        rects,
        bar,
    }
}

#[cfg(test)]
#[test]
fn stage_controls_keep_separate_targets_inside_the_stage() {
    use crate::{stage::Show, subscription::ViewMode};
    for width in [1600.0, 900.0, 600.0, 420.0, 210.0, 155.0] {
        let ctx = egui::Context::default();
        let area = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 500.0));
        let entries = [StageEntry {
            id: "ana".into(),
            label: "Ana · Tela".into(),
            on_stage: true,
        }];
        let menu = ViewMenu {
            show: Show::All,
            mode: ViewMode::Spotlight,
            entries: &entries,
        };
        let controls = StageControls {
            menu: &menu,
            side_by_side: true,
            fullscreen: false,
            can_reconnect: true,
            can_pause: true,
            paused: false,
        };
        let mut outcome = None;
        // Areas take their size from the previous frame.
        for _ in 0..3 {
            let _ = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(area),
                    ..Default::default()
                },
                |root| {
                    let stage = root.max_rect();
                    outcome = Some(stage_controls(
                        root.ctx(),
                        stage,
                        &[(stage, "Sala · Ana · Tela".into())],
                        &controls,
                        &mut 70,
                        &mut false,
                    ));
                },
            );
        }
        let outcome = outcome.unwrap();
        assert!(
            area.contains_rect(outcome.bar),
            "{width}: {:?}",
            outcome.bar
        );
        assert!(outcome.bar.height() >= HIT, "{width}: {:?}", outcome.bar);
        assert!(
            (outcome.bar.center().x - area.center().x).abs() < 1.0,
            "{width}: {:?}",
            outcome.bar
        );
        let tag = outcome.rects[0].0;
        assert!(!tag.intersects(outcome.bar), "{width}: tag {tag:?}");
    }
}

/// Everything the viewer does as a participant, shown at the foot of the
/// participants panel (or as icons in the collapsed rail).
pub(crate) struct Me<'a> {
    pub name: &'a str,
    pub status: &'a str,
    pub room: &'a str,
    pub rail: bool,
    pub connected: bool,
    pub can_publish: bool,
    pub publishing: bool,
    pub selecting: bool,
    pub share_pending: bool,
    pub self_video: Option<&'a str>,
    pub camera_active: bool,
    pub camera_starting: bool,
    pub preview: bool,
    pub paused: bool,
    pub resetting: bool,
    pub can_switch_room: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RoomChoice {
    SwitchRoom,
    Pause,
    Reconnect,
    Update,
    Diagnostics,
}

#[derive(Default)]
pub(crate) struct MeOutcome {
    /// Share (or stop sharing) was pressed; the id restores focus later.
    pub share: Option<egui::Id>,
    pub gear: Option<egui::Id>,
    pub camera: bool,
    pub preview: bool,
    pub leave: bool,
    pub menu: Option<RoomChoice>,
    /// The avatar was pressed while a local preview is available.
    pub focus_self: bool,
}

const RAIL_ICON: f32 = HIT;

pub(crate) fn me_status(
    connected: bool,
    can_publish: bool,
    sharing: bool,
    camera: bool,
) -> &'static str {
    use crate::i18n::label;
    match (sharing, camera) {
        (true, true) => label("Transmitindo · câmera ligada", "Sharing · camera on"),
        (true, false) => label("Transmitindo sua tela", "Sharing your screen"),
        (false, true) => label("Câmera ligada", "Camera on"),
        _ if connected && !can_publish => label("Só assistindo", "Watching only"),
        _ => label("Assistindo", "Watching"),
    }
}

fn camera_label(me: &Me<'_>) -> &'static str {
    if me.camera_starting {
        crate::i18n::label("Abrindo câmera…", "Opening camera…")
    } else if me.camera_active {
        crate::i18n::label("Desligar câmera", "Turn off camera")
    } else {
        crate::i18n::label("Ligar câmera", "Turn on camera")
    }
}

fn avatar(
    ui: &mut egui::Ui,
    initial: char,
    size: f32,
    live: bool,
    focused: bool,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::click());
    let painter = ui.painter();
    let center = rect.center();
    let radius = size / 2.0 - 1.0;
    painter.circle_filled(
        center,
        radius,
        if focused {
            crate::kit::SELECTED
        } else {
            crate::desktop_ui::SURFACE
        },
    );
    painter.circle_stroke(
        center,
        radius,
        egui::Stroke::new(
            if focused { 2.0 } else { 1.0 },
            if focused {
                crate::desktop_ui::ACCENT
            } else {
                BORDER
            },
        ),
    );
    painter.text(
        center,
        egui::Align2::CENTER_CENTER,
        initial.to_uppercase().to_string(),
        egui::FontId::proportional(size * 0.42),
        if focused {
            crate::desktop_ui::ACCENT
        } else {
            TEXT
        },
    );
    if live {
        let dot = center + egui::vec2(radius * 0.72, radius * 0.72);
        painter.circle_filled(dot, 4.5, PANEL);
        painter.circle_filled(dot, 3.0, crate::desktop_ui::ACCENT);
    }
    focus_ring(ui, &response, size / 2.0);
    response
}

fn room_menu(ui: &egui::Ui, anchor: &egui::Response, me: &Me<'_>) -> Option<RoomChoice> {
    use crate::i18n::label;
    popup(ui, anchor, 240.0, |ui| {
        let mut choice = None;
        menu_heading(ui, label("Sala e interface", "Room and interface"));
        if !me.room.is_empty() {
            crate::desktop_ui::copy_room_name(ui, me.room);
        }
        if me.can_switch_room
            && menu_row(ui, label("Trocar de sala", "Switch room"), true, None).clicked()
        {
            choice = Some(RoomChoice::SwitchRoom);
        }
        let pause = if me.paused {
            label("Retomar recepção", "Resume playback")
        } else {
            label("Pausar recepção", "Pause playback")
        };
        if menu_row(ui, pause, me.connected, None).clicked() {
            choice = Some(RoomChoice::Pause);
        }
        if menu_row(
            ui,
            label("Reconectar recepção (F5)", "Reconnect playback (F5)"),
            me.connected && !me.resetting,
            None,
        )
        .clicked()
        {
            choice = Some(RoomChoice::Reconnect);
        }
        crate::desktop_ui::display_scale(ui);
        if menu_row(ui, label("Atualizar", "Update"), true, None).clicked() {
            choice = Some(RoomChoice::Update);
        }
        if menu_row(ui, label("Diagnóstico", "Diagnostics"), true, None).clicked() {
            choice = Some(RoomChoice::Diagnostics);
        }
        if choice.is_some() {
            ui.close();
        }
        choice
    })
    .flatten()
}

fn icon(
    ui: &mut egui::Ui,
    icon: RoomIcon,
    label: &str,
    enabled: bool,
    tone: Tone,
) -> egui::Response {
    crate::kit::icon_button_tone(ui, label, enabled, tone, HIT, |painter, center, stroke| {
        crate::desktop_ui::paint_room_icon(painter, center, stroke, icon)
    })
}

/// The "you" card (or rail) at the foot of the participants panel.
const ME_CARD_PADDING: i8 = 10;
const ME_ROW_GAP: f32 = 8.0;
const ME_RAIL_GAP: f32 = 6.0;

pub(crate) fn me_panel(ui: &mut egui::Ui, me: &Me<'_>) -> MeOutcome {
    const TOP_MARGIN: i8 = 8;

    let show_share = me.can_publish || me.publishing || me.selecting;
    let show_gear = me.publishing && !me.selecting;
    let show_camera = me.can_publish || me.camera_active;
    let show_preview = me.self_video.is_some();
    let desired = if me.rail {
        let buttons = 2 + [show_share, show_gear, show_camera, show_preview]
            .into_iter()
            .filter(|show| *show)
            .count();
        buttons as f32 * HIT + (buttons - 1) as f32 * ME_RAIL_GAP
    } else {
        let rows = 2 + usize::from(show_share);
        rows as f32 * HIT
            + (rows - 1) as f32 * ME_ROW_GAP
            + 2.0 * (f32::from(ME_CARD_PADDING) + 1.0)
    };
    let available = (ui.available_height() - 48.0).max(46.0);
    let height = (desired + f32::from(TOP_MARGIN)).min(available);
    egui::Panel::bottom("room_me")
        .exact_size(height)
        .frame(egui::Frame::new().inner_margin(egui::Margin {
            top: TOP_MARGIN,
            ..Default::default()
        }))
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, true])
                .max_height(ui.available_height())
                .show(ui, |ui| {
                    me_controls(ui, me, show_share, show_gear, show_camera, show_preview)
                })
                .inner
        })
        .inner
}

fn me_controls(
    ui: &mut egui::Ui,
    me: &Me<'_>,
    show_share: bool,
    show_gear: bool,
    show_camera: bool,
    show_preview: bool,
) -> MeOutcome {
    use crate::i18n::label;
    let mut out = MeOutcome::default();
    let share_label = if me.selecting {
        label("Cancelar seleção", "Cancel selection")
    } else if me.publishing {
        label("Parar compartilhamento", "Stop screen sharing")
    } else {
        label("Compartilhar tela", "Share screen")
    };
    let share_enabled =
        me.selecting || me.publishing || (me.connected && me.can_publish && !me.share_pending);
    let gear_enabled = me.connected && me.can_publish && !me.selecting && !me.share_pending;
    let destructive_share = me.publishing || me.selecting;
    let camera_enabled = me.connected && me.can_publish && !me.camera_starting;
    let preview_label = if me.preview {
        label("Ocultar minha prévia", "Hide my preview")
    } else {
        label("Mostrar minha prévia", "Show my preview")
    };
    let leave_label = label("Sair da sala", "Leave room");
    let more_label = label("Mais opções", "More options");
    if me.rail {
        ui.vertical_centered(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(0.0, ME_RAIL_GAP);
            if show_share {
                let share = icon(
                    ui,
                    if destructive_share {
                        RoomIcon::Stop
                    } else {
                        RoomIcon::Share
                    },
                    share_label,
                    share_enabled,
                    if destructive_share {
                        Tone::Danger
                    } else {
                        Tone::Primary
                    },
                );
                if share.clicked() {
                    out.share = Some(share.id);
                }
            }
            if show_gear {
                let gear = icon(
                    ui,
                    RoomIcon::Gear,
                    crate::desktop_ui::stream_options_title(),
                    gear_enabled,
                    Tone::Secondary,
                );
                if gear.clicked() {
                    out.gear = Some(gear.id);
                }
            }
            if show_camera {
                out.camera = icon(
                    ui,
                    RoomIcon::Camera(me.camera_active),
                    camera_label(me),
                    camera_enabled,
                    if me.camera_active {
                        Tone::Selected
                    } else {
                        Tone::Secondary
                    },
                )
                .clicked();
            }
            if show_preview {
                out.preview = icon(
                    ui,
                    RoomIcon::Preview(me.preview),
                    preview_label,
                    true,
                    Tone::Secondary,
                )
                .clicked();
            }
            let more = icon(ui, RoomIcon::More, more_label, true, Tone::Secondary);
            out.menu = room_menu(ui, &more, me);
            out.leave = icon(ui, RoomIcon::Leave, leave_label, true, Tone::Danger).clicked();
        });
        return out;
    }
    egui::Frame::new()
        .fill(crate::desktop_ui::SURFACE)
        .stroke(egui::Stroke::new(1.0, BORDER))
        .corner_radius(crate::kit::PANEL_RADIUS)
        .inner_margin(egui::Margin::same(ME_CARD_PADDING))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing = Vec2::new(6.0, ME_ROW_GAP);
            ui.horizontal(|ui| {
                let initial = me.name.chars().next().unwrap_or('V');
                let you = avatar(ui, initial, 36.0, me.publishing || me.camera_active, false);
                if let Some(video) = me.self_video.filter(|_| me.preview) {
                    let hint = crate::i18n_format!("Ver {} no palco", "Watch {} on stage", video);
                    you.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &hint)
                    });
                    out.focus_self = you.on_hover_text(hint).clicked();
                }
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 1.0;
                    let name = if me.name.is_empty() {
                        label("Você", "You").to_owned()
                    } else {
                        crate::i18n_format!("{} (você)", "{} (you)", me.name)
                    };
                    ui.add(egui::Label::new(RichText::new(name).size(14.0).color(TEXT)).truncate());
                    ui.add(
                        egui::Label::new(RichText::new(me.status).size(12.0).color(MUTED))
                            .truncate(),
                    )
                    .on_hover_text(me.status);
                });
            });
            if show_share {
                ui.horizontal(|ui| {
                    let gear_width = if show_gear {
                        HIT + ui.spacing().item_spacing.x
                    } else {
                        0.0
                    };
                    let width = (ui.available_width() - gear_width).max(HIT);
                    let share = crate::kit::labeled_button(
                        ui,
                        if show_gear {
                            label("Parar", "Stop")
                        } else {
                            share_label
                        },
                        share_label,
                        if destructive_share {
                            Tone::Danger
                        } else {
                            Tone::Primary
                        },
                        share_enabled,
                        Some(width),
                        HIT,
                    );
                    if share.clicked() {
                        out.share = Some(share.id);
                    }
                    if show_gear {
                        let gear = icon(
                            ui,
                            RoomIcon::Gear,
                            crate::desktop_ui::stream_options_title(),
                            gear_enabled,
                            Tone::Secondary,
                        );
                        if gear.clicked() {
                            out.gear = Some(gear.id);
                        }
                    }
                });
            }
            ui.horizontal(|ui| {
                if show_camera {
                    out.camera = icon(
                        ui,
                        RoomIcon::Camera(me.camera_active),
                        camera_label(me),
                        camera_enabled,
                        if me.camera_active {
                            Tone::Selected
                        } else {
                            Tone::Secondary
                        },
                    )
                    .clicked();
                }
                if show_preview {
                    out.preview = icon(
                        ui,
                        RoomIcon::Preview(me.preview),
                        preview_label,
                        true,
                        Tone::Secondary,
                    )
                    .clicked();
                }
                let more = icon(ui, RoomIcon::More, more_label, true, Tone::Secondary);
                out.menu = room_menu(ui, &more, me);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    out.leave =
                        icon(ui, RoomIcon::Leave, leave_label, true, Tone::Danger).clicked();
                });
            });
        });
    out
}

/// One participant in the collapsed rail.
#[derive(Clone, Copy)]
pub(crate) struct RailPerson<'a> {
    pub name: &'a str,
    pub status: &'a str,
    pub live: bool,
    pub focused: bool,
    /// `Some(true)` for a camera, `Some(false)` for a screen.
    pub camera: Option<bool>,
}

pub(crate) fn participant_status(live: Option<bool>, focused: Option<bool>) -> &'static str {
    use crate::i18n::label;
    match (live, focused) {
        (_, Some(true)) => label("Em destaque · câmera", "Focused · camera"),
        (_, Some(false)) => label("Em destaque · compartilhando", "Focused · sharing"),
        (Some(true), None) => label("Câmera ligada", "Camera on"),
        (Some(false), None) => label("Compartilhando tela", "Sharing screen"),
        (None, None) => label("Assistindo à sala", "Watching the room"),
    }
}

pub(crate) fn participant_card(ui: &mut egui::Ui, person: &RailPerson<'_>) -> egui::Response {
    let response = ui
        .add_enabled_ui(person.live, |ui| {
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 52.0), egui::Sense::click())
                .1
        })
        .inner;
    let radius = f32::from(crate::kit::RADIUS);
    paint_frame(
        ui,
        &response,
        if person.focused {
            Tone::Selected
        } else {
            Tone::Secondary
        },
        radius,
    );
    focus_ring(ui, &response, radius);
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            person.live,
            format!("{}: {}", person.name, person.status),
        )
    });
    let rect = response.rect;
    let painter = ui.painter();
    let center = rect.left_center() + egui::vec2(27.0, 0.0);
    let initial = person.name.chars().next().unwrap_or('P');
    let tint = if person.focused {
        crate::desktop_ui::ACCENT
    } else {
        MUTED
    };
    painter.circle_filled(center, 16.0, tint.gamma_multiply(0.14));
    painter.text(
        center,
        egui::Align2::CENTER_CENTER,
        initial.to_uppercase().to_string(),
        egui::FontId::proportional(15.0),
        tint,
    );
    if person.live {
        let dot = center + Vec2::splat(11.0);
        painter.circle_filled(dot, 5.0, PANEL);
        painter.circle_filled(dot, 3.0, crate::desktop_ui::ACCENT);
    }
    // What they share is a glyph on the right; the words stay in the tooltip.
    let glyph = person.camera.map(|camera| {
        if camera {
            RoomIcon::Camera(true)
        } else {
            RoomIcon::Share
        }
    });
    let right = rect.right() - if glyph.is_some() { 40.0 } else { 10.0 };
    let name = egui::Rect::from_min_max(
        egui::pos2(rect.left() + 52.0, rect.top()),
        egui::pos2(right, rect.bottom()),
    );
    crate::desktop_ui::paint_video_label(ui, name, person.name, 14.0, TEXT);
    if let Some(glyph) = glyph {
        crate::desktop_ui::paint_room_icon(
            ui.painter(),
            egui::pos2(rect.right() - 22.0, rect.center().y),
            egui::Stroke::new(1.5, tint),
            glyph,
        );
    }
    response.on_hover_text(format!("{}\n{}", person.name, person.status))
}

/// Participants as avatars in the collapsed panel. Returns the one pressed.
pub(crate) fn rail_people(ui: &mut egui::Ui, people: &[RailPerson<'_>]) -> Option<usize> {
    let mut chosen = None;
    ui.vertical_centered(|ui| {
        ui.add_space(4.0);
        crate::desktop_ui::brand_mark(ui, 28.0).on_hover_text("HyperCast");
        ui.add_space(6.0);
        ui.separator();
    });
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.vertical_centered(|ui| {
                // Room for the focus ring and live dot of the first avatar.
                ui.add_space(4.0);
                ui.spacing_mut().item_spacing = Vec2::new(0.0, 8.0);
                for (index, person) in people.iter().enumerate() {
                    let initial = person.name.chars().next().unwrap_or('P');
                    let response = avatar(ui, initial, RAIL_ICON, person.live, person.focused)
                        .on_hover_text(format!("{}\n{}", person.name, person.status));
                    response.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::WidgetType::Button,
                            true,
                            format!("{}: {}", person.name, person.status),
                        )
                    });
                    if response.clicked() {
                        chosen = Some(index);
                    }
                }
            });
        });
    chosen
}

#[cfg(test)]
#[test]
fn the_you_panel_fits_the_sidebar_and_the_rail() {
    for (width, rail) in [(272.0, false), (186.0, false), (40.0, true)] {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let area = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 600.0));
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(area),
                ..Default::default()
            },
            |root| {
                egui::CentralPanel::default()
                    .frame(egui::Frame::new())
                    .show(root, |ui| {
                        let me = Me {
                            name: "Ana",
                            status: "Transmitindo · câmera ligada",
                            room: "sala",
                            rail,
                            connected: true,
                            can_publish: true,
                            publishing: true,
                            selecting: false,
                            share_pending: false,
                            self_video: Some("Minha tela"),
                            camera_active: true,
                            camera_starting: false,
                            preview: true,
                            paused: false,
                            resetting: false,
                            can_switch_room: true,
                        };
                        let _ = me_panel(ui, &me);
                    });
            },
        );
        let update = output
            .platform_output
            .accesskit_update
            .expect("accessibility tree");
        let buttons: Vec<_> = update
            .nodes
            .iter()
            .filter(|(_, node)| node.role() == egui::accesskit::Role::Button)
            .filter_map(|(_, node)| node.bounds())
            .collect();
        assert_eq!(
            buttons.len(),
            if rail { 6 } else { 7 },
            "{width}: {buttons:?}"
        );
        for (i, b) in buttons.iter().enumerate() {
            assert!(b.width() >= 35.9 && b.height() >= 35.9, "{width}: {b:?}");
            assert!(
                b.x0 >= 0.0 && b.x1 <= f64::from(width) + 0.01,
                "{width}: {b:?}"
            );
            assert!(
                b.y0 >= 0.0 && b.y1 <= f64::from(area.bottom()) + 0.01,
                "{width}: {b:?}"
            );
            for other in &buttons[i + 1..] {
                let apart = b.x1 <= other.x0 + 0.01
                    || other.x1 <= b.x0 + 0.01
                    || b.y1 <= other.y0 + 0.01
                    || other.y1 <= b.y0 + 0.01;
                assert!(apart, "{width}: {b:?} overlaps {other:?}");
            }
        }
    }
}
