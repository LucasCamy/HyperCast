//! Shared bound for capture/decoder allocations; native encoders also check device caps.
pub const MAX_PIXELS: u64=33_554_432;
pub fn valid(w:u32,h:u32)->bool {w>=2 && h>=2 && w<=8192 && h<=8192 && u64::from(w)*u64::from(h)<=MAX_PIXELS}
pub fn fit(w:u32,h:u32,max_w:u32,max_h:u32)->Option<(u32,u32)> {
    if !valid(w,h) || max_w<2 || max_h<2 {return None;}
    let max_w=max_w.min(8192)&!1;let max_h=max_h.min(8192)&!1;
    let scale=1.0_f64.min(f64::from(max_w)/f64::from(w)).min(f64::from(max_h)/f64::from(h));
    let mut out_w=((f64::from(w)*scale/2.0).round() as u32*2).clamp(2,max_w);
    let mut out_h=((f64::from(h)*scale/2.0).round() as u32*2).clamp(2,max_h);
    while u64::from(out_w)*u64::from(out_h)>MAX_PIXELS {if out_w>=out_h {out_w-=2;}else {out_h-=2;}}
    Some((out_w,out_h))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn odd_ultrawide_and_device_fallback() {
        assert_eq!(fit(4587,1920,8192,8192),Some((4588,1920)));
        assert_eq!(fit(4587,1920,3840,2160),Some((3840,1608)));
        assert_eq!(fit(4587,1920,3440,1440),Some((3440,1440)));
        assert_eq!(fit(2560,1440,8192,8192),Some((2560,1440)));
        assert!(fit(8192,8192,8192,8192).is_none());
        assert!(fit(0,1920,8192,8192).is_none());
        for (w,h) in [(8191,4096),(1920,4587),(7680,4320),(3,3)] {
            let (a,b)=fit(w,h,8192,8192).unwrap();assert!(valid(a,b));assert_eq!(a%2+b%2,0);
        }
    }
}
