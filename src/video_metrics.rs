//! Bounded, content-free video measurements. Window-local timings, cumulative RTP counters.
use std::time::Duration;
#[derive(Clone,Copy,Debug,Default)]
pub struct Throughput {pub fps:f64,pub mbps:f64}
#[derive(Clone,Debug,Default)]
pub struct QualitySnapshot {
    pub network:Option<VideoNetwork>,pub encoder_backend:Option<&'static str>,pub codec:Option<&'static str>,pub applied_encoder:Option<(u32,u32)>,
    pub send:Option<Throughput>,pub receive:Option<Throughput>,
    pub resolution:Option<(u32,u32)>,pub send_paused:bool,
}
/// Samples real counter deltas, never the configured target FPS or bitrate.
#[derive(Default)]
pub struct TrafficMeter {previous:Option<(std::time::Instant,u64,u64)>}
impl TrafficMeter {
    pub fn sample(&mut self,now:std::time::Instant,frames:u64,bytes:u64)->Option<Throughput>{
        let Some((at,old_frames,old_bytes))=self.previous else{self.previous=Some((now,frames,bytes));return None;};
        if frames<old_frames || bytes<old_bytes{self.previous=Some((now,frames,bytes));return None;}
        let elapsed=now.saturating_duration_since(at).as_secs_f64();
        if elapsed<1.0{return None;}
        self.previous=Some((now,frames,bytes));
        Some(Throughput{fps:(frames-old_frames) as f64/elapsed,mbps:(bytes-old_bytes) as f64*8.0/elapsed/1_000_000.0})
    }
}
#[derive(Default)]
pub struct EncodeWindow {
    durations:Vec<u64>,frames:u64,bytes:u64,keys:u64,requested_keys:u64,max_bytes:usize,qp_sum:u64,qp_samples:u64,
}
impl EncodeWindow {
    pub fn record(&mut self,elapsed:Duration,bytes:usize,key:bool,requested_key:bool,qp:Option<u32>) {
        if self.durations.len()<1200 {self.durations.push(elapsed.as_micros().min(u64::MAX as u128) as u64);}
        self.frames+=1;self.bytes+=bytes as u64;self.keys+=u64::from(key);self.requested_keys+=u64::from(requested_key);self.max_bytes=self.max_bytes.max(bytes);
        if let Some(qp)=qp{self.qp_sum+=qp as u64;self.qp_samples+=1;}
    }
    pub fn summary_and_reset(&mut self)->String {
        self.durations.sort_unstable();
        let percentile=|p:usize|self.durations.get(self.durations.len().saturating_sub(1)*p/100).copied().unwrap_or(0) as f64/1000.0;
        let text=format!("frames={} bytes={} keys={} requested_keys={} max_frame_bytes={} encode_p50_ms={:.3} encode_p95_ms={:.3} qp_avg={} qp_samples={}",
            self.frames,self.bytes,self.keys,self.requested_keys,self.max_bytes,percentile(50),percentile(95),
            if self.qp_samples>0{format!("{:.2}",self.qp_sum as f64/self.qp_samples as f64)}else{"unavailable".into()},self.qp_samples);
        *self=Self::default();text
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]fn throughput_uses_elapsed_time_and_handles_stop_and_counter_reset(){
        let mut meter=TrafficMeter::default();let now=std::time::Instant::now();
        assert!(meter.sample(now,20,1000).is_none());
        assert!(meter.sample(now+Duration::from_millis(100),30,2000).is_none());
        let rate=meter.sample(now+Duration::from_secs(2),140,1_001_000).unwrap();
        assert_eq!(rate.fps,60.0);assert_eq!(rate.mbps,4.0);
        let idle=meter.sample(now+Duration::from_secs(3),140,1_001_000).unwrap();assert_eq!(idle.fps,0.0);
        assert!(meter.sample(now+Duration::from_secs(4),1,100).is_none());
        assert_eq!(meter.sample(now+Duration::from_secs(5),61,500100).unwrap().fps,60.0);
    }
    #[test]fn bounded_windows_reset_and_missing_qp_is_not_zero(){
        let mut window=EncodeWindow::default();
        for _ in 0..1500{window.record(Duration::from_micros(1000),20,false,false,None);}
        assert_eq!(window.durations.len(),1200);
        let text=window.summary_and_reset();assert!(text.contains("frames=1500 bytes=30000"));assert!(text.contains("qp_avg=unavailable"));
        assert!(window.summary_and_reset().contains("frames=0 bytes=0"));
    }
}

