//! Shared desktop controls used by the main window and room picker.
use eframe::egui::{self,Color32,RichText,Vec2};
pub const PANEL:Color32=Color32::from_rgb(21,27,28);
pub const BORDER:Color32=Color32::from_rgb(42,50,50);
pub const MUTED:Color32=Color32::from_rgb(154,167,161);
pub const ACCENT:Color32=Color32::from_rgb(191,239,121);
pub const SURFACE:Color32=Color32::from_rgb(28,35,36);
pub const TEXT:Color32=Color32::from_rgb(234,239,234);
/// A 32px target around a small 10px X, closing its enclosing Window.
pub fn modal_header(ui:&mut egui::Ui,title:&str) {
    ui.horizontal(|ui|{
        ui.label(RichText::new(title).size(16.0).strong().color(TEXT));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{
            let response=ui.scope(|ui|{
                ui.spacing_mut().button_padding=Vec2::ZERO;
                ui.spacing_mut().interact_size=Vec2::splat(32.0);
                ui.add_sized([32.0,32.0],egui::Button::new("").corner_radius(8)
                    .fill(if ui.is_enabled(){SURFACE}else{Color32::TRANSPARENT}).stroke(egui::Stroke::NONE))
            }).inner;
            response.widget_info(||egui::WidgetInfo::labeled(egui::WidgetType::Button,true,format!("Fechar {title}")));
            let stroke=egui::Stroke::new(1.5,if response.hovered()||response.has_focus(){TEXT}else{MUTED});
            for direction in [-1.0,1.0]{let offset=Vec2::new(5.0,5.0*direction);ui.painter().line_segment([response.rect.center()-offset,response.rect.center()+offset],stroke);}
            if response.clicked(){ui.close_kind(egui::UiKind::Window);}
        });
    });
}
pub fn upload_selector(ui:&mut egui::Ui,profile:&mut crate::bandwidth::UploadProfile,enabled:bool)->bool {
    let previous=*profile;
    ui.add_enabled_ui(enabled,|ui|{
        ui.label(RichText::new("Limite de upload · vídeo").size(11.0).color(MUTED));
        egui::ComboBox::from_id_salt("upload_profile").width(ui.available_width()).selected_text(profile.label()).show_ui(ui,|ui|{
            for choice in crate::bandwidth::UploadProfile::ALL{ui.selectable_value(profile,choice,choice.label());}
        });
        ui.label(RichText::new(if enabled{"Mantém resolução e FPS. Menos Mbps pode reduzir os detalhes."}else{"Pare o compartilhamento para alterar o limite."}).size(11.0).color(MUTED));
    });
    previous!=*profile
}
pub fn volume_slider(ui:&mut egui::Ui,volume:&mut u32)->egui::Response {
    ui.scope(|ui|{
        ui.spacing_mut().slider_width=ui.available_width();ui.spacing_mut().slider_rail_height=6.0;
        ui.visuals_mut().widgets.inactive.bg_fill=Color32::from_rgb(65,78,73);
        ui.visuals_mut().widgets.inactive.fg_stroke=egui::Stroke::new(1.5,ACCENT);
        ui.visuals_mut().selection.bg_fill=ACCENT;
        ui.add(egui::Slider::new(volume,0..=100).show_value(false).trailing_fill(true).handle_shape(egui::style::HandleShape::Circle))
    }).inner
}

