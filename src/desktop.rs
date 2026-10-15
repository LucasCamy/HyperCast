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
const SURFACE: Color32 = Color32::from_rgb(28, 35, 36);
const DANGER: Color32 = Color32::from_rgb(235, 127, 119);

fn compact_action(ui:&mut egui::Ui,label:&str,enabled:bool)->egui::Response {
    crate::desktop_ui::compact(ui,label,enabled)
}

fn utility_window_frame(style:&egui::Style)->egui::Frame {
    egui::Frame::window(style).fill(PANEL).inner_margin(12).corner_radius(12)
        .stroke(egui::Stroke::new(1.0,BORDER))
}

fn update_action(ui: &mut egui::Ui, label: &str, primary: bool) -> egui::Response {
    ui.add_sized(
        [ui.available_width(), 40.0],
        egui::Button::new(RichText::new(label).size(13.0).color(if primary { ACCENT } else { TEXT }))
            .corner_radius(8)
            .fill(if primary { ACCENT.gamma_multiply(0.16) } else { SURFACE })
            .stroke(egui::Stroke::new(1.0, if primary { ACCENT.gamma_multiply(0.45) } else { BORDER })),
    )
}

fn exit_fullscreen(ui: &mut egui::Ui) -> egui::Response {
    let response = ui.scope(|ui| {
        ui.spacing_mut().button_padding = Vec2::ZERO;
        ui.spacing_mut().interact_size = Vec2::splat(20.0);
        ui.add_sized([20.0, 20.0], egui::Button::new("").corner_radius(5).fill(SURFACE))
    }).inner;
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Sair da tela cheia"));
    let center = response.rect.center();
    let stroke = egui::Stroke::new(1.5, if response.hovered() || response.has_focus() { TEXT } else { MUTED });
    for direction in [-1.0, 1.0] {
        let offset = Vec2::new(3.5, 3.5 * direction);
        ui.painter().line_segment([center - offset, center + offset], stroke);
    }
    response.on_hover_text("Sair da tela cheia · Esc / F11")
}

fn fullscreen_sound(ui: &mut egui::Ui, muted: bool) -> egui::Response {
    let response = ui.scope(|ui| {
        ui.spacing_mut().button_padding = Vec2::ZERO;
        ui.spacing_mut().interact_size = Vec2::splat(20.0);
        ui.add_sized([20.0, 20.0], egui::Button::new("").corner_radius(5).fill(SURFACE))
    }).inner;
    let label = if muted { "Reativar áudio recebido" } else { "Silenciar áudio recebido" };
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    let center = response.rect.center();
    let tint = if response.hovered() || response.has_focus() { TEXT } else { MUTED };
    ui.painter().rect_filled(egui::Rect::from_center_size(center - Vec2::new(4.0, 0.0), Vec2::new(3.0, 5.0)), 0.5, tint);
    ui.painter().add(egui::Shape::convex_polygon(vec![
        center + Vec2::new(-3.0,-2.5), center + Vec2::new(1.0,-5.0),
        center + Vec2::new(1.0,5.0), center + Vec2::new(-3.0,2.5),
    ], tint, egui::Stroke::NONE));
    let stroke = egui::Stroke::new(1.3, if muted { DANGER } else { tint });
    if muted {
        ui.painter().line_segment([center - Vec2::splat(5.0), center + Vec2::splat(5.0)], stroke);
    } else {
        ui.painter().line_segment([center + Vec2::new(4.0,-3.0), center + Vec2::new(5.5,0.0)], stroke);
        ui.painter().line_segment([center + Vec2::new(5.5,0.0), center + Vec2::new(4.0,3.0)], stroke);
    }
    response.on_hover_text(label)
}

fn refresh_reception(ui:&mut egui::Ui,enabled:bool,size:f32)->egui::Response {
    let response=ui.scope(|ui|{
        ui.spacing_mut().button_padding=Vec2::ZERO;
        ui.spacing_mut().interact_size=Vec2::splat(size);
        ui.add_enabled(enabled,egui::Button::new(RichText::new("↻").size(if size>24.0{20.0}else{16.0}))
            .min_size(Vec2::splat(size)).corner_radius(6).fill(SURFACE))
    }).inner;
    response.widget_info(||egui::WidgetInfo::labeled(egui::WidgetType::Button,enabled,"Reconectar recepção"));
    response.on_hover_text("Reconectar vídeo e áudio recebidos · F5")
}

fn monitor_symbol(painter: &egui::Painter, center: egui::Pos2, color: Color32, size: f32) {
    let stroke = egui::Stroke::new(1.6, color);
    let screen = egui::Rect::from_center_size(center - Vec2::new(0.0, size * 0.08), Vec2::new(size, size * 0.62));
    painter.rect_stroke(screen, 3.0, stroke, egui::StrokeKind::Inside);
    painter.line_segment([center + Vec2::new(0.0, size * 0.24), center + Vec2::new(0.0, size * 0.43)], stroke);
    painter.line_segment([center + Vec2::new(-size * 0.25, size * 0.43), center + Vec2::new(size * 0.25, size * 0.43)], stroke);
}

/// Native button keeps keyboard focus/activation and accessibility semantics;
/// painter supplies the visual hierarchy without image assets or CPU capture.
fn choice_card(ui: &mut egui::Ui, title: &str, subtitle: &str, selected: bool, enabled: bool, avatar: Option<char>) -> egui::Response {
    let response = ui.add_enabled(enabled, egui::Button::new("")
        .min_size(Vec2::new(ui.available_width(), 66.0)).corner_radius(9)
        .fill(if selected { Color32::from_rgb(40, 51, 40) } else { SURFACE })
        .stroke(egui::Stroke::new(1.0, if selected { ACCENT.gamma_multiply(0.65) } else { BORDER })));
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, format!("{title}: {subtitle}")));
    let center = response.rect.left_center() + Vec2::new(30.0, 0.0);
    let tint = if selected { ACCENT } else { MUTED };
    if let Some(initial) = avatar {
        ui.painter().circle_filled(center, 18.0, tint.gamma_multiply(0.14));
        ui.painter().text(center, egui::Align2::CENTER_CENTER, initial.to_uppercase().collect::<String>(), egui::FontId::proportional(17.0), tint);
        if subtitle.contains("compartilhando") || subtitle.contains("Compartilhando") || subtitle.contains("Transmitindo") || subtitle.contains("transmitindo") {
            let dot=center+Vec2::new(13.0,13.0);
            ui.painter().circle_filled(dot,5.0,PANEL);
            ui.painter().circle_filled(dot,3.5,ACCENT);
        }
    } else { monitor_symbol(ui.painter(), center, tint, 38.0); }
    let painter = ui.painter().with_clip_rect(response.rect.shrink(8.0));
    painter.text(response.rect.left_top() + Vec2::new(60.0, 15.0), egui::Align2::LEFT_TOP,
        title, egui::FontId::proportional(14.0), TEXT);
    painter.text(response.rect.left_top() + Vec2::new(60.0, 37.0), egui::Align2::LEFT_TOP,
        subtitle, egui::FontId::proportional(11.0), if selected { ACCENT } else { MUTED });
    if response.has_focus() { painter.rect_stroke(response.rect.shrink(2.0), 8.0, egui::Stroke::new(1.5, ACCENT), egui::StrokeKind::Inside); }
    response
}