/// Aggregated cumulative counters. No ICE address, participant or track identifiers.
#[derive(Clone,Copy,Debug,Default)]
pub struct VideoNetwork {pub rtt_ms:Option<f64>,pub streams:u32,pub bytes:u64,pub packets:u64,pub lost:i64,pub decoded:u64,pub dropped:u64,pub decode_ms:f64}
#[cfg(feature="desktop")]
pub struct ReportState {pub connected:bool,pub reconnecting:bool,pub publishing:bool,pub sent:u64,pub received:u64,pub restarts:u64}
#[cfg(feature="desktop")]
pub fn technical_report(q:&QualitySnapshot,state:ReportState,settings:crate::resolution::StreamSettings,upload:crate::bandwidth::UploadProfile)->String{
    fn number(v:f64)->Option<f64>{v.is_finite().then_some(v).filter(|v|*v>=0.0)}
    let network=q.network.map(|n|serde_json::json!({"sample_interval_seconds":5,"rtt_ms":n.rtt_ms.and_then(number),"video_rtp":if n.streams==0{serde_json::Value::Null}else{serde_json::json!({"streams":n.streams,"bytes":n.bytes,"packets_received":n.packets,"packets_lost":n.lost,"frames_decoded":n.decoded,"frames_dropped":n.dropped,"average_decode_ms":if n.decoded==0{None}else{number(n.decode_ms/n.decoded as f64)}})}}));
    serde_json::to_string_pretty(&serde_json::json!({
        "schema":1,"version":env!("CARGO_PKG_VERSION"),"os":std::env::consts::OS,
        "session":{"connected":state.connected,"reconnecting":state.reconnecting,"publishing":state.publishing,"receive_restarts":state.restarts},
        "requested":{"resolution":match settings.size{crate::resolution::OutputSize::Native=>"native",crate::resolution::OutputSize::FullHd=>"1080p",crate::resolution::OutputSize::QuadHd=>"1440p"},"fps":settings.fps,"video_bitrate_ceiling_bps":upload.bps()},
        "video":{"encoder":q.encoder_backend.filter(|v|matches!(*v,"NVENC"|"AMF"|"nvh265enc"|"nvav1enc"|"x265enc"|"vaav1enc"|"vah265enc")),"codec":q.codec.filter(|v|matches!(*v,"HEVC"|"AV1")),"accepted_encoder_config":q.applied_encoder.map(|(bps,fps)|serde_json::json!({"bitrate_bps":bps,"fps":fps})),"encoded_resolution":q.resolution,"send_paused":q.send_paused,"sent_frames":state.sent,"received_sink_frames":state.received,"send_fps":q.send.and_then(|v|number(v.fps)),"send_encoded_mbps":q.send.and_then(|v|number(v.mbps)),"receive_sink_fps":q.receive.and_then(|v|number(v.fps))},
        "network":network,"end_to_end_latency_ms":serde_json::Value::Null,
        "notes":["RTT is not end-to-end latency","Accepted encoder settings are not measured throughput","RTP counters and average decode time are cumulative across current receivers","Missing or unsupported measurements are null","No tokens, addresses, room names or participant names are included"]
    })).unwrap_or_else(|_|"{}".into())
}
#[cfg(all(test,feature="desktop"))]mod report_tests {
    use super::*;
    #[test]fn export_allowlist_excludes_untrusted_strings_and_invalid_numbers(){
        let q=QualitySnapshot{encoder_backend:Some("Bearer PRIVATE-SECRET"),codec:Some("room-private"),send:Some(Throughput{fps:f64::NAN,mbps:f64::INFINITY}),network:Some(VideoNetwork{rtt_ms:Some(-1.0),..Default::default()}),..Default::default()};
        let text=technical_report(&q,ReportState{connected:true,reconnecting:false,publishing:false,sent:0,received:0,restarts:2},Default::default(),Default::default());
        assert!(!text.contains("PRIVATE-SECRET"));assert!(!text.contains("room-private"));assert!(text.len()<4096);
        let v:serde_json::Value=serde_json::from_str(&text).unwrap();assert_eq!(v["schema"],1);assert!(v["video"]["send_fps"].is_null());assert!(v["network"]["rtt_ms"].is_null());assert!(v["network"]["video_rtp"].is_null());assert!(v["end_to_end_latency_ms"].is_null());
    }
}
