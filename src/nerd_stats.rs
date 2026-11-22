//! Live "stats for nerds": a small on-screen panel, sampled at 1 Hz only while
//! it is open. Values are read from WebRTC counters as deltas between two
//! samples; nothing is estimated. Shown locally, never exported, and it holds
//! no address, participant name or credential (rows are keyed by the stage id
//! the interface already uses).
use livekit::webrtc::stats::{IceCandidateType, IceServerTransportProtocol, RtcStats};
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

/// Panel refresh while open; closed, the reception monitor keeps its 5 s cadence.
pub const INTERVAL: Duration = Duration::from_secs(1);
/// Seconds of throughput kept for each sparkline.
pub const HISTORY: usize = 60;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Path {
    /// UDP/TCP straight to the SFU (host, server- or peer-reflexive candidate).
    Direct,
    /// Through the TURN relay.
    Relay,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transport {
    Udp,
    Tcp,
    Tls,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Route {
    pub path: Path,
    pub transport: Transport,
    pub rtt_ms: Option<f64>,
    /// Sender-side bandwidth estimate of the selected pair (publisher only).
    pub available_out_bps: Option<f64>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Limit {
    Cpu,
    Bandwidth,
    Other,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Inbound {
    /// Stage id (`owner` or `owner/camera`) used to label the row.
    pub key: String,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub mbps: Option<f64>,
    pub loss_pct: Option<f64>,
    pub jitter_ms: f64,
    /// Mean jitter-buffer wait of the frames emitted in the last interval.
    pub buffer_ms: Option<f64>,
    pub decode_ms: Option<f64>,
    pub freezes: u32,
    pub codec: &'static str,
    pub hardware: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Outbound {
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    /// RTP payload rate of the video, retransmissions included.
    pub mbps: Option<f64>,
    pub limit: Option<Limit>,
    pub codec: &'static str,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    pub inbound: Vec<Inbound>,
    pub outbound: Option<Outbound>,
    pub receive: Option<Route>,
    pub send: Option<Route>,
}

#[derive(Clone, Copy)]
struct Counters {
    at: Instant,
    bytes: u64,
    received: u64,
    lost: i64,
    buffer: f64,
    emitted: u64,
    decode: f64,
    decoded: u64,
}

/// Turns cumulative counters into per-interval rates. A counter that goes
/// backwards (receiver replaced) restarts that row instead of showing a spike.
#[derive(Default)]
pub struct Sampler {
    inbound: HashMap<String, Counters>,
    outbound: Option<(Instant, u64)>,
}
impl Sampler {
    pub fn reset(&mut self) {
        self.inbound.clear();
        self.outbound = None;
    }
    /// `tracks` maps an RTC track id to its stage id; inbound video not in it
    /// (unknown owner) is ignored rather than shown under a guessed name.
    pub fn sample(
        &mut self,
        now: Instant,
        publisher: &[RtcStats],
        subscriber: &[RtcStats],
        tracks: &[(String, String)],
    ) -> Snapshot {
        let mut snapshot = Snapshot {
            receive: route(subscriber),
            send: route(publisher),
            ..Default::default()
        };
        let mut seen = Vec::new();
        for entry in subscriber {
            let RtcStats::InboundRtp(r) = entry else {
                continue;
            };
            if r.stream.kind != "video" {
                continue;
            }
            let Some((_, key)) = tracks
                .iter()
                .find(|(rtc, _)| !rtc.is_empty() && *rtc == r.inbound.track_identifier)
            else {
                continue;
            };
            // Simulcast or a replaced receiver can briefly report two rows.
            if seen.contains(key) {
                continue;
            }
            seen.push(key.clone());
            let now_counters = Counters {
                at: now,
                bytes: r.inbound.bytes_received,
                received: r.received.packets_received,
                lost: r.received.packets_lost,
                buffer: r.inbound.jitter_buffer_delay,
                emitted: r.inbound.jitter_buffer_emitted_count,
                decode: r.inbound.total_decode_time,
                decoded: r.inbound.frames_decoded as u64,
            };
            let old = self
                .inbound
                .insert(key.clone(), now_counters)
                .filter(|old| {
                    old.bytes <= now_counters.bytes
                        && old.received <= now_counters.received
                        && old.emitted <= now_counters.emitted
                        && old.decoded <= now_counters.decoded
                });
            let mut row = Inbound {
                key: key.clone(),
                width: r.inbound.frame_width,
                height: r.inbound.frame_height,
                fps: r.inbound.frames_per_second.max(0.0),
                mbps: None,
                loss_pct: None,
                jitter_ms: r.received.jitter.max(0.0) * 1000.0,
                buffer_ms: None,
                decode_ms: None,
                freezes: r.inbound.freeze_count,
                codec: codec_name(subscriber, &r.stream.codec_id),
                hardware: r.inbound.power_efficient_decoder,
            };
            if let Some(old) = old {
                let dt = now.saturating_duration_since(old.at).as_secs_f64();
                if dt > 0.0 {
                    row.mbps = Some((now_counters.bytes - old.bytes) as f64 * 8.0 / dt / 1e6);
                }
                let received = (now_counters.received - old.received) as f64;
                let lost = (now_counters.lost - old.lost).max(0) as f64;
                if received + lost > 0.0 {
                    row.loss_pct = Some(lost * 100.0 / (received + lost));
                }
                let emitted = now_counters.emitted - old.emitted;
                if emitted > 0 {
                    row.buffer_ms =
                        Some((now_counters.buffer - old.buffer).max(0.0) * 1000.0 / emitted as f64);
                }
                let decoded = now_counters.decoded - old.decoded;
                if decoded > 0 {
                    row.decode_ms =
                        Some((now_counters.decode - old.decode).max(0.0) * 1000.0 / decoded as f64);
                }
            }
            snapshot.inbound.push(row);
        }
        self.inbound.retain(|key, _| seen.contains(key));
        snapshot.inbound.sort_by(|a, b| a.key.cmp(&b.key));

        // The screen share is the largest outbound video; a camera may also publish.
        let video = publisher
            .iter()
            .filter_map(|entry| match entry {
                RtcStats::OutboundRtp(r) if r.stream.kind == "video" => Some(r),
                _ => None,
            })
            .max_by_key(|r| r.outbound.frame_width as u64 * r.outbound.frame_height as u64);
        match video {
            Some(r) => {
                let bytes = r.sent.bytes_sent;
                let mbps = self
                    .outbound
                    .filter(|(_, old)| *old <= bytes)
                    .and_then(|(at, old)| {
                        let dt = now.saturating_duration_since(at).as_secs_f64();
                        (dt > 0.0).then(|| (bytes - old) as f64 * 8.0 / dt / 1e6)
                    });
                self.outbound = Some((now, bytes));
                use livekit::webrtc::stats::QualityLimitationReason as Q;
                snapshot.outbound = Some(Outbound {
                    width: r.outbound.frame_width,
                    height: r.outbound.frame_height,
                    fps: r.outbound.frames_per_second.max(0.0),
                    mbps,
                    limit: match r.outbound.quality_limitation_reason {
                        Q::None => None,
                        Q::Cpu => Some(Limit::Cpu),
                        Q::Bandwidth => Some(Limit::Bandwidth),
                        Q::Other => Some(Limit::Other),
                    },
                    codec: codec_name(publisher, &r.stream.codec_id),
                });
            }
            None => self.outbound = None,
        }
        snapshot
    }
}

fn codec_name(stats: &[RtcStats], id: &str) -> &'static str {
    let mime = stats.iter().find_map(|entry| match entry {
        RtcStats::Codec(c) if !id.is_empty() && c.rtc.id == id => Some(c.codec.mime_type.as_str()),
        _ => None,
    });
    let Some(mime) = mime else { return "" };
    let name = mime.rsplit('/').next().unwrap_or(mime);
    match name.to_ascii_uppercase().as_str() {
        "H265" | "HEVC" => "HEVC",
        "AV1" => "AV1",
        "H264" => "H.264",
        "VP8" => "VP8",
        "VP9" => "VP9",
        _ => "",
    }
}

/// The selected ICE pair of one peer connection, without its addresses.
fn route(stats: &[RtcStats]) -> Option<Route> {
    let pair = stats.iter().find_map(|entry| {
        let RtcStats::Transport(t) = entry else {
            return None;
        };
        let id = &t.transport.selected_candidate_pair_id;
        stats.iter().find_map(|p| match p {
            RtcStats::CandidatePair(p) if !id.is_empty() && p.rtc.id == *id => Some(p),
            _ => None,
        })
    })?;
    let local = stats.iter().find_map(|entry| match entry {
        RtcStats::LocalCandidate(c) if c.rtc.id == pair.candidate_pair.local_candidate_id => {
            Some(&c.local_candidate)
        }
        _ => None,
    })?;
    let remote_relay = stats.iter().any(|entry| {
        matches!(entry, RtcStats::RemoteCandidate(c)
            if c.rtc.id == pair.candidate_pair.remote_candidate_id
                && c.remote_candidate.candidate_type == Some(IceCandidateType::Relay))
    });
    let relay = local.candidate_type == Some(IceCandidateType::Relay) || remote_relay;
    let transport = match (relay, local.relay_protocol) {
        (true, Some(IceServerTransportProtocol::Tls)) => Transport::Tls,
        (true, Some(IceServerTransportProtocol::Tcp)) => Transport::Tcp,
        (true, Some(IceServerTransportProtocol::Udp)) => Transport::Udp,
        _ if local.protocol.eq_ignore_ascii_case("tcp") => Transport::Tcp,
        _ => Transport::Udp,
    };
    let finite = |v: f64| (v.is_finite() && v > 0.0).then_some(v);
    Some(Route {
        path: if relay { Path::Relay } else { Path::Direct },
        transport,
        rtt_ms: finite(pair.candidate_pair.current_round_trip_time).map(|s| s * 1000.0),
        available_out_bps: finite(pair.candidate_pair.available_outgoing_bitrate),
    })
}

/// Last minute of throughput per row, for the sparklines. Kept by the UI.
#[derive(Default)]
pub struct History {
    rows: HashMap<String, VecDeque<f32>>,
    last: Option<Instant>,
}
impl History {
    pub const SEND: &'static str = "\u{0}send";
    /// Record one new snapshot; repeated frames of the same sample are ignored.
    pub fn push(&mut self, at: Instant, snapshot: &Snapshot, send_mbps: Option<f64>) {
        if self.last == Some(at) {
            return;
        }
        self.last = Some(at);
        let mut keep = Vec::new();
        let mut add = |key: &str, value: Option<f64>| {
            let Some(value) = value.filter(|v| v.is_finite()) else {
                return;
            };
            let row = self.rows.entry(key.to_owned()).or_default();
            if row.len() == HISTORY {
                row.pop_front();
            }
            row.push_back(value.max(0.0) as f32);
            keep.push(key.to_owned());
        };
        for row in &snapshot.inbound {
            add(&row.key, row.mbps);
        }
        add(Self::SEND, send_mbps);
        self.rows.retain(|key, _| keep.contains(key));
    }
    pub fn values(&self, key: &str) -> Vec<f32> {
        self.rows
            .get(key)
            .map(|row| row.iter().copied().collect())
            .unwrap_or_default()
    }
    pub fn clear(&mut self) {
        self.rows.clear();
        self.last = None;
    }
}

pub fn resolution(width: u32, height: u32) -> String {
    if width == 0 || height == 0 {
        "—".into()
    } else {
        format!("{width}×{height}")
    }
}

pub fn mbps(value: Option<f64>) -> String {
    match value.filter(|v| v.is_finite()) {
        Some(v) if v >= 10.0 => format!("{v:.0} Mb/s"),
        Some(v) => format!("{v:.1} Mb/s"),
        None => "— Mb/s".into(),
    }
}

pub fn ms(value: Option<f64>) -> String {
    match value.filter(|v| v.is_finite()) {
        Some(v) if v >= 10.0 => format!("{v:.0} ms"),
        Some(v) if v < 0.05 => "<0.1 ms".into(),
        Some(v) => format!("{v:.1} ms"),
        None => "— ms".into(),
    }
}

impl Route {
    pub fn label(&self) -> String {
        let path = match self.path {
            Path::Direct => crate::i18n::label("direto", "direct"),
            Path::Relay => "TURN",
        };
        let transport = match self.transport {
            Transport::Udp => "UDP",
            Transport::Tcp => "TCP",
            Transport::Tls => "TLS",
        };
        format!("{path} {transport}")
    }
}

impl Limit {
    pub fn label(self) -> &'static str {
        match self {
            Self::Cpu => crate::i18n::label("limitado: CPU", "limited: CPU"),
            Self::Bandwidth => crate::i18n::label("limitado: rede", "limited: network"),
            Self::Other => crate::i18n::label("limitado", "limited"),
        }
    }
}

/// One stream of the panel: a value line and a dimmer detail line.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub up: bool,
    pub name: String,
    pub main: String,
    pub rate: String,
    pub detail: String,
    pub spark: Vec<f32>,
    /// Loss or freezes worth noticing; the detail line turns amber.
    pub alert: bool,
}

/// What the local publisher is doing, from the encoder's own counters.
pub struct Sending<'a> {
    pub quality: &'a crate::video_metrics::QualitySnapshot,
    pub outbound: Option<&'a Outbound>,
}

pub fn lines(
    snapshot: Option<&Snapshot>,
    sending: Option<Sending<'_>>,
    history: &History,
    mut name: impl FnMut(&str) -> String,
) -> Vec<Line> {
    use crate::i18n::label;
    let mut out = Vec::new();
    if let Some(Sending { quality, outbound }) = sending {
        let (width, height) = quality
            .resolution
            .or(outbound.map(|o| (o.width, o.height)))
            .unwrap_or_default();
        let fps = quality.send.map(|s| s.fps).or(outbound.map(|o| o.fps));
        let codec = quality
            .codec
            .or(outbound.map(|o| o.codec).filter(|c| !c.is_empty()));
        let mut main = format!(
            "{} {}",
            resolution(width, height),
            fps.map_or("— fps".into(), |f| format!("{f:.0} fps"))
        );
        for part in [codec, quality.encoder_backend].into_iter().flatten() {
            main.push(' ');
            main.push_str(part);
        }
        let mut detail = Vec::new();
        if quality.send_paused {
            detail.push(label("pausado pelo servidor", "paused by the server").to_owned());
        }
        if let Some(rate) = quality.adaptive_upload {
            if rate.target > 0 {
                detail.push(format!(
                    "{} {}",
                    label("alvo", "target"),
                    mbps(Some(rate.target as f64 / 1e6))
                ));
            }
            if let Some(network) = rate.network {
                detail.push(format!(
                    "{} {}",
                    label("rede", "network"),
                    mbps(Some(network as f64 / 1e6))
                ));
            }
        }
        if let Some(qp) = quality.encode_window.and_then(|w| w.qp_avg) {
            detail.push(format!("QP {qp:.0}"));
        }
        let limit = outbound.and_then(|o| o.limit);
        if let Some(limit) = limit {
            detail.push(limit.label().to_owned());
        }
        out.push(Line {
            up: true,
            name: label("Você", "You").to_owned(),
            main,
            rate: mbps(
                quality
                    .send
                    .map(|s| s.mbps)
                    .or(outbound.and_then(|o| o.mbps)),
            ),
            detail: detail.join(" · "),
            spark: history.values(History::SEND),
            alert: quality.send_paused || limit.is_some(),
        });
    }
    for row in snapshot.map(|s| &s.inbound[..]).unwrap_or_default() {
        let mut main = format!("{} {:.0} fps", resolution(row.width, row.height), row.fps);
        if !row.codec.is_empty() {
            main.push(' ');
            main.push_str(row.codec);
        }
        if row.hardware {
            main.push_str(" hw");
        }
        let mut detail = vec![
            format!(
                "{} {}",
                label("perda", "loss"),
                row.loss_pct.map_or("—".into(), |l| format!("{l:.1}%"))
            ),
            format!("jitter {}", ms(Some(row.jitter_ms))),
            format!("buffer {}", ms(row.buffer_ms)),
            format!("decode {}", ms(row.decode_ms)),
        ];
        if row.freezes > 0 {
            detail.push(format!("{} {}", row.freezes, label("travadas", "freezes")));
        }
        out.push(Line {
            up: false,
            name: name(&row.key),
            main,
            rate: mbps(row.mbps),
            detail: detail.join(" · "),
            spark: history.values(&row.key),
            alert: row.loss_pct.is_some_and(|l| l >= 2.0),
        });
    }
    out
}

/// The connection row: RTT and path of reception, plus the send path when it differs.
pub fn network(snapshot: Option<&Snapshot>, fallback_rtt: Option<f64>, sending: bool) -> String {
    use crate::i18n::label;
    let receive = snapshot.and_then(|s| s.receive);
    let send = snapshot.and_then(|s| s.send).filter(|_| sending);
    let rtt = receive
        .and_then(|r| r.rtt_ms)
        .or(send.and_then(|r| r.rtt_ms))
        .or(fallback_rtt);
    let mut parts = vec![format!("RTT {}", ms(rtt))];
    match (receive, send) {
        (Some(r), Some(s)) if r.label() != s.label() => {
            parts.push(format!("{} {}", label("recebe", "in"), r.label()));
            parts.push(format!("{} {}", label("envia", "out"), s.label()));
        }
        (Some(r), _) => parts.push(r.label()),
        (None, Some(s)) => parts.push(s.label()),
        (None, None) => {}
    }
    parts.join(" · ")
}

/// The panel's egui area; its native-video cutout comes from `panel`'s return value.
pub fn panel_id() -> eframe::egui::Id {
    eframe::egui::Id::new("nerd-stats")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Corner {
    TopLeft,
    TopRight,
}

/// Draw the panel in a stage corner. Returns its rect so the native video is
/// cut around it (the caller also finds it among the foreground layers).
pub fn panel(
    ctx: &eframe::egui::Context,
    stage: eframe::egui::Rect,
    corner: Corner,
    lines: &[Line],
    network: &str,
) -> eframe::egui::Rect {
    use crate::desktop_ui::{ACCENT, MUTED, PANEL, TEXT};
    use eframe::egui::{self, Color32, FontId, RichText};
    const AMBER: Color32 = Color32::from_rgb(240, 196, 110);
    let font = FontId::monospace(11.0);
    let small = FontId::monospace(10.0);
    let name_width = lines
        .iter()
        .map(|l| l.name.chars().count().min(12))
        .max()
        .unwrap_or(0)
        .max(4);
    let main_width = lines
        .iter()
        .map(|l| l.main.chars().count())
        .max()
        .unwrap_or(0);
    let rate_width = lines
        .iter()
        .map(|l| l.rate.chars().count())
        .max()
        .unwrap_or(0);
    let (pivot, at) = match corner {
        Corner::TopRight => (
            egui::Align2::RIGHT_TOP,
            stage.right_top() + egui::vec2(-10.0, 10.0),
        ),
        Corner::TopLeft => (
            egui::Align2::LEFT_TOP,
            stage.left_top() + egui::vec2(10.0, 10.0),
        ),
    };
    egui::Area::new(panel_id())
        .order(egui::Order::Foreground)
        .pivot(pivot)
        .fixed_pos(at)
        .constrain_to(stage.shrink(4.0))
        .interactable(false)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(PANEL.gamma_multiply(0.94))
                .stroke(egui::Stroke::new(1.0, crate::desktop_ui::BORDER))
                .corner_radius(crate::kit::RADIUS)
                .inner_margin(egui::Margin::symmetric(9, 7))
                .show(ui, |ui| {
                    ui.set_max_width((stage.width() - 28.0).max(120.0));
                    ui.spacing_mut().item_spacing = egui::vec2(6.0, 1.0);
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                    // Detail lines start under the value column, measured in pixels
                    // because they use a smaller font than the name column.
                    let name_cell = |ui: &mut egui::Ui, arrow: &str, name: &str| -> f32 {
                        let name: String = name.chars().take(name_width).collect();
                        let cell = ui.label(
                            RichText::new(format!("{arrow} {name:<name_width$}"))
                                .font(font.clone())
                                .color(MUTED),
                        );
                        cell.rect.right() + ui.spacing().item_spacing.x
                    };
                    let detail = |ui: &mut egui::Ui, column: Option<f32>, text: String, color| {
                        ui.horizontal(|ui| {
                            if let Some(x) = column {
                                ui.add_space((x - ui.cursor().left()).max(0.0));
                            }
                            ui.label(RichText::new(text).font(small.clone()).color(color));
                        });
                    };
                    for line in lines {
                        let column = ui
                            .horizontal(|ui| {
                                let column =
                                    name_cell(ui, if line.up { "↑" } else { "↓" }, &line.name);
                                ui.label(
                                    RichText::new(format!(
                                        "{:<main_width$} {:>rate_width$}",
                                        line.main, line.rate
                                    ))
                                    .font(font.clone())
                                    .color(TEXT),
                                );
                                sparkline(ui, &line.spark, ACCENT);
                                column
                            })
                            .inner;
                        if !line.detail.is_empty() {
                            detail(
                                ui,
                                Some(column),
                                line.detail.clone(),
                                if line.alert { AMBER } else { MUTED },
                            );
                        }
                    }
                    ui.horizontal(|ui| {
                        name_cell(ui, "⇅", crate::i18n::label("Rede", "Net"));
                        ui.label(RichText::new(network).font(font.clone()).color(MUTED));
                    });
                })
                .response
                .rect
        })
        // The frame, not the area: an area keeps its larger size after shrinking.
        .inner
}

