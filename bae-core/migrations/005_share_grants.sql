CREATE TABLE IF NOT EXISTS share_grants (
    id TEXT PRIMARY KEY,
    from_library_id TEXT NOT NULL,
    from_user_pubkey TEXT NOT NULL,
    release_id TEXT NOT NULL,
    bucket TEXT NOT NULL,
    region TEXT NOT NULL,
    endpoint TEXT,
    wrapped_payload BLOB NOT NULL,
    expires TEXT,
    signature TEXT NOT NULL,
    accepted_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
