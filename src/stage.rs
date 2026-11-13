//! Viewer-side arrangement of the room: the videos on the stage and the strip
//! of the others floating over it. Pure policy and geometry, no rendering.
use eframe::egui::{Pos2, Rect, Vec2, pos2, vec2};

/// What the viewer wants to see. Applies to every participant, including the
/// viewer's own preview when the preview eye is on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Show {
    #[default]
    All,
    Screens,
    Cameras,
}
impl Show {
    pub fn label(self) -> &'static str {
        match self {
            Self::All => crate::i18n::label("Tudo", "All"),
            Self::Screens => crate::i18n::label("Telas", "Screens"),
            Self::Cameras => crate::i18n::label("Câmeras", "Cameras"),
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Screens => "screens",
            Self::Cameras => "cameras",
        }
    }
    pub fn from_key(key: &str) -> Option<Self> {
        [Self::All, Self::Screens, Self::Cameras]
            .into_iter()
            .find(|show| show.key() == key)
    }
    pub fn allows(self, camera: bool) -> bool {
        match self {
            Self::All => true,
            Self::Screens => !camera,
            Self::Cameras => camera,
        }
    }
}

/// Where the strip rests after it is dropped: corners and edge middles.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Anchor {
    TopLeft,
    Top,
    TopRight,
    Left,
    Right,
    BottomLeft,
    Bottom,
    #[default]
    BottomRight,
}
impl Anchor {
    pub const ALL: [Self; 8] = [
        Self::TopLeft,
        Self::Top,
        Self::TopRight,
        Self::Left,
        Self::Right,
        Self::BottomLeft,
        Self::Bottom,
        Self::BottomRight,
    ];
    pub fn key(self) -> &'static str {
        match self {
            Self::TopLeft => "top-left",
            Self::Top => "top",
            Self::TopRight => "top-right",
            Self::Left => "left",
            Self::Right => "right",
            Self::BottomLeft => "bottom-left",
            Self::Bottom => "bottom",
            Self::BottomRight => "bottom-right",
        }
    }
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|anchor| anchor.key() == key)
    }
    /// Side anchors stack the strip as a column; the rest form a row.
    pub fn vertical(self) -> bool {
        matches!(self, Self::Left | Self::Right)
    }
    pub fn bottom(self) -> bool {
        matches!(self, Self::BottomLeft | Self::Bottom | Self::BottomRight)
    }
    /// -1, 0 or 1 on each axis.
    fn grid(self) -> (i8, i8) {
        match self {
            Self::TopLeft => (-1, -1),
            Self::Top => (0, -1),
            Self::TopRight => (1, -1),
            Self::Left => (-1, 0),
            Self::Right => (1, 0),
            Self::BottomLeft => (-1, 1),
            Self::Bottom => (0, 1),
            Self::BottomRight => (1, 1),
        }
    }
}

/// A published or locally previewed video, in arrival order.
#[derive(Clone, Copy, Debug)]
pub struct Source<'a> {
    pub id: &'a str,
    pub owner: &'a str,
    pub camera: bool,
    pub local: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Arrangement {
    /// Videos sharing the stage: one chosen video, or the screens side by side.
    pub stage: Vec<String>,
    /// Everything else that is visible, for the strip. Screens first.
    pub strip: Vec<String>,
}

#[derive(Default)]
pub(crate) struct RoomVideos {
    pub arrival: Vec<String>,
    previous: Vec<(String, String)>,
}

