-- Per-release privacy flag for discovery network participation controls.
-- When true, the release is excluded from DHT announces and attestation sharing.
ALTER TABLE releases ADD COLUMN private BOOLEAN NOT NULL DEFAULT 0;
