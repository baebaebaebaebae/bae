use serde::{Deserialize, Serialize};

use crate::db::Database;

/// Controls how this library participates in the bae discovery network.
///
/// Off by default. Users opt in via settings. The three modes are:
/// - `Off`: no DHT announces, no attestation sharing.
/// - `AttestationsOnly`: share attestations (metadata) but don't seed files via BitTorrent.
/// - `Full`: share attestations and seed files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParticipationMode {
    Off,
    AttestationsOnly,
    Full,
}

/// Serde default function for config deserialization.
pub fn default_participation() -> ParticipationMode {
    ParticipationMode::Off
}

/// Decides whether network operations should proceed based on participation mode
/// and per-release privacy flags.
pub struct ParticipationService {
    mode: ParticipationMode,
    db: Database,
}

impl ParticipationService {
    pub fn new(mode: ParticipationMode, db: Database) -> Self {
        Self { mode, db }
    }

    /// Whether this library should share attestations at all.
    /// True for `AttestationsOnly` and `Full` modes.
    pub fn should_share_attestations(&self) -> bool {
        match self.mode {
            ParticipationMode::Off => false,
            ParticipationMode::AttestationsOnly | ParticipationMode::Full => true,
        }
    }

    /// Whether a specific release should be announced on the DHT and have its
    /// attestations shared. Returns false if:
    /// - Participation mode is `Off`
    /// - The release is marked as private
    pub async fn should_announce(&self, release_id: &str) -> Result<bool, sqlx::Error> {
        if self.mode == ParticipationMode::Off {
            return Ok(false);
        }

        let private = self.db.is_release_private(release_id).await?;
        Ok(!private)
    }

    /// Whether this library should seed files via BitTorrent for a given release.
    /// Only true in `Full` mode and when the release is not private.
    pub async fn should_seed(&self, release_id: &str) -> Result<bool, sqlx::Error> {
        if self.mode != ParticipationMode::Full {
            return Ok(false);
        }

        let private = self.db.is_release_private(release_id).await?;
        Ok(!private)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_db() -> Database {
        let tmp = tempfile::TempDir::new().unwrap();
        let db_path = tmp.path().join("test.db");
        // Leak the TempDir so it lives for the duration of the test
        let db = Database::new(db_path.to_str().unwrap()).await.unwrap();
        std::mem::forget(tmp);
        db
    }

    #[test]
    fn participation_mode_serde_roundtrip() {
        let modes = [
            (ParticipationMode::Off, "\"off\""),
            (ParticipationMode::AttestationsOnly, "\"attestations_only\""),
            (ParticipationMode::Full, "\"full\""),
        ];

        for (mode, expected_json) in modes {
            let json = serde_json::to_string(&mode).unwrap();
            assert_eq!(json, expected_json);

            let parsed: ParticipationMode = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, mode);
        }
    }

    #[test]
    fn participation_mode_yaml_roundtrip() {
        let yaml = "off\n";
        let mode: ParticipationMode = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(mode, ParticipationMode::Off);

        let yaml = "attestations_only\n";
        let mode: ParticipationMode = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(mode, ParticipationMode::AttestationsOnly);

        let yaml = "full\n";
        let mode: ParticipationMode = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(mode, ParticipationMode::Full);
    }

    #[tokio::test]
    async fn should_share_attestations_respects_mode() {
        let db = test_db().await;

        let svc = ParticipationService::new(ParticipationMode::Off, db.clone());
        assert!(!svc.should_share_attestations());

        let svc = ParticipationService::new(ParticipationMode::AttestationsOnly, db.clone());
        assert!(svc.should_share_attestations());

        let svc = ParticipationService::new(ParticipationMode::Full, db);
        assert!(svc.should_share_attestations());
    }

    #[tokio::test]
    async fn should_announce_returns_false_when_off() {
        let db = test_db().await;
        let svc = ParticipationService::new(ParticipationMode::Off, db);
        assert!(!svc.should_announce("any-release-id").await.unwrap());
    }

    #[tokio::test]
    async fn should_announce_returns_true_for_non_private_release() {
        let db = test_db().await;

        let album = crate::db::DbAlbum::new_test("Test Album");
        let release = crate::db::DbRelease::new_test(&album.id, "rel-1");
        db.insert_album_with_release_and_tracks(&album, &release, &[])
            .await
            .unwrap();

        let svc = ParticipationService::new(ParticipationMode::Full, db);
        assert!(svc.should_announce("rel-1").await.unwrap());
    }

    #[tokio::test]
    async fn should_announce_returns_false_for_private_release() {
        let db = test_db().await;

        let album = crate::db::DbAlbum::new_test("Test Album");
        let release = crate::db::DbRelease::new_test(&album.id, "rel-1");
        db.insert_album_with_release_and_tracks(&album, &release, &[])
            .await
            .unwrap();
        db.set_release_private("rel-1", true).await.unwrap();

        let svc = ParticipationService::new(ParticipationMode::AttestationsOnly, db);
        assert!(!svc.should_announce("rel-1").await.unwrap());
    }

    #[tokio::test]
    async fn should_seed_only_in_full_mode() {
        let db = test_db().await;

        let album = crate::db::DbAlbum::new_test("Test Album");
        let release = crate::db::DbRelease::new_test(&album.id, "rel-1");
        db.insert_album_with_release_and_tracks(&album, &release, &[])
            .await
            .unwrap();

        // AttestationsOnly mode: no seeding
        let svc = ParticipationService::new(ParticipationMode::AttestationsOnly, db.clone());
        assert!(!svc.should_seed("rel-1").await.unwrap());

        // Full mode: seeding allowed
        let svc = ParticipationService::new(ParticipationMode::Full, db.clone());
        assert!(svc.should_seed("rel-1").await.unwrap());

        // Full mode but private release: no seeding
        db.set_release_private("rel-1", true).await.unwrap();
        assert!(!svc.should_seed("rel-1").await.unwrap());
    }

    #[tokio::test]
    async fn should_announce_nonexistent_release_is_not_private() {
        let db = test_db().await;
        let svc = ParticipationService::new(ParticipationMode::Full, db);
        // Non-existent release: is_release_private returns false
        assert!(svc.should_announce("nonexistent").await.unwrap());
    }
}
