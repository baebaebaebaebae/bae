//! Storage profiles state store

use crate::components::settings::StorageProfile;
use dioxus::prelude::*;

/// Storage profiles state
#[derive(Clone, Debug, Default, PartialEq, Store)]
pub struct StorageProfilesState {
    /// List of storage profiles
    pub profiles: Vec<StorageProfile>,
    /// Whether profiles are currently loading
    pub loading: bool,
    /// Error message if loading failed
    pub error: Option<String>,
}
