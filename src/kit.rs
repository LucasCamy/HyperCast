//! One set of controls for every screen: text, icon and segmented buttons,
//! dropdowns, menu rows and the popup frame. Sizes and corners live here so
//! screens stop inventing their own.
use crate::desktop_ui::{ACCENT, BORDER, PANEL, SURFACE, TEXT};
use eframe::egui::{self, Color32, RichText, Vec2};

/// Standard control height and the larger one for the home screen's actions.
pub const HEIGHT: f32 = 36.0;
pub const LARGE: f32 = 44.0;
/// Inline secondary actions ("Atualizar", "Cancelar" in a list).
pub const SMALL: f32 = 30.0;
/// Rows in menus and dropdowns.
pub const ROW: f32 = 34.0;
/// Buttons and cards; menus, dialogs and panels use `PANEL_RADIUS`.
pub const RADIUS: u8 = 8;
pub const PANEL_RADIUS: u8 = 12;
pub const TEXT_SIZE: f32 = 14.0;
const PADDING: f32 = 14.0;

pub const HOVER: Color32 = Color32::from_rgb(39, 48, 44);
pub const SELECTED: Color32 = Color32::from_rgb(40, 51, 40);
pub const OUTLINE: Color32 = Color32::from_rgb(74, 86, 82);
const PRIMARY_HOVER: Color32 = Color32::from_rgb(208, 245, 150);
const DANGER_TEXT: Color32 = Color32::from_rgb(255, 178, 170);
const DANGER_FILL: Color32 = Color32::from_rgb(59, 34, 34);
const DANGER_HOVER: Color32 = Color32::from_rgb(78, 42, 42);
const DANGER_BORDER: Color32 = Color32::from_rgb(96, 52, 52);

/// The role of a button. Every screen picks one of these.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    /// The one main action of a view.
    Primary,
    /// Everything else.
    Secondary,
    /// Leaving, stopping or signing out.
    Danger,
    /// An option that is currently on (segments, toggles, chosen cards).
    Selected,
}
impl Tone {
    pub fn text(self) -> Color32 {
        match self {
            Self::Primary => PANEL,
            Self::Secondary => TEXT,
            Self::Danger => DANGER_TEXT,
            Self::Selected => ACCENT,
        }
    }
    fn fill(self, hovered: bool) -> Color32 {
        match (self, hovered) {
            (Self::Primary, false) => ACCENT,
            (Self::Primary, true) => PRIMARY_HOVER,
            (Self::Secondary, false) => SURFACE,
            (Self::Secondary, true) => HOVER,
            (Self::Danger, false) => DANGER_FILL,
            (Self::Danger, true) => DANGER_HOVER,
            (Self::Selected, _) => SELECTED,
        }
    }
    fn border(self, hovered: bool) -> Color32 {
        match self {
            Self::Primary => ACCENT,
            Self::Secondary if hovered => OUTLINE,
            Self::Secondary => BORDER,
            Self::Danger => DANGER_BORDER,
            Self::Selected => ACCENT.gamma_multiply(0.5),
        }
    }
}

/// Paints a button background for `tone` in the state of `response`.
pub fn paint_frame(ui: &egui::Ui, response: &egui::Response, tone: Tone, radius: f32) {
    let hovered =
        response.enabled() && (response.hovered() || response.is_pointer_button_down_on());
    let mut fill = tone.fill(hovered);
    let mut border = tone.border(hovered);
    if !response.enabled() {
        fill = fill.gamma_multiply(0.55);
        border = border.gamma_multiply(0.55);
    }
    ui.painter().rect(
        response.rect,
        radius,
        fill,
        egui::Stroke::new(1.0, border),
        egui::StrokeKind::Inside,
    );
}

/// The keyboard focus ring shared by every control.
pub fn focus_ring(ui: &egui::Ui, response: &egui::Response, radius: f32) {
    if response.has_focus() {
        ui.painter().rect_stroke(
            response.rect.expand(2.0),
            radius + 2.0,
            egui::Stroke::new(2.0, TEXT),
            egui::StrokeKind::Outside,
        );
    }
}

fn text_galley(
    ui: &egui::Ui,
    text: &str,
    color: Color32,
    size: f32,
    max_width: f32,
) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple_singleline(
        text.to_owned(),
        egui::FontId::proportional(size),
        color,
    );
    job.wrap.max_width = max_width;
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = false;
    job.wrap.overflow_character = Some('…');
    ui.painter().layout_job(job)
}

