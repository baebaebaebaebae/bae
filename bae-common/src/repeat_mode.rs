/// Repeat mode for playback
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepeatMode {
    None,
    Track,
    Album,
}

#[allow(clippy::derivable_impls)]
impl Default for RepeatMode {
    fn default() -> Self {
        RepeatMode::None
    }
}
