//! Room creation without sharing administrator credentials with the desktop.
use crate::desktop_ui as controls;
use anyhow::{Result, ensure};
use eframe::egui;
use std::{io::Read, sync::mpsc};

pub struct SavedRoom {
    pub room: String,
    pub title: String,
    pub persistent: bool,
    pub expired: bool,
    pub participants: u64,
}
enum CatalogAction {
    Keep(String, bool),
    Delete(String),
}
#[derive(Default)]
pub struct Catalog {
    pub open: bool,
    busy: bool,
    status: String,
    rooms: Vec<SavedRoom>,
    origin: String,
    last_refresh: Option<std::time::Instant>,
    confirm_delete: Option<String>,
    receiver: Option<mpsc::Receiver<(String, String, Result<Vec<SavedRoom>>)>>,
}
impl Catalog {
    /// Latest loading or error message of the room list.
    #[cfg_attr(not(windows), allow(dead_code))]
    pub(crate) fn status(&self) -> &str {
        &self.status
    }
}
impl Catalog {
    pub fn open(&mut self, address: String, session: String, ctx: egui::Context) {
        self.open = true;
        self.request(address, session, None, ctx);
    }
    fn request(
        &mut self,
        address: String,
        session: String,
        action: Option<CatalogAction>,
        ctx: egui::Context,
    ) {
        if self.busy {
            return;
        }
        if self.origin != address {
            self.rooms.clear();
        }
        self.origin = address.clone();
        self.last_refresh = Some(std::time::Instant::now());
        self.busy = true;
        self.status.clear();
        let (tx, rx) = mpsc::channel();
        self.receiver = Some(rx);
        std::thread::spawn(move || {
            let result = (|| -> Result<Vec<SavedRoom>> {
                if let Some(action) = action {
                    match action {
                        CatalogAction::Keep(room, persistent) => {
                            catalog_post(
                                &address,
                                &session,
                                "/api/rooms/keep",
                                serde_json::json!({"room":room,"persistent":persistent}),
                            )?;
                        }
                        CatalogAction::Delete(room) => {
                            catalog_post(
                                &address,
                                &session,
                                "/api/rooms/delete",
                                serde_json::json!({"room":room}),
                            )?;
                        }
                    }
                }
                let value =
                    catalog_post(&address, &session, "/api/rooms/list", serde_json::json!({}))?;
                let rows = value["rooms"].as_array().ok_or_else(|| {
                    anyhow::anyhow!(crate::i18n::label(
                        "Lista de salas inválida",
                        "Invalid room list"
                    ))
                })?;
                ensure!(
                    rows.len() <= 32,
                    crate::i18n::label(
                        "Lista de salas excedeu o limite",
                        "Room list exceeded the limit"
                    )
                );
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)?
                    .as_secs();
                rows.iter()
                    .map(|v| {
                        let text = |key: &str| -> Result<String> {
                            Ok(v[key]
                                .as_str()
                                .ok_or_else(|| {
                                    anyhow::anyhow!(crate::i18n::label(
                                        "Sala inválida",
                                        "Invalid room"
                                    ))
                                })?
                                .to_owned())
                        };
                        let room = text("room")?;
                        let title = text("room_name")?;
                        ensure!(
                            room.len() <= 64
                                && !title.is_empty()
                                && title.chars().count() <= 64
                                && !title.chars().any(char::is_control),
                            crate::i18n::label("Sala inválida", "Invalid room")
                        );
                        let persistent = v["persistent"].as_bool().ok_or_else(|| {
                            anyhow::anyhow!(crate::i18n::label("Sala inválida", "Invalid room"))
                        })?;
                        let expires = v["expires_at"].as_u64().ok_or_else(|| {
                            anyhow::anyhow!(crate::i18n::label("Sala inválida", "Invalid room"))
                        })?;
                        Ok(SavedRoom {
                            room,
                            title,
                            persistent,
                            expired: !persistent && expires <= now,
                            participants: v["participants"].as_u64().unwrap_or(0).min(12),
                        })
                    })
                    .collect()
            })();
            let _ = tx.send((address, session, result));
            ctx.request_repaint();
        });
    }
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        address: &str,
        session: String,
        name: &mut String,
        current_room: Option<&str>,
        transitioning: bool,
    ) -> Option<CreatedRoom> {
        if let Some(rx) = &self.receiver {
            match rx.try_recv() {
                Ok((origin, requested_session, result)) => {
                    self.receiver = None;
                    self.busy = false;
                    if origin == address && requested_session == session && !session.is_empty() {
                        match result {
                            Ok(rooms) => self.rooms = rooms,
                            Err(e) => self.status = e.to_string(),
                        }
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.receiver = None;
                    self.busy = false;
                    self.status = crate::i18n::label(
                        "Consulta interrompida; tente novamente",
                        "Request interrupted; try again",
                    )
                    .into();
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if self.origin != address || session.is_empty() {
            self.open = false;
            self.rooms.clear();
            return None;
        }
        if !self.open {
            return None;
        }
        if !self.busy
            && self
                .last_refresh
                .is_none_or(|at| at.elapsed() >= std::time::Duration::from_secs(30))
        {
            self.request(address.to_owned(), session.clone(), None, ctx.clone());
        }

        let mut selected = None;
        let mut action = None;
        let mut refresh = false;
        let modal = controls::dialog(
            ctx,
            crate::i18n::label("Escolher sala", "Choose room"),
            "saved_rooms_window",
            440.0,
            |ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), 34.0),
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| {
                        refresh = controls::refresh_rooms_button(ui, !self.busy).clicked();
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            if current_room.is_none() {
                                controls::input(
                                    ui,
                                    name,
                                    crate::i18n::label("Seu nome", "Your name"),
                                    false,
                                    64,
                                    !transitioning,
                                );
                            } else {
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(crate::i18n_format!(
                                            "{} sala(s)",
                                            "{} room(s)",
                                            self.rooms.len()
                                        ))
                                        .color(controls::MUTED),
                                    )
                                    .truncate(),
                                );
                            }
                        });
                    },
                );
                let list_height =
                    (ctx.content_rect().height() - ui.min_size().y - 54.0).clamp(0.0, 520.0);
                ui.spacing_mut().scroll.floating = false;
                egui::ScrollArea::vertical()
                    .id_salt("saved_rooms_list")
                    .auto_shrink([false, true])
                    // The modal retains its previous size while the room list loads.
                    // Use the window budget here so the scroll area can grow on the next frame.
                    .min_scrolled_height(list_height)
                    .max_height(list_height)
                    .show(ui, |ui| {
                        // Windows shows the status as a notice.
                        if !cfg!(windows) && !self.status.is_empty() {
                            ui.add(egui::Label::new(&self.status).truncate())
                                .on_hover_text(&self.status);
                        }
                        if self.rooms.is_empty() {
                            ui.label(if self.busy {
                                crate::i18n::label("Atualizando salas…", "Refreshing rooms…")
                            } else {
                                crate::i18n::label(
                                    "Nenhuma sala salva. Crie uma para começar.",
                                    "No saved rooms. Create one to get started.",
                                )
                            });
                        }
                        let valid_name = !name.is_empty()
                            && name.len() <= 64
                            && name
                                .bytes()
                                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
                        let card_width = ui.available_width();
                        for entry in &self.rooms {
                            ui.push_id(&entry.room, |ui| {
                                egui::Frame::new()
                                    .fill(controls::SURFACE)
                                    .stroke(egui::Stroke::new(1.0, controls::BORDER))
                                    .corner_radius(10)
                                    .inner_margin(8)
                                    .show(ui, |ui| {
                                        ui.set_width((card_width - 18.0).max(0.0));
                                        let current = current_room == Some(entry.room.as_str());
                                        ui.allocate_ui_with_layout(
                                            egui::vec2(ui.available_width(), 36.0),
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                let enabled = !self.busy
                                                    && !transitioning
                                                    && !entry.expired
                                                    && !current
                                                    && valid_name;
                                                let enter = controls::primary(
                                                    ui,
                                                    if current {
                                                        crate::i18n::label(
                                                            "Nesta sala",
                                                            "Current room",
                                                        )
                                                    } else {
                                                        crate::i18n::label("Entrar", "Join")
                                                    },
                                                    enabled,
                                                    egui::vec2(
                                                        if current { 88.0 } else { 64.0 },
                                                        36.0,
                                                    ),
                                                );
                                                enter.widget_info(|| {
                                                    egui::WidgetInfo::labeled(
                                                        egui::WidgetType::Button,
                                                        enabled,
                                                        crate::i18n_format!(
                                                            "Entrar em {}",
                                                            "Join {}",
                                                            entry.title
                                                        ),
                                                    )
                                                });
                                                if enter.clicked() {
                                                    selected = Some(CreatedRoom {
                                                        room: entry.room.clone(),
                                                        title: entry.title.clone(),
                                                    });
                                                }
                                                ui.with_layout(
                                                    egui::Layout::top_down(egui::Align::Min),
                                                    |ui| {
                                                        ui.spacing_mut().item_spacing.y = 2.0;
                                                        ui.add(
                                                            egui::Label::new(
                                                                egui::RichText::new(&entry.title)
                                                                    .strong()
                                                                    .size(15.0),
                                                            )
                                                            .truncate(),
                                                        )
                                                        .on_hover_text(&entry.title);
                                                        let summary = crate::i18n_format!(
                                                            "{} · {} pessoa(s)",
                                                            "{} · {} person(s)",
                                                            if entry.expired {
                                                                crate::i18n::label(
                                                                    "Expirada", "Expired",
                                                                )
                                                            } else if entry.persistent {
                                                                crate::i18n::label(
                                                                    "Permanente",
                                                                    "Permanent",
                                                                )
                                                            } else {
                                                                crate::i18n::label(
                                                                    "Temporária · 24 h",
                                                                    "Temporary · 24 h",
                                                                )
                                                            },
                                                            entry.participants
                                                        );
                                                        ui.add(
                                                            egui::Label::new(
                                                                egui::RichText::new(&summary)
                                                                    .size(12.0)
                                                                    .color(controls::MUTED),
                                                            )
                                                            .truncate(),
                                                        )
                                                        .on_hover_text(summary);
                                                    },
                                                );
                                            },
                                        );
                                        let can_delete = !self.busy
                                            && !transitioning
                                            && entry.participants == 0
                                            && !current;
                                        ui.horizontal_wrapped(|ui| {
                                            let mut persistent = entry.persistent;
                                            let checkbox = ui.add_enabled(
                                                !self.busy,
                                                egui::Checkbox::new(
                                                    &mut persistent,
                                                    crate::i18n::label(
                                                        "Manter esta sala",
                                                        "Keep this room",
                                                    ),
                                                ),
                                            );
                                            checkbox.widget_info(|| {
                                                egui::WidgetInfo::selected(
                                                    egui::WidgetType::Checkbox,
                                                    !self.busy,
                                                    persistent,
                                                    crate::i18n_format!(
                                                        "Manter sala {}",
                                                        "Keep room {}",
                                                        entry.title
                                                    ),
                                                )
                                            });
                                            if checkbox.changed() {
                                                action = Some(CatalogAction::Keep(
                                                    entry.room.clone(),
                                                    persistent,
                                                ));
                                            }
                                            {
                                                let delete = controls::compact(
                                                    ui,
                                                    crate::i18n::label(
                                                        "Excluir sala",
                                                        "Delete room",
                                                    ),
                                                    can_delete,
                                                );
                                                delete.widget_info(|| {
                                                    egui::WidgetInfo::labeled(
                                                        egui::WidgetType::Button,
                                                        can_delete,
                                                        crate::i18n_format!(
                                                            "Excluir sala {}",
                                                            "Delete room {}",
                                                            entry.title
                                                        ),
                                                    )
                                                });
                                                if delete.clicked() {
                                                    self.confirm_delete = Some(entry.room.clone());
                                                }
                                            }
                                        });
                                        if self.confirm_delete.as_deref() == Some(&entry.room) {
                                            ui.horizontal_wrapped(|ui| {
                                                ui.label(
                                                    egui::RichText::new(crate::i18n::label(
                                                        "Excluir esta sala?",
                                                        "Delete this room?",
                                                    ))
                                                    .size(11.0)
                                                    .color(controls::MUTED),
                                                );
                                                if controls::compact(
                                                    ui,
                                                    crate::i18n::label("Cancelar", "Cancel"),
                                                    !self.busy,
                                                )
                                                .clicked()
                                                {
                                                    self.confirm_delete = None;
                                                }
                                                let confirm = controls::compact(
                                                    ui,
                                                    crate::i18n::label("Excluir", "Delete"),
                                                    can_delete,
                                                );
                                                confirm.widget_info(|| {
                                                    egui::WidgetInfo::labeled(
                                                        egui::WidgetType::Button,
                                                        can_delete,
                                                        crate::i18n_format!(
                                                            "Confirmar exclusão de {}",
                                                            "Confirm deletion of {}",
                                                            entry.title
                                                        ),
                                                    )
                                                });
                                                if confirm.clicked() {
                                                    action = Some(CatalogAction::Delete(
                                                        entry.room.clone(),
                                                    ));
                                                    self.confirm_delete = None;
                                                }
                                            });
                                        }
                                        if entry.expired {
                                            ui.label(
                                        egui::RichText::new(if entry.participants > 0 {
                                            crate::i18n::label(
                                                "A limpeza aguarda os participantes saírem.",
                                                "Cleanup will start after everyone leaves.",
                                            )
                                        } else {
                                            crate::i18n::label(
                                                "Aguardando limpeza automática.",
                                                "Waiting for automatic cleanup.",
                                            )
                                        })
                                        .size(11.0)
                                        .color(controls::MUTED),
                                    );
                                        }
                                    });
                            });
                        }
                        if transitioning {
                            ui.small(crate::i18n::label(
                                "Aguarde a troca de sala…",
                                "Wait for the room switch…",
                            ));
                        } else if current_room.is_some() {
                            ui.small(crate::i18n::label(
                        "Ao entrar em outra sala, seu compartilhamento atual será encerrado.",
                        "Joining another room will stop your current stream.",
                    ));
                        }
                    });
            },
        );
        self.open = !modal.should_close() && selected.is_none();
        if refresh || action.is_some() {
            self.request(address.to_owned(), session, action, ctx.clone());
        }
        selected
    }
}
fn catalog_post(
    address: &str,
    session: &str,
    path: &str,
    body: serde_json::Value,
) -> Result<serde_json::Value> {
    ensure!(
        !session.is_empty() && session.len() <= 4096,
        crate::i18n::label("Entre no servidor novamente", "Connect to the server again")
    );
    let response = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(15))
        .build()?
        .post(crate::invitation::endpoint(address, path)?)
        .bearer_auth(session)
        .json(&body)
        .send()
        .map_err(|_| {
            anyhow::anyhow!(crate::i18n::label(
                "Servidor indisponível; tente novamente",
                "Server unavailable; try again"
            ))
        })?;
    match response.status().as_u16() {
        200 => {}
        401 | 403 => anyhow::bail!(crate::i18n::label(
            "Sessão expirada; entre no servidor novamente",
            "Session expired; connect to the server again"
        )),
        404 => anyhow::bail!(crate::i18n::label(
            "Sala já removida; atualize a lista",
            "Room already removed; refresh the list"
        )),
        409 if path == "/api/rooms/delete" => anyhow::bail!(crate::i18n::label(
            "A sala recebeu participantes. Aguarde todos saírem para excluir",
            "The room has participants. Wait until everyone leaves before deleting it"
        )),
        _ => anyhow::bail!(crate::i18n::label(
            "Não foi possível atualizar a lista de salas",
            "Could not refresh the room list"
        )),
    }
    let mut bytes = Vec::new();
    response.take(65537).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 65536,
        crate::i18n::label("Resposta excedeu o limite", "Response exceeded the limit")
    );
    Ok(serde_json::from_slice(&bytes)?)
}

