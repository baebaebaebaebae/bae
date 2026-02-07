# bae

A desktop music library app. Album-oriented, metadata-first: you pick releases from Discogs or MusicBrainz, point bae at your files, and it handles storage, playback, and organization.

## Features

**Import**
- Local folders (file-per-track or CUE/FLAC)
- Metadata-driven: select a Discogs or MusicBrainz release, bae matches your files and pulls album art, credits, label info, catalog numbers

**Playback**
- Native audio via cpal
- CUE/FLAC pregap support — plays tracks with original pregaps intact
- Repeat modes (track, album), queue management, volume control
- macOS media key support
- Subsonic 1.16.1 API on localhost:4533 for external clients (DSub, play:Sub, etc.)

**Storage**
- Local filesystem or S3-compatible cloud (AWS, Backblaze B2, etc.)
- Optional AES-GCM encryption
- Storage profiles for different destinations

**Metadata**
- MusicBrainz with DiscID matching from CUE sheets
- Discogs search and matching
- Cover art from local files, MusicBrainz, or Discogs
- Album art cached locally for fast browsing

## Roadmap

- LP mode — pause at side breaks, "flip" to continue
- CD ripping with AccurateRip verification
- Torrent import with built-in client
- Shuffle
- Windows & Linux

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
