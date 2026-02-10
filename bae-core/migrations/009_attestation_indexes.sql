-- Prevent duplicate attestations from the same author for the same mbid+infohash.
CREATE UNIQUE INDEX IF NOT EXISTS idx_attestations_unique
    ON attestations (mbid, infohash, author_pubkey);

-- Support get_attestations_by_author queries.
CREATE INDEX IF NOT EXISTS idx_attestations_author_pubkey
    ON attestations (author_pubkey);
