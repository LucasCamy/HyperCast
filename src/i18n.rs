//! Installer-selected UI language. Machine identifiers and user content are never translated.
use std::{io::Read,path::PathBuf,sync::OnceLock};
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum Language {Portuguese,English}
impl Language {
    fn parse(code:&str)->Option<Self>{match code.trim(){"pt-BR"|"pt"|"brazilianportuguese"=>Some(Self::Portuguese),"en-US"|"en"|"english"=>Some(Self::English),_=>None}}
}
fn path()->Option<PathBuf>{
    #[cfg(windows)]
    let root=std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    #[cfg(not(windows))]
    let root=std::env::var_os("XDG_STATE_HOME").map(PathBuf::from).filter(|p|p.is_absolute()).or_else(||std::env::var_os("HOME").map(|p|PathBuf::from(p).join(".local/state")));
    root.filter(|p|p.is_absolute()).map(|p|p.join(if cfg!(windows){"HyperCast"}else{"hypercast"}).join("language"))
}
fn read_language(path:&std::path::Path)->Option<Language>{let mut value=String::new();std::fs::File::open(path).ok()?.take(33).read_to_string(&mut value).ok()?;if value.len()>32{return None;}Language::parse(&value)}
pub fn language()->Language {
    static LANGUAGE:OnceLock<Language>=OnceLock::new();
    *LANGUAGE.get_or_init(||std::env::var("HYPERCAST_LANGUAGE").ok().and_then(|v|Language::parse(&v)).or_else(||path().and_then(|p|read_language(&p))).unwrap_or(Language::Portuguese))
}
pub fn label(pt:&'static str,en:&'static str)->&'static str{if language()==Language::English{en}else{pt}}
#[macro_export]
macro_rules! i18n_format {($pt:literal,$en:literal $(,$($args:tt)*)?)=>{{
    if $crate::i18n::language()==$crate::i18n::Language::English{format!($en $(,$($args)*)?)}else{format!($pt $(,$($args)*)?)}
}};}
#[cfg(test)]mod tests {
    use super::*;
    #[test]fn supported_installer_codes_and_corrupt_preferences(){
        assert_eq!(Language::parse("english"),Some(Language::English));assert_eq!(Language::parse("pt-BR\n"),Some(Language::Portuguese));assert!(Language::parse("../en-US").is_none());
        let file=std::env::temp_dir().join(format!("hypercast-language-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::write(&file,"en-US").unwrap();assert_eq!(read_language(&file),Some(Language::English));std::fs::write(&file,"x".repeat(100)).unwrap();assert_eq!(read_language(&file),None);std::fs::remove_file(file).unwrap();
    }
}
