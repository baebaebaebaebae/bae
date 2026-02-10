CREATE TABLE IF NOT EXISTS attestations (
    id TEXT PRIMARY KEY,
    mbid TEXT NOT NULL,
    infohash TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    format TEXT NOT NULL,
    author_pubkey TEXT NOT NULL,
    timestamp TEXT NOT NULL,
    signature TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_attestations_mbid ON attestations(mbid);
CREATE INDEX idx_attestations_infohash ON attestations(infohash);
CREATE INDEX idx_attestations_content_hash ON attestations(content_hash);
