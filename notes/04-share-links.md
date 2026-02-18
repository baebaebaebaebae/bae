# Share Links

Share a release with anyone via a URL. The recipient opens the link in their browser, where all decryption happens client-side. The server never sees plaintext audio or metadata.

## Architecture

### URL format

```
{share_base_url}/share/{share_id}#{base64url(per_share_key)}
```

- **share_id** -- UUID identifying the share in cloud storage
- **per_share_key** -- random 32-byte key that decrypts the share metadata, encoded as base64url in the URL fragment

The fragment (`#...`) is never sent to the server. The server only sees the share_id.

### Cloud storage layout

Each share creates two objects in the cloud home:

```
shares/{share_id}/meta.enc          -- ShareMeta JSON, encrypted with per_share_key
shares/{share_id}/manifest.json     -- ShareManifest JSON, unencrypted (lists allowed file keys)
```

**ShareMeta** contains: album name, artist, year, tracks (title, number, duration, file_key, format), cover_image_key, and the base64-encoded per-release encryption key.

**ShareManifest** lists the S3 keys (audio files + cover image) that bae-proxy may serve for this share.

Audio files and cover images are stored encrypted with per-release keys at their normal cloud home paths (`storage/{ab}/{cd}/{file_id}`, `images/{ab}/{cd}/{image_id}`). The share metadata includes the per-release key so the browser can decrypt them.

### Generation flow (bae-desktop)

1. User clicks "Copy Link" on a cloud-managed release
2. Desktop generates a random share_id (UUID) and per_share_key (32 bytes)
3. Gathers album metadata, track info, file keys from database
4. Derives per-release encryption key via `derive_release_encryption(release_id)`
5. Builds ShareMeta JSON and encrypts it with per_share_key
6. Builds ShareManifest JSON listing all file keys
7. Uploads `meta.enc` and `manifest.json` to cloud home
8. Constructs URL and copies to clipboard

### Playback flow (bae-web)

1. Recipient opens the link in a browser
2. bae-web reads the URL fragment (per_share_key) -- never sent to server
3. Fetches `meta.enc` from bae-proxy via `/share/{share_id}/meta`
4. Decrypts ShareMeta with per_share_key (client-side, using XChaCha20-Poly1305)
5. Extracts the per-release key from the decrypted metadata
6. Fetches encrypted audio/image files from bae-proxy via `/share/{share_id}/file/{file_key}`
7. Decrypts each file client-side with the per-release key
8. Creates blob URLs for playback and display

### Deployment

The share URL requires:
- A **bae-proxy** instance that can read from the same cloud home (S3 bucket, etc.)
- The `share_base_url` configured in bae-desktop settings (Settings > Subsonic > Share Links)

bae-proxy validates that requested file keys appear in the share's manifest before serving them.

### Security properties

- **Zero-knowledge server**: The server stores and serves encrypted blobs. It never has access to the per_share_key (URL fragment) or per-release key.
- **Possession = access**: Anyone with the full URL (including fragment) can decrypt and play the share.
- **No expiry**: Shares persist as long as the cloud storage objects exist and the release files remain in the cloud home.
- **Revocation**: Delete the `shares/{share_id}/` prefix from cloud storage.
