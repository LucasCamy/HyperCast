//! Non-secret display name, shared by the Windows and Linux interfaces.
use std::{io::{Read, Write}, path::PathBuf};
fn path() -> Option<PathBuf> {
    #[cfg(windows)]
    let root = std::env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join("HyperCast"));
    #[cfg(not(windows))]
    let root = std::env::var_os("XDG_STATE_HOME").map(PathBuf::from).filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".local/state")))
        .map(|p| p.join("hypercast"));
    root.filter(|p| p.is_absolute()).map(|p| p.join("display-name"))
}
fn valid(name: &str) -> bool {
    !name.is_empty() && name.len() <= 256 && name.chars().count() <= 64
        && !name.chars().any(char::is_control)
}
pub fn load_name() -> String {
    if let Ok(name) = std::env::var("HYPERCAST_NAME") { return name; }
    path().and_then(|p| load(&p).ok()).unwrap_or_default()
}
fn load(path: &std::path::Path) -> std::io::Result<String> {
    let mut name = String::new();
    std::fs::File::open(path)?.take(257).read_to_string(&mut name)?;
    if !valid(&name) { return Err(std::io::Error::from(std::io::ErrorKind::InvalidData)); }
    Ok(name)
}
pub fn save_name(name: &str) -> std::io::Result<()> {
    if std::env::var_os("HYPERCAST_NAME").is_some() { return Ok(()); }
    let name = name.trim();
    if !valid(name) { return Ok(()); }
    let Some(path) = path() else { return Ok(()); };
    save(&path, name)
}
fn save(path: &std::path::Path, name: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(path.parent().unwrap())?;
    // A unique file prevents partial reads and collisions between running clients.
    let temp = path.with_extension(uuid::Uuid::new_v4().simple().to_string());
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)] { use std::os::unix::fs::OpenOptionsExt; options.mode(0o600); }
    let result = (|| {
        let mut file = options.open(&temp)?;
        file.write_all(name.as_bytes())?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temp, path)
    })();
    if result.is_err() { let _ = std::fs::remove_file(temp); }
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn name_round_trip_and_corrupt_cache() {
        let dir = std::env::temp_dir().join(format!("hypercast-name-{}", uuid::Uuid::new_v4()));
        let path = dir.join("display-name");
        save(&path, "Lucas").unwrap();
        assert_eq!(load(&path).unwrap(), "Lucas");
        save(&path, "Amigo").unwrap();
        assert_eq!(load(&path).unwrap(), "Amigo");
        std::fs::write(&path, "x".repeat(257)).unwrap();
        assert!(load(&path).is_err());
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }
    #[test]
    fn bounded_name() {
        assert!(!valid("")); assert!(!valid("Lucas\n"));
        assert!(!valid(&"a".repeat(65))); assert!(valid("Lucas"));
    }
}
