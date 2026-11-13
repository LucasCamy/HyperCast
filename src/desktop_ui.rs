//! Shared desktop controls used by the main window and room picker.
use eframe::egui::{self, Color32, RichText, Vec2};
pub const PANEL: Color32 = Color32::from_rgb(21, 27, 28);
pub const BORDER: Color32 = Color32::from_rgb(42, 50, 50);
pub const MUTED: Color32 = Color32::from_rgb(154, 167, 161);
pub const ACCENT: Color32 = Color32::from_rgb(191, 239, 121);
pub const SURFACE: Color32 = Color32::from_rgb(28, 35, 36);
pub const TEXT: Color32 = Color32::from_rgb(234, 239, 234);
fn modal_header(ui: &mut egui::Ui, title: &str) {
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), crate::kit::HEIGHT),
        egui::Layout::right_to_left(egui::Align::Center),
        |ui| {
            let response = crate::kit::icon_button(
                ui,
                &crate::i18n_format!("Fechar {title}", "Close {title}"),
                true,
                false,
                crate::kit::HEIGHT,
                |painter, center, stroke| {
                    for direction in [-1.0, 1.0] {
                        let offset = Vec2::new(5.0, 5.0 * direction);
                        painter.line_segment([center - offset, center + offset], stroke);
                    }
                },
            );
            if response.clicked() {
                ui.close_kind(egui::UiKind::Modal);
            }
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.add(
                    egui::Label::new(RichText::new(title).size(18.0).strong().color(TEXT))
                        .truncate(),
                )
                .on_hover_text(title);
            });
        },
    );
}

pub(crate) fn popup_style(ui: &mut egui::Ui) {
    let visuals = ui.visuals_mut();
    visuals.window_fill = PANEL;
    visuals.window_stroke = egui::Stroke::new(1.0, BORDER);
    visuals.window_corner_radius = egui::CornerRadius::same(crate::kit::PANEL_RADIUS);
    for widget in [
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
    ] {
        widget.corner_radius = egui::CornerRadius::same(crate::kit::RADIUS);
    }
    visuals.widgets.inactive.bg_fill = SURFACE;
    visuals.widgets.inactive.weak_bg_fill = SURFACE;
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, BORDER);
}

