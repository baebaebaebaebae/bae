CREATE TABLE dht_announcements (
    release_id TEXT PRIMARY KEY,
    mbid TEXT NOT NULL,
    rendezvous_key TEXT NOT NULL,
    last_announced_at TEXT,
    enabled BOOLEAN NOT NULL DEFAULT 1
);
