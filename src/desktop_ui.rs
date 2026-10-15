//! Shared desktop controls used by the main window and room picker.
use eframe::egui::{self,Color32,RichText,Vec2};
pub const PANEL:Color32=Color32::from_rgb(21,27,28);
pub const BORDER:Color32=Color32::from_rgb(42,50,50);
pub const MUTED:Color32=Color32::from_rgb(154,167,161);
pub const ACCENT:Color32=Color32::from_rgb(191,239,121);
pub const SURFACE:Color32=Color32::from_rgb(28,35,36);

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
    ui.add_enabled_ui(enabled,|ui|ui.add_sized(size,
        egui::Button::new(RichText::new(label).size(13.0).color(ACCENT)).corner_radius(8)
            .fill(ACCENT.gamma_multiply(0.16)).stroke(egui::Stroke::new(1.0,ACCENT.gamma_multiply(0.45))))).inner
}