/// The standard dropdown; `choices` should use `crate::kit::choice` rows.
pub(crate) fn select_menu<R>(
    ui: &mut egui::Ui,
    id: &'static str,
    selected: impl Into<egui::WidgetText>,
    choices: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<Option<R>> {
    let selected: egui::WidgetText = selected.into();
    crate::kit::dropdown(ui, id, selected.text(), choices)
}

#[cfg(windows)]
pub(crate) fn take_video_popups(ctx: &egui::Context) -> Vec<(egui::Rect, f32)> {
    ctx.data_mut(|data| std::mem::take(data.get_temp_mut_or_default(egui::Id::new("video_popups"))))
}
pub fn upload_selector(
    ui: &mut egui::Ui,
    profile: &mut crate::bandwidth::UploadProfile,
    enabled: bool,
) {
    ui.add_enabled_ui(enabled, |ui| {
        ui.label(
            RichText::new(crate::i18n::label(
                "Limite de upload · vídeo",
                "Upload limit · video",
            ))
            .size(11.0)
            .color(MUTED),
        );
        select_menu(ui, "upload_profile", profile.label(), |ui| {
            for choice in crate::bandwidth::UploadProfile::ALL {
                crate::kit::choice(ui, profile, choice, choice.label());
            }
        });
        ui.label(
            RichText::new(if enabled {
                crate::i18n::label(
                    "Mantém resolução e FPS. Menos Mbps pode reduzir os detalhes.",
                    "Keeps resolution and FPS. Lower Mbps may reduce detail.",
                )
            } else {
                crate::i18n::label(
                    "Pare o compartilhamento para alterar o limite.",
                    "Stop sharing to change the limit.",
                )
            })
            .size(11.0)
            .color(MUTED),
        );
    });
}
/// Shared, explicit mode selection; publishing must stop before changing it.
pub fn rate_mode_selector(
    ui: &mut egui::Ui,
    mode: &mut crate::adaptive_upload::Mode,
    enabled: bool,
    allow_automatic: bool,
) {
    ui.add_enabled_ui(enabled, |ui| {
        ui.label(
            RichText::new(crate::i18n::label(
                "Controle de qualidade",
                "Quality control",
            ))
            .size(11.0)
            .color(MUTED),
        );
        select_menu(ui, "adaptive_upload_mode", mode.label(), |ui| {
            for choice in [
                crate::adaptive_upload::Mode::Manual,
                crate::adaptive_upload::Mode::Automatic,
            ] {
                ui.add_enabled_ui(
                    choice != crate::adaptive_upload::Mode::Automatic || allow_automatic,
                    |ui| {
                        crate::kit::choice(ui, mode, choice, choice.label());
                    },
                );
            }
        });
        ui.add(
            egui::Label::new(
                RichText::new(if !enabled {
                    crate::i18n::label(
                        "Pare o compartilhamento para alterar.",
                        "Stop sharing to change.",
                    )
                } else if *mode == crate::adaptive_upload::Mode::Automatic {
                    crate::i18n::label(
                        "Retoma o bitrate aos poucos após estabilizar a rede.",
                        "Raises bitrate gradually after the network stabilizes.",
                    )
                } else {
                    crate::i18n::label(
                        "Segue o limite de banda informado pelo WebRTC.",
                        "Follows the bandwidth limit reported by WebRTC.",
                    )
                })
                .size(11.0)
                .color(MUTED),
            )
            .wrap(),
        );
    });
}
pub(crate) fn speaker_toggle(
    ui: &mut egui::Ui,
    volume: &mut u32,
    muted: &mut bool,
    size: f32,
) -> egui::Response {
    let silent = *muted || *volume == 0;
    let label = if silent {
        crate::i18n::label("Reativar som recebido", "Unmute incoming audio")
    } else {
        crate::i18n::label("Silenciar som recebido", "Mute incoming audio")
    };
    let response = room_icon_button_sized(ui, RoomIcon::Speaker(silent), label, true, false, size);
    if response.clicked() {
        toggle_mute(volume, muted);
    }
    response
}

/// Shared by the speaker button and the full-screen `M` shortcut.
pub(crate) fn toggle_mute(volume: &mut u32, muted: &mut bool) {
    let silent = *muted || *volume == 0;
    *muted = !silent;
    if silent && *volume == 0 {
        *volume = 80;
    }
}

pub(crate) fn volume_slider(
    ui: &mut egui::Ui,
    volume: &mut u32,
    muted: &mut bool,
    width: f32,
    height: f32,
) -> bool {
    ui.scope(|ui| {
        ui.spacing_mut().slider_width = width;
        ui.spacing_mut().interact_size.y = height;
        ui.spacing_mut().slider_rail_height = 6.0;
        ui.visuals_mut().widgets.inactive.bg_fill = Color32::from_rgb(65, 78, 73);
        ui.visuals_mut().widgets.inactive.fg_stroke = egui::Stroke::new(1.5, ACCENT);
        ui.visuals_mut().selection.bg_fill = if *muted { MUTED } else { ACCENT };
        let slider = ui.add(
            egui::Slider::new(volume, 0..=100)
                .show_value(false)
                .trailing_fill(true)
                .handle_shape(egui::style::HandleShape::Circle),
        );
        slider.widget_info(|| {
            egui::WidgetInfo::slider(
                true,
                *volume as f64,
                crate::i18n::label("Volume recebido", "Incoming volume"),
            )
        });
        let changed = slider.changed();
        slider.on_hover_text(crate::i18n_format!(
            "Volume recebido · {}%",
            "Incoming volume · {}%",
            *volume
        ));
        if changed {
            *muted = false;
        }
        changed
    })
    .inner
}

pub(crate) fn utility_frame(style: &egui::Style) -> egui::Frame {
    egui::Frame::window(style)
        .fill(PANEL)
        .inner_margin(12)
        .corner_radius(12)
        .stroke(egui::Stroke::new(1.0, BORDER))
}

pub(crate) fn dialog<R>(
    ctx: &egui::Context,
    title: &str,
    id: &'static str,
    width: f32,
    content: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::ModalResponse<R> {
    modal(
        egui::Modal::new(egui::Id::new(id)),
        ctx,
        title,
        width,
        content,
    )
}

#[cfg(windows)]
pub(crate) fn sharing_dialog<R>(
    ctx: &egui::Context,
    title: &str,
    width: f32,
    content: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::ModalResponse<R> {
    // The native video is composited above the parent window. Leave the backdrop
    // transparent so the surrounding video and room have the same brightness.
    modal(
        egui::Modal::new(egui::Id::new("choose_source")).backdrop_color(Color32::TRANSPARENT),
        ctx,
        title,
        width,
        content,
    )
}

fn modal<R>(
    modal: egui::Modal,
    ctx: &egui::Context,
    title: &str,
    width: f32,
    content: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::ModalResponse<R> {
    let width = width.min((ctx.content_rect().width() - 40.0).max(96.0));
    modal
        .frame(utility_frame(&ctx.style_of(egui::Theme::Dark)))
        .show(ctx, |ui| {
            ui.set_width(width);
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
            ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
            modal_header(ui, title);
            ui.separator();
            content(ui)
        })
}

pub(crate) fn dialog_body<R>(
    ui: &mut egui::Ui,
    reserve_height: f32,
    content: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let height = (ui.ctx().content_rect().height() - reserve_height).max(40.0);
    // A centered modal starts each pass with its previous size. Allow newly
    // loaded content to use the window budget while still shrinking to fit.
    egui::ScrollArea::vertical()
        .min_scrolled_height(height)
        .max_height(height)
        .show(ui, content)
        .inner
}

pub(crate) fn update_dialog<R>(
    ctx: &egui::Context,
    content: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::ModalResponse<R> {
    scrolling_dialog(
        ctx,
        crate::i18n::label("Atualização", "Update"),
        "update_window",
        440.0,
        content,
    )
}

pub(crate) fn scrolling_dialog<R>(
    ctx: &egui::Context,
    title: &str,
    id: &'static str,
    width: f32,
    content: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::ModalResponse<R> {
    dialog(ctx, title, id, width, |ui| dialog_body(ui, 80.0, content))
}

pub(crate) fn stream_options_title() -> &'static str {
    crate::i18n::label("Editar compartilhamento", "Edit screen share")
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum ShareTab {
    #[default]
    Source,
    Quality,
}

pub(crate) fn share_tabs(ui: &mut egui::Ui, tab: &mut ShareTab) {
    let labels = [
        crate::i18n::label("Fonte", "Source"),
        crate::i18n::label("Qualidade", "Quality"),
    ];
    let selected = usize::from(*tab == ShareTab::Quality);
    match crate::kit::segments(ui, &labels, selected) {
        Some(0) => *tab = ShareTab::Source,
        Some(_) => *tab = ShareTab::Quality,
        None => {}
    }
}

pub(crate) fn share_confirmation(ui: &mut egui::Ui, label: &str, enabled: bool) -> (bool, bool) {
    ui.separator();
    ui.horizontal_wrapped(|ui| {
        let cancel =
            session_button(ui, crate::i18n::label("Cancelar", "Cancel"), true, false).clicked();
        let confirm = session_button(ui, label, enabled, true).clicked();
        (cancel, confirm)
    })
    .inner
}

pub(crate) fn share_audio(ui: &mut egui::Ui, enabled: &mut bool, app_audio: bool) -> bool {
    let label = if app_audio {
        crate::i18n::label("Incluir áudio do aplicativo", "Include app audio")
    } else {
        crate::i18n::label("Incluir áudio do sistema", "Include system audio")
    };
    ui.checkbox(enabled, label).changed()
}

pub fn action(ui: &mut egui::Ui, label: &str, enabled: bool, primary: bool) -> egui::Response {
    let width = ui.available_width();
    let tone = if primary {
        crate::kit::Tone::Selected
    } else {
        crate::kit::Tone::Secondary
    };
    crate::kit::button_with(ui, label, tone, enabled, Some(width), crate::kit::HEIGHT)
}
pub fn quality_strip(
    ui: &mut egui::Ui,
    value: &crate::video_metrics::QualitySnapshot,
    reconnecting: bool,
    publishing: bool,
    reception_paused: bool,
) {
    ui.horizontal_wrapped(|ui|{
        ui.spacing_mut().item_spacing.x=16.0;
        ui.label(RichText::new(if reconnecting{crate::i18n::label("Sala · reconectando…","Room · reconnecting…")}else{crate::i18n::label("Sala · conectada","Room · connected")}).size(11.0).color(if reconnecting{Color32::from_rgb(236,190,105)}else{ACCENT}));
        if publishing {
            let text=if reconnecting{crate::i18n::label("Envio aguardando conexão","Publishing · waiting for connection").into()}else if value.send_paused{crate::i18n::label("Envio pausado","Publishing paused").into()}else if let Some(rate)=value.send{
                let size=value.resolution.map(|(w,h)|format!("{w}×{h} · ")).unwrap_or_default();
                crate::i18n_format!("Envio · {size}{:.0} FPS · {:.1} Mbps","Sending · {size}{:.0} FPS · {:.1} Mbps",rate.fps,rate.mbps)
            }else{crate::i18n::label("Envio · medindo…","Sending · measuring…").into()};
            ui.label(RichText::new(text).size(11.0).color(MUTED)).on_hover_text(crate::i18n::label("Quadros e bytes de vídeo enviados na última janela, sem overhead da rede. Não medem fidelidade da imagem nem latência fim a fim.","Video frames and bytes sent during the last interval, excluding network overhead. These do not measure image fidelity or end-to-end latency."));
        }
        let receive=if reception_paused{crate::i18n::label("Recepção pausada","Playback paused").into()}else if reconnecting{crate::i18n::label("Recepção aguardando conexão","Playback · waiting for connection").into()}else if let Some(rate)=value.receive{crate::i18n_format!("Recepção · {:.0} quadros/s no total","Playback · {:.0} total frames/s",rate.fps)}else{crate::i18n::label("Recepção · medindo…","Playback · measuring…").into()};
        ui.label(RichText::new(receive).size(11.0).color(MUTED)).on_hover_text(crate::i18n::label("Soma dos quadros decodificados de todas as transmissões recebidas; não é FPS de uma única tela nem FPS de apresentação.","Sum of decoded frames across all received streams; not the FPS of one stream or the presentation FPS."));
    });
}

/// Embedded brand asset, decoded once per UI context and shared by both desktops.
pub fn brand_mark(ui: &mut egui::Ui, size: f32) -> egui::Response {
    let id = egui::Id::new("hypercast_brand_texture");
    let texture = ui
        .ctx()
        .data_mut(|data| data.get_temp::<egui::TextureHandle>(id));
    let texture = texture.or_else(|| {
        let icon =
            eframe::icon_data::from_png_bytes(include_bytes!("../assets/hypercast.png")).ok()?;
        let image = egui::ColorImage::from_rgba_unmultiplied(
            [icon.width as usize, icon.height as usize],
            &icon.rgba,
        );
        let texture = ui
            .ctx()
            .load_texture("HyperCast brand", image, egui::TextureOptions::LINEAR);
        ui.ctx()
            .data_mut(|data| data.insert_temp(id, texture.clone()));
        Some(texture)
    });
    if let Some(texture) = texture {
        ui.add(egui::Image::from_texture((texture.id(), Vec2::splat(size))))
    } else {
        ui.allocate_response(Vec2::splat(size), egui::Sense::hover())
    }
}

pub fn input(
    ui: &mut egui::Ui,
    text: &mut String,
    hint: &str,
    password: bool,
    limit: usize,
    enabled: bool,
) -> egui::Response {
    ui.scope(|ui| {
        ui.visuals_mut().widgets.inactive.corner_radius = egui::CornerRadius::same(8);
        ui.visuals_mut().widgets.hovered.corner_radius = egui::CornerRadius::same(8);
        ui.visuals_mut().widgets.active.corner_radius = egui::CornerRadius::same(8);
        ui.visuals_mut().widgets.inactive.bg_stroke = egui::Stroke::new(1.0, BORDER);
        ui.visuals_mut().widgets.hovered.bg_stroke = egui::Stroke::new(1.0, MUTED);
        ui.visuals_mut().selection.stroke = egui::Stroke::new(1.5, ACCENT);
        ui.add_enabled(
            enabled,
            egui::TextEdit::singleline(text)
                .password(password)
                .char_limit(limit)
                .desired_width(f32::INFINITY)
                .margin(egui::Margin::symmetric(12, 10))
                .background_color(SURFACE)
                .hint_text(hint),
        )
    })
    .inner
}
pub fn compact(ui: &mut egui::Ui, label: &str, enabled: bool) -> egui::Response {
    crate::kit::small(ui, label, enabled)
}

pub(crate) fn sidebar_update(ui: &mut egui::Ui) -> egui::Response {
    let narrow = ui.available_width() < 160.0;
    ui.horizontal_wrapped(|ui| {
        brand_mark(ui, 22.0);
        ui.label(RichText::new("HyperCast").strong());
        if narrow {
            ui.end_row();
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            compact(ui, crate::i18n::label("Atualizar", "Update"), true)
        })
        .inner
    })
    .inner
}
pub fn primary(ui: &mut egui::Ui, label: &str, enabled: bool, size: Vec2) -> egui::Response {
    let width = size
        .x
        .max(crate::kit::natural_width(ui, label, crate::kit::HEIGHT));
    crate::kit::button_with(
        ui,
        label,
        crate::kit::Tone::Primary,
        enabled,
        Some(width),
        crate::kit::HEIGHT,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scrolling_dialog_grows_when_update_check_adds_actions() {
        for size in [egui::vec2(1320.0, 820.0), egui::vec2(230.0, 180.0)] {
            let ctx = egui::Context::default();
            let area = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
            let draw = |finished, events| {
                ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(area),
                        events,
                        ..Default::default()
                    },
                    |root| {
                        update_dialog(root.ctx(), |ui| {
                            ui.label("Current version: 0.9.5");
                            ui.small("Installed");
                            if finished {
                                ui.label("You are up to date");
                                action(ui, "Check again", true, false);
                                action(ui, "Downloads", true, false);
                            } else {
                                ui.label("Checking for updates…");
                            }
                        });
                    },
                )
            };
            for _ in 0..3 {
                let _ = draw(false, Vec::new());
            }
            for _ in 0..3 {
                let _ = draw(true, Vec::new());
            }
            let output = draw(true, Vec::new());
            let visible = |output: &egui::FullOutput, text: &str| {
                output.shapes.iter().any(|shape| {
                    matches!(&shape.shape, egui::Shape::Text(label)
                        if label.galley.text() == text && shape.clip_rect.contains_rect(label.visual_bounding_rect())
                            && area.contains_rect(label.visual_bounding_rect()))
                })
            };
            assert!(
                visible(&output, "Check again"),
                "Update action clipped at {size:?}"
            );
            if size.y > 180.0 {
                assert!(
                    visible(&output, "Downloads"),
                    "Downloads action clipped at {size:?}"
                );
            } else {
                let mut output = draw(
                    true,
                    vec![
                        egui::Event::PointerMoved(area.center()),
                        egui::Event::MouseWheel {
                            unit: egui::MouseWheelUnit::Point,
                            delta: egui::vec2(0.0, -300.0),
                            modifiers: Default::default(),
                            phase: egui::TouchPhase::Move,
                        },
                    ],
                );
                for _ in 0..12 {
                    output = draw(true, Vec::new());
                }
                assert!(
                    visible(&output, "Downloads"),
                    "Downloads must be reachable by scrolling"
                );
            }
        }
    }

    #[test]
    fn modal_closes_from_target_corners_outside_the_small_x() {
        for (offset, closes) in [
            (Vec2::new(-13.0, -13.0), true),
            (Vec2::new(13.0, -13.0), true),
            (Vec2::new(-13.0, 13.0), true),
            (Vec2::new(13.0, 13.0), true),
            (Vec2::new(22.0, 0.0), false),
        ] {
            let ctx = egui::Context::default();
            let mut open = true;
            let render = |ui: &mut egui::Ui, open: &mut bool| {
                if *open {
                    *open = !dialog(ui.ctx(), "Teste", "test_dialog", 260.0, |_| {}).should_close();
                }
            };
            let mut center = None;
            for _ in 0..3 {
                let output = ctx.run_ui(Default::default(), |ui| render(ui, &mut open));
                for s in output.shapes {
                    if let egui::Shape::LineSegment { points, .. } = s.shape {
                        if ((points[1].x - points[0].x).abs() - 10.0).abs() < 0.1
                            && ((points[1].y - points[0].y).abs() - 10.0).abs() < 0.1
                        {
                            center = Some(points[0] + (points[1] - points[0]) * 0.5);
                        }
                    }
                }
            }
            let pos = center.expect("Visible X") + offset;
            for pressed in [true, false] {
                let input = egui::RawInput {
                    events: vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: Default::default(),
                        },
                    ],
                    ..Default::default()
                };
                let _ = ctx.run_ui(input, |ui| render(ui, &mut open));
            }
            assert_eq!(!open, closes, "Unexpected close state at {offset:?}");
        }
    }
    #[test]
    fn current_room_label_stays_on_one_line_inside_the_button() {
        let ctx = egui::Context::default();
        let mut button = egui::Rect::NOTHING;
        let output = ctx.run_ui(Default::default(), |ui| {
            button = primary(ui, "Nesta sala", false, egui::vec2(88.0, 36.0)).rect;
        });
        let label = output
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::Shape::Text(t) if t.galley.text() == "Nesta sala" => Some(t),
                _ => None,
            })
            .unwrap();
        assert_eq!(label.galley.rows.len(), 1);
        assert!(button.contains_rect(label.visual_bounding_rect()));
    }
}

