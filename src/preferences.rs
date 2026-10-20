//! Non-secret display name, shared by the Windows and Linux interfaces.
use std::{
    io::{Read, Write},
    path::PathBuf,
};
fn path() -> Option<PathBuf> {
    #[cfg(windows)]
    let root = std::env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join("HyperCast"));
    #[cfg(not(windows))]
    let root = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".local/state")))
        .map(|p| p.join("hypercast"));
    root.filter(|p| p.is_absolute())
        .map(|p| p.join("display-name"))
}
fn valid(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 256
        && name.chars().count() <= 64
        && !name.chars().any(char::is_control)
}
pub fn load_name() -> String {
    if let Ok(name) = std::env::var("HYPERCAST_NAME") {
        return name;
    }
    path().and_then(|p| load(&p).ok()).unwrap_or_default()
}
fn load(path: &std::path::Path) -> std::io::Result<String> {
    let mut name = String::new();
    std::fs::File::open(path)?
        .take(257)
        .read_to_string(&mut name)?;
    if !valid(&name) {
        return Err(std::io::Error::from(std::io::ErrorKind::InvalidData));
    }
    Ok(name)
}
pub fn save_name(name: &str) -> std::io::Result<()> {
    if std::env::var_os("HYPERCAST_NAME").is_some() {
        return Ok(());
    }
    let name = name.trim();
    if !valid(name) {
        return Ok(());
    }
    let Some(path) = path() else {
        return Ok(());
    };
    save(&path, name)
}
fn save(path: &std::path::Path, name: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(path.parent().unwrap())?;
    // A unique file prevents partial reads and collisions between running clients.
    let temp = path.with_extension(uuid::Uuid::new_v4().simple().to_string());
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = (|| {
        let mut file = options.open(&temp)?;
        file.write_all(name.as_bytes())?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temp);
    }
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
        assert!(!valid(""));
        assert!(!valid("Lucas\n"));
        assert!(!valid(&"a".repeat(65)));
        assert!(valid("Lucas"));
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VideoCodec {
    #[default]
    Hevc,
    Av1,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AmdPreset {
    #[default]
    Fast,
    Text,
    Compatibility,
}

/// Non-secret, versioned UI choices. Never includes addresses, room IDs or credentials.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    pub codec: VideoCodec,
    pub amd_profile: AmdPreset,
    pub stream: crate::resolution::StreamSettings,
    pub upload: crate::bandwidth::UploadProfile,
    pub rate_mode: crate::adaptive_upload::Mode,
    pub volume: u32,
    pub muted: bool,
    pub sidebar: bool,
    pub self_preview: bool,
    pub grid: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            codec: Default::default(),
            amd_profile: Default::default(),
            stream: Default::default(),
            upload: Default::default(),
            rate_mode: Default::default(),
            volume: 80,
            muted: false,
            sidebar: true,
            self_preview: true,
            grid: false,
        }
    }
}
impl Settings {
    fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() > 4096 {
            return None;
        }
        let v: serde_json::Value = serde_json::from_slice(bytes).ok()?;
        if v["schema"].as_u64()? != 1 {
            return None;
        }
        let size = match v["resolution"].as_str()? {
            "native" => crate::resolution::OutputSize::Native,
            "1080p" => crate::resolution::OutputSize::FullHd,
            "1440p" => crate::resolution::OutputSize::QuadHd,
            _ => return None,
        };
        let fps = u32::try_from(v["fps"].as_u64()?).ok()?;
        let stream = crate::resolution::StreamSettings { size, fps };
        if !stream.valid() {
            return None;
        }
        let upload = crate::bandwidth::UploadProfile::from_mbps(
            u32::try_from(v["upload_mbps"].as_u64()?).ok()?,
        )?;
        let volume = u32::try_from(v["volume"].as_u64()?).ok()?;
        if volume > 100 {
            return None;
        }
        let rate_mode = match v.get("rate_mode") {
            None => crate::adaptive_upload::Mode::Manual,
            Some(value) => match value.as_str()? {
                "manual" => crate::adaptive_upload::Mode::Manual,
                "automatic" => crate::adaptive_upload::Mode::Automatic,
                _ => return None,
            },
        };
        let codec = match v.get("codec") {
            None => VideoCodec::Hevc,
            Some(v) => match v.as_str()? {
                "hevc" => VideoCodec::Hevc,
                "av1" => VideoCodec::Av1,
                _ => return None,
            },
        };
        let amd_profile = match v.get("amd_profile") {
            None => AmdPreset::Fast,
            Some(v) => match v.as_str()? {
                "fast" => AmdPreset::Fast,
                "text" => AmdPreset::Text,
                "compatibility" => AmdPreset::Compatibility,
                _ => return None,
            },
        };
        Some(Self {
            codec,
            amd_profile,
            stream,
            upload,
            rate_mode,
            volume,
            muted: v["muted"].as_bool()?,
            sidebar: v["sidebar"].as_bool()?,
            self_preview: v["self_preview"].as_bool()?,
            grid: v["grid"].as_bool()?,
        })
    }
    fn encode(self) -> String {
        serde_json::json!({"schema":1,"codec":match self.codec{VideoCodec::Hevc=>"hevc",VideoCodec::Av1=>"av1"},"amd_profile":match self.amd_profile{AmdPreset::Fast=>"fast",AmdPreset::Text=>"text",AmdPreset::Compatibility=>"compatibility"},"resolution":match self.stream.size{crate::resolution::OutputSize::Native=>"native",crate::resolution::OutputSize::FullHd=>"1080p",crate::resolution::OutputSize::QuadHd=>"1440p"},"fps":self.stream.fps,"upload_mbps":self.upload.bps()/1_000_000,"rate_mode":match self.rate_mode{crate::adaptive_upload::Mode::Manual=>"manual",crate::adaptive_upload::Mode::Automatic=>"automatic"},"volume":self.volume,"muted":self.muted,"sidebar":self.sidebar,"self_preview":self.self_preview,"grid":self.grid}).to_string()
    }
    fn file() -> Option<PathBuf> {
        path().map(|p| p.with_file_name("preferences.json"))
    }
    fn ephemeral() -> bool {
        cfg!(test) || std::env::var_os("HYPERCAST_SMOKE_SECONDS").is_some()
    }
    pub fn load() -> Self {
        if Self::ephemeral() {
            return Self::default();
        }
        Self::file()
            .and_then(|p| {
                let mut b = Vec::new();
                std::fs::File::open(p)
                    .ok()?
                    .take(4097)
                    .read_to_end(&mut b)
                    .ok()?;
                Self::decode(&b)
            })
            .unwrap_or_default()
    }
}
// Serialize read/check/replace across cooperating processes. The stable lock file
// must not be removed: replacing it would allow locking different inodes.
fn lock_settings(path: &std::path::Path) -> std::io::Result<std::fs::File> {
    std::fs::create_dir_all(path.parent().ok_or(std::io::ErrorKind::InvalidInput)?)?;
    let mut options = std::fs::OpenOptions::new();
    options.read(true).write(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let lock = options.open(path.with_extension("lock"))?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
    loop {
        match lock.try_lock() {
            Ok(()) => return Ok(lock),
            Err(std::fs::TryLockError::WouldBlock) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(10))
            }
            Err(std::fs::TryLockError::WouldBlock) => {
                return Err(std::io::ErrorKind::WouldBlock.into());
            }
            Err(std::fs::TryLockError::Error(error)) => return Err(error),
        }
    }
}
fn read_settings(path: &std::path::Path) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(4097)
        .read_to_end(&mut bytes)?;
    Ok(bytes)
}
fn protected(bytes: &[u8]) -> bool {
    // Oversize files cannot be inspected safely; preserve them too.
    bytes.len() > 4096
        || serde_json::from_slice::<serde_json::Value>(bytes)
            .ok()
            .and_then(|v| v.get("schema").cloned())
            .is_some_and(|v| v.as_u64() != Some(1))
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
enum SaveState {
    Idle = 0,
    Saved = 1,
    Protected = 2,
    Failed = 3,
    Pending = 4,
}
fn write_settings(path: &std::path::Path, value: Settings) -> std::io::Result<SaveState> {
    let _lock = lock_settings(path)?;
    let existing = match read_settings(path) {
        Ok(bytes) => Some(bytes),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e),
    };
    if existing.as_deref().is_some_and(protected) {
        return Ok(SaveState::Protected);
    }
    // Keep additive, unknown schema-1 keys when updating known choices.
    let mut document = existing
        .as_deref()
        .and_then(|b| serde_json::from_slice::<serde_json::Value>(b).ok())
        .filter(|v| v.is_object())
        .unwrap_or_else(|| serde_json::json!({}));
    let known: serde_json::Value =
        serde_json::from_str(&value.encode()).expect("Settings encodes an object");
    document
        .as_object_mut()
        .unwrap()
        .extend(known.as_object().unwrap().clone());
    let encoded = document.to_string();
    if encoded.len() > 4096 {
        return Ok(SaveState::Protected);
    }
    save(path, &encoded)?;
    Ok(SaveState::Saved)
}
/// Disk writes run outside painting and are coalesced while a slider is moving.
/// Whole settings snapshots use last complete writer wins, not per-field merging.
pub struct Store {
    latest: Settings,
    queued: Settings,
    tx: Option<std::sync::mpsc::SyncSender<Settings>>,
    worker: Option<std::thread::JoinHandle<()>>,
    state: std::sync::Arc<std::sync::atomic::AtomicU8>,
}
impl Store {
    fn empty(initial: Settings) -> Self {
        Self {
            latest: initial,
            queued: initial,
            tx: None,
            worker: None,
            state: std::sync::Arc::new(std::sync::atomic::AtomicU8::new(SaveState::Idle as u8)),
        }
    }
    pub fn new(initial: Settings) -> Self {
        if Settings::ephemeral() {
            return Self::empty(initial);
        }
        let Some(file) = Settings::file() else {
            return Self::empty(initial);
        };
        Self::at_file(initial, file)
    }
    fn at_file(initial: Settings, file: PathBuf) -> Self {
        use std::sync::atomic::Ordering;
        let mut store = Self::empty(initial);
        if read_settings(&file).ok().as_deref().is_some_and(protected) {
            store
                .state
                .store(SaveState::Protected as u8, Ordering::Relaxed);
        }
        let state = store.state.clone();
        let (tx, rx) = std::sync::mpsc::sync_channel::<Settings>(1);
        if let Ok(worker) = std::thread::Builder::new()
            .name("preferences".into())
            .spawn(move || {
                let persist = |value| {
                    state.store(
                        write_settings(&file, value).unwrap_or(SaveState::Failed) as u8,
                        Ordering::Relaxed,
                    )
                };
                while let Ok(mut value) = rx.recv() {
                    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
                    loop {
                        match rx.recv_timeout(std::time::Duration::from_millis(350)) {
                            Ok(next) => {
                                value = next;
                                if std::time::Instant::now() >= deadline {
                                    break;
                                }
                            }
                            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => break,
                            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                                persist(value);
                                return;
                            }
                        }
                    }
                    persist(value);
                }
            })
        {
            store.tx = Some(tx);
            store.worker = Some(worker);
        } else {
            store
                .state
                .store(SaveState::Failed as u8, Ordering::Relaxed);
        }
        store
    }
    pub fn current(&self) -> Settings {
        self.latest
    }
    pub fn notice(&self) -> Option<&'static str> {
        use crate::i18n::label;
        match self.state.load(std::sync::atomic::Ordering::Relaxed) {
            1 => Some(label("Preferências salvas.", "Preferences saved.")),
            2 => Some(label(
                "Arquivo de preferências incompatível preservado. As alterações valem apenas nesta sessão.",
                "Incompatible preferences file preserved. Changes apply only to this session.",
            )),
            3 => Some(label(
                "Não foi possível salvar. Tente novamente; as escolhas continuam nesta sessão.",
                "Could not save. Try again; your choices remain in this session.",
            )),
            4 => Some(label("Salvando preferências…", "Saving preferences…")),
            _ => None,
        }
    }
    pub fn retry_available(&self) -> bool {
        self.tx.is_some()
            && self.state.load(std::sync::atomic::Ordering::Relaxed) == SaveState::Failed as u8
    }
    pub fn retry(&mut self) {
        self.queue(self.latest);
    }
    fn queue(&mut self, value: Settings) {
        self.latest = value;
        if self
            .tx
            .as_ref()
            .is_some_and(|tx| tx.try_send(value).is_ok())
        {
            self.queued = value;
            self.state.store(
                SaveState::Pending as u8,
                std::sync::atomic::Ordering::Relaxed,
            );
        }
    }
    pub fn reset(&mut self) {
        self.queue(Settings::default());
    }
    pub fn update(&mut self, value: Settings) {
        self.latest = value;
        if self.queued == value {
            return;
        }
        self.queue(value);
    }
}
impl Drop for Store {
    fn drop(&mut self) {
        if let Some(tx) = self.tx.take() {
            if self.latest != self.queued {
                let _ = tx.send(self.latest);
            }
            drop(tx);
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
#[cfg(test)]
mod settings_tests {
    use super::*;
    #[test]
    fn writer_flushes_latest_choice_on_immediate_close() {
        let dir = std::env::temp_dir().join(format!("hypercast-settings-{}", uuid::Uuid::new_v4()));
        let file = dir.join("preferences.json");
        {
            let mut store = Store::at_file(Default::default(), file.clone());
            for volume in 0..=99 {
                store.update(Settings {
                    volume,
                    ..Default::default()
                });
            }
        }
        assert_eq!(
            Settings::decode(&std::fs::read(&file).unwrap())
                .unwrap()
                .volume,
            99
        );
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2);
        std::fs::remove_file(file.with_extension("lock")).unwrap();
        std::fs::remove_file(file).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }
    #[test]
    fn versioned_round_trip_rejects_corrupt_unsafe_and_future_values() {
        let value = Settings {
            codec: VideoCodec::Av1,
            amd_profile: AmdPreset::Text,
            rate_mode: crate::adaptive_upload::Mode::Automatic,
            volume: 35,
            sidebar: false,
            stream: crate::resolution::StreamSettings {
                size: crate::resolution::OutputSize::FullHd,
                fps: 60,
            },
            ..Default::default()
        };
        assert_eq!(Settings::decode(value.encode().as_bytes()), Some(value));
        let base: serde_json::Value = serde_json::from_str(&value.encode()).unwrap();
        let mut legacy = base.clone();
        legacy.as_object_mut().unwrap().remove("rate_mode");
        legacy.as_object_mut().unwrap().remove("codec");
        legacy.as_object_mut().unwrap().remove("amd_profile");
        let old = Settings::decode(legacy.to_string().as_bytes()).unwrap();
        assert_eq!(old.rate_mode, crate::adaptive_upload::Mode::Manual);
        assert_eq!(old.codec, VideoCodec::Hevc);
        assert_eq!(old.amd_profile, AmdPreset::Fast);
        assert_eq!(old.volume, 35);
        for (field, bad) in [
            ("codec", serde_json::json!("wrong")),
            ("amd_profile", serde_json::json!("wrong")),
            ("schema", serde_json::json!(2)),
            ("fps", serde_json::json!(999)),
            ("volume", serde_json::json!(101)),
            ("upload_mbps", serde_json::json!(500)),
            ("rate_mode", serde_json::json!("invalid")),
            ("resolution", serde_json::json!("../private")),
        ] {
            let mut changed = base.clone();
            changed[field] = bad;
            assert!(Settings::decode(changed.to_string().as_bytes()).is_none());
        }
        assert!(Settings::decode(b"{broken").is_none());
        assert!(Settings::decode(&vec![b' '; 4097]).is_none());
    }
    fn fixture() -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!("hypercast-prefs-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        let file = dir.join("preferences.json");
        (dir, file)
    }
    fn cleanup(dir: &std::path::Path) {
        for entry in std::fs::read_dir(dir).unwrap() {
            std::fs::remove_file(entry.unwrap().path()).unwrap();
        }
        std::fs::remove_dir(dir).unwrap();
    }
    #[test]
    fn future_file_is_preserved_even_if_created_after_startup_and_reset() {
        let (dir, file) = fixture();
        let lock = lock_settings(&file).unwrap();
        let mut store = Store::at_file(Default::default(), file.clone());
        store.update(Settings {
            volume: 25,
            ..Default::default()
        });
        let future = b"{\"schema\":2,\"future_preference\":\"keep exact bytes\"}";
        save(&file, std::str::from_utf8(future).unwrap()).unwrap();
        drop(lock);
        store.reset();
        let status = store.state.clone();
        drop(store);
        assert_eq!(std::fs::read(&file).unwrap(), future);
        assert_eq!(
            status.load(std::sync::atomic::Ordering::Relaxed),
            SaveState::Protected as u8
        );
        std::fs::write(&file, vec![b' '; 4097]).unwrap();
        assert_eq!(
            write_settings(&file, Default::default()).unwrap(),
            SaveState::Protected
        );
        assert_eq!(std::fs::metadata(&file).unwrap().len(), 4097);
        cleanup(&dir);
    }
    #[test]
    fn reset_keeps_identity_language_and_unknown_keys_and_repairs_corruption_on_request() {
        let (dir, file) = fixture();
        save(&dir.join("display-name"), "Lucas").unwrap();
        save(&dir.join("language"), "en-US").unwrap();
        let mut original: serde_json::Value = serde_json::from_str(
            &Settings {
                volume: 12,
                codec: VideoCodec::Av1,
                ..Default::default()
            }
            .encode(),
        )
        .unwrap();
        original["extra_schema_one"] = serde_json::json!({"keep":true});
        save(&file, &original.to_string()).unwrap();
        {
            let mut store = Store::at_file(Default::default(), file.clone());
            store.reset();
        }
        let data = std::fs::read(&file).unwrap();
        assert_eq!(Settings::decode(&data), Some(Settings::default()));
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&data).unwrap()["extra_schema_one"]["keep"],
            true
        );
        save(&file, "{truncated").unwrap();
        {
            let _store = Store::at_file(Default::default(), file.clone());
        }
        assert_eq!(std::fs::read(&file).unwrap(), b"{truncated");
        {
            let mut store = Store::at_file(Default::default(), file.clone());
            store.reset();
        }
        assert_eq!(
            Settings::decode(&std::fs::read(&file).unwrap()),
            Some(Settings::default())
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("display-name")).unwrap(),
            "Lucas"
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("language")).unwrap(),
            "en-US"
        );
        cleanup(&dir);
    }
    #[test]
    fn failed_save_keeps_session_choices_and_retry_recovers() {
        let (dir, file) = fixture();
        std::fs::create_dir(&file).unwrap();
        let value = Settings {
            volume: 47,
            ..Default::default()
        };
        let mut store = Store::at_file(Default::default(), file.clone());
        store.update(value);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while !store.retry_available() {
            assert!(
                std::time::Instant::now() < deadline,
                "Save error was not reported"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(store.current(), value);
        std::fs::remove_dir(&file).unwrap();
        store.retry();
        drop(store);
        assert_eq!(
            Settings::decode(&std::fs::read(&file).unwrap()),
            Some(value)
        );
        cleanup(&dir);
    }
    #[test]
    #[ignore = "Child fixture; executed by concurrent_processes_publish_complete_snapshots"]
    fn process_writer() {
        let file = PathBuf::from(
            std::env::var_os("HYPERCAST_PREF_TEST_FILE").expect("isolated test path"),
        );
        assert!(file.is_absolute());
        let av1 = std::env::var_os("HYPERCAST_PREF_TEST_AV1").is_some();
        for _ in 0..12 {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            loop {
                match write_settings(
                    &file,
                    Settings {
                        codec: if av1 {
                            VideoCodec::Av1
                        } else {
                            VideoCodec::Hevc
                        },
                        volume: if av1 { 23 } else { 77 },
                        ..Default::default()
                    },
                ) {
                    Ok(state) => {
                        assert_eq!(state, SaveState::Saved);
                        break;
                    }
                    Err(e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            && std::time::Instant::now() < deadline =>
                    {
                        std::thread::sleep(std::time::Duration::from_millis(20))
                    }
                    Err(e) => panic!("Preference write failed: {e}"),
                }
            }
        }
    }
    #[test]
    fn concurrent_processes_publish_complete_snapshots() {
        let (dir, file) = fixture();
        let exe = std::env::current_exe().unwrap();
        let launch = |av1| {
            let mut command = std::process::Command::new(&exe);
            command
                .args([
                    "--exact",
                    "preferences::settings_tests::process_writer",
                    "--ignored",
                    "--nocapture",
                ])
                .env("HYPERCAST_PREF_TEST_FILE", &file)
                .env_remove("HYPERCAST_PREF_TEST_AV1")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::inherit());
            if av1 {
                command.env("HYPERCAST_PREF_TEST_AV1", "1");
            }
            command.spawn().unwrap()
        };
        let mut children = [launch(false), launch(true)];
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        let mut reads = 0;
        loop {
            if let Ok(bytes) = read_settings(&file) {
                let v = Settings::decode(&bytes).expect("No partial JSON may be observed");
                assert!(
                    (v.codec == VideoCodec::Hevc && v.volume == 77)
                        || (v.codec == VideoCodec::Av1 && v.volume == 23)
                );
                reads += 1;
            }
            if children.iter_mut().all(|c| c.try_wait().unwrap().is_some()) {
                break;
            }
            if std::time::Instant::now() > deadline {
                for c in &mut children {
                    let _ = c.kill();
                    let _ = c.wait();
                }
                panic!("Preference writers did not finish");
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        for child in &mut children {
            let status = child.wait().unwrap();
            assert!(status.success(), "Writer exited with {status}");
        }
        assert!(reads > 0);
        assert!(Settings::decode(&read_settings(&file).unwrap()).is_some());
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2);
        cleanup(&dir);
    }
}
