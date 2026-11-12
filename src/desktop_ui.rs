//! Shared desktop controls used by the main window and room picker.
use eframe::egui::{self, Color32, RichText, Vec2};
pub const PANEL: Color32 = Color32::from_rgb(21, 27, 28);
pub const BORDER: Color32 = Color32::from_rgb(42, 50, 50);
pub const MUTED: Color32 = Color32::from_rgb(154, 167, 161);
pub const ACCENT: Color32 = Color32::from_rgb(191, 239, 121);
pub const SURFACE: Color32 = Color32::from_rgb(28, 35, 36);
pub const TEXT: Color32 = Color32::from_rgb(234, 239, 234);
/// A 32px target around a small 10px X, closing its enclosing Window.
pub fn modal_header(ui: &mut egui::Ui, title: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(title).size(16.0).strong().color(TEXT));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let response = ui
                .scope(|ui| {
                    ui.spacing_mut().button_padding = Vec2::ZERO;
                    ui.spacing_mut().interact_size = Vec2::splat(32.0);
                    ui.add_sized(
                        [32.0, 32.0],
                        egui::Button::new("")
                            .corner_radius(8)
                            .fill(if ui.is_enabled() {
                                SURFACE
                            } else {
                                Color32::TRANSPARENT
                            })
                            .stroke(egui::Stroke::NONE),
                    )
                })
                .inner;
            response.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::WidgetType::Button,
                    true,
                    crate::i18n_format!("Fechar {title}", "Close {title}"),
                )
            });
            let stroke = egui::Stroke::new(
                1.5,
                if response.hovered() || response.has_focus() {
                    TEXT
                } else {
                    MUTED
                },
            );
            for direction in [-1.0, 1.0] {
                let offset = Vec2::new(5.0, 5.0 * direction);
                ui.painter().line_segment(
                    [
                        response.rect.center() - offset,
                        response.rect.center() + offset,
                    ],
                    stroke,
                );
            }
            if response.clicked() {
                ui.close_kind(egui::UiKind::Window);
            }
        });
    });
}
pub fn upload_selector(
    ui: &mut egui::Ui,
    profile: &mut crate::bandwidth::UploadProfile,
    enabled: bool,
) -> bool {
    let previous = *profile;
    ui.add_enabled_ui(enabled, |ui| {
        ui.label(
            RichText::new(crate::i18n::label(
                "Limite de upload · vídeo",
                "Upload limit · video",
            ))
            .size(11.0)
            .color(MUTED),
        );
        egui::ComboBox::from_id_salt("upload_profile")
            .width(ui.available_width())
            .selected_text(profile.label())
            .show_ui(ui, |ui| {
                for choice in crate::bandwidth::UploadProfile::ALL {
                    ui.selectable_value(profile, choice, choice.label());
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
    previous != *profile
}
/// Shared, explicit mode selection; publishing must stop before changing it.
pub fn rate_mode_selector(
    ui: &mut egui::Ui,
    mode: &mut crate::adaptive_upload::Mode,
    enabled: bool,
    allow_automatic: bool,
) -> bool {
    let previous = *mode;
    ui.add_enabled_ui(enabled, |ui| {
        ui.label(
            RichText::new(crate::i18n::label(
                "Controle de qualidade",
                "Quality control",
            ))
            .size(11.0)
            .color(MUTED),
        );
        egui::ComboBox::from_id_salt("adaptive_upload_mode")
            .width(ui.available_width())
            .selected_text(mode.label())
            .show_ui(ui, |ui| {
                for choice in [
                    crate::adaptive_upload::Mode::Manual,
                    crate::adaptive_upload::Mode::Automatic,
                ] {
                    ui.add_enabled_ui(
                        choice != crate::adaptive_upload::Mode::Automatic || allow_automatic,
                        |ui| {
                            ui.selectable_value(mode, choice, choice.label());
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
    previous != *mode
}
pub fn volume_slider(ui: &mut egui::Ui, volume: &mut u32) -> egui::Response {
    ui.scope(|ui| {
        ui.spacing_mut().slider_width = ui.available_width();
        ui.spacing_mut().slider_rail_height = 6.0;
        ui.visuals_mut().widgets.inactive.bg_fill = Color32::from_rgb(65, 78, 73);
        ui.visuals_mut().widgets.inactive.fg_stroke = egui::Stroke::new(1.5, ACCENT);
        ui.visuals_mut().selection.bg_fill = ACCENT;
        ui.add(
            egui::Slider::new(volume, 0..=100)
                .show_value(false)
                .trailing_fill(true)
                .handle_shape(egui::style::HandleShape::Circle),
        )
    })
    .inner
}

pub fn utility_frame(style: &egui::Style) -> egui::Frame {
    egui::Frame::window(style)
        .fill(PANEL)
        .inner_margin(12)
        .corner_radius(12)
        .stroke(egui::Stroke::new(1.0, BORDER))
}

pub(crate) fn utility_window(
    ctx: &egui::Context,
    title: &'static str,
    id: &'static str,
    width: f32,
) -> egui::Window<'static> {
    let area = ctx.content_rect();
    let width = width.min((area.width() - 56.0).max(96.0));
    egui::Window::new(title)
        .id(egui::Id::new(id))
        .title_bar(false)
        .frame(utility_frame(&ctx.style_of(egui::Theme::Dark)))
        .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 12.0))
        .default_width(width)
        .max_width(width)
        .max_height((area.height() - 54.0).max(64.0))
        .resizable(false)
        .collapsible(false)
        .vscroll(true)
}
pub fn action(ui: &mut egui::Ui, label: &str, enabled: bool, primary: bool) -> egui::Response {
    action_with_wrap(ui, label, enabled, primary, egui::TextWrapMode::Extend)
}
#[cfg(target_os = "linux")]
pub(crate) fn participant_action(
    ui: &mut egui::Ui,
    label: &str,
    enabled: bool,
    focused: bool,
) -> egui::Response {
    action_with_wrap(ui, label, enabled, focused, egui::TextWrapMode::Truncate).on_hover_text(label)
}
fn action_with_wrap(
    ui: &mut egui::Ui,
    label: &str,
    enabled: bool,
    primary: bool,
    wrap_mode: egui::TextWrapMode,
) -> egui::Response {
    let width = ui.available_width();
    ui.add_enabled_ui(enabled, |ui| {
        ui.add_sized(
            [width, 40.0],
            egui::Button::new(RichText::new(label).size(13.0).color(if primary {
                ACCENT
            } else {
                TEXT
            }))
            .wrap_mode(wrap_mode)
            .corner_radius(8)
            .fill(if primary {
                ACCENT.gamma_multiply(0.16)
            } else {
                SURFACE
            })
            .stroke(egui::Stroke::new(
                1.0,
                if primary {
                    ACCENT.gamma_multiply(0.45)
                } else {
                    BORDER
                },
            )),
        )
    })
    .inner
}
pub fn exit_fullscreen(ui: &mut egui::Ui) -> egui::Response {
    let response = ui
        .scope(|ui| {
            ui.spacing_mut().button_padding = Vec2::ZERO;
            ui.spacing_mut().interact_size = Vec2::splat(20.0);
            ui.add_sized(
                [20.0, 20.0],
                egui::Button::new("").corner_radius(5).fill(SURFACE),
            )
        })
        .inner;
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            true,
            crate::i18n::label("Sair da tela cheia", "Exit full screen"),
        )
    });
    let center = response.rect.center();
    let stroke = egui::Stroke::new(
        1.5,
        if response.hovered() || response.has_focus() {
            TEXT
        } else {
            MUTED
        },
    );
    for direction in [-1.0, 1.0] {
        let offset = Vec2::new(3.5, 3.5 * direction);
        ui.painter()
            .line_segment([center - offset, center + offset], stroke);
    }
    response.on_hover_text(crate::i18n::label(
        "Sair da tela cheia · Esc / F11",
        "Exit full screen · Esc / F11",
    ))
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
    ui.scope(|ui| {
        ui.spacing_mut().button_padding = Vec2::new(10.0, 4.0);
        ui.spacing_mut().interact_size.y = 26.0;
        ui.add_enabled(
            enabled,
            egui::Button::new(RichText::new(label).size(11.0).color(MUTED))
                .wrap_mode(egui::TextWrapMode::Extend)
                .min_size(Vec2::new(0.0, 26.0))
                .corner_radius(9)
                .fill(SURFACE)
                .stroke(egui::Stroke::new(1.0, BORDER)),
        )
    })
    .inner
}
pub fn primary(ui: &mut egui::Ui, label: &str, enabled: bool, size: Vec2) -> egui::Response {
    ui.add_enabled_ui(enabled, |ui| {
        ui.spacing_mut().button_padding = Vec2::new(8.0, 6.0);
        ui.add_sized(
            size,
            egui::Button::new(RichText::new(label).size(13.0).color(ACCENT))
                .wrap_mode(egui::TextWrapMode::Extend)
                .corner_radius(8)
                .fill(ACCENT.gamma_multiply(0.16))
                .stroke(egui::Stroke::new(1.0, ACCENT.gamma_multiply(0.45))),
        )
    })
    .inner
}

#[cfg(test)]
mod tests {
    use super::*;
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
                egui::Window::new("Teste")
                    .title_bar(false)
                    .open(open)
                    .resizable(false)
                    .collapsible(false)
                    .fixed_pos(egui::pos2(40.0, 40.0))
                    .fixed_size([260.0, 120.0])
                    .show(ui.ctx(), |ui| modal_header(ui, "Teste"));
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

    #[test]
    fn compact_room_keeps_actions_and_playback_inside_the_viewport() {
        let ctx = egui::Context::default();
        let area = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(320.0, 250.0));
        let _ = ctx.run_ui(egui::RawInput { screen_rect: Some(area), ..Default::default() }, |root| {
            let mut actions = Vec::new();
            session_panel(area.width()).show(root, |ui| {
                session_status(ui, true, true, "Compartilhando janela do aplicativo · 1920×1080");
                session_actions(ui, |ui| {
                    actions.push(session_stop(ui, "Parar compartilhamento", true).rect);
                    actions.push(session_button(ui, "Desligar câmera", true, false).rect);
                    actions.push(session_button(ui, "Mais opções", true, false).rect);
                    actions.push(session_stop(ui, "Sair da sala", true).rect);
                });
            });
            egui::Panel::left("test_sidebar").exact_size(sidebar_width(area.width())).show(root, |_ui| {
                #[cfg(target_os = "linux")]
                {
                    let bounds = _ui.available_rect_before_wrap();
                    let participant = participant_action(_ui, "Participante com nome longo · Em destaque · transmitindo", true, true);
                    assert!(bounds.contains_rect(participant.rect), "participant {:?}, sidebar {bounds:?}", participant.rect);
                }
            });
            egui::CentralPanel::default()
                .frame(egui::Frame::new().inner_margin(room_margin(area.width(), false)))
                .show(root, |ui| {
                    room_header(ui, "Nome longo da sala de compartilhamento", "Conectado à sala", |ui, compact| {
                        assert!(compact);
                        let (panel, modes) = room_view_controls(ui, true, crate::subscription::ViewMode::Grid);
                        actions.push(panel.rect);
                        actions.extend(modes.into_iter().map(|(_, response)| response.rect));
                    });
                    let playback = ui.available_rect_before_wrap();
                    assert!(playback.height() >= 56.0, "{playback:?}");
                    assert!(area.contains_rect(playback));
                    for (index, rect) in actions.iter().enumerate() {
                        assert!(area.contains_rect(*rect) && !rect.intersects(playback),
                            "action {index} at {rect:?}, viewport {area:?}, playback {playback:?}");
                    }
                });
        });
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
            egui::ComboBox::from_id_salt("send_resolution")
                .truncate()
                .width(columns[0].available_width())
                .selected_text(value.size.label())
                .show_ui(&mut columns[0], |ui| {
                    for size in [
                        crate::resolution::OutputSize::Native,
                        crate::resolution::OutputSize::FullHd,
                        crate::resolution::OutputSize::QuadHd,
                    ] {
                        ui.selectable_value(&mut value.size, size, size.label());
                    }
                });
            columns[1].label(
                RichText::new(crate::i18n::label("FPS de envio", "Send FPS"))
                    .size(11.0)
                    .color(MUTED),
            );
            egui::ComboBox::from_id_salt("send_fps")
                .truncate()
                .width(columns[1].available_width())
                .selected_text(format!("{} FPS", value.fps))
                .show_ui(&mut columns[1], |ui| {
                    for fps in [30, 60, 120].into_iter().filter(|fps| *fps <= max_fps) {
                        ui.selectable_value(&mut value.fps, fps, format!("{fps} FPS"));
                    }
                });
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
        ui.add(egui::Label::new(RichText::new(notice).size(11.0).color(MUTED)).wrap());
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

pub(crate) fn spotlight_card(area: egui::Rect, index: usize, count: usize) -> egui::Rect {
    if count <= 1 {
        return area;
    }
    let gap = 8.0;
    if area.width() >= 520.0 && area.height() >= 160.0 {
        let rail_width = (area.width() * 0.26).clamp(128.0, 224.0);
        let main = egui::Rect::from_min_max(
            area.min,
            egui::pos2(area.right() - rail_width - gap, area.bottom()),
        );
        if index == 0 {
            return main;
        }
        let rail = egui::Rect::from_min_max(egui::pos2(main.right() + gap, area.top()), area.max);
        return video_grid_card(rail, index - 1, count - 1, gap);
    }
    let gap = gap.min(area.height() * 0.1);
    let rail_height = (area.height() * 0.28).min(112.0);
    let main = egui::Rect::from_min_max(
        area.min,
        egui::pos2(area.right(), area.bottom() - rail_height - gap),
    );
    if index == 0 {
        return main;
    }
    let width = ((area.width() - gap * (count - 2) as f32) / (count - 1) as f32).max(0.0);
    let start = area.left() + (index - 1) as f32 * (width + gap);
    egui::Rect::from_min_size(
        egui::pos2(start, main.bottom() + gap),
        Vec2::new(width, rail_height),
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

pub(crate) fn sidebar_width(width: f32) -> f32 {
    (width * 0.29).clamp(100.0, 272.0)
}

pub(crate) fn room_header(
    ui: &mut egui::Ui,
    title: &str,
    status: &str,
    content: impl FnOnce(&mut egui::Ui, bool),
) {
    let compact = ui.available_width() < 520.0;
    let show_status = ui.available_height() >= 180.0 || status != room_state(true, false);
    ui.spacing_mut().item_spacing = Vec2::new(6.0, 5.0);
    ui.add(
        egui::Label::new(
            RichText::new(title)
                .size(if compact { 18.0 } else { 24.0 })
                .strong(),
        )
        .truncate(),
    )
    .on_hover_text(title);
    if show_status {
        ui.add(egui::Label::new(RichText::new(status).size(13.0).color(MUTED)).truncate())
            .on_hover_text(status);
    }
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().button_padding = Vec2::new(8.0, 4.0);
        content(ui, compact);
    });
    ui.add_space(if compact { 4.0 } else { 8.0 });
}

pub(crate) fn compact_room_header(viewport_width: f32, sidebar_open: bool) -> bool {
    let sidebar = if sidebar_open {
        sidebar_width(viewport_width)
    } else {
        0.0
    };
    viewport_width - sidebar - 2.0 * f32::from(room_margin(viewport_width, false)) < 520.0
}

pub(crate) fn room_view_controls(
    ui: &mut egui::Ui,
    sidebar_open: bool,
    mode: crate::subscription::ViewMode,
) -> (
    egui::Response,
    [(crate::subscription::ViewMode, egui::Response); 2],
) {
    use crate::{i18n::label, subscription::ViewMode};
    let narrow = ui.available_width() < 300.0;
    let panel_label = if sidebar_open {
        label("Ocultar painel", "Hide panel")
    } else {
        label("Pessoas e opções", "People and settings")
    };
    let panel = if narrow {
        header_button(ui, label("Painel", "Panel"), true, Some(sidebar_open))
            .on_hover_text(panel_label)
    } else {
        header_button(ui, panel_label, true, None)
    };
    let modes = [
        (
            ViewMode::Grid,
            if narrow {
                label("Grade", "Grid")
            } else {
                label("Em grade", "Grid")
            },
        ),
        (
            ViewMode::Spotlight,
            if narrow {
                label("Foco", "Focus")
            } else {
                label("Em destaque", "Focus")
            },
        ),
    ]
    .map(|(option, title)| (option, header_button(ui, title, true, Some(option == mode))));
    (panel, modes)
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

#[cfg(test)]
#[test]
fn spotlight_keeps_previews_in_bounds_and_prioritizes_the_main_stream() {
    for size in [
        egui::vec2(980.0, 460.0),
        egui::vec2(310.0, 128.0),
        egui::vec2(120.0, 32.0),
    ] {
        let area = egui::Rect::from_min_size(egui::pos2(20.0, 30.0), size);
        assert_eq!(spotlight_card(area, 0, 1), area);
        let cards: Vec<_> = (0..3).map(|index| spotlight_card(area, index, 3)).collect();
        assert!(cards[0].area() > area.area() * 0.55);
        for (index, card) in cards.iter().enumerate() {
            assert!(card.is_positive() && area.expand(0.01).contains_rect(*card));
            assert!(
                cards[..index]
                    .iter()
                    .all(|previous| !card.intersects(*previous))
            );
        }
    }
}

pub(crate) fn session_panel(width: f32) -> egui::Panel {
    let compact = width < 626.0;
    egui::Panel::bottom("session_actions")
        .resizable(false)
        .default_size(if compact { 106.0 } else { 88.0 })
        .frame(
            egui::Frame::new()
                .fill(PANEL)
                .stroke(egui::Stroke::new(1.0, BORDER))
                .inner_margin(if compact {
                    egui::Margin::symmetric(12, 6)
                } else {
                    egui::Margin::symmetric(16, 10)
                }),
        )
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

pub(crate) fn session_status(ui: &mut egui::Ui, sharing: bool, camera_active: bool, status: &str) {
    let status = (!status.is_empty() && status != crate::i18n::label("Conectado", "Connected"))
        .then_some(status);
    if !sharing && !camera_active && status.is_none() {
        return;
    }
    if ui.available_width() < 440.0 {
        let sharing_label = crate::i18n::label("Compartilhando tela", "Sharing screen");
        let camera_label = crate::i18n::label("Câmera ligada", "Camera on");
        let summary = match (sharing, camera_active) {
            (true, true) => format!("{sharing_label} · {camera_label}"),
            (true, false) => sharing_label.to_owned(),
            (false, true) => camera_label.to_owned(),
            (false, false) => status.unwrap_or_default().to_owned(),
        };
        let detail = status.filter(|_| sharing || camera_active);
        ui.horizontal(|ui| {
            ui.add(
                egui::Label::new(RichText::new(&summary).strong().color(
                    if sharing || camera_active {
                        ACCENT
                    } else {
                        MUTED
                    },
                ))
                .truncate(),
            )
            .on_hover_text(detail.unwrap_or(&summary));
            if let Some(detail) = detail {
                ui.add(egui::Label::new(RichText::new(detail).size(13.0).color(MUTED)).truncate())
                    .on_hover_text(detail);
            }
        });
        ui.add_space(2.0);
        return;
    }
    ui.horizontal_wrapped(|ui| {
        if sharing {
            ui.label(
                RichText::new(crate::i18n::label("Compartilhando tela", "Sharing screen"))
                    .strong()
                    .color(ACCENT),
            );
        }
        if camera_active {
            ui.label(
                RichText::new(crate::i18n::label("Câmera ligada", "Camera on"))
                    .strong()
                    .color(ACCENT),
            );
        }
        if let Some(status) = status {
            ui.add(egui::Label::new(RichText::new(status).size(13.0).color(MUTED)).truncate())
                .on_hover_text(status);
        }
    });
    ui.add_space(2.0);
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

pub(crate) fn session_actions(ui: &mut egui::Ui, content: impl FnOnce(&mut egui::Ui)) {
    let width = ui.available_width();
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(6.0, 5.0);
        if width < 600.0 {
            ui.spacing_mut().button_padding = Vec2::new(6.0, 4.0);
            ui.spacing_mut().interact_size =
                Vec2::new(((width - 7.0) / 2.0).floor().max(0.0), 32.0);
        } else {
            ui.spacing_mut().interact_size.y = 40.0;
        }
        content(ui);
    });
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
        if primary { PANEL } else { TEXT },
        if primary { ACCENT } else { SURFACE },
    )
}

pub(crate) fn session_stop(ui: &mut egui::Ui, label: &str, enabled: bool) -> egui::Response {
    session_action(
        ui,
        label,
        enabled,
        Color32::from_rgb(255, 178, 170),
        Color32::from_rgb(59, 34, 34),
    )
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
        .show(|ui| {
            ui.spacing_mut().button_padding = Vec2::new(8.0, 4.0);
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
            egui::ScrollArea::vertical()
                .max_height((area.height() - 44.0).max(48.0))
                .show(ui, content);
        })
        .map(|popup| popup.response.rect)
}

pub(crate) fn display_scale(ui: &mut egui::Ui) {
    ui.label(crate::i18n::label("Escala da interface", "Interface scale"));
    ui.horizontal_wrapped(|ui| {
        let current = ui.ctx().zoom_factor();
        for (factor, label) in [(1.0, "100%"), (1.5, "150%"), (2.0, "200%")] {
            if ui
                .selectable_label((current - factor).abs() < 0.01, label)
                .clicked()
            {
                ui.ctx().set_zoom_factor(factor);
                ui.close();
            }
        }
    });
}

fn session_action(
    ui: &mut egui::Ui,
    label: &str,
    enabled: bool,
    color: Color32,
    fill: Color32,
) -> egui::Response {
    let text = egui::WidgetText::from(RichText::new(label).size(14.0).color(color));
    let width = ui.spacing().interact_size.x;
    let (text, elided) = if width > 64.0 {
        let galley = text.into_galley(
            ui,
            Some(egui::TextWrapMode::Truncate),
            width - 2.0 * ui.spacing().button_padding.x,
            egui::TextStyle::Button,
        );
        let elided = galley.elided;
        (egui::WidgetText::from(galley), elided)
    } else {
        (text, false)
    };
    let response = ui.add_enabled(
        enabled,
        egui::Button::new(text)
            .fill(fill)
            .wrap_mode(egui::TextWrapMode::Wrap)
            .min_size(Vec2::new(
                ui.spacing().interact_size.x,
                ui.spacing().interact_size.y.max(30.0),
            ))
            .corner_radius(8),
    );
    if response.has_focus() {
        ui.painter().rect_stroke(
            response.rect.expand(2.0),
            9.0,
            egui::Stroke::new(2.0, TEXT),
            egui::StrokeKind::Outside,
        );
    }
    if elided {
        response.on_hover_text(label)
    } else {
        response
    }
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
    let modal = egui::Modal::new(egui::Id::new("confirm_leave")).show(ctx, |ui| {
        ui.set_width((ctx.content_rect().width() - 36.0).clamp(120.0, 320.0));
        ui.heading(crate::i18n::label("Sair da sala?", "Leave the room?"));
        ui.label(crate::i18n::label(
            "Seu compartilhamento será encerrado ao sair.",
            "Your sharing will end when you leave.",
        ));
        ui.add_space(12.0);
        ui.horizontal_wrapped(|ui| {
            if session_button(
                ui,
                crate::i18n::label("Continuar na sala", "Stay in the room"),
                true,
                false,
            )
            .clicked()
            {
                return Some(false);
            }
            if session_stop(ui, crate::i18n::label("Sair da sala", "Leave room"), true).clicked() {
                return Some(true);
            }
            None
        })
        .inner
    });
    modal
        .inner
        .or_else(|| modal.should_close().then_some(false))
}

pub(crate) fn header_button(
    ui: &mut egui::Ui,
    label: &str,
    enabled: bool,
    selected: Option<bool>,
) -> egui::Response {
    let active = selected == Some(true);
    let mut button = egui::Button::new(RichText::new(label).size(13.0).color(if active {
        ACCENT
    } else {
        TEXT
    }))
    .fill(if active {
        Color32::from_rgb(40, 51, 40)
    } else {
        SURFACE
    })
    .stroke(egui::Stroke::new(
        1.0,
        if active {
            ACCENT.gamma_multiply(0.5)
        } else {
            BORDER
        },
    ))
    .wrap_mode(egui::TextWrapMode::Extend)
    .min_size(Vec2::new(0.0, 34.0))
    .corner_radius(8);
    if let Some(selected) = selected {
        button = button.selected(selected);
    }
    ui.add_enabled(enabled, button)
}

/// Share the visible room name so someone with access can find it in the catalog.
pub(crate) fn copy_room_name(ui: &mut egui::Ui, name: &str) {
    let response = header_button(
        ui,
        crate::i18n::label("Copiar nome da sala", "Copy room name"),
        true,
        None,
    )
    .on_hover_text(crate::i18n::label(
        "Quem tem acesso ao servidor pode procurar este nome na lista de salas.",
        "People with server access can find this name in the room list.",
    ));
    if response.clicked() {
        ui.ctx().copy_text(name.to_owned());
        let now = ui.input(|input| input.time);
        ui.ctx().data_mut(|data| data.insert_temp(response.id, now));
    }
    let copied = ui.ctx().data(|data| data.get_temp::<f64>(response.id));
    if copied.is_some_and(|at| ui.input(|input| input.time) - at < 2.0) {
        ui.label(crate::i18n::label("Nome copiado", "Name copied"));
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_secs(2));
    }
}