impl RoomVideos {
    pub fn arrange(
        &mut self,
        sources: &[Source<'_>],
        show: Show,
        focus: &mut Option<String>,
    ) -> Arrangement {
        let source = |id: &str| sources.iter().find(|source| source.id == id);
        self.arrival.retain(|id| source(id).is_some());
        for source in sources {
            if !self.arrival.iter().any(|id| id == source.id) {
                self.arrival.push(source.id.to_owned());
            }
        }
        if let Some(id) = focus.as_deref()
            && source(id).is_none()
            && let Some((_, owner)) = self.previous.iter().find(|(previous, _)| previous == id)
        {
            *focus = sources
                .iter()
                .find(|s| s.owner == owner && show.allows(s.camera))
                .map(|s| s.id.to_owned());
        }
        let ordered: Vec<_> = self
            .arrival
            .iter()
            .filter_map(|id| source(id).copied())
            .collect();
        let arrangement = arrange(&ordered, show, focus.as_deref(), &self.previous);
        self.previous = arrangement
            .stage
            .iter()
            .filter_map(|id| source(id).map(|s| (id.clone(), s.owner.to_owned())))
            .collect();
        arrangement
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Playback {
    pub main: Vec<String>,
    pub strip: Vec<String>,
    pub audio: Vec<String>,
}

impl Playback {
    pub fn wants_video(&self, id: &str) -> bool {
        self.main.iter().chain(&self.strip).any(|video| video == id)
    }
}

impl Arrangement {
    pub(crate) fn playback(
        &self,
        sources: &[Source<'_>],
        show: Show,
        mode: crate::subscription::ViewMode,
        rect: Rect,
        anchor: Anchor,
    ) -> Playback {
        use crate::subscription::ViewMode;
        let remote_screens = sources.iter().filter(|s| !s.camera && !s.local);
        if mode == ViewMode::Grid {
            return Playback {
                main: sources
                    .iter()
                    .filter(|s| show.allows(s.camera))
                    .map(|s| s.id.to_owned())
                    .collect(),
                audio: remote_screens.map(|s| s.owner.to_owned()).collect(),
                ..Default::default()
            };
        }
        let shown = if rect.is_positive() && !too_small_for_cards(rect) && !self.strip.is_empty() {
            strip_layout(rect, self.strip.len(), anchor).shown
        } else {
            0
        };
        // With a camera on stage, keep screen audio audible.
        let stage_audio: Vec<_> = remote_screens
            .clone()
            .filter(|s| self.stage.iter().any(|id| id == s.id))
            .map(|s| s.owner.to_owned())
            .collect();
        Playback {
            main: self.stage.clone(),
            strip: self.strip[..shown].to_vec(),
            audio: if stage_audio.is_empty() {
                remote_screens.map(|s| s.owner.to_owned()).collect()
            } else {
                stage_audio
            },
        }
    }
}

/// Screens that share the stage before the rest go to the strip.
pub const MAX_TILED: usize = 4;

/// `manual` is the viewer's explicit choice; `previous` what the stage held.
fn arrange(
    sources: &[Source<'_>],
    show: Show,
    manual: Option<&str>,
    previous: &[(String, String)],
) -> Arrangement {
    let visible: Vec<&Source<'_>> = sources.iter().filter(|s| show.allows(s.camera)).collect();
    // Remote videos first: watching yourself is a fallback.
    let mut screens: Vec<&Source<'_>> = visible.iter().copied().filter(|s| !s.camera).collect();
    screens.sort_by_key(|s| s.local);
    let mut cameras: Vec<&Source<'_>> = visible.iter().copied().filter(|s| s.camera).collect();
    cameras.sort_by_key(|s| s.local);
    let stage: Vec<&Source<'_>> =
        if let Some(chosen) = visible.iter().find(|s| Some(s.id) == manual) {
            vec![*chosen]
        } else if !screens.is_empty() {
            screens.iter().copied().take(MAX_TILED).collect()
        } else {
            cameras
                .iter()
                .copied()
                .find(|s| previous.iter().any(|(id, _)| id == s.id))
                .or_else(|| {
                    // A shared screen ended: keep watching the same person.
                    cameras
                        .iter()
                        .copied()
                        .find(|s| previous.iter().any(|(_, owner)| owner == s.owner))
                })
                .or_else(|| cameras.first().copied())
                .into_iter()
                .collect()
        };
    let strip = screens
        .iter()
        .chain(&cameras)
        .filter(|s| !stage.iter().any(|on| on.id == s.id))
        .map(|s| s.id.to_owned())
        .collect();
    Arrangement {
        stage: stage.iter().map(|s| s.id.to_owned()).collect(),
        strip,
    }
}

/// Tiles for the videos sharing the stage: 1 full, 2 side by side, 3-4 in a
/// 2 × 2 grid. Wide stages keep 2 videos in a row; tall ones stack them.
pub fn stage_tiles(stage: Rect, count: usize, gap: f32) -> Vec<Rect> {
    if count == 0 {
        return Vec::new();
    }
    if count == 1 {
        return vec![stage];
    }
    let (columns, rows) = match count {
        2 if stage.width() >= stage.height() => (2, 1),
        2 => (1, 2),
        _ => (2, count.div_ceil(2)),
    };
    let size = vec2(
        (stage.width() - gap * (columns - 1) as f32) / columns as f32,
        (stage.height() - gap * (rows - 1) as f32) / rows as f32,
    );
    (0..count)
        .map(|i| {
            let (column, row) = (i % columns, i / columns);
            Rect::from_min_size(
                stage.min + vec2(column as f32 * (size.x + gap), row as f32 * (size.y + gap)),
                size,
            )
        })
        .collect()
}

pub const MARGIN: f32 = 12.0;
pub const GAP: f32 = 8.0;
/// Name bar under each card's video.
pub const LABEL: f32 = 22.0;
const MIN_CARD: f32 = 96.0;

/// Below this stage size cards would hide most of the main video.
pub fn too_small_for_cards(stage: Rect) -> bool {
    stage.width() < 480.0 || stage.height() < 270.0
}

fn card_size(width: f32) -> Vec2 {
    vec2(width, width * 9.0 / 16.0 + LABEL)
}

/// The strip's cards and how many videos did not fit (shown as "+N").
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StripLayout {
    pub card: Vec2,
    pub shown: usize,
    pub overflow: usize,
    pub vertical: bool,
}
impl StripLayout {
    /// Slots along the strip: cards plus the "+N" chip.
    fn slots(self) -> usize {
        self.shown + usize::from(self.overflow > 0)
    }
    pub fn size(self) -> Vec2 {
        let n = self.slots() as f32;
        if self.vertical {
            vec2(self.card.x, self.card.y * n + GAP * (n - 1.0))
        } else {
            vec2(self.card.x * n + GAP * (n - 1.0), self.card.y)
        }
    }
    /// Slot rectangles from the strip's top-left corner; the last slot is the
    /// "+N" chip when something overflowed.
    pub fn slots_at(self, origin: Pos2) -> Vec<Rect> {
        (0..self.slots())
            .map(|i| {
                let step = i as f32;
                let offset = if self.vertical {
                    vec2(0.0, (self.card.y + GAP) * step)
                } else {
                    vec2((self.card.x + GAP) * step, 0.0)
                };
                Rect::from_min_size(origin + offset, self.card)
            })
            .collect()
    }
}

/// Cards keep a readable size and shrink only to fit along the stage edge.
pub fn strip_layout(stage: Rect, count: usize, anchor: Anchor) -> StripLayout {
    let vertical = anchor.vertical();
    let room = stage.shrink(MARGIN);
    let length = |width: f32, n: usize| {
        let size = card_size(width);
        let n = n as f32;
        if vertical {
            size.y * n + GAP * (n - 1.0)
        } else {
            size.x * n + GAP * (n - 1.0)
        }
    };
    let available = if vertical {
        room.height()
    } else {
        room.width()
    };
    // The card also has to fit across the strip.
    let across = if vertical {
        room.width()
    } else {
        (room.height() - LABEL) * 16.0 / 9.0
    };
    let mut width = (stage.width() * 0.18).clamp(120.0, 240.0).min(across);
    while width > MIN_CARD && length(width, count) > available {
        width -= 4.0;
    }
    let width = width.max(MIN_CARD.min(across)).max(1.0);
    let mut shown = count;
    while shown > 1 && length(width, shown + usize::from(shown < count)) > available {
        shown -= 1;
    }
    StripLayout {
        card: card_size(width),
        shown,
        overflow: count - shown,
        vertical,
    }
}

/// Resting position (top-left) of a strip of `size` at `anchor`. `reserve`
/// keeps bottom anchors clear of the control bar while it is visible.
pub fn anchor_origin(stage: Rect, size: Vec2, anchor: Anchor, reserve: f32) -> Pos2 {
    let room = stage.shrink(MARGIN);
    let (gx, gy) = anchor.grid();
    let x = match gx {
        -1 => room.left(),
        1 => room.right() - size.x,
        _ => room.center().x - size.x / 2.0,
    };
    let y = match gy {
        -1 => room.top(),
        1 => room.bottom() - size.y - reserve,
        _ => room.center().y - size.y / 2.0,
    };
    clamp_origin(stage, size, pos2(x, y))
}

/// Keeps a dragged strip of `size` inside the stage.
pub fn clamp_origin(stage: Rect, size: Vec2, origin: Pos2) -> Pos2 {
    pos2(
        origin
            .x
            .clamp(stage.left(), (stage.right() - size.x).max(stage.left())),
        origin
            .y
            .clamp(stage.top(), (stage.bottom() - size.y).max(stage.top())),
    )
}

/// Where a strip dropped with its center at `point` should rest: the stage is
/// split in thirds, and the dead center goes to the nearer top or bottom.
pub fn nearest_anchor(stage: Rect, point: Pos2) -> Anchor {
    let third = |value: f32, start: f32, length: f32| {
        let t = (value - start) / length.max(1.0);
        if t < 1.0 / 3.0 {
            -1
        } else if t > 2.0 / 3.0 {
            1
        } else {
            0
        }
    };
    let column = third(point.x, stage.left(), stage.width());
    let row = third(point.y, stage.top(), stage.height());
    match (column, row) {
        (-1, -1) => Anchor::TopLeft,
        (0, -1) => Anchor::Top,
        (1, -1) => Anchor::TopRight,
        (-1, 0) => Anchor::Left,
        (1, 0) => Anchor::Right,
        (-1, 1) => Anchor::BottomLeft,
        (0, 1) => Anchor::Bottom,
        (1, 1) => Anchor::BottomRight,
        _ if point.y < stage.center().y => Anchor::Top,
        _ => Anchor::Bottom,
    }
}

/// One step of a slightly underdamped spring: a small overshoot, then rest.
pub fn spring_step(position: Pos2, velocity: Vec2, target: Pos2, dt: f32) -> (Pos2, Vec2) {
    const STIFFNESS: f32 = 260.0;
    const DAMPING: f32 = 24.0;
    // Sub-steps keep the integration stable when a frame arrives late.
    let steps = (dt / (1.0 / 240.0)).ceil().clamp(1.0, 16.0);
    let h = dt.min(1.0 / 15.0) / steps;
    let (mut p, mut v) = (position, velocity);
    for _ in 0..steps as usize {
        let force = (target - p) * STIFFNESS - v * DAMPING;
        v += force * h;
        p += v * h;
    }
    (p, v)
}

/// True once the strip is close enough to its target to stop animating.
pub fn at_rest(position: Pos2, velocity: Vec2, target: Pos2) -> bool {
    (target - position).length() < 0.5 && velocity.length() < 5.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen<'a>(owner: &'a str) -> Source<'a> {
        Source {
            id: owner,
            owner,
            camera: false,
            local: false,
        }
    }
    fn camera<'a>(id: &'a str, owner: &'a str) -> Source<'a> {
        Source {
            id,
            owner,
            camera: true,
            local: false,
        }
    }
    fn ids(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| (*v).to_owned()).collect()
    }

    #[test]
    fn a_single_camera_takes_the_whole_stage() {
        let a = arrange(&[camera("ana/camera", "ana")], Show::All, None, &[]);
        assert_eq!(a.stage, ids(&["ana/camera"]));
        assert!(a.strip.is_empty());
    }

    #[test]
    fn screens_share_the_stage_and_cameras_line_up_in_the_strip() {
        let sources = [
            camera("ana/camera", "ana"),
            screen("ana"),
            screen("bia"),
            camera("bia/camera", "bia"),
        ];
        let a = arrange(
            &sources,
            Show::All,
            None,
            &[("ana/camera".into(), "ana".into())],
        );
        assert_eq!(a.stage, ids(&["ana", "bia"]));
        assert_eq!(a.strip, ids(&["ana/camera", "bia/camera"]));
    }

    #[test]
    fn choosing_a_video_moves_the_rest_to_the_strip_and_back() {
        let sources = [screen("ana"), screen("bia"), camera("ana/camera", "ana")];
        let a = arrange(&sources, Show::All, Some("ana/camera"), &[]);
        assert_eq!(a.stage, ids(&["ana/camera"]));
        assert_eq!(a.strip, ids(&["ana", "bia"]));
        let b = arrange(&sources, Show::All, Some("bia"), &[]);
        assert_eq!(b.stage, ids(&["bia"]));
        assert_eq!(b.strip, ids(&["ana", "ana/camera"]));
        let tiled = arrange(&sources, Show::All, None, &[("bia".into(), "bia".into())]);
        assert_eq!(tiled.stage, ids(&["ana", "bia"]));
    }

    #[test]
    fn a_stopped_screen_hands_the_stage_to_the_sharers_camera() {
        let mut videos = RoomVideos::default();
        let mut focus = Some("screen-track".into());
        let shared = Source {
            id: "screen-track",
            ..screen("ana")
        };
        let cameras = [camera("camera-b", "bia"), camera("camera-a", "ana")];
        videos.arrange(&[shared, cameras[0], cameras[1]], Show::All, &mut focus);
        let a = videos.arrange(
            &[screen("bia"), cameras[0], cameras[1]],
            Show::All,
            &mut focus,
        );
        assert_eq!(a.stage, ids(&["camera-a"]));
        assert_eq!(a.strip, ids(&["bia", "camera-b"]));
        assert_eq!(focus.as_deref(), Some("camera-a"));
    }

    #[test]
    fn playback_requests_only_visible_videos_and_keeps_screen_audio_on_cameras() {
        use crate::subscription::ViewMode;
        let sources = [screen("ana"), screen("bia"), camera("camera-a", "ana")];
        let wide = Rect::from_min_size(Pos2::ZERO, vec2(960.0, 540.0));
        let compact = Rect::from_min_size(Pos2::ZERO, vec2(400.0, 240.0));
        let arrangement = arrange(&sources, Show::All, Some("ana"), &[]);
        let playback = arrangement.playback(
            &sources,
            Show::All,
            ViewMode::Spotlight,
            wide,
            Anchor::BottomRight,
        );
        assert_eq!(playback.main, ids(&["ana"]));
        assert_eq!(playback.strip, ids(&["bia", "camera-a"]));
        assert_eq!(playback.audio, ids(&["ana"]));
        let playback = arrangement.playback(
            &sources,
            Show::All,
            ViewMode::Spotlight,
            compact,
            Anchor::BottomRight,
        );
        assert!(playback.strip.is_empty());
        assert!(!playback.wants_video("camera-a"));
        let arrangement = arrange(&sources, Show::Cameras, None, &[]);
        for mode in [ViewMode::Spotlight, ViewMode::Grid] {
            let playback =
                arrangement.playback(&sources, Show::Cameras, mode, wide, Anchor::BottomRight);
            assert_eq!(playback.main, ids(&["camera-a"]));
            assert_eq!(playback.audio, ids(&["ana", "bia"]));
        }
    }

    #[test]
    fn remote_videos_come_before_your_own() {
        let mine = Source {
            id: "/local-camera",
            owner: "me",
            camera: true,
            local: true,
        };
        let a = arrange(&[mine, camera("ana/camera", "ana")], Show::All, None, &[]);
        assert_eq!(a.stage, ids(&["ana/camera"]));
        assert_eq!(a.strip, ids(&["/local-camera"]));
    }

    #[test]
    fn view_filters_hide_one_kind_without_switching_modes() {
        let sources = [screen("ana"), camera("ana/camera", "ana")];
        let screens = arrange(&sources, Show::Screens, None, &[]);
        assert_eq!(screens.stage, ids(&["ana"]));
        assert!(screens.strip.is_empty());
        let cameras = arrange(
            &sources,
            Show::Cameras,
            None,
            &[("ana".into(), "ana".into())],
        );
        assert_eq!(cameras.stage, ids(&["ana/camera"]));
        assert!(cameras.strip.is_empty());
        let none = arrange(&sources[1..], Show::Screens, None, &[]);
        assert_eq!(none, Arrangement::default());
    }

    #[test]
    fn more_than_four_screens_spill_into_the_strip() {
        let sources = [
            screen("a"),
            screen("b"),
            screen("c"),
            screen("d"),
            screen("e"),
        ];
        let a = arrange(&sources, Show::All, None, &[]);
        assert_eq!(a.stage.len(), MAX_TILED);
        assert_eq!(a.strip, ids(&["e"]));
    }

    #[test]
    fn stage_tiles_cover_the_stage_without_overlapping() {
        let stage = Rect::from_min_size(pos2(10.0, 20.0), vec2(1200.0, 700.0));
        for count in 1..=MAX_TILED {
            let tiles = stage_tiles(stage, count, 8.0);
            assert_eq!(tiles.len(), count);
            for (i, tile) in tiles.iter().enumerate() {
                assert!(stage.expand(0.01).contains_rect(*tile), "{count}: {tile:?}");
                for other in &tiles[i + 1..] {
                    assert!(!tile.shrink(0.5).intersects(*other));
                }
            }
        }
        let two = stage_tiles(stage, 2, 8.0);
        assert_eq!(
            two[0].top(),
            two[1].top(),
            "a wide stage keeps two side by side"
        );
    }

    #[test]
    fn the_strip_lines_up_inside_the_stage_at_every_anchor() {
        for stage in [
            Rect::from_min_size(pos2(10.0, 50.0), vec2(1600.0, 900.0)),
            Rect::from_min_size(Pos2::ZERO, vec2(700.0, 380.0)),
            Rect::from_min_size(Pos2::ZERO, vec2(480.0, 270.0)),
        ] {
            for anchor in Anchor::ALL {
                for count in 1..=6 {
                    let layout = strip_layout(stage, count, anchor);
                    assert_eq!(layout.shown + layout.overflow, count);
                    assert!(layout.shown >= 1);
                    let origin = anchor_origin(stage, layout.size(), anchor, 0.0);
                    let slots = layout.slots_at(origin);
                    for (i, slot) in slots.iter().enumerate() {
                        assert!(
                            stage.expand(0.01).contains_rect(*slot),
                            "{stage:?} {anchor:?} {count}: {slot:?}"
                        );
                        for other in &slots[i + 1..] {
                            assert!(!slot.shrink(0.5).intersects(*other));
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn a_large_stage_shows_everyone_at_a_readable_size() {
        let stage = Rect::from_min_size(Pos2::ZERO, vec2(1920.0, 1000.0));
        let layout = strip_layout(stage, 4, Anchor::Bottom);
        assert_eq!((layout.shown, layout.overflow), (4, 0));
        assert!(layout.card.x >= 240.0 - 0.01);
        let origin = anchor_origin(stage, layout.size(), Anchor::Bottom, 0.0);
        assert!((origin.x + layout.size().x / 2.0 - stage.center().x).abs() < 0.01);
        assert!((origin.y + layout.size().y - (stage.bottom() - MARGIN)).abs() < 0.01);
        let column = strip_layout(stage, 3, Anchor::Right);
        assert!(column.vertical && column.size().y > column.size().x);
    }

    #[test]
    fn dropping_snaps_to_corners_and_edge_middles() {
        let stage = Rect::from_min_size(Pos2::ZERO, vec2(900.0, 600.0));
        assert_eq!(nearest_anchor(stage, pos2(20.0, 20.0)), Anchor::TopLeft);
        assert_eq!(nearest_anchor(stage, pos2(450.0, 30.0)), Anchor::Top);
        assert_eq!(nearest_anchor(stage, pos2(880.0, 300.0)), Anchor::Right);
        assert_eq!(nearest_anchor(stage, pos2(450.0, 590.0)), Anchor::Bottom);
        assert_eq!(nearest_anchor(stage, pos2(450.0, 280.0)), Anchor::Top);
        assert_eq!(nearest_anchor(stage, pos2(450.0, 320.0)), Anchor::Bottom);
    }

    #[test]
    fn the_spring_overshoots_slightly_and_settles() {
        let target = pos2(100.0, 0.0);
        let (mut p, mut v) = (Pos2::ZERO, Vec2::ZERO);
        let mut peak = 0.0_f32;
        let mut frames = 0;
        while !at_rest(p, v, target) {
            (p, v) = spring_step(p, v, target, 1.0 / 60.0);
            peak = peak.max(p.x);
            frames += 1;
            assert!(frames < 120, "settles within two seconds");
        }
        assert!(
            peak > 100.5 && peak < 115.0,
            "a visible but small bounce: {peak}"
        );
        let (late, _) = spring_step(Pos2::ZERO, Vec2::ZERO, target, 0.25);
        assert!(late.x.is_finite() && late.x < 150.0);
    }

    #[test]
    fn keys_round_trip() {
        for show in [Show::All, Show::Screens, Show::Cameras] {
            assert_eq!(Show::from_key(show.key()), Some(show));
        }
        for anchor in Anchor::ALL {
            assert_eq!(Anchor::from_key(anchor.key()), Some(anchor));
        }
        assert_eq!(Anchor::from_key("middle"), None);
    }
}