pub fn stream_settings(
    ui: &mut egui::Ui,
    value: &mut crate::resolution::StreamSettings,
    enabled: bool,
    max_fps: u32,
) {
    ui.add_enabled_ui(enabled, |ui| {
        ui.columns(2, |columns| {
            columns[0].label(
                RichText::new(crate::i18n::label("Resolução", "Resolution"))
                    .size(11.0)
                    .color(MUTED),
            );
            select_menu(
                &mut columns[0],
                "send_resolution",
                value.size.label(),
                |ui| {
                    for size in [
                        crate::resolution::OutputSize::Native,
                        crate::resolution::OutputSize::FullHd,
                        crate::resolution::OutputSize::QuadHd,
                    ] {
                        crate::kit::choice(ui, &mut value.size, size, size.label());
                    }
                },
            );
            columns[1].label(
                RichText::new(crate::i18n::label("FPS de envio", "Send FPS"))
                    .size(11.0)
                    .color(MUTED),
            );
            select_menu(
                &mut columns[1],
                "send_fps",
                format!("{} FPS", value.fps),
                |ui| {
                    for fps in [30, 60, 120].into_iter().filter(|fps| *fps <= max_fps) {
                        crate::kit::choice(ui, &mut value.fps, fps, format!("{fps} FPS"));
                    }
                },
            );
        });
    });
    ui.label(
        RichText::new(crate::i18n::label(
            "Mantém a proporção; fontes menores não são ampliadas.",
            "Keeps the aspect ratio; smaller sources are not upscaled.",
        ))
        .size(11.0)
        .color(MUTED),
    );
}