/// A text button. `width` is `None` to fit the text, or a fixed width that
/// truncates long labels (the full label goes to the tooltip).
pub fn button_with(
    ui: &mut egui::Ui,
    text: &str,
    tone: Tone,
    enabled: bool,
    width: Option<f32>,
    height: f32,
) -> egui::Response {
    let enabled = enabled && ui.is_enabled();
    let color = tone.text();
    let (size, padding) = if height < HEIGHT {
        (13.0, 10.0)
    } else {
        (TEXT_SIZE, PADDING)
    };
    let natural = text_galley(ui, text, color, size, f32::INFINITY);
    let width = width.unwrap_or(natural.size().x + padding * 2.0);
    let galley = text_galley(ui, text, color, size, (width - padding * 2.0).max(8.0));
    let elided = galley.elided;
    let (rect, response) = ui
        .add_enabled_ui(enabled, |ui| {
            ui.allocate_exact_size(Vec2::new(width, height), egui::Sense::click())
        })
        .inner;
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, text));
    if ui.is_rect_visible(rect) {
        paint_frame(ui, &response, tone, f32::from(RADIUS));
        let color = if enabled {
            color
        } else {
            color.gamma_multiply(0.55)
        };
        let position = rect.center() - galley.size() / 2.0;
        ui.painter().galley(position, galley, color);
        focus_ring(ui, &response, f32::from(RADIUS));
    }
    if enabled && response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if elided {
        response.on_hover_text(text)
    } else {
        response
    }
}

/// Small inline action, sized to its label.
pub fn small(ui: &mut egui::Ui, text: &str, enabled: bool) -> egui::Response {
    button_with(ui, text, Tone::Secondary, enabled, None, SMALL)
}

/// A segment of a choice row: selected or not, same size as a button.
pub fn segment(
    ui: &mut egui::Ui,
    text: &str,
    selected: bool,
    width: Option<f32>,
) -> egui::Response {
    let tone = if selected {
        Tone::Selected
    } else {
        Tone::Secondary
    };
    let response = button_with(ui, text, tone, true, width, HEIGHT);
    response
        .widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, selected, text));
    response
}

/// Equal-width segments filling the row. Returns the chosen index.
pub fn segments(ui: &mut egui::Ui, labels: &[&str], selected: usize) -> Option<usize> {
    let gap = ui.spacing().item_spacing.x;
    let count = labels.len().max(1) as f32;
    let width = ((ui.available_width() - gap * (count - 1.0)) / count).max(HEIGHT);
    ui.horizontal(|ui| {
        let mut chosen = None;
        for (index, label) in labels.iter().enumerate() {
            if segment(ui, label, index == selected, Some(width)).clicked() {
                chosen = Some(index);
            }
        }
        chosen
    })
    .inner
}

/// A square icon button. `paint` draws the icon centred in the given rect.
pub fn icon_button(
    ui: &mut egui::Ui,
    label: &str,
    enabled: bool,
    selected: bool,
    size: f32,
    paint: impl FnOnce(&egui::Painter, egui::Pos2, egui::Stroke),
) -> egui::Response {
    let enabled = enabled && ui.is_enabled();
    let (rect, response) = ui
        .add_enabled_ui(enabled, |ui| {
            ui.allocate_exact_size(Vec2::splat(size), egui::Sense::click())
        })
        .inner;
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, enabled, selected, label)
    });
    if ui.is_rect_visible(rect) {
        let tone = if selected {
            Tone::Selected
        } else {
            Tone::Secondary
        };
        paint_frame(ui, &response, tone, f32::from(RADIUS));
        let color = if !enabled {
            Tone::Secondary.text().gamma_multiply(0.45)
        } else {
            tone.text()
        };
        paint(ui.painter(), rect.center(), egui::Stroke::new(1.5, color));
        focus_ring(ui, &response, f32::from(RADIUS));
    }
    if enabled && response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response.on_hover_text(label)
}

/// Frame for menus, dropdown lists and popovers.
pub fn popup_frame(style: &egui::Style) -> egui::Frame {
    egui::Frame::window(style)
        .fill(PANEL)
        .stroke(egui::Stroke::new(1.0, OUTLINE))
        .corner_radius(PANEL_RADIUS)
        .inner_margin(8)
}

/// Section title inside a menu.
pub fn menu_heading(ui: &mut egui::Ui, text: &str) {
    ui.add_space(4.0);
    ui.label(
        RichText::new(text)
            .size(11.0)
            .strong()
            .color(crate::desktop_ui::MUTED),
    );
}

