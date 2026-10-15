#[cfg(all(windows, feature = "desktop"))]
pub mod system_audio;
#[cfg(all(windows, feature = "desktop"))]
pub mod video_capture;
#[cfg(all(windows, feature = "desktop"))]
pub mod desktop;
#[cfg(all(windows, feature = "desktop"))]
pub mod updater;
#[cfg(all(windows, feature = "desktop"))]
pub mod desktop_engine;
#[cfg(all(windows, feature = "desktop"))]
pub mod diagnostics;
#[cfg(feature = "desktop")]
pub mod invitation;
#[cfg(feature = "desktop")]
pub mod desktop_ui;
#[cfg(feature = "desktop")]
pub mod room_transition;
#[cfg(feature = "desktop")]
pub mod server_status;
#[cfg(feature = "desktop")]
pub mod preferences;
#[cfg(all(windows, feature = "nvenc"))]
pub mod encoder;
#[cfg(feature = "livekit-network")]
pub mod network;
#[cfg(feature = "livekit-network")]
pub mod video_metrics;
pub mod pacing;
pub mod resolution;
#[cfg(all(windows, feature = "desktop"))]
pub mod presenter;
#[cfg(feature = "livekit-network")]
pub mod receiver;
#[cfg(windows)]
pub mod screen_capture;
pub mod subscription;

#[cfg(feature = "desktop")]
pub mod room_creation;

#[cfg(all(target_os = "linux", feature = "desktop"))]
pub mod linux;