/// Compact measurements; label the transport RTT separately from end-to-end latency.
pub fn network_summary(ui: &mut egui::Ui, q: &crate::video_metrics::QualitySnapshot) {
    if let Some(n) = q.network {
        if let Some(ms) = n.rtt_ms.filter(|v| v.is_finite() && *v >= 0.0) {
            ui.label(crate::i18n_format!(
                "RTT da rede: {ms:.1} ms",
                "Network RTT: {ms:.1} ms"
            ));
        }
        if n.streams > 0 {
            ui.label(crate::i18n_format!(
                "RTP acumulado · perdidos: {} · descartados: {}",
                "Cumulative RTP · lost: {} · dropped: {}",
                n.lost,
                n.dropped
            ));
        }
    } else {
        ui.small(crate::i18n::label(
            "Métricas da rede ainda indisponíveis",
            "Network measurements not available yet",
        ));
    }
    ui.small(crate::i18n::label(
        "RTT não representa a latência fim a fim.",
        "RTT does not represent end-to-end latency.",
    ));
}

/// Explicit scope and inline confirmation; reset is local and never joins/shares.
pub fn preferences_controls(
    ui: &mut egui::Ui,
    store: &mut crate::preferences::Store,
    idle: bool,
) -> bool {
    let key = ui.id().with("reset-preferences");
    let mut confirm = ui.data_mut(|d| d.get_temp::<bool>(key).unwrap_or(false));
    ui.separator();
    ui.label(
        RichText::new(crate::i18n::label("PREFERÊNCIAS", "PREFERENCES"))
            .size(10.0)
            .color(MUTED),
    );
    ui.add(egui::Label::new(crate::i18n::label("Restaura envio, codec, perfil AMD, volume e visualização. Preserva nome, idioma e acesso ao servidor.","Resets sending, codec, AMD preset, volume and view. Keeps your name, language and server access.")).wrap());
    if !idle {
        confirm = false;
        ui.add(
            egui::Label::new(
                RichText::new(crate::i18n::label(
                    "Saia da sala para restaurar as preferências.",
                    "Leave the room to restore preferences.",
                ))
                .size(11.0)
                .color(MUTED),
            )
            .wrap(),
        );
    }
    let mut reset = false;
    if confirm {
        ui.add(
            egui::Label::new(crate::i18n::label(
                "Restaurar os padrões agora?",
                "Restore defaults now?",
            ))
            .wrap(),
        );
        if action(
            ui,
            crate::i18n::label("Restaurar padrões", "Restore defaults"),
            idle,
            false,
        )
        .clicked()
        {
            store.reset();
            reset = true;
            confirm = false;
        }
        if compact(ui, crate::i18n::label("Cancelar", "Cancel"), true).clicked() {
            confirm = false;
        }
    } else if compact(
        ui,
        crate::i18n::label("Restaurar preferências…", "Restore preferences…"),
        idle,
    )
    .clicked()
    {
        confirm = true;
    }
    ui.data_mut(|d| d.insert_temp(key, confirm));
    if let Some(notice) = store.notice() {
        if cfg!(windows) {
            crate::kit::notify_on_change(ui.ctx(), "preferences", notice, None);
        } else {
            ui.add(egui::Label::new(RichText::new(notice).size(11.0).color(MUTED)).wrap());
        }
    }
    if store.retry_available()
        && compact(
            ui,
            crate::i18n::label("Tentar salvar novamente", "Retry saving"),
            true,
        )
        .clicked()
    {
        store.retry();
    }
    reset
}

