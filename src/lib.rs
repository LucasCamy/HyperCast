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
#[cfg(all(windows, feature = "desktop"))]
pub mod invitation;
#[cfg(all(windows, feature = "nvenc"))]
pub mod encoder;
#[cfg(feature = "livekit-network")]
pub mod network;
pub mod pacing;
#[cfg(all(windows, feature = "desktop"))]
pub mod presenter;
#[cfg(feature = "livekit-network")]
pub mod receiver;
#[cfg(windows)]
pub mod screen_capture;
pub mod subscription;

#[cfg(all(windows, feature = "desktop"))]
pub mod room_creation;
