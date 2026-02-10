//! Push-related types for the sync system.
//!
//! The actual push orchestration happens in `SyncService::sync()`, which
//! returns an `OutgoingChangeset` for the caller to encrypt and upload.
//! This module holds the shared types and the schema version constant.

/// Current schema version -- matches the latest migration number.
pub const SCHEMA_VERSION: u32 = 3;

/// An outgoing changeset ready to be pushed to the sync bucket.
pub struct OutgoingChangeset {
    /// The packed envelope + changeset bytes (plaintext, ready for encryption).
    pub packed: Vec<u8>,
    /// The sequence number for this changeset.
    pub seq: u64,
}
