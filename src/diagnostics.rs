//! Bounded, opt-in-to-export diagnostics. Never record tokens, screen content or participant names.
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::{Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};

const LIMIT: u64 = 1024 * 1024;
struct Log {
    path: PathBuf,
    writer: Mutex<(File, u64)>,
}
static LOG: OnceLock<Log> = OnceLock::new();

pub fn init() -> std::io::Result<()> {
    let root = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .ok_or_else(|| std::io::Error::other("LOCALAPPDATA unavailable"))?
        .join("HyperCast/diagnostics");
    fs::create_dir_all(&root)?;
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    let path = root.join(format!("gui-{stamp}-{}.log", std::process::id()));
    let file = OpenOptions::new().write(true).create_new(true).open(&path)?;
    let _ = LOG.set(Log { path, writer: Mutex::new((file, 0)) });
    // Keep only our generated flat GUI logs; never recurse into script diagnostics.
    let mut old: Vec<_> = fs::read_dir(&root)?.filter_map(Result::ok).filter(|entry| {
        let name = entry.file_name().to_string_lossy().into_owned();
        name.strip_prefix("gui-").and_then(|n| n.strip_suffix(".log"))
            .is_some_and(|n| n.split_once('-').is_some_and(|(stamp, pid)| {
                !stamp.is_empty() && !pid.is_empty() && stamp.bytes().all(|b| b.is_ascii_digit())
                    && pid.bytes().all(|b| b.is_ascii_digit())
            })) && entry.file_type().is_ok_and(|t| t.is_file())
    }).collect();
    old.sort_by_key(|entry| entry.file_name());
    let excess = old.len().saturating_sub(16);
    for entry in old.into_iter().take(excess) { let _ = fs::remove_file(entry.path()); }
    write("application", concat!("HyperCast ", env!("CARGO_PKG_VERSION")), "");
    Ok(())
}

fn redact(message: &str, secret: &str) -> String {
    let message = if secret.is_empty() { message.to_owned() } else { message.replace(secret, "[redacted]") };
    // SDK errors can contain a bearer JWT even when no current token is available.
    let mut clean = String::new();
    for (i, word) in message.split_whitespace().enumerate() {
        if i > 0 { clean.push(' '); }
        if word.contains("eyJ") && word.matches('.').count() >= 2 {
            clean.push_str("[redacted]");
        } else {
            clean.extend(word.chars().filter(|c| !c.is_control()).take(1024));
        }
        if clean.len() >= 4096 { break; }
    }
    clean
}

pub fn write(event: &str, message: &str, secret: &str) {
    let Some(log) = LOG.get() else { return; };
    let Ok(mut writer) = log.writer.lock() else { return; };
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis();
    let line = format!("{stamp} {event}: {}\n", redact(message, secret));
    if writer.1 + line.len() as u64 > LIMIT { return; }
    if writer.0.write_all(line.as_bytes()).is_ok() {
        writer.1 += line.len() as u64;
        let _ = writer.0.flush();
    }
}

pub fn path() -> Option<PathBuf> { LOG.get().map(|log| log.path.clone()) }

pub fn export_text() -> std::io::Result<String> {
    let path = path().ok_or_else(|| std::io::Error::other("Diagnostic file unavailable"))?;
    use std::io::Read;
    let mut text = String::new();
    File::open(path)?.take(LIMIT).read_to_string(&mut text)?;
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn removes_known_tokens_and_untrusted_line_breaks() {
        assert_eq!(redact("error\nsecret-token\r\nend", "secret-token"), "error [redacted] end");
        assert_eq!(redact("Bearer eyJabc.def.ghi expired", ""), "Bearer [redacted] expired");
    }
}
