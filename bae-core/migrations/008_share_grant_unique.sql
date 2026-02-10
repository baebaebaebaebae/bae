-- Prevent accepting the same grant twice (same sender + release).
CREATE UNIQUE INDEX IF NOT EXISTS idx_share_grants_unique_grant
    ON share_grants (from_library_id, release_id, from_user_pubkey);
