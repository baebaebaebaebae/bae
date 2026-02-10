-- Store the unwrapped release key and S3 credentials after accepting a grant.
-- These are populated locally when the recipient decrypts the wrapped_payload.
ALTER TABLE share_grants ADD COLUMN release_key_hex TEXT;
ALTER TABLE share_grants ADD COLUMN s3_access_key TEXT;
ALTER TABLE share_grants ADD COLUMN s3_secret_key TEXT;