fn compact_window_title(title:&str)->String {
    let clean:String=title.chars().filter(|c|!c.is_control()).collect();
    let mut chars=clean.chars();let short:String=chars.by_ref().take(30).collect();
    if chars.next().is_some(){format!("{short}…")}else{short}
}

fn section_title(ui: &mut egui::Ui, label: &str, detail: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).size(11.0).strong().color(MUTED));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| { ui.label(RichText::new(detail).size(11.0).color(MUTED)); });
    });
}

fn text_input(ui:&mut egui::Ui,text:&mut String,hint:&str,password:bool,limit:usize,enabled:bool)->egui::Response {
    crate::desktop_ui::input(ui,text,hint,password,limit,enabled)
}

struct App {
    engine: Option<Engine>,
    address: String,
    token: String,
    invite: String,
    saved_room:Option<String>,
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
    window_mode:bool,
    windows:Vec<crate::video_capture::ApplicationWindow>,
    selected_window:Option<usize>,
    window_picker_open:bool,
    window_previews:HashMap<usize,crate::video_capture::WindowPreview>,
    fullscreen:bool,
    debug_open:bool,
    updater:crate::updater::Updater,
    creator:crate::room_creation::Creator,
    catalog:crate::room_creation::Catalog,
    server_access:crate::room_creation::ServerAccess,
    server_status:crate::server_status::ServerStatus,
    before_fullscreen_mode:ViewMode,
    codec: crate::encoder::Codec,
    parent: usize,
    presenters: HashMap<String, Presenter>,
    error: String,
    auto_connect: bool,
    relay_only: bool,
    started: Instant,
    smoke_seconds: Option<u64>,
    rendered: u64,
    last_presented: HashMap<String, (Arc<crate::desktop_engine::VideoFrame>, [i32; 4])>,
    smoke_reconnected: bool,
    smoke_layout_stage: u8,
    smoke_share_stage: u8,
    smoke_sound_stage: u8,
    smoke_source_stage:u8,
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
                .unwrap_or_else(|_| "wss://hypercast.cloudlane.com.br".into()),
            token: std::env::var("HYPERCAST_ROOM_TOKEN").unwrap_or_default(),
            invite: std::env::var("HYPERCAST_INVITE").unwrap_or_default(),saved_room:None,
            name: crate::preferences::load_name(),
            sidebar_open: true, self_preview: true, single_view: false,
            client_id: crate::invitation::client_id(),
            token_mode: std::env::var("HYPERCAST_ROOM_TOKEN").is_ok(),
            mode: if std::env::var("HYPERCAST_VIEW_MODE").as_deref() == Ok("grid") {
                ViewMode::Grid
            } else {
                ViewMode::Spotlight
            },
            focus: std::env::var("HYPERCAST_FOCUS").ok(),
            paused: false,
            system_audio: std::env::var("HYPERCAST_SYSTEM_AUDIO").as_deref() == Ok("1"),
            volume: 80,
            sound_muted: false,
            displays: crate::screen_capture::displays().unwrap_or_default(),
            selected_display: 0,
            window_mode:false,windows:crate::video_capture::windows(),selected_window:None,window_picker_open:false,window_previews:HashMap::new(),fullscreen:false,debug_open:false,updater:crate::updater::Updater::default(),creator:crate::room_creation::Creator::default(),catalog:crate::room_creation::Catalog::default(),server_access:crate::room_creation::ServerAccess::default(),server_status:crate::server_status::ServerStatus::default(),before_fullscreen_mode:ViewMode::Spotlight,
            codec: if std::env::var("HYPERCAST_CODEC").as_deref() == Ok("av1") {
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
            last_presented: HashMap::new(),
            smoke_reconnected: false,
            smoke_layout_stage: 0,
            smoke_share_stage: 0,
            smoke_sound_stage: 0,smoke_source_stage:0,
        }
    }
    fn target(&self)->Option<crate::video_capture::CaptureTarget>{
        let display=self.displays.get(self.selected_display)?;
        Some(if self.window_mode{
            let window=self.windows.iter().find(|w|Some(w.hwnd)==self.selected_window)?;
            crate::video_capture::CaptureTarget::Window{adapter:display.adapter_index,hwnd:window.hwnd,pid:window.pid}
        }else{crate::video_capture::CaptureTarget::Monitor{adapter:display.adapter_index,output:display.output_index}})
    }
    fn set_fullscreen(&mut self,ctx:&egui::Context,enabled:bool){
        if self.fullscreen==enabled{return;}
        self.fullscreen=enabled;
        if enabled {self.before_fullscreen_mode=self.mode;self.mode=ViewMode::Spotlight;}
        else {self.mode=self.before_fullscreen_mode;}
        self.send_layout();
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(enabled));
        self.last_presented.clear();
    }
    fn window_selection(&mut self,ui:&mut egui::Ui,space:f32)->bool {
        let mut selected=false;
        let mut visible=Vec::with_capacity(4);
        ui.style_mut().wrap_mode=Some(egui::TextWrapMode::Truncate);
        egui::ScrollArea::vertical().max_height(space.clamp(78.0,320.0))
            .auto_shrink([false,true]).show(ui,|ui|{
                for (index,window) in self.windows.iter().enumerate(){
                    let response=choice_card(ui,&format!("Janela {}",index+1),
                        &compact_window_title(&window.title),self.selected_window==Some(window.hwnd),true,None);
                    // DWM draws into the native viewport, so only register fully visible
                    // screens. Keep compositor work bounded and release offscreen sources.
                    let center=response.rect.left_center()+Vec2::new(30.0,-38.0*0.08);
                    let screen=egui::Rect::from_center_size(center,Vec2::new(38.0,38.0*0.62)).shrink(2.0);
                    if visible.len()<4 && ui.clip_rect().contains_rect(screen) && window.available(){
                        visible.push(window.hwnd);
                        if !self.window_previews.contains_key(&window.hwnd){
                            if let Some(preview)=crate::video_capture::WindowPreview::new(self.parent,window){
                                self.window_previews.insert(window.hwnd,preview);
                            }
                        }
                        let scale=ui.ctx().pixels_per_point();
                        let shown=self.window_previews.get_mut(&window.hwnd).is_some_and(|preview|preview.present(
                            (screen.left()*scale).round() as i32,(screen.top()*scale).round() as i32,
                            (screen.width()*scale).round() as i32,(screen.height()*scale).round() as i32));
                        if !shown {self.window_previews.remove(&window.hwnd);}
                    }
                    if response.clicked() && window.available(){
                        self.selected_window=Some(window.hwnd);self.window_mode=true;selected=true;ui.close();
                    }
                }
                if self.windows.is_empty(){ui.label("Nenhuma janela disponível");}
            });
        self.window_previews.retain(|hwnd,_|visible.contains(hwnd));
        selected
    }
    fn sharing_settings(&mut self,ui:&mut egui::Ui,publishing:bool){
        let old_target=self.target();
        section_title(ui,"COMPARTILHAR",if publishing{"AO VIVO"}else{""});
        ui.add_space(8.0);
        ui.horizontal(|ui|{
            let width=(ui.available_width()-ui.spacing().item_spacing.x)/2.0;
            for (window,label) in [(false,"Monitor"),(true,"Janela de app")] {
                let selected=self.window_mode==window;
                if ui.add(egui::Button::new(RichText::new(label).color(if selected{ACCENT}else{TEXT}))
                    .min_size(Vec2::new(width,38.0)).corner_radius(8)
                    .fill(if selected{Color32::from_rgb(40,51,40)}else{SURFACE}).selected(selected)).clicked(){
                    if publishing && window && !self.window_mode{self.selected_window=None;}
                    self.window_mode=window;
                }
            }
        });
        ui.scope(|ui|{
            ui.style_mut().wrap_mode=Some(egui::TextWrapMode::Truncate);
            let width=ui.available_width();
            ui.spacing_mut().interact_size.y=38.0;
            ui.visuals_mut().widgets.inactive.corner_radius=egui::CornerRadius::same(8);
            ui.visuals_mut().widgets.hovered.corner_radius=egui::CornerRadius::same(8);
            if self.window_mode{
                let title=self.windows.iter().position(|w|Some(w.hwnd)==self.selected_window)
                    .map(|index|format!("Janela {} · {}",index+1,compact_window_title(&self.windows[index].title)))
                    .unwrap_or("Selecionar janela…".into());
                let bounds=ui.ctx().content_rect();let top=ui.cursor().top();
                let space=(top-bounds.top()).max(bounds.bottom()-top-38.0)-32.0;
                let response=egui::ComboBox::from_id_salt("window_picker").width(width).height(380.0).truncate()
                    .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).selected_text(title)
                    .show_ui(ui,|ui|{
                        if !self.window_picker_open {
                            self.windows=crate::video_capture::windows();self.window_previews.clear();
                        }
                        self.window_selection(ui,space)
                    });
                self.window_picker_open=response.inner==Some(false);
                if !self.window_picker_open {self.window_previews.clear();}
            }else{
                let title=self.displays.get(self.selected_display).map_or("Nenhum monitor".into(),|d|format!("Monitor {} · {} × {}",self.selected_display+1,d.width,d.height));
                egui::ComboBox::from_id_salt("display_picker").width(width).truncate().selected_text(title).show_ui(ui,|ui|{
                    for (index,display) in self.displays.iter().enumerate(){
                        if choice_card(ui,&format!("Monitor {}",index+1),&format!("{} × {}",display.width,display.height),index==self.selected_display,true,None)
                            .on_hover_text(format!("{}\n{}",display.display_name,display.gpu_name)).clicked(){self.selected_display=index;}
                    }
                });
            }
        });
        if !self.window_mode {self.window_picker_open=false;self.window_previews.clear();}
        if self.window_mode && compact_action(ui,"Atualizar janelas",true).clicked(){self.windows=crate::video_capture::windows();}
        ui.label(RichText::new(if publishing{"Troque a origem sem parar o envio"}else if self.window_mode{"Apenas a janela escolhida"}else{"Seu monitor inteiro será compartilhado"}).size(11.0).color(MUTED));
        egui::CollapsingHeader::new(format!("Formato de vídeo · {}",if self.codec==crate::encoder::Codec::H265{"HEVC"}else{"AV1"})).id_salt("video_format").show(ui,|ui|{
            ui.horizontal(|ui|{
                let width=(ui.available_width()-ui.spacing().item_spacing.x)/2.0;
                for (codec,title) in [(crate::encoder::Codec::H265,"HEVC / H.265"),(crate::encoder::Codec::Av1,"AV1")]{
                    let selected=self.codec==codec;
                    if ui.add_enabled(!publishing,egui::Button::new(title).selected(selected).min_size(Vec2::new(width,38.0))).clicked(){self.codec=codec;}
                }
            });
            ui.label(RichText::new(if publishing{"Pare o envio para trocar o formato"}else if self.codec==crate::encoder::Codec::Av1{"Envio e recepção exigem GPU compatível com AV1"}else{"NVIDIA e AMD com suporte HEVC por hardware"}).size(11.0).color(MUTED));
        });
        if publishing && self.target()!=old_target{
            if let (Some(engine),Some(target))=(&self.engine,self.target()){let _=engine.commands.send(Command::ChangeSource(target));}
        }
    }
    fn sharing_controls(&mut self,ui:&mut egui::Ui,connected:bool,can_publish:bool,publishing:bool,active:Option<crate::video_capture::CaptureTarget>,status:&str){
        ui.spacing_mut().item_spacing=Vec2::new(8.0,8.0);
        ui.horizontal(|ui|{
        ui.label(RichText::new(if publishing{if active.is_some_and(|t|t.process().is_some()){"JANELA AO VIVO"}else{"MONITOR AO VIVO"}}else{"SEU COMPARTILHAMENTO" }).size(10.0).strong().color(if publishing{ACCENT}else{MUTED}));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{
            if compact_action(ui,"Debug",true).on_hover_text("Diagnóstico e opções de depuração").clicked(){self.debug_open=!self.debug_open;if self.debug_open{self.window_picker_open=false;egui::Popup::close_all(ui.ctx());self.updater.open=false;if !self.creator.busy{self.creator.open=false;}}}
        });
        });
        ui.scope(|ui|{
            ui.style_mut().wrap_mode=Some(egui::TextWrapMode::Truncate);
            ui.label(RichText::new(status).size(11.0).color(MUTED)).on_hover_text(status);
        });
        let app_audio=if publishing{active.is_some_and(|t|t.process().is_some())}else{self.window_mode};
        if ui.add_enabled(!connected||can_publish,egui::Checkbox::new(&mut self.system_audio,if app_audio{"Áudio do aplicativo"}else{"Áudio do sistema"}))
            .on_hover_text("Som do sistema ou do aplicativo escolhido, sem microfone. Pode incluir outras janelas do mesmo programa.").changed(){
            if let Some(engine)=&self.engine{let _=engine.commands.send(Command::SystemAudio(self.system_audio));}
        }
        let ready=connected&&can_publish&&(publishing||self.target().is_some());
        if ui.add_enabled(ready,egui::Button::new(RichText::new(if publishing{"Parar compartilhamento"}else{"Compartilhar tela"}).strong().color(if publishing{DANGER}else{BG}))
            .min_size(Vec2::new(ui.available_width(),46.0)).corner_radius(9)
            .fill(if publishing{Color32::from_rgb(59,34,34)}else{ACCENT}))
            .on_hover_text(if can_publish{"Enviar a origem escolhida para a sala"}else{"Sua sessão não permite compartilhar"}).clicked(){
            if let Some(engine)=&self.engine{
                if publishing{let _=engine.commands.send(Command::StopShare);}
                else if let Some(target)=self.target(){let command=match target{
                    crate::video_capture::CaptureTarget::Monitor{adapter,output}=>Command::Share{adapter,output,codec:self.codec},
                    crate::video_capture::CaptureTarget::Window{adapter,hwnd,pid}=>Command::ShareWindow{adapter,hwnd,pid,codec:self.codec},
                };let _=engine.commands.send(command);}
            }
        }
        ui.horizontal(|ui|{
            let exit_width=56.0;let pause_width=(ui.available_width()-exit_width-36.0-ui.spacing().item_spacing.x*2.0).max(100.0);
            if ui.add_enabled(connected,egui::Button::new(if self.paused{"Retomar vídeo"}else{"Pausar vídeo"}).min_size(Vec2::new(pause_width,40.0)).corner_radius(8)).clicked(){self.paused=!self.paused;self.send_layout();}
            let resetting=self.engine.as_ref().is_some_and(|e|e.shared.lock().unwrap().reception_resetting);
            if refresh_reception(ui,connected&&!resetting,36.0).clicked(){self.restart_reception();}
            if ui.add_enabled(self.engine.is_some(),egui::Button::new(RichText::new("Sair").color(DANGER)).min_size(Vec2::new(exit_width,40.0)).corner_radius(8)).clicked(){self.engine=None;self.presenters.clear();self.last_presented.clear();}
        });
    }
    fn restart_reception(&mut self) {
        if let Some(engine)=&self.engine {let _=engine.commands.send(Command::RestartReception);}
        self.presenters.clear();self.last_presented.clear();self.error.clear();
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
    fn join_room(&self, ctx: egui::Context) -> Engine {
        let _ = crate::preferences::save_name(&self.name);
        let engine = if self.token_mode {
            Engine::connect(self.address.trim().to_owned(), self.token.trim().to_owned(), ctx, self.parent, self.relay_only)
        } else if let Some(room)=&self.saved_room {
            Engine::connect_room(self.address.trim().to_owned(),room.clone(),self.name.trim().to_owned(),self.client_id.clone(),self.server_access.token(),ctx,self.parent,self.relay_only)
        } else {
            Engine::connect_invite(self.address.trim().to_owned(), self.invite.trim().to_owned(), self.name.trim().to_owned(), self.client_id.clone(), self.server_access.token(), ctx, self.parent, self.relay_only)
        };
        let _ = engine.commands.send(Command::SystemAudio(self.system_audio));
        let _ = engine.commands.send(Command::Layout(Layout {
                self_preview: self.self_preview,
            mode: self.mode, focus: self.focus.clone(), paused: self.paused,
            volume: if self.sound_muted { 0 } else { self.volume },
        }));
        engine
    }
}
impl Drop for App {
    fn drop(&mut self) {
        let _ = crate::preferences::save_name(&self.name);
        self.presenters.clear();self.last_presented.clear();self.engine=None;
        if self.smoke_seconds.is_some(){println!("GUI shutdown: {} retained GPU decoder surfaces",livekit::webrtc::native::d3d11_live_surfaces());}
    }
}
impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if ctx.input(|i|i.key_pressed(egui::Key::F5)) && self.engine.as_ref().is_some_and(|e|{let s=e.shared.lock().unwrap();s.connected&&!s.reception_resetting}) {
            self.restart_reception();
        }
        self.updater.poll();
        self.server_access.poll(&self.address,&self.client_id,ctx.clone());
        self.server_status.poll(&self.address,&ctx);
        if let Some(created)=self.creator.poll() {
            self.saved_room=Some(created.room);self.invite.clear();self.token_mode=false;self.token.clear();
            self.error.clear();
            self.engine=Some(self.join_room(ctx.clone()));
        }
        if ctx.input(|i|i.key_pressed(egui::Key::F11)){self.set_fullscreen(&ctx,!self.fullscreen);}
        if self.fullscreen && ctx.input(|i|i.key_pressed(egui::Key::Escape)){self.set_fullscreen(&ctx,false);}
        if self.smoke_seconds.is_some() && std::env::var("HYPERCAST_SMOKE_SOUND").as_deref() == Ok("1") {
            let elapsed = self.started.elapsed().as_secs();
            if self.smoke_sound_stage == 0 && elapsed >= 6 {
                self.sound_muted = true; self.send_layout(); self.smoke_sound_stage = 1;
                println!("GUI audio: muted");
            } else if self.smoke_sound_stage == 1 && elapsed >= 12 {
                self.sound_muted = false; self.send_layout(); self.smoke_sound_stage = 2;
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
        if self.auto_connect && self.engine.is_none() && (!self.token.is_empty() || !self.invite.is_empty()) {
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
        let mut reception_resetting=false;
        let mut publishing = false;
        let mut can_publish = false;
        let mut identity = String::new();
        let mut room = "Sua sala de trabalho".to_owned();
        let mut status = "Pronto para conectar".to_owned();
        let mut audio_status = String::new();
        let mut active_target=None;
        let mut source_error=String::new();
        let mut people = Vec::new();
        let mut screens = Vec::new();
        let mut videos = Vec::new();
        let mut frozen = Vec::new();
        let mut received = 0;
        let mut sent = 0;
        if let Some(engine) = &self.engine {
            let state = engine.shared.lock().unwrap();
            connected = state.connected;reception_resetting=state.reception_resetting;
            publishing = state.publishing;
            can_publish = state.can_publish;
            identity = state.identity.clone();
            if !state.room.is_empty() {
                room = state.room.clone();
            }
            status = state.status.clone();
            audio_status = state.audio_status.clone();
            active_target=state.active_target;source_error=state.source_error.clone();
            people = state.participants.clone();
            screens = state.screens.clone();
            frozen = state.frozen.keys().cloned().collect();
            videos = state
                .frames
                .iter()
                .filter(|(_, frame)| self.self_preview || !frame.local())
                .map(|(id, frame)| (id.clone(), Arc::clone(frame)))
                .collect();
            received = state.received;
            sent = state.sent;
        }
        if !publishing && !identity.is_empty() && self.focus.as_ref()==Some(&identity) {
            self.focus=None;self.send_layout();
        }
        people.sort();
        screens.sort();
        videos.sort_by_key(|(id, _)| id.clone());
        let watched = if self.paused || self.mode!=ViewMode::Spotlight { None } else {
            self.focus.as_ref().filter(|id| screens.contains(id)||videos.iter().any(|(owner,_)|owner==*id))
                .or_else(||screens.first()).or_else(||videos.iter().find(|(_,f)|!f.local()).map(|(id,_)|id))
                .or_else(||videos.first().map(|(id,_)|id)).cloned()
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
            let cycles = if std::env::var("HYPERCAST_SMOKE_SOURCE_SWITCH").as_deref()==Ok("1"){1}else{std::env::var("HYPERCAST_SMOKE_SHARE_CYCLES")
                .ok().and_then(|raw| raw.parse::<u32>().ok()).unwrap_or(2).clamp(2, 6)};
            let stage = u32::from(self.smoke_share_stage);
            if stage < cycles * 2 - 1 && stage % 2 == 0
                && (stage == 0 || elapsed >= 2 + u64::from(stage) * 3) {
                if let (Some(engine), Some(display)) = (&self.engine, self.displays.first()) {
                    let window=std::env::var("HYPERCAST_SMOKE_WINDOW_PID").ok().and_then(|s|s.parse::<u32>().ok())
                        .and_then(|pid|self.windows.iter().find(|w|w.pid==pid));
                    let window_adapter=std::env::var("HYPERCAST_SMOKE_WINDOW_ADAPTER").ok()
                        .and_then(|value|value.parse::<u32>().ok()).unwrap_or(display.adapter_index);
                    let command=if let Some(w)=window{Command::ShareWindow{adapter:window_adapter,hwnd:w.hwnd,pid:w.pid,codec:self.codec}}
                        else {Command::Share{adapter:display.adapter_index,output:display.output_index,codec:self.codec}};
                    let _ = engine.commands.send(command);
                    self.smoke_share_stage += 1;
                    println!("GUI publisher: start");
                }
            } else if stage < cycles * 2 - 1 && stage % 2 == 1 && elapsed >= 2 + u64::from(stage) * 3 {
                if let Some(engine) = &self.engine {
                    let _ = engine.commands.send(Command::StopShare);
                }
                self.smoke_share_stage += 1;
                println!("GUI publisher: stopped");
            }
        }
        if self.smoke_seconds.is_some() && connected && std::env::var("HYPERCAST_SMOKE_SOURCE_SWITCH").as_deref()==Ok("1") {
            let elapsed=self.started.elapsed().as_secs();
            let stage=self.smoke_source_stage;
            if publishing && ((stage==0 && elapsed>=5)||(stage==1 && elapsed>=9)) {
                let target=if stage==0 {self.displays.first().map(|d|crate::video_capture::CaptureTarget::Monitor{adapter:d.adapter_index,output:d.output_index})}
                    else {std::env::var("HYPERCAST_SMOKE_WINDOW_PID").ok().and_then(|s|s.parse::<u32>().ok()).and_then(|pid|self.windows.iter().find(|w|w.pid==pid))
                        .map(|w|crate::video_capture::CaptureTarget::Window{adapter:self.displays[0].adapter_index,hwnd:w.hwnd,pid:w.pid})};
                if let (Some(e),Some(t))=(&self.engine,target){let _=e.commands.send(Command::ChangeSource(t));self.smoke_source_stage+=1;}
            }
        }
        if self.smoke_seconds.is_some() && std::env::var("HYPERCAST_SMOKE_FULLSCREEN").as_deref()==Ok("1") {
            let elapsed=self.started.elapsed().as_secs();
            if !self.fullscreen && (6..16).contains(&elapsed){self.set_fullscreen(&ctx,true);}
            else if self.fullscreen && elapsed>=16{self.set_fullscreen(&ctx,false);}
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
            if let Some(e)=&self.engine {println!("GUI reception refreshes: {}",e.shared.lock().unwrap().reception_restarts);}
            println!("GUI preview: enabled={} local_surfaces={} rendered={}",self.self_preview,videos.iter().filter(|(_,f)|f.local()).count(),self.rendered);
            println!(
                "GUI status: connected={connected}, can_publish={can_publish}; {}",
                if self.token.is_empty() { status.clone() } else { status.replace(&self.token, "[token]") }
            );
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        if !self.fullscreen && self.sidebar_open {
        egui::Panel::left("room_sidebar")
            .exact_size(312.0)
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(PANEL)
                    .inner_margin(egui::Margin::same(20)),
            )
            .show(ui, |ui| {
                egui::Panel::bottom("sharing_actions").show_separator_line(false).exact_size(204.0)
                    .frame(egui::Frame::new().fill(PANEL).inner_margin(egui::Margin::symmetric(0,12)))
                    .show(ui,|ui|{self.sharing_controls(ui,connected,can_publish,publishing,active_target,if source_error.is_empty(){&status}else{&source_error});});
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing=Vec2::new(8.0,8.0);
                        ui.spacing_mut().button_padding=Vec2::new(12.0,6.0);
                        ui.horizontal(|ui|{
                            ui.label(RichText::new("H").size(24.0).strong().color(ACCENT));
                            ui.label(RichText::new("HYPERCAST").size(16.0).strong());
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{
                                ui.label(RichText::new(self.server_status.label()).size(10.0).color(if self.server_status.status==crate::server_status::Status::Online{ACCENT}else{MUTED})).on_hover_text(format!("Servidor: {}",self.server_status.detail));
                                if compact_action(ui,"Atualizar",true).on_hover_text("Verificar atualização do HyperCast").clicked(){
                                    self.window_picker_open=false;egui::Popup::close_all(&ctx);self.debug_open=false;if !self.creator.busy{self.creator.open=false;}self.updater.open=true;self.updater.check(ctx.clone());
                                }
                            });
                        });
                        ui.add_space(8.0);
                        if !connected {section_title(ui,if !self.server_access.authenticated(){"ACESSO AO SERVIDOR"}else{"ENTRAR NA SALA"}, "");}
                        ui.add_space(8.0);
                        if connected {
                            ui.label(RichText::new(&room).size(15.0).strong());


                            if self.server_access.authenticated() && compact_action(ui,"Escolher sala",true).clicked(){self.catalog.open(self.address.clone(),self.server_access.token(),ctx.clone());}
                            ui.label(RichText::new(if can_publish { "Você pode assistir e compartilhar" } else { "Você está assistindo à sala" }).size(11.0).color(MUTED));
                        } else {
                        egui::CollapsingHeader::new("Servidor").show(ui, |ui| {
                        ui.label("Servidor");
                        text_input(ui,&mut self.address,"wss://seu-servidor",false,2048,!self.server_access.busy && (self.engine.is_none() || self.engine.as_ref().is_some_and(|engine|engine.finished())));
                        });
                        if !self.server_access.authenticated() {
                            let label=ui.label("Chave de acesso").id;
                            text_input(ui,&mut self.server_access.code,"Chave compartilhada do servidor",true,64,!self.server_access.busy).labelled_by(label);
                            ui.checkbox(&mut self.server_access.remember,"Lembrar neste Windows");
                            ui.add_enabled_ui(!self.server_access.busy && !self.server_access.code.trim().is_empty(),|ui|{
                                if update_action(ui,"Entrar no servidor",true).clicked(){self.server_access.start(self.address.clone(),self.client_id.clone(),ctx.clone());}
                            });
                            if self.server_access.busy {ui.spinner();}
                            if !self.server_access.status.is_empty(){ui.label(RichText::new(&self.server_access.status).size(12.0).color(MUTED));}
                        } else {
                            ui.label("Seu nome");
                            if text_input(ui,&mut self.name,"Como quer aparecer na sala",false,64,self.engine.is_none() || self.engine.as_ref().is_some_and(|engine|engine.finished())).lost_focus() { let _ = crate::preferences::save_name(&self.name); }
                        ui.horizontal(|ui| {
                            let width=(ui.available_width()-ui.spacing().item_spacing.x)/2.0;
                            let ready=(self.engine.is_none() || self.engine.as_ref().is_some_and(|engine|engine.finished())) && !self.creator.busy;
                            if ui.add_enabled(ready,
                                egui::Button::new("Escolher sala").min_size(Vec2::new(width,44.0)).corner_radius(9)).clicked() {
                                self.creator.open=false;self.catalog.open(self.address.clone(),self.server_access.token(),ctx.clone());
                            }
                            if ui.add_enabled(self.server_access.authenticated() && !self.creator.busy && (self.engine.is_none() || !status.starts_with("Conectando")),
                                egui::Button::new("Criar sala").min_size(Vec2::new(width,44.0)).corner_radius(9)).clicked() {
                                self.window_picker_open=false;egui::Popup::close_all(&ctx);self.creator.open(&self.address);self.debug_open=false;self.updater.open=false;
                            }
                        });
                        if !self.server_access.status.is_empty(){ui.label(RichText::new(&self.server_access.status).size(11.0).color(MUTED));}
                        if compact_action(ui,"Trocar acesso ao servidor",!self.server_access.busy).clicked(){self.server_access.logout(&self.address);}
                        }
                        }
                        ui.add_space(8.0);
                        ui.separator();
                        ui.add_space(8.0);
                        self.sharing_settings(ui,publishing);
                        if ui.checkbox(&mut self.self_preview,"Mostrar minha prévia").changed(){
                            if !self.self_preview && self.focus.as_ref()==Some(&identity){self.focus=None;}
                            self.send_layout();
                        }
                        egui::CollapsingHeader::new(if self.sound_muted{"Som recebido · silenciado".into()}else{format!("Som recebido · {}%",self.volume)}).id_salt("received_audio").show(ui,|ui|{
                        if ui.checkbox(&mut self.sound_muted, "Silenciar áudio recebido").changed() { self.send_layout(); }
                        let volume_label = ui.horizontal(|ui| {
                            let label = ui.label(RichText::new("Volume").size(13.0).color(TEXT));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(RichText::new(format!("{}%", self.volume)).size(13.0).color(ACCENT));
                            });
                            label.id
                        }).inner;
                        let changed = ui.scope(|ui| {
                            ui.spacing_mut().slider_width = ui.available_width();
                            ui.spacing_mut().slider_rail_height = 6.0;
                            // The global inactive fill matches the sidebar, hiding the rail.
                            ui.visuals_mut().widgets.inactive.bg_fill = Color32::from_rgb(65, 78, 73);
                            ui.visuals_mut().widgets.inactive.fg_stroke = egui::Stroke::new(1.5, ACCENT);
                            ui.visuals_mut().selection.bg_fill = ACCENT;
                            ui.add(egui::Slider::new(&mut self.volume, 0..=100).show_value(false)
                                .trailing_fill(true).handle_shape(egui::style::HandleShape::Circle))
                                .labelled_by(volume_label).changed()
                        }).inner;
                        if changed { self.send_layout(); }
                        ui.label(RichText::new(if self.mode == ViewMode::Grid { "Som das telas visíveis" } else { "Som da tela em destaque" }).size(11.0).color(MUTED));

                        });
                        ui.add_space(8.0);
                        section_title(ui, "PARTICIPANTES", &format!("{} na sala", people.len() + usize::from(connected)));
                        ui.add_space(8.0);
                        if connected {
                            let label = if self.name.is_empty() { "Você".to_owned() } else { format!("{} (você)", self.name) };
                            if publishing {
                                let response=choice_card(ui,&label,if !self.self_preview {"Transmitindo · prévia oculta"} else if watched.as_ref()==Some(&identity) {"Em destaque · transmitindo"} else {"Transmitindo · sua tela"},self.self_preview && watched.as_ref()==Some(&identity),self.self_preview,Some(self.name.chars().next().unwrap_or('V')));
                                if response.clicked(){self.focus=Some(identity.clone());self.mode=ViewMode::Spotlight;self.single_view=response.double_clicked();self.send_layout();}
                            }else{ui.label(RichText::new(format!("{label} · assistindo")).size(12.0).color(MUTED));}
                        }
                        people.sort_by(|a,b| screens.contains(&b.0).cmp(&screens.contains(&a.0)).then_with(|| a.1.cmp(&b.1)).then_with(|| a.0.cmp(&b.0)));
                        for (identity, name) in &people {
                            let live = screens.contains(identity);
                            let focused = watched.as_ref()==Some(identity);
                            let response=choice_card(ui, if name.is_empty() { "Participante" } else { name }, if live { if focused && self.mode == ViewMode::Spotlight { "Em destaque · compartilhando" } else { "Compartilhando tela" } } else { "Assistindo à sala" }, focused && live && self.mode==ViewMode::Spotlight, live, Some(name.chars().next().unwrap_or('P')));
                            if response.clicked()
                            {
                                self.single_view=response.double_clicked();
                                self.focus = Some(identity.clone());
                                self.mode = ViewMode::Spotlight;
                                self.send_layout();
                            }
                        }
                        if people.is_empty() {
                            egui::Frame::new().fill(SURFACE).corner_radius(9).inner_margin(12).show(ui, |ui| {
                                ui.set_min_width(ui.available_width());
                                ui.style_mut().wrap_mode=Some(egui::TextWrapMode::Wrap);
                                ui.label(RichText::new(if connected { "Aguardando seus amigos" } else { "Seus amigos aparecem aqui" }).size(13.0).color(TEXT));
                                ui.label(RichText::new(if connected { "Seus amigos podem entrar pela lista de salas." } else { "Entre na sala para acompanhar as telas." }).size(11.0).color(MUTED));
                            });
                        }
                        if audio_status.starts_with("Áudio:") { ui.colored_label(DANGER,&audio_status); }
                        if !source_error.is_empty(){ui.add(egui::Label::new(RichText::new(&source_error).color(DANGER)).wrap());}
                        if !self.error.is_empty() {
                            ui.colored_label(Color32::LIGHT_RED, &self.error);
                        }
                    });
            });
        }
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(BG)
                    .inner_margin(egui::Margin::same(if self.fullscreen { 8 } else { 28 })),
            )
            .show(ui, |ui| {
                if self.fullscreen {
                    ui.spacing_mut().item_spacing = Vec2::new(8.0, 4.0);
                    ui.horizontal(|ui|{
                        ui.label(RichText::new(if reception_resetting{"Reconectando recepção…"}else{"Tela em destaque"}).size(13.0).color(MUTED));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if exit_fullscreen(ui).clicked(){self.set_fullscreen(&ctx,false);}
                            let resetting=self.engine.as_ref().is_some_and(|e|e.shared.lock().unwrap().reception_resetting);
                            if refresh_reception(ui,connected&&!resetting,20.0).clicked(){self.restart_reception();}
                            let muted = self.sound_muted || self.volume == 0;
                            if fullscreen_sound(ui, muted).clicked() {
                                self.sound_muted = !muted;
                                if muted && self.volume == 0 { self.volume = 80; }
                                self.send_layout();
                            }
                            let volume = ui.scope(|ui| {
                                ui.spacing_mut().slider_width = 88.0;
                                ui.spacing_mut().interact_size.y = 20.0;
                                ui.visuals_mut().selection.bg_fill = if self.sound_muted { MUTED } else { ACCENT };
                                ui.add(egui::Slider::new(&mut self.volume, 0..=100).show_value(false)
                                    .trailing_fill(true).handle_shape(egui::style::HandleShape::Circle))
                            }).inner;
                            volume.widget_info(|| egui::WidgetInfo::slider(true, self.volume as f64, "Volume da transmissão"));
                            if volume.changed() { self.sound_muted = false; self.send_layout(); }
                            volume.on_hover_text(format!("Volume da transmissão · {}%", self.volume));
                        });
                    });
                } else {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            if compact_action(ui,if self.sidebar_open {"Recolher menu"} else {"Abrir menu"},true).clicked(){
                                self.sidebar_open=!self.sidebar_open;
                                if !self.sidebar_open {self.window_picker_open=false;egui::Popup::close_all(&ctx);}
                            }
                            ui.label(RichText::new("Telas da sala").size(26.0).strong());
                        if !self.sidebar_open {
                            let resetting=self.engine.as_ref().is_some_and(|e|e.shared.lock().unwrap().reception_resetting);
                            if refresh_reception(ui,connected&&!resetting,26.0).clicked(){self.restart_reception();}
                        }
                        });
                        ui.label(
                            RichText::new(if reception_resetting {"Reconectando vídeo e áudio recebidos…"}else{"Escolha uma tela para acompanhar de perto."})
                                .size(13.0)
                                .color(MUTED),
                        );
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add_enabled(!videos.is_empty(),egui::Button::new("Tela cheia · F11")).clicked(){self.set_fullscreen(&ctx,true);}
                        if ui
                            .selectable_label(self.mode == ViewMode::Grid, "Grid")
                            .clicked()
                        {
                            self.mode = ViewMode::Grid; self.single_view=false;
                            self.send_layout();
                        }
                        if ui
                            .selectable_label(self.mode == ViewMode::Spotlight, "Destaque")
                            .clicked()
                        {
                            self.mode = ViewMode::Spotlight; self.single_view=false;
                            self.send_layout();
                        }
                    });
                });
                ui.add_space(24.0);
                }
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
                        .filter(|id|screens.contains(id)||videos.iter().any(|(owner,_)|owner==*id))
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
                    if self.mode == ViewMode::Spotlight && !self.fullscreen && !self.single_view {
                        selected.extend(
                            videos
                                .iter()
                                .filter(|(id, _)| Some(id) != focus.as_ref())
                                .take(2),
                        );
                    }
                    let unseen: Vec<_> = if self.mode == ViewMode::Spotlight && !self.fullscreen && !self.single_view {
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
                        let response=ui.interact(card, egui::Id::new(("focus",identity)),egui::Sense::click());
                        response.widget_info(||egui::WidgetInfo::labeled(egui::WidgetType::Button,true,format!("Transmissão de {}",if frame.local(){"Minha tela"}else{name})));
                        if response.double_clicked() || (response.clicked() && response.interact_pointer_pos().is_some_and(|p|p.y<card.top()+32.0))
                        {
                            self.single_view=response.double_clicked();
                            self.focus = Some((*identity).clone());
                            self.mode = ViewMode::Spotlight;
                            self.send_layout();
                        }
                        let (frame_width, frame_height) = frame.dimensions();
                        ui.painter().text(
                            card.left_bottom() + Vec2::new(16.0, -18.0),
                            egui::Align2::LEFT_CENTER,
                            if frozen.contains(identity) {
                                "Reconectando • último quadro".into()
                            } else if thumbnail && !frame.local() {
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
                        let response=ui.interact(card,egui::Id::new(("unseen",id)),egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
                        if response.clicked()
                        {
                            self.single_view=response.double_clicked();
                            self.focus = Some((*id).clone());
                            self.send_layout();
                        }
                    }
                }
            });
        if !self.window_picker_open || self.fullscreen || self.debug_open || self.updater.open {self.window_previews.clear();}
        if !self.fullscreen && self.creator.open {
            let mut open=true;
            egui::Window::new("Criar sala").id(egui::Id::new("create_room_window"))
                .frame(utility_window_frame(ui.style()))
                .open(&mut open).fixed_pos(egui::pos2(16.0,48.0)).default_width(260.0).max_width(260.0)
                .resizable(false).collapsible(false).show(&ctx,|ui| {
                    ui.style_mut().wrap_mode=Some(egui::TextWrapMode::Wrap);
                    let title_label=ui.label("Nome da sala").id;
                    text_input(ui,&mut self.creator.title,"Jogos com amigos",false,64,!self.creator.busy).labelled_by(title_label);
                    let name_label=ui.label("Seu nome").id;
                    text_input(ui,&mut self.name,"Como vão chamar você",false,64,!self.creator.busy).labelled_by(name_label);
                    ui.checkbox(&mut self.creator.persistent,"Manter sala indefinidamente");
                    ui.label(RichText::new(if self.creator.persistent{"Até 12 pessoas · 3 telas · sem expiração"}else{"Até 12 pessoas · 3 telas · válida por 24 horas"}).size(11.0).color(MUTED));
                    if !self.creator.status.is_empty(){ui.label(RichText::new(&self.creator.status).size(12.0).color(MUTED));}
                    if self.creator.busy {ui.spinner();}
                    let valid_name=!self.name.is_empty() && self.name.len()<=64 && self.name.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'-'||b==b'_');
                    ui.add_enabled_ui(!self.creator.busy && valid_name && !self.creator.title.trim().is_empty() && self.server_access.authenticated(),|ui| {
                        if update_action(ui,"Criar e entrar",true).clicked() {self.creator.start(self.address.clone(),self.server_access.token(),ctx.clone());}
                    });
                    if !valid_name && !self.name.is_empty() {ui.label(RichText::new("Seu nome: letras sem acentos, números, hífen ou sublinhado").size(11.0).color(MUTED));}
                });
            self.creator.open=open || self.creator.busy;
        }
        if !self.fullscreen {
            if let Some(selected)=self.catalog.show(&ctx,&self.address,self.server_access.token(),&mut self.name,connected) {
                self.saved_room=Some(selected.room);self.invite.clear();self.token_mode=false;self.token.clear();self.error.clear();
                self.engine=Some(self.join_room(ctx.clone()));
            }
        }
        if !self.fullscreen && self.debug_open {
            let mut open=self.debug_open;
            egui::Window::new("Diagnóstico").id(egui::Id::new("debug_window"))
                .frame(utility_window_frame(ui.style()))
                .open(&mut open).fixed_pos(egui::pos2(16.0,48.0))
                .default_width(260.0).max_width(260.0).resizable(false).collapsible(false)
                .show(&ctx,|ui|{
                    ui.style_mut().wrap_mode=Some(egui::TextWrapMode::Wrap);
                    ui.label(RichText::new(&status).size(12.0));
                    if !audio_status.is_empty(){ui.label(RichText::new(&audio_status).size(11.0).color(MUTED));}
                    let idle=self.engine.is_none() || self.engine.as_ref().is_some_and(|engine|engine.finished());
                    ui.add_space(4.0);
                    ui.label(RichText::new("CONEXÃO AVANÇADA").size(10.0).color(MUTED));
                    ui.add_enabled(idle && !self.creator.busy,egui::Checkbox::new(&mut self.token_mode,"Token manual (avançado)"));
                    if self.token_mode {
                        ui.label(RichText::new("JWT temporário de sala, fornecido pelo administrador para testes.").size(11.0).color(MUTED));
                        let label=ui.label("Token de diagnóstico").id;
                        text_input(ui,&mut self.token,"Cole o JWT da sala",true,8192,idle).labelled_by(label);
                        ui.add_enabled_ui(idle && !self.creator.busy && !self.token.trim().is_empty(),|ui|{
                            if update_action(ui,"Entrar com token",true).clicked(){self.error.clear();self.engine=Some(self.join_room(ctx.clone()));}
                        });
                        if !idle {ui.label(RichText::new("Saia da sala para alterar o acesso de diagnóstico.").size(11.0).color(MUTED));}
                    }
                    ui.add_enabled(idle,egui::Checkbox::new(&mut self.relay_only,"Forçar TURN/TLS (teste)"))
                        .on_hover_text("Ative antes de entrar para testar o relay; desativado escolhe a rota de menor custo");
                    if update_action(ui,"Copiar diagnóstico",false).on_hover_text("Copiar o log da sessão, sem token ou imagens").clicked(){
                        match crate::diagnostics::export_text(){Ok(text)=>ctx.copy_text(text),Err(error)=>self.error=format!("Diagnóstico: {error}")}
                    }
                    if let Some(path)=crate::diagnostics::path(){
                        ui.label(RichText::new("Log salvo automaticamente").size(11.0).color(MUTED)).on_hover_text(path.display().to_string());
                    }
                    if !self.error.is_empty(){ui.colored_label(DANGER,&self.error);}
                });
            self.debug_open=open;
        }
        if !self.fullscreen && self.updater.open {
            let mut open=self.updater.open;
            let mut install=false;
            egui::Window::new("Atualização").id(egui::Id::new("update_window"))
                .frame(utility_window_frame(ui.style()))
                .open(&mut open).fixed_pos(egui::pos2(16.0,48.0)).default_width(260.0).max_width(260.0)
                .resizable(false).collapsible(false).show(&ctx,|ui|{
                    ui.style_mut().wrap_mode=Some(egui::TextWrapMode::Wrap);
                    ui.label(format!("Versão atual: {}",env!("CARGO_PKG_VERSION")));
                    ui.label(RichText::new(match &self.updater.installation{
                        crate::updater::Installation::Installed(_)=>"Instalada · atualização pelo aplicativo",
                        _=>"Portátil · download pelo navegador",
                    }).size(11.0).color(MUTED));
                    ui.label(&self.updater.status);
                    if let Some((progress,label))=self.updater.progress(){ui.add(egui::ProgressBar::new(progress).text(label));}
                    if let crate::updater::Installation::Unknown(message)=&self.updater.installation {
                        ui.label(RichText::new("Não foi possível reconhecer a instalação").size(11.0).color(MUTED)).on_hover_text(message);
                    }
                    if !self.updater.busy {
                        if self.updater.installer.is_some(){
                            ui.label(RichText::new("O HyperCast será fechado para instalar. A sessão atual será encerrada.").size(12.0).color(MUTED));
                            if update_action(ui,"Instalar e reiniciar",true).clicked(){install=true;}
                        }else if let Some(release)=self.updater.release.clone(){
                            if matches!(self.updater.installation,crate::updater::Installation::Installed(_)){
                                if update_action(ui,"Baixar atualização",true).clicked(){self.updater.download(ctx.clone());}
                            }else if update_action(ui,"Baixar no navegador",true).clicked(){ctx.open_url(egui::OpenUrl::new_tab(release.page));}
                        }
                        if update_action(ui,"Verificar atualização",false).clicked(){self.updater.check(ctx.clone());}
                        if update_action(ui,"Abrir página de downloads",false).clicked(){ctx.open_url(egui::OpenUrl::new_tab(crate::updater::RELEASES));}
                    }
                });
            self.updater.open=open;
            if install {
                self.engine=None;self.presenters.clear();self.last_presented.clear();
                let result=self.updater.installer.as_ref().unwrap().launch(&self.updater.installation);
                match result {
                    Ok(())=>ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                    Err(error)=>self.updater.status=format!("Não foi possível instalar: {error:#}"),
                }
            }
        }
        ctx.request_repaint_after(Duration::from_millis(if self.updater.busy || self.creator.busy || self.server_access.busy{100}else{500}));
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