fn sparkline(ui: &mut eframe::egui::Ui, values: &[f32], color: eframe::egui::Color32) {
    use eframe::egui;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(48.0, 12.0), egui::Sense::hover());
    ui.painter().hline(
        rect.x_range(),
        rect.bottom(),
        egui::Stroke::new(1.0, crate::desktop_ui::BORDER),
    );
    if values.len() < 2 {
        return;
    }
    // Headroom keeps a steady rate mid-height instead of glued to the top.
    let top = values.iter().copied().fold(0.1_f32, f32::max) * 1.4;
    let step = rect.width() / (HISTORY - 1) as f32;
    let start = rect.right() - step * (values.len() - 1) as f32;
    let points: Vec<_> = values
        .iter()
        .enumerate()
        .map(|(i, v)| {
            egui::pos2(
                start + step * i as f32,
                rect.bottom() - rect.height() * v / top,
            )
        })
        .collect();
    ui.painter().add(egui::Shape::line(
        points,
        egui::Stroke::new(1.0, color.gamma_multiply(0.85)),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use livekit::webrtc::stats::{
        CandidatePairStats, CodecStats, InboundRtpStats, LocalCandidateStats, OutboundRtpStats,
        QualityLimitationReason, RemoteCandidateStats, TransportStats,
    };

    fn inbound(track: &str, bytes: u64, received: u64, lost: i64, decoded: u32) -> RtcStats {
        let mut r = InboundRtpStats::default();
        r.stream.kind = "video".into();
        r.stream.codec_id = "c1".into();
        r.inbound.track_identifier = track.into();
        r.inbound.bytes_received = bytes;
        r.received.packets_received = received;
        r.received.packets_lost = lost;
        r.received.jitter = 0.004;
        r.inbound.frames_decoded = decoded;
        r.inbound.total_decode_time = decoded as f64 * 0.002;
        r.inbound.jitter_buffer_emitted_count = decoded as u64;
        r.inbound.jitter_buffer_delay = decoded as f64 * 0.020;
        r.inbound.frame_width = 1920;
        r.inbound.frame_height = 1080;
        r.inbound.frames_per_second = 60.0;
        RtcStats::InboundRtp(r)
    }
    fn codec() -> RtcStats {
        let mut c = CodecStats::default();
        c.rtc.id = "c1".into();
        c.codec.mime_type = "video/H265".into();
        RtcStats::Codec(c)
    }
    fn path(relay: bool) -> Vec<RtcStats> {
        let mut t = TransportStats::default();
        t.transport.selected_candidate_pair_id = "p".into();
        let mut p = CandidatePairStats::default();
        p.rtc.id = "p".into();
        p.candidate_pair.local_candidate_id = "l".into();
        p.candidate_pair.remote_candidate_id = "r".into();
        p.candidate_pair.current_round_trip_time = 0.012;
        p.candidate_pair.available_outgoing_bitrate = 30e6;
        let mut l = LocalCandidateStats::default();
        l.rtc.id = "l".into();
        l.local_candidate.address = "192.0.2.10".into();
        l.local_candidate.protocol = "udp".into();
        l.local_candidate.candidate_type = Some(if relay {
            IceCandidateType::Relay
        } else {
            IceCandidateType::Host
        });
        l.local_candidate.relay_protocol = relay.then_some(IceServerTransportProtocol::Tls);
        let mut r = RemoteCandidateStats::default();
        r.rtc.id = "r".into();
        vec![
            RtcStats::Transport(t),
            RtcStats::CandidatePair(p),
            RtcStats::LocalCandidate(l),
            RtcStats::RemoteCandidate(r),
        ]
    }

    #[test]
    fn rates_come_from_deltas_and_unknown_tracks_are_skipped() {
        let tracks = vec![("rtc-a".to_owned(), "ana".to_owned())];
        let start = Instant::now();
        let mut sampler = Sampler::default();
        let first = sampler.sample(
            start,
            &[],
            &[
                codec(),
                inbound("rtc-a", 1_000_000, 1000, 0, 60),
                inbound("rtc-x", 9, 9, 0, 9),
            ],
            &tracks,
        );
        assert_eq!(first.inbound.len(), 1);
        let row = &first.inbound[0];
        assert_eq!((row.key.as_str(), row.codec), ("ana", "HEVC"));
        assert!(row.mbps.is_none() && row.loss_pct.is_none() && row.buffer_ms.is_none());
        assert!((row.jitter_ms - 4.0).abs() < 1e-9);

        let second = sampler.sample(
            start + Duration::from_secs(1),
            &[],
            &[codec(), inbound("rtc-a", 2_500_000, 1990, 10, 120)],
            &tracks,
        );
        let row = &second.inbound[0];
        assert!((row.mbps.unwrap() - 12.0).abs() < 1e-9);
        assert!((row.loss_pct.unwrap() - 1.0).abs() < 1e-9);
        assert!((row.buffer_ms.unwrap() - 20.0).abs() < 1e-9);
        assert!((row.decode_ms.unwrap() - 2.0).abs() < 1e-9);
    }

    #[test]
    fn a_replaced_receiver_restarts_its_rates_instead_of_spiking() {
        let tracks = vec![("rtc-a".to_owned(), "ana".to_owned())];
        let start = Instant::now();
        let mut sampler = Sampler::default();
        sampler.sample(
            start,
            &[],
            &[inbound("rtc-a", 9_000_000, 9000, 0, 900)],
            &tracks,
        );
        let reset = sampler.sample(
            start + Duration::from_secs(1),
            &[],
            &[inbound("rtc-a", 10, 1, 0, 1)],
            &tracks,
        );
        assert!(reset.inbound[0].mbps.is_none());
    }

    #[test]
    fn route_reports_path_and_transport_without_addresses() {
        let direct = route(&path(false)).unwrap();
        assert_eq!(
            (direct.path, direct.transport),
            (Path::Direct, Transport::Udp)
        );
        assert!((direct.rtt_ms.unwrap() - 12.0).abs() < 1e-9);
        let relay = route(&path(true)).unwrap();
        assert_eq!((relay.path, relay.transport), (Path::Relay, Transport::Tls));
        assert!(!format!("{relay:?}{}", relay.label()).contains("192.0.2"));
        assert!(route(&[]).is_none());
    }

    #[test]
    fn outbound_uses_the_largest_video_and_its_limitation() {
        let mut small = OutboundRtpStats::default();
        small.stream.kind = "video".into();
        small.outbound.frame_width = 640;
        small.outbound.frame_height = 360;
        small.sent.bytes_sent = 1;
        let mut screen = small.clone();
        screen.outbound.frame_width = 2560;
        screen.outbound.frame_height = 1440;
        screen.outbound.frames_per_second = 60.0;
        screen.outbound.quality_limitation_reason = QualityLimitationReason::Bandwidth;
        screen.sent.bytes_sent = 1_000_000;
        let start = Instant::now();
        let mut sampler = Sampler::default();
        let stats = |bytes: u64| {
            let mut screen = screen.clone();
            screen.sent.bytes_sent = bytes;
            vec![
                RtcStats::OutboundRtp(small.clone()),
                RtcStats::OutboundRtp(screen),
            ]
        };
        sampler.sample(start, &stats(1_000_000), &[], &[]);
        let out = sampler
            .sample(start + Duration::from_secs(2), &stats(6_000_000), &[], &[])
            .outbound
            .unwrap();
        assert_eq!((out.width, out.height), (2560, 1440));
        assert_eq!(out.limit, Some(Limit::Bandwidth));
        assert!((out.mbps.unwrap() - 20.0).abs() < 1e-9);
    }

    #[test]
    fn panel_lines_show_essentials_and_flag_loss() {
        let tracks = vec![("rtc-a".to_owned(), "ana/camera".to_owned())];
        let start = Instant::now();
        let mut sampler = Sampler::default();
        sampler.sample(
            start,
            &[],
            &[codec(), inbound("rtc-a", 0, 0, 0, 0)],
            &tracks,
        );
        let mut stats = path(false);
        stats.extend([codec(), inbound("rtc-a", 1_500_000, 950, 50, 60)]);
        let snapshot = sampler.sample(start + Duration::from_secs(1), &[], &stats, &tracks);
        let quality = crate::video_metrics::QualitySnapshot {
            resolution: Some((2560, 1440)),
            send: Some(crate::video_metrics::Throughput {
                fps: 59.6,
                mbps: 18.4,
            }),
            codec: Some("HEVC"),
            encoder_backend: Some("NVENC"),
            ..Default::default()
        };
        let lines = lines(
            Some(&snapshot),
            Some(Sending {
                quality: &quality,
                outbound: None,
            }),
            &History::default(),
            |key| key.replace("/camera", " cam"),
        );
        assert_eq!(lines.len(), 2);
        assert!(lines[0].up);
        assert_eq!(lines[0].main, "2560×1440 60 fps HEVC NVENC");
        assert_eq!(lines[0].rate, "18 Mb/s");
        assert!(!lines[0].alert);
        assert_eq!(lines[1].name, "ana cam");
        assert_eq!(lines[1].main, "1920×1080 60 fps HEVC");
        assert_eq!(lines[1].rate, "12 Mb/s");
        assert!(lines[1].alert, "5% loss is highlighted");
        let net = network(Some(&snapshot), None, false);
        assert!(net.starts_with("RTT 12 ms"), "{net}");
        assert!(net.ends_with("UDP"), "{net}");
        assert!(network(None, None, false).contains('—'));
    }

    #[test]
    fn history_is_bounded_and_drops_rows_that_left() {
        let mut history = History::default();
        let start = Instant::now();
        for i in 0..(HISTORY as u64 + 5) {
            let snapshot = Snapshot {
                inbound: vec![Inbound {
                    key: "ana".into(),
                    width: 1,
                    height: 1,
                    fps: 1.0,
                    mbps: Some(i as f64),
                    loss_pct: None,
                    jitter_ms: 0.0,
                    buffer_ms: None,
                    decode_ms: None,
                    freezes: 0,
                    codec: "",
                    hardware: false,
                }],
                ..Default::default()
            };
            let at = start + Duration::from_secs(i);
            history.push(at, &snapshot, Some(1.0));
            history.push(at, &snapshot, Some(1.0));
        }
        let ana = history.values("ana");
        assert_eq!(ana.len(), HISTORY);
        assert_eq!(*ana.last().unwrap(), (HISTORY + 4) as f32);
        history.push(start + Duration::from_secs(999), &Snapshot::default(), None);
        assert!(history.values("ana").is_empty());
        assert!(history.values(History::SEND).is_empty());
    }
}
