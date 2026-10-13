/// Policy only: the LiveKit adapter and UI are step 3/4 work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewMode {
    Grid,
    Spotlight,
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
}
