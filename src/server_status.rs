//! Reachability of the SFU, independent from room membership and credentials.
use std::{io::Read, sync::mpsc, time::{Duration, Instant}};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status { Checking, Online, Offline, InvalidAddress }

pub struct ServerStatus {
    pub status: Status,
    pub detail: &'static str,
    address: String,
    changed: Instant,
    checked: Option<Instant>,
    receiver: Option<mpsc::Receiver<(String, Result<(), &'static str>)>>,
}

impl Default for ServerStatus {
    fn default() -> Self {
        Self { status: Status::Checking, detail: crate::i18n::label("Verificando disponibilidade do servidor","Checking server availability"),
            address: String::new(), changed: Instant::now(), checked: None, receiver: None }
    }
}

impl ServerStatus {
    pub fn label(&self) -> &'static str {
        match self.status {
            Status::Checking => crate::i18n::label("VERIFICANDO","CHECKING"),
            Status::Online => "ONLINE",
            Status::Offline => "OFFLINE",
            Status::InvalidAddress => crate::i18n::label("INVÁLIDO","INVALID"),
        }
    }

    pub fn poll(&mut self, address: &str, ctx: &eframe::egui::Context) {
        let address = address.trim();
        if self.address != address {
            self.address = address.to_owned();
            self.changed = Instant::now();
            self.checked = None;
            self.status = Status::Checking;
            self.detail = crate::i18n::label("Verificando disponibilidade do servidor","Checking server availability");
        }
        if let Some(rx) = &self.receiver {
            match rx.try_recv() {
                Ok((requested, result)) => {
                    self.receiver = None;
                    // An old response must never mark a newly edited server online.
                    if requested == self.address {
                        self.checked = Some(Instant::now());
                        match result {
                            Ok(()) => { self.status = Status::Online; self.detail = crate::i18n::label("Servidor disponível · independente da conexão com a sala","Server available · independent of room connection"); }
                            Err(detail) => { self.status = Status::Offline; self.detail = detail; }
                        }
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.receiver = None;
                    self.checked = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        let Ok(endpoint) = crate::invitation::endpoint(address, "/") else {
            self.status = Status::InvalidAddress;
            self.detail = crate::i18n::label("Informe um endereço WSS válido para verificar o servidor","Enter a valid WSS address to check the server");
            return;
        };
        if self.receiver.is_some() || self.changed.elapsed() < Duration::from_millis(600)
            || self.checked.is_some_and(|at| at.elapsed() < Duration::from_secs(15)) { return; }
        let (tx, rx) = mpsc::channel();
        self.receiver = Some(rx);
        let requested = address.to_owned();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send((requested, probe(endpoint)));
            ctx.request_repaint();
        });
    }
}

fn probe(endpoint: url::Url) -> Result<(), &'static str> {
    let client = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(3)).timeout(Duration::from_secs(4))
        .user_agent(concat!("HyperCast-Desktop/", env!("CARGO_PKG_VERSION")))
        .build().map_err(|_| crate::i18n::label("Não foi possível iniciar a verificação do servidor","Could not start the server check"))?;
    // No room token, shared key or device ID. Require LiveKit's health response;
    // a proxy login page, redirect or Cloudflare error is not a healthy SFU.
    let response = client.get(endpoint).header("Cache-Control", "no-cache, no-store")
        .send().map_err(|_| crate::i18n::label("Servidor inacessível · verifique a rede e o endereço","Server unreachable · check your network and the address"))?;
    if response.status().as_u16() == 403 {
        return Err(crate::i18n::label("Acesso ao servidor bloqueado pelo proxy ou firewall (HTTP 403)","Server access blocked by proxy or firewall (HTTP 403)"));
    }
    if response.status().as_u16() != 200 { return Err(crate::i18n::label("Servidor indisponível ou ainda não está pronto","Server unavailable or not ready yet")); }
    let mut body = Vec::new();
    response.take(17).read_to_end(&mut body).map_err(|_| crate::i18n::label("Resposta do servidor interrompida","Server response interrupted"))?;
    if body.len() <= 16 && body.as_slice() == b"OK" { Ok(()) }
    else { Err(crate::i18n::label("O endereço não retornou a confirmação de saúde do LiveKit","The address did not return the LiveKit health confirmation")) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Write, net::TcpListener};

    #[test]
    fn health_requires_livekit_and_never_follows_redirects() {
        for (status, body, healthy) in [("200 OK", "OK", true), ("200 OK", "<html>login</html>", false),
            ("503 Unavailable", "OK", false), ("302 Found", "OK", false), ("403 Forbidden", "OK", false)] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let endpoint = url::Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
            let worker = std::thread::spawn(move || {
                let (mut socket, _) = listener.accept().unwrap();
                socket.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
                let mut request = [0; 4096]; let count = socket.read(&mut request).unwrap();
                let request = String::from_utf8_lossy(&request[..count]).to_lowercase();
                assert!(!request.contains("authorization:") && !request.contains("cookie:"));
                write!(socket, "HTTP/1.1 {status}\r\nContent-Length: {}\r\nLocation: http://127.0.0.1:1/\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            });
            assert_eq!(probe(endpoint).is_ok(), healthy);
            worker.join().unwrap();
        }
    }

    #[test]
    fn editing_address_discards_previous_result() {
        let mut status = ServerStatus::default();
        let (tx, rx) = mpsc::channel();
        status.address = "wss://previous.example".into();
        status.receiver = Some(rx);
        tx.send((status.address.clone(), Ok(()))).unwrap();
        status.poll("wss://new.example", &eframe::egui::Context::default());
        assert_eq!(status.status, Status::Checking);
        assert!(status.checked.is_none());
        status.poll("ws://public.example", &eframe::egui::Context::default());
        assert_eq!(status.status, Status::InvalidAddress);
    }
}
