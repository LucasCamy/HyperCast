//! Bounded, content-free video measurements. Window-local timings, cumulative RTP counters.
use std::time::Duration;
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
    #[test]fn bounded_windows_reset_and_missing_qp_is_not_zero(){
        let mut window=EncodeWindow::default();
        for _ in 0..1500{window.record(Duration::from_micros(1000),20,false,false,None);}
        assert_eq!(window.durations.len(),1200);
        let text=window.summary_and_reset();assert!(text.contains("frames=1500 bytes=30000"));assert!(text.contains("qp_avg=unavailable"));
        assert!(window.summary_and_reset().contains("frames=0 bytes=0"));
    }
}