pub fn utility_frame(style:&egui::Style)->egui::Frame {
    egui::Frame::window(style).fill(PANEL).inner_margin(12).corner_radius(12)
        .stroke(egui::Stroke::new(1.0,BORDER))
}
pub fn action(ui:&mut egui::Ui,label:&str,enabled:bool,primary:bool)->egui::Response {
    let width=ui.available_width();
    ui.add_enabled_ui(enabled,|ui|ui.add_sized([width,40.0],
        egui::Button::new(RichText::new(label).size(13.0).color(if primary{ACCENT}else{TEXT}))
            .wrap_mode(egui::TextWrapMode::Extend).corner_radius(8)
            .fill(if primary{ACCENT.gamma_multiply(0.16)}else{SURFACE})
            .stroke(egui::Stroke::new(1.0,if primary{ACCENT.gamma_multiply(0.45)}else{BORDER})))).inner
}
pub fn exit_fullscreen(ui:&mut egui::Ui)->egui::Response {
    let response=ui.scope(|ui|{
        ui.spacing_mut().button_padding=Vec2::ZERO;ui.spacing_mut().interact_size=Vec2::splat(20.0);
        ui.add_sized([20.0,20.0],egui::Button::new("").corner_radius(5).fill(SURFACE))
    }).inner;
    response.widget_info(||egui::WidgetInfo::labeled(egui::WidgetType::Button,true,"Sair da tela cheia"));
    let center=response.rect.center();let stroke=egui::Stroke::new(1.5,if response.hovered()||response.has_focus(){TEXT}else{MUTED});
    for direction in [-1.0,1.0]{let offset=Vec2::new(3.5,3.5*direction);ui.painter().line_segment([center-offset,center+offset],stroke);}
    response.on_hover_text("Sair da tela cheia · Esc / F11")
}
pub fn quality_strip(ui:&mut egui::Ui,value:&crate::video_metrics::QualitySnapshot,reconnecting:bool,publishing:bool,reception_paused:bool){
    ui.horizontal_wrapped(|ui|{
        ui.spacing_mut().item_spacing.x=16.0;
        ui.label(RichText::new(if reconnecting{"Sala · reconectando…"}else{"Sala · conectada"}).size(11.0).color(if reconnecting{Color32::from_rgb(236,190,105)}else{ACCENT}));
        if publishing {
            let text=if reconnecting{"Envio aguardando conexão".into()}else if value.send_paused{"Envio pausado".into()}else if let Some(rate)=value.send{
                let size=value.resolution.map(|(w,h)|format!("{w}×{h} · ")).unwrap_or_default();
                format!("Envio · {size}{:.0} FPS · {:.1} Mbps",rate.fps,rate.mbps)
            }else{"Envio · medindo…".into()};
            ui.label(RichText::new(text).size(11.0).color(MUTED)).on_hover_text("Quadros e bytes de vídeo enviados na última janela, sem overhead da rede. Não medem fidelidade da imagem nem latência fim a fim.");
        }
        let receive=if reception_paused{"Recepção pausada".into()}else if reconnecting{"Recepção aguardando conexão".into()}else if let Some(rate)=value.receive{format!("Recepção · {:.0} quadros/s no total",rate.fps)}else{"Recepção · medindo…".into()};
        ui.label(RichText::new(receive).size(11.0).color(MUTED)).on_hover_text("Soma dos quadros decodificados de todas as transmissões recebidas; não é FPS de uma única tela nem FPS de apresentação.");
    });
}

/// Embedded brand asset, decoded once per UI context and shared by both desktops.
pub fn brand_mark(ui:&mut egui::Ui,size:f32)->egui::Response {
    let id=egui::Id::new("hypercast_brand_texture");
    let texture=ui.ctx().data_mut(|data|data.get_temp::<egui::TextureHandle>(id));
    let texture=texture.or_else(||{
        let icon=eframe::icon_data::from_png_bytes(include_bytes!("../assets/hypercast.png")).ok()?;
        let image=egui::ColorImage::from_rgba_unmultiplied([icon.width as usize,icon.height as usize],&icon.rgba);
        let texture=ui.ctx().load_texture("HyperCast brand",image,egui::TextureOptions::LINEAR);
        ui.ctx().data_mut(|data|data.insert_temp(id,texture.clone()));Some(texture)
    });
    if let Some(texture)=texture {ui.add(egui::Image::from_texture((texture.id(),Vec2::splat(size))))}
    else {ui.allocate_response(Vec2::splat(size),egui::Sense::hover())}
}