/// A menu row. `selected` adds a check mark for choices; `None` is a plain
/// action.
pub fn menu_row(
    ui: &mut egui::Ui,
    text: &str,
    enabled: bool,
    selected: Option<bool>,
) -> egui::Response {
    let enabled = enabled && ui.is_enabled();
    let width = ui.available_width().max(80.0);
    let on = selected == Some(true);
    let color = if on { ACCENT } else { TEXT };
    let galley = text_galley(ui, text, color, TEXT_SIZE, width - 20.0 - 24.0);
    let (rect, response) = ui
        .add_enabled_ui(enabled, |ui| {
            ui.allocate_exact_size(Vec2::new(width, ROW), egui::Sense::click())
        })
        .inner;
    response.widget_info(|| match selected {
        Some(on) => egui::WidgetInfo::selected(egui::WidgetType::RadioButton, enabled, on, text),
        None => egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, text),
    });
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        if on {
            painter.rect_filled(rect, f32::from(RADIUS), ACCENT.gamma_multiply(0.12));
        } else if enabled && response.hovered() {
            painter.rect_filled(rect, f32::from(RADIUS), HOVER);
        }
        let color = if enabled {
            color
        } else {
            color.gamma_multiply(0.45)
        };
        painter.galley(
            egui::pos2(rect.left() + 10.0, rect.center().y - galley.size().y / 2.0),
            galley,
            color,
        );
        if on {
            let c = egui::pos2(rect.right() - 16.0, rect.center().y);
            let stroke = egui::Stroke::new(2.0, ACCENT);
            painter.line_segment(
                [c + egui::vec2(-5.0, 0.0), c + egui::vec2(-1.5, 3.5)],
                stroke,
            );
            painter.line_segment(
                [c + egui::vec2(-1.5, 3.5), c + egui::vec2(5.0, -4.0)],
                stroke,
            );
        }
        focus_ring(ui, &response, f32::from(RADIUS));
    }
    if enabled && response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response
}

/// A choice inside a dropdown: sets `current` and closes the list.
pub fn choice<T: PartialEq>(
    ui: &mut egui::Ui,
    current: &mut T,
    value: T,
    text: impl AsRef<str>,
) -> egui::Response {
    let response = menu_row(ui, text.as_ref(), true, Some(*current == value));
    if response.clicked() {
        *current = value;
        ui.close();
    }
    response
}

fn paint_chevron(painter: &egui::Painter, center: egui::Pos2, color: Color32) {
    let stroke = egui::Stroke::new(1.5, color);
    painter.line_segment(
        [
            center + egui::vec2(-4.0, -2.0),
            center + egui::vec2(0.0, 2.0),
        ],
        stroke,
    );
    painter.line_segment(
        [
            center + egui::vec2(0.0, 2.0),
            center + egui::vec2(4.0, -2.0),
        ],
        stroke,
    );
}

/// Paints a small glyph centred at a point in a colour.
pub type Glyph = dyn Fn(&egui::Painter, egui::Pos2, Color32);

/// The closed dropdown: label on the left, chevron on the right. `icon`
/// optionally paints a leading glyph.
pub fn dropdown_button(
    ui: &mut egui::Ui,
    text: &str,
    accessible: &str,
    width: Option<f32>,
    icon: Option<&Glyph>,
) -> egui::Response {
    let lead = if icon.is_some() { 26.0 } else { 0.0 };
    let natural = text_galley(ui, text, TEXT, TEXT_SIZE, f32::INFINITY);
    let width = width.unwrap_or(PADDING + lead + natural.size().x + 10.0 + 22.0);
    let galley = text_galley(
        ui,
        text,
        TEXT,
        TEXT_SIZE,
        (width - PADDING - lead - 32.0).max(8.0),
    );
    let enabled = ui.is_enabled();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, HEIGHT), egui::Sense::click());
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, enabled, accessible));
    if ui.is_rect_visible(rect) {
        let open = egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&response));
        paint_frame(
            ui,
            &response,
            if open {
                Tone::Selected
            } else {
                Tone::Secondary
            },
            f32::from(RADIUS),
        );
        let color = if enabled {
            TEXT
        } else {
            TEXT.gamma_multiply(0.45)
        };
        let painter = ui.painter();
        if let Some(icon) = icon {
            icon(
                painter,
                egui::pos2(rect.left() + PADDING + 9.0, rect.center().y),
                color,
            );
        }
        painter.galley(
            egui::pos2(
                rect.left() + PADDING + lead,
                rect.center().y - galley.size().y / 2.0,
            ),
            galley,
            color,
        );
        paint_chevron(
            painter,
            egui::pos2(rect.right() - 18.0, rect.center().y),
            color,
        );
        focus_ring(ui, &response, f32::from(RADIUS));
    }
    if enabled && response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response
}