/// Fit stream cards to the usable width while keeping every stream visible.
pub(crate) fn video_grid_card(
    area: egui::Rect,
    index: usize,
    count: usize,
    gap: f32,
) -> egui::Rect {
    let preferred_columns = if area.width() >= 900.0 {
        3
    } else if area.width() >= 480.0 {
        2
    } else {
        1
    };
    let count = count.max(1);
    let rows_with_room = ((area.height() + gap) / (56.0 + gap)).floor() as usize;
    let columns_that_fit = ((area.width() + gap) / (96.0 + gap)).floor() as usize;
    let columns = count
        .div_ceil(rows_with_room.max(1))
        .clamp(
            preferred_columns,
            columns_that_fit.clamp(preferred_columns, 3),
        )
        .min(count);
    let rows = count.div_ceil(columns);
    let row_gap = gap.min(area.height().max(0.0) / rows as f32 * 0.25);
    let row = index / columns;
    let column = index % columns;
    let row_count = count.saturating_sub(row * columns).clamp(1, columns);
    let column_gap = gap.min(area.width().max(0.0) / row_count as f32 * 0.25);
    let width = ((area.width() - column_gap * (row_count - 1) as f32) / row_count as f32).max(0.0);
    let height = ((area.height() - row_gap * (rows - 1) as f32) / rows as f32).max(0.0);
    egui::Rect::from_min_size(
        area.min
            + egui::vec2(
                column as f32 * (width + column_gap),
                row as f32 * (height + row_gap),
            ),
        egui::vec2(width, height),
    )
}

pub(crate) fn room_margin(width: f32, fullscreen: bool) -> i8 {
    if fullscreen || width < 480.0 {
        8
    } else if width < 900.0 {
        14
    } else {
        24
    }
}

pub(crate) fn participant_heading(ui: &mut egui::Ui, count: usize) {
    use crate::i18n::label;
    ui.add_space(5.0);
    ui.separator();
    ui.add_space(3.0);
    let heading = RichText::new(label("PARTICIPANTES", "PARTICIPANTS"))
        .size(11.0)
        .strong()
        .color(MUTED);
    let count = RichText::new(crate::i18n_format!(
        "{count} na sala",
        "{count} in the room"
    ))
    .size(11.0)
    .color(MUTED);
    if ui.available_width() < 180.0 {
        ui.label(heading);
        ui.label(count);
    } else {
        ui.horizontal(|ui| {
            ui.label(heading);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(count);
            });
        });
    }
    ui.add_space(4.0);
}

pub(crate) fn sidebar_width(width: f32) -> f32 {
    (width * 0.29).clamp(100.0, 272.0)
}

pub(crate) fn paint_video_label(
    ui: &egui::Ui,
    rect: egui::Rect,
    label: &str,
    size: f32,
    color: Color32,
) {
    if !rect.is_positive() {
        return;
    }
    let painter = ui.painter().with_clip_rect(rect);
    let mut job = egui::text::LayoutJob::simple_singleline(
        label.to_owned(),
        egui::FontId::proportional(size),
        color,
    );
    job.wrap.max_width = rect.width();
    job.wrap.max_rows = 1;
    let galley = painter.layout_job(job);
    painter.galley(
        egui::pos2(rect.left(), rect.center().y - galley.size().y * 0.5),
        galley,
        color,
    );
}

#[cfg(test)]
#[test]
fn shallow_room_grid_uses_more_columns_to_keep_video_visible() {
    let area = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(360.0, 84.0));
    let cards: Vec<_> = (0..3)
        .map(|index| video_grid_card(area, index, 3, 16.0))
        .collect();
    assert!(
        cards
            .iter()
            .all(|card| card.top() == 0.0 && card.height() == 84.0)
    );
    assert!(
        cards
            .iter()
            .all(|card| area.expand(0.01).contains_rect(*card))
    );
}

#[cfg(test)]
#[test]
fn video_grid_preserves_three_columns_and_fits_six_cards() {
    let area = egui::Rect::from_min_size(egui::pos2(20.0, 30.0), egui::vec2(932.0, 616.0));
    let gap = 16.0;
    for index in 0..3 {
        let screen = video_grid_card(area, index, 3, gap);
        assert_eq!(screen.size(), egui::vec2(300.0, 616.0));
        assert_eq!(screen.min, egui::pos2(20.0 + index as f32 * 316.0, 30.0));
    }
    let cards: Vec<_> = (0..6)
        .map(|index| video_grid_card(area, index, 6, gap))
        .collect();
    for (index, card) in cards.iter().enumerate() {
        assert_eq!(card.size(), egui::vec2(300.0, 300.0));
        assert!(area.contains_rect(*card));
        assert!(
            cards[..index]
                .iter()
                .all(|previous| !card.intersects(*previous))
        );
    }
}

#[cfg(test)]
#[test]
fn narrow_room_grid_keeps_cards_inside_the_viewport() {
    for (width, columns) in [(380.0, 1), (500.0, 2), (940.0, 3)] {
        let area = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 420.0));
        let cards: Vec<_> = (0..6).map(|i| video_grid_card(area, i, 6, 12.0)).collect();
        assert_eq!(
            cards.iter().filter(|card| card.top() == 0.0).count(),
            columns
        );
        assert!(
            cards
                .iter()
                .all(|card| area.expand(0.01).contains_rect(*card))
        );
    }
}

pub(crate) fn room_state(connected: bool, reconnecting: bool) -> &'static str {
    if reconnecting {
        crate::i18n::label("Reconectando à sala…", "Reconnecting to the room…")
    } else if connected {
        crate::i18n::label("Conectado à sala", "Connected to the room")
    } else {
        crate::i18n::label("Fora da sala", "Outside the room")
    }
}

