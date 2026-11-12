pub mod bandwidth;
#[cfg(all(windows, feature = "desktop"))]
pub mod desktop;
#[cfg(all(windows, feature = "desktop"))]
pub mod desktop_engine;
#[cfg(feature = "desktop")]
pub mod desktop_ui;
#[cfg(all(windows, feature = "desktop"))]
pub mod diagnostics;
#[cfg(all(windows, feature = "nvenc"))]
pub mod encoder;
pub mod i18n;
#[cfg(feature = "desktop")]
pub mod invitation;
#[cfg(feature = "livekit-network")]
pub mod network;
pub mod pacing;
#[cfg(feature = "desktop")]
pub mod preferences;
#[cfg(all(windows, feature = "desktop"))]
pub mod presenter;
#[cfg(feature = "livekit-network")]
pub mod receiver;
pub mod resolution;
#[cfg(feature = "desktop")]
pub mod room_transition;
#[cfg(windows)]
pub mod screen_capture;
#[cfg(feature = "desktop")]
pub mod server_status;
pub mod subscription;
#[cfg(all(windows, feature = "desktop"))]
pub mod system_audio;
#[cfg(all(windows, feature = "desktop"))]
pub mod updater;
#[cfg(all(windows, feature = "desktop"))]
pub mod video_capture;
#[cfg(feature = "livekit-network")]
pub mod video_metrics;
#[cfg(all(windows, feature = "desktop"))]
mod windows_camera;

#[cfg(feature = "desktop")]
pub mod room_creation;

#[cfg(all(target_os = "linux", feature = "desktop"))]
pub mod linux;

#[cfg(feature = "desktop")]
pub mod resilience;

pub mod reception_recovery;

pub mod adaptive_upload;

pub mod encode_caps;

#[cfg(feature = "desktop")]
pub mod evidence;
