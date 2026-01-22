//! Library state store

use crate::display_types::{Album, Artist};
use dioxus::prelude::*;
use std::collections::HashMap;

/// State for the library view
#[derive(Clone, Debug, Default, PartialEq, Store)]
pub struct LibraryState {
    /// Albums in the library
    pub albums: Vec<Album>,
    /// Artists keyed by album ID
    pub artists_by_album: HashMap<String, Vec<Artist>>,
    /// Whether the library is loading
    pub loading: bool,
    /// Error message if loading failed
    pub error: Option<String>,
}