pub(crate) fn left_room_status(server_access: bool) -> &'static str {
    if server_access {
        crate::i18n::label(
            "Você saiu da sala. Seu acesso ao servidor continua ativo.",
            "You left the room. Your server access is still active.",
        )
    } else {
        crate::i18n::label("Você saiu da sala", "You left the room")
    }
}

pub(crate) fn room_activity(sharing: bool, camera: bool, status: &str, fallback: &str) -> String {
    use crate::i18n::label;
    let mut parts = Vec::new();
    if !status.is_empty() && status != label("Conectado", "Connected") {
        parts.push(status);
    } else if sharing {
        parts.push(label("Compartilhando tela", "Sharing screen"));
    }
    if camera {
        parts.push(label("Câmera ligada", "Camera on"));
    }
    if parts.is_empty() {
        parts.push(fallback);
    }
    parts.join(" · ")
}

/// A paused viewer can resume without searching the room controls.
pub(crate) fn playback_empty(
    ui: &mut egui::Ui,
    area: egui::Rect,
    paused: bool,
    reconnecting: bool,
    waiting_for_video: bool,
    can_publish: bool,
) -> bool {
    if area.width() < 32.0 || area.height() < 32.0 {
        return false;
    }
    let title = if paused {
        crate::i18n::label("Vídeo pausado por você", "You paused incoming video")
    } else if reconnecting {
        crate::i18n::label(
            "Aguardando a conexão voltar",
            "Waiting for the connection to return",
        )
    } else if waiting_for_video {
        crate::i18n::label("Aguardando vídeo", "Waiting for video")
    } else {
        crate::i18n::label("Nenhum vídeo compartilhado", "No shared video yet")
    };
    let hint = if can_publish {
        crate::i18n::label(
            "Compartilhe sua tela ou aguarde alguém começar.",
            "Share your screen or wait for someone to start.",
        )
    } else {
        crate::i18n::label(
            "Você pode assistir quando alguém compartilhar.",
            "You can watch when someone starts sharing.",
        )
    };
    let compact = area.height() < 108.0;
    let mut resume = false;
    ui.scope_builder(egui::UiBuilder::new().max_rect(area.shrink(8.0)), |ui| {
        ui.set_clip_rect(ui.clip_rect().intersect(area));
        ui.vertical_centered(|ui| {
            ui.add_space(if compact {
                0.0
            } else {
                (area.height() / 2.0 - 44.0).max(0.0)
            });
            ui.label(
                RichText::new(title)
                    .size(if compact { 16.0 } else { 24.0 })
                    .color(TEXT),
            );
            if paused && area.height() >= 64.0 {
                resume = session_button(
                    ui,
                    crate::i18n::label("Retomar vídeo recebido", "Resume incoming video"),
                    true,
                    true,
                )
                .clicked();
            } else if !compact && !reconnecting && !waiting_for_video {
                ui.label(RichText::new(hint).size(14.0).color(MUTED));
            }
        });
    });
    resume
}

pub(crate) fn refresh_rooms_button(ui: &mut egui::Ui, enabled: bool) -> egui::Response {
    room_icon_button_sized(
        ui,
        RoomIcon::Reconnect,
        crate::i18n::label("Atualizar lista", "Refresh list"),
        enabled,
        false,
        crate::kit::HEIGHT,
    )
}

pub(crate) fn session_button(
    ui: &mut egui::Ui,
    label: &str,
    enabled: bool,
    primary: bool,
) -> egui::Response {
    session_action(
        ui,
        label,
        enabled,
        if primary {
            crate::kit::Tone::Primary
        } else {
            crate::kit::Tone::Secondary
        },
    )
}

pub(crate) fn session_stop(ui: &mut egui::Ui, label: &str, enabled: bool) -> egui::Response {
    session_action(ui, label, enabled, crate::kit::Tone::Danger)
}

pub(crate) fn server_logout_button(ui: &mut egui::Ui, enabled: bool) -> egui::Response {
    ui.scope(|ui| {
        ui.spacing_mut().interact_size = Vec2::new(ui.available_width(), crate::kit::HEIGHT);
        session_stop(
            ui,
            crate::i18n::label("Sair do servidor", "Sign out of server"),
            enabled,
        )
    })
    .inner
}

pub(crate) fn session_menu(
    ui: &mut egui::Ui,
    content: impl FnOnce(&mut egui::Ui),
) -> Option<egui::Rect> {
    let more = session_button(
        ui,
        crate::i18n::label("Mais opções", "More options"),
        true,
        false,
    );
    let area = ui.ctx().content_rect();
    egui::Popup::menu(&more)
        .width((area.width() - 28.0).clamp(96.0, 254.0))
        .frame(crate::kit::popup_frame(ui.style()))
        .show(|ui| {
            popup_style(ui);
            ui.spacing_mut().button_padding = Vec2::new(8.0, 4.0);
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
            egui::ScrollArea::vertical()
                .max_height((area.height() - 44.0).max(48.0))
                .show(ui, content);
        })
        .map(|popup| popup.response.rect)
}

pub(crate) fn menu_action(ui: &mut egui::Ui, label: &str, enabled: bool) -> egui::Response {
    crate::kit::menu_row(ui, label, enabled, None)
}

pub(crate) fn display_scale(ui: &mut egui::Ui) {
    crate::kit::menu_heading(
        ui,
        crate::i18n::label("Escala da interface", "Interface scale"),
    );
    let current = ui.ctx().zoom_factor();
    let factors = [1.0, 1.5, 2.0];
    let selected = factors
        .iter()
        .position(|factor| (current - factor).abs() < 0.01)
        .unwrap_or(usize::MAX);
    if let Some(index) = crate::kit::segments(ui, &["100%", "150%", "200%"], selected) {
        ui.ctx().set_zoom_factor(factors[index]);
        ui.close();
    }
}

fn session_action(
    ui: &mut egui::Ui,
    label: &str,
    enabled: bool,
    tone: crate::kit::Tone,
) -> egui::Response {
    let width = ui.spacing().interact_size.x;
    crate::kit::button_with(
        ui,
        label,
        tone,
        enabled,
        // Callers that size the row set a width; egui's default (40) means "fit".
        (width >= 48.0).then_some(width),
        crate::kit::HEIGHT,
    )
}

pub(crate) fn server_indicator(ui: &mut egui::Ui, server: &crate::server_status::ServerStatus) {
    let color = if server.status == crate::server_status::Status::Online {
        ACCENT
    } else {
        MUTED
    };
    ui.label(RichText::new(server.label()).size(11.0).color(color))
        .on_hover_text(crate::i18n_format!(
            "Servidor: {}",
            "Server: {}",
            server.detail
        ));
}