/// Native video windows sit above egui: popups drawn over video register
/// their rect so the video is cut around them.
pub fn keep_popup_above_video(_ctx: &egui::Context, _rect: egui::Rect) {
    #[cfg(windows)]
    _ctx.data_mut(|data| {
        data.get_temp_mut_or_default::<Vec<(egui::Rect, f32)>>(egui::Id::new("video_popups"))
            .push((_rect, f32::from(PANEL_RADIUS)));
    });
}

/// Opens `content` in the standard popup frame below `anchor`.
pub fn popup<R>(
    ui: &egui::Ui,
    anchor: &egui::Response,
    width: f32,
    content: impl FnOnce(&mut egui::Ui) -> R,
) -> Option<R> {
    let area = ui.ctx().content_rect();
    let shown = egui::Popup::menu(anchor)
        .width(width)
        .frame(popup_frame(ui.style()))
        .show(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(4.0, 2.0);
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
            egui::ScrollArea::vertical()
                .max_height((area.height() * 0.6).clamp(80.0, 360.0))
                .show(ui, content)
                .inner
        })?;
    keep_popup_above_video(ui.ctx(), shown.response.rect);
    Some(shown.inner)
}

/// A full-width dropdown: the closed button shows `selected`; the list holds
/// `choices` (usually `choice` rows).
pub fn dropdown<R>(
    ui: &mut egui::Ui,
    id: &'static str,
    selected: &str,
    choices: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<Option<R>> {
    let width = ui.available_width();
    let button = ui
        .push_id(id, |ui| {
            dropdown_button(ui, selected, selected, Some(width), None)
        })
        .inner;
    let inner = popup(ui, &button, width, choices);
    egui::InnerResponse::new(inner, button)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(width: f32, draw: impl FnMut(&mut egui::Ui)) {
        let ctx = egui::Context::default();
        let area = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 400.0));
        let mut draw = draw;
        let _ = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(area),
                ..Default::default()
            },
            |root| {
                egui::CentralPanel::default().show(root, |ui| draw(ui));
            },
        );
    }

    #[test]
    fn buttons_share_one_height_and_truncate_long_labels() {
        run(600.0, |ui| {
            let button = |ui: &mut egui::Ui, text: &str, tone, enabled| {
                button_with(ui, text, tone, enabled, None, HEIGHT)
            };
            let regular = button(ui, "Cancelar", Tone::Secondary, true);
            let primary = button(ui, "Aplicar alterações", Tone::Primary, true);
            let danger = button(ui, "Sair da sala", Tone::Danger, true);
            let disabled = button(ui, "Compartilhar", Tone::Primary, false);
            for response in [&regular, &primary, &danger, &disabled] {
                assert_eq!(response.rect.height(), HEIGHT);
            }
            assert!(!disabled.enabled());
            let narrow = button_with(
                ui,
                "Um rótulo bem comprido que não cabe",
                Tone::Secondary,
                true,
                Some(120.0),
                HEIGHT,
            );
            assert_eq!(narrow.rect.width(), 120.0);
            let icon = icon_button(ui, "Reconectar", true, false, HEIGHT, |_, _, _| {});
            assert_eq!(icon.rect.size(), Vec2::splat(HEIGHT));
        });
    }

    #[test]
    fn segments_fill_the_row_evenly() {
        run(500.0, |ui| {
            let before = ui.available_width();
            let chosen = segments(ui, &["Fonte", "Qualidade"], 0);
            assert_eq!(chosen, None);
            assert!(before > 0.0);
        });
    }

    #[test]
    fn dropdown_and_rows_use_the_standard_sizes() {
        run(420.0, |ui| {
            let button = dropdown_button(ui, "Nativa", "Resolução", Some(300.0), None);
            assert_eq!(button.rect.size(), Vec2::new(300.0, HEIGHT));
            let row = menu_row(ui, "120 FPS", true, Some(true));
            assert_eq!(row.rect.height(), ROW);
            let mut fps = 60;
            let _ = choice(ui, &mut fps, 120, "120 FPS");
            assert_eq!(fps, 60, "nothing changes without a click");
        });
    }
}
