# Share Links

Share a track or album with anyone via a URL. Whoever has the link can play the track in their browser — no account, no app. The share page also lets recipients download the track, or save it directly to their own bae library.

## Architecture

### The share link

```
https://music.example.com/share/{token}
```

The token is a signed blob encoding the resource reference and access constraints:

```
base64url(track_id + expiry + HMAC(secret, track_id + expiry))
```

- **track_id** (or album_id) — what to play
- **expiry** — optional timestamp after which the link stops working
- **HMAC signature** — server-side secret prevents forging or tampering

The token is the auth. Possession of the URL = permission to play that resource. No credentials needed.

### Flow

**Generation (desktop):**

1. User clicks "Share" on a track in bae-desktop
2. Desktop generates the token using a local secret, constructs the URL using the configured `share_base_url`
3. User copies the link and sends it

**Playback (browser):**

1. Recipient opens the link
2. Server decodes the token, validates the HMAC, checks expiry
3. Serves a minimal HTML page — cover art, track title, artist, play button
4. The `<audio>` element points to `/rest/stream?id={track_id}&shareToken={token}`
5. Server validates the token again on the stream request, decrypts the file if needed, streams audio

### Deployment paths

**Desktop + custom domain:** User runs bae-desktop, reverse-proxies with their domain to port 4533. Share link = `https://your-domain.com/share/{token}`.

**bae-server in the cloud:** Operator runs `bae-server --s3-bucket ... --recovery-key ... --web-dir ./bae-web/dist --bind 0.0.0.0`. Same share link format, same token validation.

Both paths use the same API and bae-web frontend.

Both require the server to be reachable from the internet. For desktop users without a static IP or custom domain, options include Tailscale Funnel, ngrok, Cloudflare Tunnel, or UPnP port forwarding.