pub(crate) fn home_panel(ui: &mut egui::Ui, status: &str, content: impl FnOnce(&mut egui::Ui)) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(20.0);
            let width = ui.available_width().min(460.0);
            ui.allocate_ui_with_layout(
                egui::vec2(width, 0.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_width(width);
                    ui.horizontal(|ui| {
                        brand_mark(ui, 28.0);
                        ui.label(RichText::new("HyperCast").size(26.0).strong().color(TEXT));
                    });
                    ui.label(
                        RichText::new(crate::i18n::label(
                            "Entre em uma sala para assistir ou compartilhar sua tela.",
                            "Join a room to watch or share your screen.",
                        ))
                        .size(16.0)
                        .color(MUTED),
                    );
                    if !status.is_empty() {
                        ui.label(RichText::new(status).color(TEXT));
                    }
                    ui.add_space(12.0);
                    content(ui);
                },
            );
        });
    });
}

pub(crate) fn confirm_leave(ctx: &egui::Context) -> Option<bool> {
    confirm_exit(
        ctx,
        crate::i18n::label("Sair da sala?", "Leave the room?"),
        "confirm_leave",
        crate::i18n::label(
            "Seu compartilhamento será encerrado ao sair.",
            "Your sharing will end when you leave.",
        ),
        crate::i18n::label("Continuar na sala", "Stay in the room"),
        crate::i18n::label("Sair da sala", "Leave room"),
    )
}

pub(crate) fn confirm_server_logout(ctx: &egui::Context) -> Option<bool> {
    confirm_exit(
        ctx,
        crate::i18n::label("Sair do servidor?", "Sign out of the server?"),
        "confirm_server_logout",
        crate::i18n::label(
            "Você precisará informar a chave de acesso novamente para entrar no servidor.",
            "You will need to enter your access key again to sign in to the server.",
        ),
        crate::i18n::label("Cancelar", "Cancel"),
        crate::i18n::label("Sair do servidor", "Sign out of server"),
    )
}

fn confirm_exit(
    ctx: &egui::Context,
    title: &str,
    id: &'static str,
    message: &str,
    cancel: &str,
    confirm: &str,
) -> Option<bool> {
    let modal = dialog(ctx, title, id, 360.0, |ui| {
        ui.label(message);
        ui.add_space(12.0);
        ui.horizontal_wrapped(|ui| {
            let cancelled = session_button(ui, cancel, true, false).clicked();
            let confirmed = session_stop(ui, confirm, true).clicked();
            if confirmed {
                Some(true)
            } else if cancelled {
                Some(false)
            } else {
                None
            }
        })
        .inner
    });
    modal
        .inner
        .or_else(|| modal.should_close().then_some(false))
}

#[derive(Clone, Copy)]
pub(crate) enum RoomIcon {
    Panel,
    Grid,
    Stage,
    Fullscreen,
    Gear,
    Preview(bool),
    Speaker(bool),
    Playback(bool),
    Reconnect,
    Share,
    Stop,
    Camera(bool),
    More,
    Leave,
}

pub(crate) fn room_icon_button_sized(
    ui: &mut egui::Ui,
    icon: RoomIcon,
    label: &str,
    enabled: bool,
    selected: bool,
    size: f32,
) -> egui::Response {
    crate::kit::icon_button(
        ui,
        label,
        enabled,
        selected,
        size,
        |painter, center, stroke| paint_room_icon(painter, center, stroke, icon),
    )
}

