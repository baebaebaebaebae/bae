# bae

A music library manager that uses decentralized identity and end-to-end encryption over pluggable storage to enable multi-device sync, collaborative curation, and discovery.

You pick releases from MusicBrainz or Discogs, point bae at your files, and it handles storage, playback, and organization. Everything in the cloud is encrypted. The storage provider sees opaque blobs. All trust lives in cryptography, not in the storage backend.

## How it works

**Import and play.** Import from local folders (file-per-track or CUE/FLAC). Match to a MusicBrainz or Discogs release for metadata, cover art, credits, label info. Browse and play with native audio, CUE pregap support, and media key integration.

**Sync across devices.** Sign in with a cloud provider (Google Drive, Dropbox, OneDrive, pCloud) or configure an S3-compatible bucket. This creates your cloud home -- one encrypted location that holds everything. Your library syncs incrementally via changesets. Same user, multiple devices, one library.

**Share links.** Right-click a track, copy a share link, paste it anywhere. Recipients click and listen in their browser -- no account, no app needed.

**Follow.** A friend with bae can browse your full catalog as a read-only remote library. You generate a follow code, they paste it in. Streaming goes through your server, never touches the cloud home.

**Join.** For collaborators who want to contribute to the same library. Both people import music, edit metadata, and curate together. A two-step code exchange handles the join -- bae grants storage access, wraps the encryption key to the joiner's public key, and bundles everything into an invite code.

**Discovery.** Users who match releases to MusicBrainz/Discogs IDs create mappings from metadata to content hashes. These mappings are shared over a DHT, enabling decentralized search and download via BitTorrent. Off by default, opt-in per device and per release.

## Architecture

- **Identity**: each user has a locally generated Ed25519/X25519 keypair. Public keys are identities. No central identity server.
- **Encryption**: one symmetric key per library, shared by all members. Everything in the cloud home is encrypted before it leaves the device.
- **Storage**: pluggable via a `CloudHome` trait -- Google Drive, Dropbox, OneDrive, pCloud, iCloud Drive, any S3-compatible bucket, or local-only.
- **Sync**: SQLite session extension captures changesets automatically. Row-level last-writer-wins conflict resolution via hybrid logical clock. Deterministic merge.
- **Membership**: append-only chain of signed membership entries. Each changeset is signed by its author and verified against the membership chain on pull.
- **Subsonic API**: localhost:4533 for external clients (DSub, play:Sub, etc.)
- **bae-server**: headless read-only server that pulls from the cloud home, decrypts on the fly, and serves the API + web frontend.

## Crates

| Crate | Description |
|-------|-------------|
| `bae-core` | Library, database, sync engine, encryption, cloud backends, import pipeline |
| `bae-desktop` | Dioxus desktop app (macOS) |
| `bae-ui` | Pure UI components (compiles for wasm), no dependency on bae-core |
| `bae-server` | Headless server, syncs from cloud home |
| `bae-web` | Browser frontend for share pages and remote browsing |
| `bae-mocks` | Interactive mock panels for UI development |

## Roadmap

- LP mode -- pause at side breaks, "flip" to continue
- CD ripping with AccurateRip verification
- Torrent import with built-in client
- Shuffle
- Windows and Linux

## Development setup

macOS only for now. Requires Homebrew.

**Prerequisites:**

```bash
# Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Dioxus CLI
cargo install dioxus-cli --locked

# System libraries
brew install cmake pkg-config libdiscid
```

**Quick start:**

```bash
# Clone with submodules
git clone --recurse-submodules <repository-url>
cd bae

# Setup bae-ffmpeg (downloads prebuilt binaries)
./scripts/setup-ffmpeg.sh

# Add to your shell profile (~/.zshrc):
export FFMPEG_DIR="$PWD/bae-ffmpeg/dist"
export PKG_CONFIG_PATH="$FFMPEG_DIR/lib/pkgconfig:$PKG_CONFIG_PATH"
export LIBRARY_PATH="$FFMPEG_DIR/lib:$LIBRARY_PATH"
export DYLD_LIBRARY_PATH="$FFMPEG_DIR/lib:$DYLD_LIBRARY_PATH"

./scripts/install-hooks.sh
npm install

# Configure
cp .env.example .env
# Edit .env with your Discogs API key (from https://www.discogs.com/settings/developers)

# Run
cd bae-desktop && dx serve
```

Dev mode activates automatically when `.env` exists.

## Configuration

**Dev mode** (debug builds with `.env`): loads from `.env` file in repo root.

**Production mode** (release builds without `.env`): loads secrets from system keyring, settings from `~/.bae/config.yaml`.

## Logging

Log levels via `RUST_LOG`:

```bash
RUST_LOG=info dx serve              # General info (default)
RUST_LOG=debug dx serve             # Detailed debugging
RUST_LOG=bae=debug dx serve         # Debug only bae module
RUST_LOG=bae::import=debug dx serve # Debug specific submodule
```