pub struct CreatedRoom {
    pub room: String,
    pub title: String,
}
pub struct Creator {
    pub open: bool,
    pub title: String,
    pub persistent: bool,
    pub busy: bool,
    pub status: String,
    request_id: String,
    requested_title: String,
    requested_persistent: bool,
    receiver: Option<mpsc::Receiver<Result<CreatedRoom>>>,
}
impl Default for Creator {
    fn default() -> Self {
        Self {
            open: false,
            title: String::new(),
            persistent: false,
            busy: false,
            status: String::new(),
            request_id: String::new(),
            requested_title: String::new(),
            requested_persistent: false,
            receiver: None,
        }
    }
}
impl Creator {
    /// The server may finish creating the room, but a cancelled UI intent must
    /// never join it later. The room remains discoverable in the catalog.
    pub fn cancel_automatic_join(&mut self) {
        if self.receiver.take().is_some() {
            self.busy = false;
            self.status =
                crate::i18n::label("Entrada automática cancelada", "Automatic join cancelled")
                    .into();
        }
    }
    pub fn open(&mut self, _address: &str) {
        self.open = true;
        self.status.clear();
        self.request_id = uuid::Uuid::new_v4().simple().to_string();
        self.requested_title.clear();
    }
    pub fn start(&mut self, address: String, session: String, ctx: egui::Context) {
        if self.busy {
            return;
        }
        let title = self.title.trim().to_owned();
        if self.requested_title != title || self.requested_persistent != self.persistent {
            self.request_id = uuid::Uuid::new_v4().simple().to_string();
            self.requested_title = title.clone();
            self.requested_persistent = self.persistent;
        }
        let persistent = self.persistent;
        let request = self.request_id.clone();
        let (tx, rx) = mpsc::channel();
        self.busy = true;
        self.status = crate::i18n::label("Criando sala…", "Creating room…").into();
        std::thread::spawn(move || {
            let _ = tx.send(create(&address, &session, &title, &request, persistent));
            ctx.request_repaint();
        });
        self.receiver = Some(rx);
    }
    pub fn poll(&mut self) -> Option<CreatedRoom> {
        let result = match self.receiver.as_ref()?.try_recv() {
            Ok(r) => r,
            Err(mpsc::TryRecvError::Empty) => return None,
            Err(_) => Err(anyhow::anyhow!(crate::i18n::label(
                "Criação interrompida; tente novamente",
                "Creation interrupted; try again"
            ))),
        };
        self.receiver = None;
        self.busy = false;
        match result {
            Ok(room) => {
                self.open = false;
                self.status.clear();
                Some(room)
            }
            Err(e) => {
                self.status = e.to_string();
                None
            }
        }
    }
}