pub(crate) fn paint_room_icon(
    painter: &egui::Painter,
    center: egui::Pos2,
    stroke: egui::Stroke,
    icon: RoomIcon,
) {
    match icon {
        RoomIcon::Panel => {
            let bounds = egui::Rect::from_center_size(center, egui::vec2(17.0, 13.0));
            painter.rect_stroke(bounds, 2.0, stroke, egui::StrokeKind::Inside);
            let x = bounds.left() + 6.0;
            painter.line_segment(
                [egui::pos2(x, bounds.top()), egui::pos2(x, bounds.bottom())],
                stroke,
            );
        }
        RoomIcon::Grid => {
            for x in [-4.5, 4.5] {
                for y in [-4.5, 4.5] {
                    let cell =
                        egui::Rect::from_center_size(center + egui::vec2(x, y), Vec2::splat(7.0));
                    painter.rect_stroke(cell, 1.5, stroke, egui::StrokeKind::Inside);
                }
            }
        }
        RoomIcon::Stage => paint_stage_icon(painter, center, stroke.color),
        RoomIcon::Fullscreen => {
            for x in [-1.0, 1.0] {
                for y in [-1.0, 1.0] {
                    let corner = center + egui::vec2(x * 8.0, y * 8.0);
                    painter.line_segment([corner, corner - egui::vec2(x * 5.0, 0.0)], stroke);
                    painter.line_segment([corner, corner - egui::vec2(0.0, y * 5.0)], stroke);
                }
            }
        }
        RoomIcon::Gear => {
            let mut outline = Vec::with_capacity(32);
            for tooth in 0..8 {
                let angle = tooth as f32 * std::f32::consts::FRAC_PI_4;
                for (offset, radius) in [(-0.32, 6.6), (-0.21, 8.5), (0.21, 8.5), (0.32, 6.6)] {
                    let (sin, cos) = (angle + offset).sin_cos();
                    outline.push(center + egui::vec2(cos, sin) * radius);
                }
            }
            painter.add(egui::Shape::closed_line(outline, stroke));
            painter.circle_stroke(center, 2.8, stroke);
        }
        RoomIcon::Preview(visible) => {
            let eye = [
                (-9.0, 0.0),
                (-5.0, -4.0),
                (0.0, -5.0),
                (5.0, -4.0),
                (9.0, 0.0),
                (5.0, 4.0),
                (0.0, 5.0),
                (-5.0, 4.0),
            ];
            painter.add(egui::Shape::closed_line(
                eye.into_iter()
                    .map(|(x, y)| center + egui::vec2(x, y))
                    .collect(),
                stroke,
            ));
            if visible {
                painter.circle_stroke(center, 2.2, stroke);
            } else {
                painter.line_segment(
                    [
                        center + egui::vec2(-7.0, 7.0),
                        center + egui::vec2(7.0, -7.0),
                    ],
                    stroke,
                );
            }
        }
        RoomIcon::Speaker(muted) => {
            let speaker = [
                (-7.0, -3.0),
                (-4.0, -3.0),
                (0.0, -6.0),
                (0.0, 6.0),
                (-4.0, 3.0),
                (-7.0, 3.0),
            ];
            painter.add(egui::Shape::closed_line(
                speaker
                    .into_iter()
                    .map(|(x, y)| center + egui::vec2(x, y))
                    .collect(),
                stroke,
            ));
            if muted {
                for direction in [-1.0, 1.0] {
                    painter.line_segment(
                        [
                            center + egui::vec2(3.0, -3.0 * direction),
                            center + egui::vec2(8.0, 3.0 * direction),
                        ],
                        stroke,
                    );
                }
            } else {
                painter.add(egui::Shape::line(
                    [(3.0, -4.0), (6.0, -2.0), (6.0, 2.0), (3.0, 4.0)]
                        .into_iter()
                        .map(|(x, y)| center + egui::vec2(x, y))
                        .collect(),
                    stroke,
                ));
            }
        }
        RoomIcon::Playback(true) => {
            painter.add(egui::Shape::closed_line(
                vec![
                    center + egui::vec2(-4.0, -7.0),
                    center + egui::vec2(7.0, 0.0),
                    center + egui::vec2(-4.0, 7.0),
                ],
                stroke,
            ));
        }
        RoomIcon::Playback(false) => {
            for x in [-3.5, 3.5] {
                painter.line_segment(
                    [center + egui::vec2(x, -7.0), center + egui::vec2(x, 7.0)],
                    egui::Stroke::new(2.5, stroke.color),
                );
            }
        }
        RoomIcon::Share => {
            let screen = egui::Rect::from_center_size(center, egui::vec2(18.0, 12.0));
            painter.rect_stroke(screen, 2.0, stroke, egui::StrokeKind::Inside);
            let tip = center + egui::vec2(0.0, -3.0);
            painter.line_segment([tip, center + egui::vec2(0.0, 3.0)], stroke);
            painter.line_segment([tip, tip + egui::vec2(-3.0, 3.0)], stroke);
            painter.line_segment([tip, tip + egui::vec2(3.0, 3.0)], stroke);
            painter.line_segment(
                [
                    center + egui::vec2(-4.0, 9.0),
                    center + egui::vec2(4.0, 9.0),
                ],
                stroke,
            );
        }
        RoomIcon::Stop => {
            painter.rect_stroke(
                egui::Rect::from_center_size(center, egui::vec2(11.0, 11.0)),
                2.0,
                stroke,
                egui::StrokeKind::Inside,
            );
        }
        RoomIcon::Camera(on) => {
            let body =
                egui::Rect::from_min_size(center + egui::vec2(-9.0, -6.0), egui::vec2(12.0, 12.0));
            painter.rect_stroke(body, 2.5, stroke, egui::StrokeKind::Inside);
            painter.add(egui::Shape::closed_line(
                vec![
                    center + egui::vec2(4.0, -2.0),
                    center + egui::vec2(9.0, -5.0),
                    center + egui::vec2(9.0, 5.0),
                    center + egui::vec2(4.0, 2.0),
                ],
                stroke,
            ));
            if !on {
                painter.line_segment(
                    [
                        center + egui::vec2(-9.0, 8.0),
                        center + egui::vec2(9.0, -8.0),
                    ],
                    stroke,
                );
            }
        }
        RoomIcon::More => {
            for x in [-6.0, 0.0, 6.0] {
                painter.circle_filled(center + egui::vec2(x, 0.0), 1.8, stroke.color);
            }
        }
        RoomIcon::Leave => {
            let door =
                egui::Rect::from_min_size(center + egui::vec2(-8.0, -8.0), egui::vec2(9.0, 16.0));
            painter.line_segment([door.left_top(), door.right_top()], stroke);
            painter.line_segment([door.left_top(), door.left_bottom()], stroke);
            painter.line_segment([door.left_bottom(), door.right_bottom()], stroke);
            let arrow = center + egui::vec2(8.0, 0.0);
            painter.line_segment([center + egui::vec2(-2.0, 0.0), arrow], stroke);
            painter.line_segment([arrow, arrow + egui::vec2(-4.0, -4.0)], stroke);
            painter.line_segment([arrow, arrow + egui::vec2(-4.0, 4.0)], stroke);
        }
        RoomIcon::Reconnect => {
            let arc: Vec<_> = (0..=16)
                .map(|step| {
                    let angle = (35.0 + step as f32 * (280.0 / 16.0)).to_radians();
                    center + egui::vec2(angle.cos(), angle.sin()) * 7.0
                })
                .collect();
            let tip = arc[arc.len() - 1];
            painter.add(egui::Shape::line(arc, stroke));
            painter.line_segment([tip, tip + egui::vec2(-5.0, 0.0)], stroke);
            painter.line_segment([tip, tip + egui::vec2(0.0, -5.0)], stroke);
        }
    }
}

/// Share the visible room name so someone with access can find it in the catalog.
pub(crate) fn copy_room_name(ui: &mut egui::Ui, name: &str) {
    let response = menu_action(
        ui,
        crate::i18n::label("Copiar nome da sala", "Copy room name"),
        true,
    )
    .on_hover_text(crate::i18n::label(
        "Quem tem acesso ao servidor pode procurar este nome na lista de salas.",
        "People with server access can find this name in the room list.",
    ));
    if response.clicked() {
        ui.ctx().copy_text(name.to_owned());
        let now = ui.input(|input| input.time);
        ui.ctx().data_mut(|data| data.insert_temp(response.id, now));
        crate::kit::notify(
            ui.ctx(),
            "copied",
            crate::i18n::label("Nome da sala copiado", "Room name copied"),
            crate::kit::NoticeKind::Success,
        );
    }
    let copied = ui.ctx().data(|data| data.get_temp::<f64>(response.id));
    if !cfg!(windows) && copied.is_some_and(|at| ui.input(|input| input.time) - at < 2.0) {
        ui.label(crate::i18n::label("Nome copiado", "Name copied"));
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_secs(2));
    }
}

pub(crate) fn paint_stage_icon(painter: &egui::Painter, center: egui::Pos2, color: Color32) {
    let bounds = egui::Rect::from_center_size(center, egui::vec2(18.0, 13.0));
    painter.rect_stroke(
        bounds,
        2.0,
        egui::Stroke::new(1.5, color),
        egui::StrokeKind::Inside,
    );
    let card = egui::Rect::from_min_max(
        bounds.right_bottom() - egui::vec2(8.0, 6.0),
        bounds.right_bottom() - egui::vec2(2.0, 2.0),
    );
    painter.rect_filled(card, 1.0, color);
}
