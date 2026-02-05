//! Artist detail state store

use crate::display_types::{Album, Artist};
use dioxus::prelude::*;
use std::collections::HashMap;

/// State for the artist detail view
#[derive(Clone, Debug, Default, PartialEq, Store)]
pub struct ArtistDetailState {
    /// The artist being viewed
    pub artist: Option<Artist>,
    /// Albums by this artist
    pub albums: Vec<Album>,
    /// Artists keyed by album ID (for compilations showing other artists)
    pub artists_by_album: HashMap<String, Vec<Artist>>,
    /// Whether data is loading
    pub loading: bool,
    /// Error message if loading failed
    pub error: Option<String>,
}
