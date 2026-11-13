//! Windows stage controls: the view menu, the controls drawn over the video,
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
pub(crate) fn strip_pill(
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
    label: &str,
    label_height: f32,
    dragging: bool,
) -> (egui::Rect, egui::Response) {
    let video = egui::Rect::from_min_max(
        slot.min + egui::vec2(2.0, 2.0),
        egui::pos2(slot.right() - 2.0, slot.bottom() - label_height),
    );
    let response = ui
        .interact(
            slot,
            egui::Id::new(("strip-card", label)),
            egui::Sense::click(),
        )
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
pub(crate) fn overflow_chip(
    ui: &egui::Ui,
    rect: egui::Rect,
    choices: &[(String, String)],
) -> Option<String> {
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

/// Room title, status and the participants toggle. View, audio and full
/// screen controls live over the video instead.
pub(crate) fn stage_header(
    ui: &mut egui::Ui,
    title: &str,
    status: &str,
    sidebar_open: bool,
) -> egui::Response {
    use crate::i18n::label;
    let narrow = ui.available_width() < 340.0;
    ui.spacing_mut().item_spacing = Vec2::new(8.0, 4.0);
    let participants = ui
        .horizontal(|ui| {
            let toggle = if sidebar_open {
                label("Ocultar participantes", "Hide participants")
            } else {
                label("Mostrar participantes", "Show participants")
            };
            let participants = if narrow {
                room_icon_button_sized(ui, RoomIcon::Panel, toggle, true, sidebar_open, HIT)
            } else {
                crate::kit::segment(
                    ui,
                    label("Participantes", "Participants"),
                    sidebar_open,
                    None,
                )
                .on_hover_text(toggle)
            };
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
    ui.add(egui::Label::new(RichText::new(status).size(13.0).color(MUTED)).truncate())
        .on_hover_text(status);
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
fn name_tag(ctx: &egui::Context, id: egui::Id, at: egui::Pos2, text: &str) -> egui::Rect {
    egui::Area::new(id)
        .order(egui::Order::Foreground)
        .fixed_pos(at)
        .interactable(false)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(PANEL)
                .stroke(egui::Stroke::new(1.0, BORDER))
                .corner_radius(crate::kit::RADIUS)
                .inner_margin(egui::Margin::symmetric(10, 5))
                .show(ui, |ui| {
                    ui.set_max_width(320.0);
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
    controls: &StageControls<'_>,
    volume: &mut u32,
    muted: &mut bool,
) -> (Option<ControlAction>, bool) {
    use crate::{i18n::label, stage::Show, subscription::ViewMode};
    let mut action = None;
    ui.spacing_mut().item_spacing = egui::vec2(6.0, 0.0);
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
    if controls.side_by_side
        && seg_button(ui, label("Lado a lado", "Side by side"), false).clicked()
    {
        action = Some(ControlAction::SideBySide);
    }
    group_separator(ui);
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
    group_separator(ui);
    if room_icon_button_sized(
        ui,
        RoomIcon::Playback(controls.paused),
        if controls.paused {
            label("Retomar recepção", "Resume playback")
        } else {
            label("Pausar recepção", "Pause playback")
        },
        controls.can_reconnect,
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
            name_tag(
                ctx,
                egui::Id::new(("stage-tag", index)),
                tile.min + egui::vec2(10.0, 10.0),
                text,
            ),
            8.0,
        ));
    }
    let compact = stage.width() < 760.0;
    let shown = egui::Area::new(egui::Id::new("stage-controls"))
        .order(egui::Order::Foreground)
        .pivot(egui::Align2::CENTER_BOTTOM)
        .fixed_pos(stage.center_bottom() - egui::vec2(0.0, 12.0))
        .constrain_to(stage)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(PANEL)
                .stroke(egui::Stroke::new(1.0, crate::kit::OUTLINE))
                .corner_radius(crate::kit::PANEL_RADIUS)
                .inner_margin(egui::Margin::same(6))
                .show(ui, |ui| {
                    ui.horizontal(|ui| controls_row(ui, compact, controls, volume, muted))
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
    for width in [1600.0, 900.0, 600.0, 420.0] {
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
    /// The avatar was pressed while sharing: watch your own screen.
    pub focus_self: bool,
}

const RAIL_ICON: f32 = HIT;

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
pub(crate) fn me_panel(ui: &mut egui::Ui, me: &Me<'_>) -> MeOutcome {
    use crate::i18n::label;
    let mut out = MeOutcome::default();
    let share_label = if me.publishing {
        label("Parar compartilhamento", "Stop screen sharing")
    } else {
        label("Compartilhar tela", "Share screen")
    };
    let share_enabled = me.connected && me.can_publish;
    let camera_enabled = me.connected && me.can_publish && !me.camera_starting;
    let preview_label = if me.preview {
        label("Ocultar minha prévia", "Hide my preview")
    } else {
        label("Mostrar minha prévia", "Show my preview")
    };
    let show_preview = me.publishing || me.camera_active;
    let leave_label = label("Sair da sala", "Leave room");
    let more_label = label("Mais opções", "More options");
    if me.rail {
        ui.vertical_centered(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(0.0, 6.0);
            if me.can_publish || me.publishing {
                let share = icon(
                    ui,
                    if me.publishing {
                        RoomIcon::Stop
                    } else {
                        RoomIcon::Share
                    },
                    share_label,
                    share_enabled || me.publishing,
                    if me.publishing {
                        Tone::Danger
                    } else {
                        Tone::Primary
                    },
                );
                if share.clicked() {
                    out.share = Some(share.id);
                }
            }
            if me.publishing {
                let gear = icon(
                    ui,
                    RoomIcon::Gear,
                    crate::desktop_ui::stream_options_title(),
                    true,
                    Tone::Secondary,
                );
                if gear.clicked() {
                    out.gear = Some(gear.id);
                }
            }
            if me.can_publish || me.camera_active {
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
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing = Vec2::new(6.0, 8.0);
            ui.horizontal(|ui| {
                let initial = me.name.chars().next().unwrap_or('V');
                let you = avatar(ui, initial, 36.0, me.publishing, false)
                    .on_hover_text(label("Ver minha tela no palco", "Watch my screen on stage"));
                if me.publishing && you.clicked() {
                    out.focus_self = true;
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
            if me.can_publish || me.publishing {
                ui.horizontal(|ui| {
                    let gear_width = if me.publishing {
                        HIT + ui.spacing().item_spacing.x
                    } else {
                        0.0
                    };
                    let width = (ui.available_width() - gear_width).max(HIT);
                    let share = crate::kit::labeled_button(
                        ui,
                        if me.publishing {
                            label("Parar", "Stop")
                        } else {
                            share_label
                        },
                        share_label,
                        if me.publishing {
                            Tone::Danger
                        } else {
                            Tone::Primary
                        },
                        share_enabled || me.publishing,
                        Some(width),
                        HIT,
                    );
                    if share.clicked() {
                        out.share = Some(share.id);
                    }
                    if me.publishing {
                        let gear = icon(
                            ui,
                            RoomIcon::Gear,
                            crate::desktop_ui::stream_options_title(),
                            true,
                            Tone::Secondary,
                        );
                        if gear.clicked() {
                            out.gear = Some(gear.id);
                        }
                    }
                });
            }
            ui.horizontal(|ui| {
                if me.can_publish || me.camera_active {
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
pub(crate) struct RailPerson<'a> {
    pub name: &'a str,
    pub status: &'a str,
    pub live: bool,
    pub focused: bool,
}

/// Participants as avatars in the collapsed panel. Returns the one pressed.
pub(crate) fn rail_people(ui: &mut egui::Ui, people: &[RailPerson<'_>]) -> Option<usize> {
    let mut chosen = None;
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.vertical_centered(|ui| {
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
        // Stop, settings, camera, preview, more options and leave.
        assert_eq!(buttons.len(), 6, "{width}: {buttons:?}");
        for (i, b) in buttons.iter().enumerate() {
            assert!(b.width() >= 35.9 && b.height() >= 35.9, "{width}: {b:?}");
            assert!(
                b.x0 >= 0.0 && b.x1 <= f64::from(width) + 0.01,
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