pub fn input(ui:&mut egui::Ui,text:&mut String,hint:&str,password:bool,limit:usize,enabled:bool)->egui::Response {
    ui.scope(|ui|{
        ui.visuals_mut().widgets.inactive.corner_radius=egui::CornerRadius::same(8);
        ui.visuals_mut().widgets.hovered.corner_radius=egui::CornerRadius::same(8);
        ui.visuals_mut().widgets.active.corner_radius=egui::CornerRadius::same(8);
        ui.visuals_mut().widgets.inactive.bg_stroke=egui::Stroke::new(1.0,BORDER);
        ui.visuals_mut().widgets.hovered.bg_stroke=egui::Stroke::new(1.0,MUTED);
        ui.visuals_mut().selection.stroke=egui::Stroke::new(1.5,ACCENT);
        ui.add_enabled(enabled,egui::TextEdit::singleline(text).password(password).char_limit(limit)
            .desired_width(f32::INFINITY).margin(egui::Margin::symmetric(12,10))
            .background_color(SURFACE).hint_text(hint))
    }).inner
}
pub fn compact(ui:&mut egui::Ui,label:&str,enabled:bool)->egui::Response {
    ui.scope(|ui|{
        ui.spacing_mut().button_padding=Vec2::new(10.0,4.0);
        ui.spacing_mut().interact_size.y=26.0;
        ui.add_enabled(enabled,egui::Button::new(RichText::new(label).size(11.0).color(MUTED))
            .min_size(Vec2::new(0.0,26.0)).corner_radius(9).fill(SURFACE)
            .stroke(egui::Stroke::new(1.0,BORDER)))
    }).inner
}
pub fn primary(ui:&mut egui::Ui,label:&str,enabled:bool,size:Vec2)->egui::Response {
    ui.add_enabled_ui(enabled,|ui|{ui.spacing_mut().button_padding=Vec2::new(8.0,6.0);ui.add_sized(size,
        egui::Button::new(RichText::new(label).size(13.0).color(ACCENT)).wrap_mode(egui::TextWrapMode::Extend).corner_radius(8)
            .fill(ACCENT.gamma_multiply(0.16)).stroke(egui::Stroke::new(1.0,ACCENT.gamma_multiply(0.45))))}).inner
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn modal_closes_from_target_corners_outside_the_small_x(){
        for (offset,closes) in [(Vec2::new(-13.0,-13.0),true),(Vec2::new(13.0,-13.0),true),(Vec2::new(-13.0,13.0),true),(Vec2::new(13.0,13.0),true),(Vec2::new(22.0,0.0),false)]{
            let ctx=egui::Context::default();let mut open=true;
            let render=|ui:&mut egui::Ui,open:&mut bool|{
                egui::Window::new("Teste").title_bar(false).open(open).resizable(false).collapsible(false)
                    .fixed_pos(egui::pos2(40.0,40.0)).fixed_size([260.0,120.0]).show(ui.ctx(),|ui|modal_header(ui,"Teste"));
            };
            let mut center=None;
            for _ in 0..3{
                let output=ctx.run_ui(Default::default(),|ui|render(ui,&mut open));
                for s in output.shapes{if let egui::Shape::LineSegment{points,..}=s.shape{
                    if ((points[1].x-points[0].x).abs()-10.0).abs()<0.1 && ((points[1].y-points[0].y).abs()-10.0).abs()<0.1{center=Some(points[0]+(points[1]-points[0])*0.5);}
                }}
            }
            let pos=center.expect("Visible X")+offset;
            for pressed in [true,false]{
                let input=egui::RawInput{events:vec![egui::Event::PointerMoved(pos),egui::Event::PointerButton{pos,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()}],..Default::default()};
                let _=ctx.run_ui(input,|ui|render(ui,&mut open));
            }
            assert_eq!(!open,closes,"Unexpected close state at {offset:?}");
        }
    }
    #[test]
    fn current_room_label_stays_on_one_line_inside_the_button() {
        let ctx=egui::Context::default();let mut button=egui::Rect::NOTHING;
        let output=ctx.run_ui(Default::default(),|ui|{button=primary(ui,"Nesta sala",false,egui::vec2(88.0,36.0)).rect;});
        let label=output.shapes.iter().find_map(|s|match &s.shape {egui::Shape::Text(t) if t.galley.text()=="Nesta sala"=>Some(t),_=>None}).unwrap();
        assert_eq!(label.galley.rows.len(),1);
        assert!(button.contains_rect(label.visual_bounding_rect()));
    }
}
