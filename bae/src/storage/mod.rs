//! Storage abstraction layer
//!
//! Provides flexible storage options for releases. Storage is configured via
//! StorageProfile (location + encrypted + chunked flags) and implemented by
//! a single ReleaseStorageImpl that applies transforms based on the profile.
mod traits;
pub use traits::{ReleaseStorage, ReleaseStorageImpl};