pub struct ServerAccess {
    pub code: String,
    pub remember: bool,
    pub busy: bool,
    pub status: String,
    token: String,
    origin: String,
    loaded: bool,
    refresh: std::time::Instant,
    receiver: Option<mpsc::Receiver<Result<AccessGrant>>>,
}

struct AccessGrant {
    token: String,
    address: String,
    code: String,
    remember: bool,
}
impl Default for ServerAccess {
    fn default() -> Self {
        Self {
            code: String::new(),
            remember: cfg!(windows),
            busy: false,
            status: String::new(),
            token: String::new(),
            origin: String::new(),
            loaded: false,
            refresh: std::time::Instant::now(),
            receiver: None,
        }
    }
}
impl ServerAccess {
    pub fn token(&self) -> String {
        if self.authenticated() {
            self.token.clone()
        } else {
            String::new()
        }
    }
    pub fn authenticated(&self) -> bool {
        !self.token.is_empty() && self.refresh.elapsed() < std::time::Duration::from_secs(840)
    }
    pub fn logout(&mut self, address: &str) {
        self.receiver = None;
        self.busy = false;
        let _ = forget_code(address);
        self.code.clear();
        self.token.clear();
        self.status.clear();
        self.loaded = true;
    }
    pub fn start(&mut self, address: String, device: String, ctx: egui::Context) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.status = "Autenticando…".into();
        let code = self.code.trim().to_owned();
        let remember = self.remember;
        let (tx, rx) = mpsc::channel();
        self.receiver = Some(rx);
        std::thread::spawn(move || {
            let result = login(&address, &code, &device).map(|token| AccessGrant {
                token,
                address,
                code,
                remember,
            });
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }
    pub fn poll(&mut self, address: &str, device: &str, ctx: egui::Context) {
        if self.origin != address && !self.busy {
            self.token.clear();
            self.code.clear();
            self.loaded = false;
            self.origin = address.to_owned();
            self.status.clear();
        }
        if !self.loaded {
            self.loaded = true;
            if let Ok(code) = load_code(address) {
                self.code = code;
                self.start(address.to_owned(), device.to_owned(), ctx.clone());
            }
        }
        if let Some(rx) = &self.receiver {
            match rx.try_recv() {
                Ok(result) => {
                    self.receiver = None;
                    self.busy = false;
                    match result {
                        Ok(grant) if grant.address == address => {
                            // Only a grant accepted by the UI may change saved access.
                            let failed = if grant.remember {
                                save_code(&grant.address, &grant.code)
                            } else {
                                forget_code(&grant.address)
                            }
                            .is_err();
                            self.token = grant.token;
                            self.refresh = std::time::Instant::now();
                            self.status = if failed {
                                crate::i18n::label("Acesso liberado; não foi possível atualizar a chave salva neste dispositivo","Access granted; could not update the key saved on this device").into()
                            } else {
                                String::new()
                            };
                        }
                        Ok(_) => {}
                        Err(e) => {
                            self.token.clear();
                            self.status = e.to_string();
                        }
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.receiver = None;
                    self.busy = false;
                    self.token.clear();
                    self.status = crate::i18n::label(
                        "Acesso interrompido; tente novamente",
                        "Access interrupted; try again",
                    )
                    .into();
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if !self.token.is_empty()
            && self.refresh.elapsed() >= std::time::Duration::from_secs(780)
            && !self.busy
        {
            self.start(address.to_owned(), device.to_owned(), ctx);
        }
    }
}

#[cfg(test)]
#[test]
fn server_logout_discards_a_pending_access_refresh() {
    let address = format!("https://logout-test-{}.invalid", uuid::Uuid::new_v4());
    let (tx, rx) = mpsc::channel();
    let mut access = ServerAccess {
        token: "test-session".into(),
        origin: address.clone(),
        loaded: true,
        busy: true,
        receiver: Some(rx),
        ..Default::default()
    };
    tx.send(Ok(AccessGrant {
        token: "refreshed-test-session".into(),
        address: address.clone(),
        code: String::new(),
        remember: false,
    }))
    .unwrap();
    access.logout(&address);
    access.poll(&address, "test-device", egui::Context::default());
    assert!(!access.authenticated());
    assert!(!access.busy);
}

fn login(address: &str, code: &str, device: &str) -> Result<String> {
    ensure!(
        (32..=64).contains(&code.len())
            && code
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
        crate::i18n::label(
            "Informe uma chave de acesso válida",
            "Enter a valid access key"
        )
    );
    let response = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(12))
        .user_agent("HyperCast-Desktop/0.6")
        .build()?
        .post(crate::invitation::endpoint(address, "/api/login")?)
        .bearer_auth(code)
        .json(&serde_json::json!({"device_id":device}))
        .send()
        .map_err(|_| {
            anyhow::anyhow!(crate::i18n::label(
                "Servidor indisponível; tente novamente",
                "Server unavailable; try again"
            ))
        })?;
    match response.status().as_u16() {
        200 => {}
        401 => anyhow::bail!(crate::i18n::label(
            "Chave inválida, expirada ou revogada",
            "Invalid, expired or revoked key"
        )),
        status => anyhow::bail!("Acesso indisponível (HTTP {status}); tente novamente"),
    }
    let mut bytes = Vec::new();
    response.take(16385).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 16384,
        crate::i18n::label(
            "Resposta de acesso excedeu o limite",
            "Access response exceeded the limit"
        )
    );
    let value: serde_json::Value = serde_json::from_slice(&bytes)?;
    let token = value["token"].as_str().ok_or_else(|| {
        anyhow::anyhow!(crate::i18n::label(
            "Resposta de acesso inválida",
            "Invalid access response"
        ))
    })?;
    ensure!(
        !token.is_empty() && token.len() <= 4096 && value["expires_in"].as_u64() == Some(900),
        crate::i18n::label("Resposta de acesso inválida", "Invalid access response")
    );
    Ok(token.to_owned())
}

pub fn create(
    address: &str,
    code: &str,
    title: &str,
    request_id: &str,
    persistent: bool,
) -> Result<CreatedRoom> {
    ensure!(
        !code.is_empty() && code.len() <= 4096,
        crate::i18n::label("Autentique-se no servidor", "Authenticate with the server")
    );
    let endpoint = crate::invitation::endpoint(address, "/api/rooms")?;
    let response = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(20))
        .user_agent("HyperCast-Desktop/0.6")
        .build()?
        .post(endpoint)
        .bearer_auth(code)
        .json(
            &serde_json::json!({"room_name":title,"request_id":request_id,"persistent":persistent}),
        )
        .send()
        .map_err(|_| {
            anyhow::anyhow!(crate::i18n::label(
                "Não foi possível acessar o servidor; tente novamente",
                "Could not reach the server; try again"
            ))
        })?;
    match response.status().as_u16() {
        200 => {}
        400 => anyhow::bail!(crate::i18n::label(
            "Use um nome de sala com 1 a 64 caracteres visíveis",
            "Use a room name with 1 to 64 visible characters"
        )),
        401 | 403 => anyhow::bail!(crate::i18n::label(
            "Sessão expirada; entre novamente no servidor",
            "Session expired; connect to the server again"
        )),
        409 => {
            let mut body = Vec::new();
            response.take(4097).read_to_end(&mut body)?;
            let value = serde_json::from_slice::<serde_json::Value>(&body).unwrap_or_default();
            anyhow::bail!(match value["error"].as_str() {
                Some("Saved room limit reached") => crate::i18n::label(
                    "Limite de 32 salas salvas atingido; exclua uma sala vazia",
                    "The limit of 32 saved rooms was reached; delete an empty room"
                ),
                _ => crate::i18n::label(
                    "Pedido de criação desatualizado; feche esta janela e tente novamente",
                    "Outdated creation request; close this window and try again"
                ),
            });
        }
        429 => anyhow::bail!(crate::i18n::label(
            "Limite de criação atingido; tente novamente mais tarde",
            "Creation limit reached; try again later"
        )),
        _ => anyhow::bail!(crate::i18n::label(
            "Criação de salas indisponível; tente novamente",
            "Room creation unavailable; try again"
        )),
    }
    let mut bytes = Vec::new();
    response.take(16385).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 16384,
        crate::i18n::label(
            "Resposta de sala excedeu o limite",
            "Room response exceeded the limit"
        )
    );
    let value: serde_json::Value = serde_json::from_slice(&bytes)?;
    let room = value["room"].as_str().ok_or_else(|| {
        anyhow::anyhow!(crate::i18n::label(
            "Resposta de sala inválida",
            "Invalid room response"
        ))
    })?;
    let title = value["room_name"].as_str().ok_or_else(|| {
        anyhow::anyhow!(crate::i18n::label(
            "Resposta de sala inválida",
            "Invalid room response"
        ))
    })?;
    ensure!(
        !room.is_empty() && room.len() <= 64 && title.chars().count() <= 64,
        crate::i18n::label("Resposta de sala inválida", "Invalid room response")
    );
    Ok(CreatedRoom {
        room: room.to_owned(),
        title: title.to_owned(),
    })
}

#[cfg(windows)]
fn credential_path(address: &str) -> Result<(std::path::PathBuf, Vec<u8>)> {
    use sha2::{Digest, Sha256};
    let origin = crate::invitation::endpoint(address, "/api/rooms")?
        .origin()
        .ascii_serialization();
    let fingerprint = format!("{:x}", Sha256::digest(origin.as_bytes()));
    let root = std::path::PathBuf::from(
        std::env::var_os("LOCALAPPDATA")
            .ok_or_else(|| anyhow::anyhow!("Windows profile unavailable"))?,
    )
    .join("HyperCast");
    Ok((
        root.join(format!("server-access-{fingerprint}.bin")),
        origin.into_bytes(),
    ))
}
#[cfg(windows)]
fn crypt(data: &[u8], entropy: &[u8], protect: bool) -> Result<Vec<u8>> {
    use windows::Win32::{
        Foundation::{HLOCAL, LocalFree},
        Security::Cryptography::*,
    };
    ensure!(
        data.len() <= 4096 && entropy.len() <= 4096,
        "Credential size exceeded"
    );
    let input = CRYPT_INTEGER_BLOB {
        cbData: data.len() as u32,
        pbData: data.as_ptr() as *mut u8,
    };
    let salt = CRYPT_INTEGER_BLOB {
        cbData: entropy.len() as u32,
        pbData: entropy.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    // SAFETY: input/salt slices live through the call. DPAPI allocates output;
    // it is copied before LocalFree, including validation error paths. No machine flag.
    unsafe {
        if protect {
            CryptProtectData(
                &input,
                windows::core::PCWSTR::null(),
                Some(&salt),
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )?;
        } else {
            CryptUnprotectData(
                &input,
                None,
                Some(&salt),
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )?;
        }
        let result = if !output.pbData.is_null() && output.cbData <= 4096 {
            Ok(std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec())
        } else {
            Err(anyhow::anyhow!("Invalid protected credential"))
        };
        LocalFree(Some(HLOCAL(output.pbData as *mut _)));
        result
    }
}
#[cfg(windows)]
pub fn load_code(address: &str) -> Result<String> {
    let (path, entropy) = credential_path(address)?;
    let mut sealed = Vec::new();
    std::fs::File::open(path)?
        .take(4097)
        .read_to_end(&mut sealed)?;
    String::from_utf8(crypt(&sealed, &entropy, false)?).map_err(Into::into)
}
#[cfg(windows)]
pub fn save_code(address: &str, code: &str) -> Result<()> {
    let (path, entropy) = credential_path(address)?;
    let sealed = crypt(code.as_bytes(), &entropy, true)?;
    std::fs::create_dir_all(path.parent().unwrap())?;
    use std::io::Write;
    let staging = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4().simple()));
    let result = (|| -> Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&staging)?;
        file.write_all(&sealed)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&staging, &path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(staging);
    }
    result
}
#[cfg(windows)]
pub fn forget_code(address: &str) -> Result<()> {
    let (path, _) = credential_path(address)?;
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

#[cfg(test)]
#[test]
fn compact_catalog_keeps_name_and_first_room_visible_and_clickable() {
    let title = "Sala com um nome longo para testar janelas pequenas";
    let mut catalog = Catalog {
        open: true,
        origin: "test".into(),
        last_refresh: Some(std::time::Instant::now()),
        rooms: (0..8)
            .map(|index| SavedRoom {
                room: index.to_string(),
                title: title.into(),
                persistent: true,
                expired: false,
                participants: 0,
            })
            .collect(),
        ..Default::default()
    };
    let ctx = egui::Context::default();
    let area = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(230.0, 180.0));
    let mut name = "LayoutTester".to_owned();
    let mut selected = None;
    let mut draw = |catalog: &mut Catalog, area, events| {
        ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(area),
                events,
                ..Default::default()
            },
            |ui| {
                if let Some(room) =
                    catalog.show(ui.ctx(), "test", "session".into(), &mut name, None, false)
                {
                    selected = Some(room.room);
                }
            },
        )
    };
    let visible_text = |output: &egui::FullOutput, area: egui::Rect, text: &str| {
        output.shapes.iter().find_map(|shape| {
            if let egui::Shape::Text(label) = &shape.shape
                && label.galley.text() == text
                && shape.clip_rect.contains_rect(label.visual_bounding_rect())
                && area.contains_rect(label.visual_bounding_rect())
            {
                return Some(label.visual_bounding_rect().center());
            }
            None
        })
    };
    let rooms = std::mem::take(&mut catalog.rooms);
    let regular = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(989.0, 695.0));
    for _ in 0..3 {
        let _ = draw(&mut catalog, regular, Vec::new());
    }
    catalog.rooms = rooms;
    // Loading changes the size of the centered modal. Let it settle before
    // checking that the room cards have reclaimed the available space.
    for _ in 0..3 {
        let _ = draw(&mut catalog, regular, Vec::new());
    }
    let output = draw(&mut catalog, regular, Vec::new());
    let join = crate::i18n::label("Entrar", "Join");
    let visible_rows = output.shapes.iter().filter(|shape| {
        matches!(&shape.shape, egui::Shape::Text(label)
            if label.galley.text() == join && shape.clip_rect.contains_rect(label.visual_bounding_rect())
                && regular.contains_rect(label.visual_bounding_rect()))
    }).count();
    assert!(
        visible_rows >= 4,
        "Only {visible_rows} room rows fit after loading"
    );

    for _ in 0..3 {
        let _ = draw(&mut catalog, area, Vec::new());
    }
    let output = draw(&mut catalog, area, Vec::new());
    assert!(
        visible_text(&output, area, "LayoutTester").is_some(),
        "Name field is clipped"
    );
    let pos = visible_text(&output, area, join)
        .expect("First room should be reachable without scrolling");
    for pressed in [true, false] {
        let _ = draw(
            &mut catalog,
            area,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                },
            ],
        );
    }
    assert_eq!(selected.as_deref(), Some("0"));
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    #[test]
    fn dpapi_is_bound_to_user_and_origin() {
        let sealed = crypt(b"creator-code", b"https://server-a", true).unwrap();
        assert_ne!(sealed, b"creator-code");
        assert_eq!(
            crypt(&sealed, b"https://server-a", false).unwrap(),
            b"creator-code"
        );
        assert!(crypt(&sealed, b"https://server-b", false).is_err());
    }
}

// Linux access credentials remain in memory until a Secret Service backend is
// implemented. Never fall back to storing a shared server key in plaintext.
#[cfg(not(windows))]
pub fn load_code(_address: &str) -> Result<String> {
    anyhow::bail!(crate::i18n::label(
        "Acesso salvo indisponível nesta plataforma",
        "Saved access is unavailable on this platform"
    ))
}
#[cfg(not(windows))]
pub fn save_code(_address: &str, _code: &str) -> Result<()> {
    anyhow::bail!(crate::i18n::label(
        "A chave permanece somente nesta sessão",
        "The key is kept only for this session"
    ))
}
#[cfg(not(windows))]
pub fn forget_code(_address: &str) -> Result<()> {
    Ok(())
}
