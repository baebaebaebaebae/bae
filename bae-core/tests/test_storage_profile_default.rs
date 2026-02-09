//! Test that only one storage profile can be the default at a time.

use bae_core::db::{Database, DbStorageProfile};
use tempfile::TempDir;

#[tokio::test]
async fn test_only_one_default_profile() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let database = Database::new(db_path.to_str().unwrap()).await.unwrap();

    // Insert first profile as default
    let profile_a = DbStorageProfile::new_local("Profile A", "/tmp/a", false).with_default(true);
    let profile_a_id = profile_a.id.clone();
    database.insert_storage_profile(&profile_a).await.unwrap();

    let default = database
        .get_default_storage_profile()
        .await
        .unwrap()
        .unwrap();
    assert_eq!(default.id, profile_a_id);

    // Insert second profile as default -- should clear first
    let profile_b = DbStorageProfile::new_local("Profile B", "/tmp/b", false).with_default(true);
    let profile_b_id = profile_b.id.clone();
    database.insert_storage_profile(&profile_b).await.unwrap();

    let all = database.get_all_storage_profiles().await.unwrap();
    let defaults: Vec<_> = all.iter().filter(|p| p.is_default).collect();
    assert_eq!(
        defaults.len(),
        1,
        "expected exactly one default after inserting second default profile"
    );
    assert_eq!(defaults[0].id, profile_b_id);

    // Insert a third non-default profile
    let profile_c = DbStorageProfile::new_local("Profile C", "/tmp/c", false);
    let profile_c_id = profile_c.id.clone();
    database.insert_storage_profile(&profile_c).await.unwrap();

    // Update third profile to be default -- should clear second
    let mut profile_c_updated = database
        .get_storage_profile(&profile_c_id)
        .await
        .unwrap()
        .unwrap();
    profile_c_updated.is_default = true;
    database
        .update_storage_profile(&profile_c_updated)
        .await
        .unwrap();

    let all = database.get_all_storage_profiles().await.unwrap();
    let defaults: Vec<_> = all.iter().filter(|p| p.is_default).collect();
    assert_eq!(
        defaults.len(),
        1,
        "expected exactly one default after updating third profile to default"
    );
    assert_eq!(defaults[0].id, profile_c_id);
}
