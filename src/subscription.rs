/// UI-independent layout policy used by the native LiveKit receiver.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum ViewMode {
    Grid,
    Spotlight,
}

#[cfg(feature = "desktop")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoomLayout {
    pub self_preview: bool,
    pub mode: ViewMode,
    pub focus: Option<String>,
    pub paused: bool,
    pub volume: u32,
    // The room UI requests the visible videos and their audio explicitly.
    pub(crate) playback: Option<crate::stage::Playback>,
}

#[cfg(feature = "desktop")]
impl RoomLayout {
    pub fn new(mode: ViewMode, volume: u32) -> Self {
        Self {
            self_preview: true,
            mode,
            focus: None,
            paused: false,
            volume,
            playback: None,
        }
    }

    #[cfg(any(windows, target_os = "linux"))]
    pub(crate) fn camera_preview_visible(&self) -> bool {
        self.self_preview
            && !self.paused
            && self
                .playback
                .as_ref()
                .is_none_or(|view| view.wants_video(crate::camera_preview::LOCAL_CAMERA))
    }
}

#[cfg(feature = "desktop")]
impl Default for RoomLayout {
    fn default() -> Self {
        Self::new(ViewMode::Spotlight, 80)
    }
}

/// A single-layer track cannot supply a cheap thumbnail. Pause it rather than
/// silently downloading/decoding another full-resolution stream in Spotlight.
pub fn delivery_demand(
    mode: ViewMode,
    visible: bool,
    focused: bool,
    reduced_layer: bool,
) -> Demand {
    match demand(mode, visible, focused) {
        Demand::Thumbnail if !reduced_layer => Demand::Paused,
        value => value,
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Demand {
    Paused,
    Thumbnail,
    Grid,
    Full,
}

pub fn demand(mode: ViewMode, visible: bool, focused: bool) -> Demand {
    if !visible {
        return Demand::Paused;
    }
    match mode {
        ViewMode::Grid => Demand::Grid,
        ViewMode::Spotlight if focused => Demand::Full,
        ViewMode::Spotlight => Demand::Thumbnail,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hidden_tracks_are_paused_even_if_focused() {
        for mode in [ViewMode::Grid, ViewMode::Spotlight] {
            assert_eq!(demand(mode, false, true), Demand::Paused);
        }
        assert_eq!(demand(ViewMode::Spotlight, true, false), Demand::Thumbnail);
        assert_eq!(demand(ViewMode::Spotlight, true, true), Demand::Full);
    }
    #[test]
    fn spotlight_only_requests_thumbnails_when_layers_exist() {
        assert_eq!(
            delivery_demand(ViewMode::Spotlight, true, false, false),
            Demand::Paused
        );
        assert_eq!(
            delivery_demand(ViewMode::Spotlight, true, false, true),
            Demand::Thumbnail
        );
        assert_eq!(
            delivery_demand(ViewMode::Spotlight, true, true, false),
            Demand::Full
        );
        assert_eq!(
            delivery_demand(ViewMode::Grid, true, false, false),
            Demand::Grid
        );
    }
}

/// Keep the receiver negotiated while the SFU pauses unwanted RTP. Only send
/// subscription changes on transitions; repeated settings must be idempotent.
#[cfg(feature = "livekit-network")]
pub fn keep_receiver(publication: &livekit::prelude::RemoteTrackPublication, enabled: bool) {
    if !publication.is_desired() {
        publication.set_subscribed(true);
    }
    if publication.is_subscribed() {
        publication.set_enabled(enabled);
    }
}
#[cfg(feature = "livekit-network")]
pub fn reset_receivers(room: &livekit::Room) {
    use livekit::prelude::TrackSource;
    for p in room.remote_participants().into_values() {
        for publication in p.track_publications().into_values() {
            if matches!(
                publication.source(),
                TrackSource::Screenshare | TrackSource::ScreenshareAudio | TrackSource::Camera
            ) && (publication.is_desired() || publication.is_subscribed())
            {
                publication.set_enabled(false);
            }
        }
    }
}

/// Replace the RTC session inside the same Room: new PeerConnections, so the
/// SFU starts a fresh downlink bandwidth estimate, as leaving and joining
/// would. The SDK republishes local tracks and resubscribes remote ones.
#[cfg(feature = "livekit-network")]
pub async fn restart_session(room: &livekit::Room) -> anyhow::Result<()> {
    room.simulate_scenario(livekit::SimulateScenario::FullReconnect)
        .await
        .map_err(|error| anyhow::anyhow!("{error}"))
}

/// Room videos the SFU stopped forwarding (congestion), keyed by `key`.
#[cfg(feature = "livekit-network")]
pub fn server_paused<K>(
    room: &livekit::Room,
    key: impl Fn(&livekit::prelude::RemoteParticipant, &livekit::prelude::RemoteTrackPublication) -> K,
) -> Vec<K> {
    use livekit::prelude::{StreamState, TrackKind, TrackSource};
    let mut paused = Vec::new();
    for participant in room.remote_participants().into_values() {
        for publication in participant.track_publications().into_values() {
            if publication.kind() == TrackKind::Video
                && matches!(
                    publication.source(),
                    TrackSource::Screenshare | TrackSource::Camera
                )
                && publication.is_subscribed()
                && publication
                    .track()
                    .is_some_and(|track| track.stream_state() == StreamState::Paused)
            {
                paused.push(key(&participant, &publication));
            }
        }
    }
    paused
}
