//! Video budgets shared by encoder and publication; audio/transport overhead is separate.
#[derive(Clone,Copy,Debug,Default,PartialEq,Eq)]
pub enum UploadProfile {Economic,#[default] Balanced,Maximum}
impl UploadProfile {
    pub const ALL:[Self;3]=[Self::Economic,Self::Balanced,Self::Maximum];
    pub fn bps(self)->u32 {match self{Self::Economic=>12_000_000,Self::Balanced=>25_000_000,Self::Maximum=>40_000_000}}
    pub fn from_mbps(value:u32)->Option<Self>{Self::ALL.into_iter().find(|p|p.bps()/1_000_000==value)}
    pub fn label(self)->&'static str {match self{Self::Economic=>"Econômico · 12 Mbps",Self::Balanced=>"Balanceado · 25 Mbps",Self::Maximum=>"Máxima qualidade · 40 Mbps"}}
    pub fn initial()->Self {
        // Bounded smoke-test override; ordinary sessions always start at 25 Mbps.
        if std::env::var("HYPERCAST_SMOKE_SECONDS").ok().and_then(|v|v.parse::<u32>().ok()).is_some_and(|n|(1..=300).contains(&n)){
            match std::env::var("HYPERCAST_SMOKE_UPLOAD_MBPS").as_deref(){Ok("12")=>return Self::Economic,Ok("40")=>return Self::Maximum,_=>{}}
        }
        Self::default()
    }
    /// For an active encoder only. Zero from WebRTC must be handled as a pause first.
    pub fn active_rate(self,requested:u64)->u32 {requested.clamp(64_000,self.bps() as u64) as u32}
}
#[cfg(test)]mod tests {
    use super::*;
    #[test]fn profiles_cap_network_feedback_and_do_not_raise_supported_lower_budgets(){
        assert_eq!(UploadProfile::default().bps(),25_000_000);
        assert_eq!(UploadProfile::from_mbps(25),Some(UploadProfile::Balanced));
        assert_eq!(UploadProfile::from_mbps(100),None);
        for profile in UploadProfile::ALL{
            assert_eq!(profile.active_rate(u64::MAX),profile.bps());
            assert_eq!(profile.active_rate(500_000),500_000);
            assert_eq!(profile.active_rate(1),64_000);
        }
    }
}
