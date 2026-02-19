# App Layout

This document describes the top-level layout shared across all desktop/native apps (Dioxus desktop, macOS, Windows, Linux). Mobile apps (iOS, Android) have their own navigation patterns.

## Two Persistent Sections

The app has two top-level sections: **Library** and **Import**. These are not routes or tabs in the platform sense — they're two views that both stay alive in memory, with only one visible at a time.

```
+----------------------------------------------+
| [Library] [Import]           [+] [sync] [gear]|
+----------------------------------------------+
|                                              |
|          (active section content)            |
|                                              |
+----------------------------------------------+
|  Now Playing Bar                             |
+----------------------------------------------+
```

### Why both stay alive

Import is a multi-step workflow: scan folder, search metadata, pick a match, import. If switching to Library destroyed the import view, the user would lose their search results and selection. Both sections share the same backing state (AppService / AppState), but each section also has local view state (selected artist, selected candidate, search results) that must survive section switches.

### Implementation pattern

Use whatever the platform's equivalent of "keep both views mounted but show one" is:

- **Dioxus (bae-desktop):** Router with separate routes (`/` for Library, `/import` for Import). Dioxus preserves component state across route changes via the global Store.
- **SwiftUI (macOS):** ZStack with opacity + allowsHitTesting. Both views exist simultaneously; the inactive one is invisible and non-interactive.
- **Future platforms:** Same idea — don't destroy/recreate views on section switch.

## Section Bar

The section bar sits at the top of the window, above the content area. It contains:

**Left side:**
- Library button (highlighted when active)
- Import button (highlighted when active, shows badge with count of in-progress imports)

**Right side:**
- Import folder button (opens file picker, scans folder, switches to Import)
- Sync button (opens sync settings)
- Settings button (opens settings)

The buttons are simple clickable labels, not a tab bar or sidebar. They look like small pill-shaped buttons with an active/inactive state.

## Library Section

Three-column layout:
1. **Artist sidebar** — list of artists, "All" at the top
2. **Album grid** — filtered by selected artist, with cover art thumbnails
3. **Album detail** — tracks, metadata, cover, storage info, share link

Search is available via a search field (either in the section bar or native to the platform's navigation pattern). Search shows results across artists, albums, and tracks.

## Import Section

Two-column layout:
1. **Candidate list** — scanned folders with status indicators (pending, importing, done, error)
2. **Metadata search** — search MusicBrainz/Discogs for the selected candidate, pick a match, import

The import section starts empty. Scanning a folder (via the "+" button, Cmd+I, or drag-and-drop) populates the candidate list and auto-switches to the Import section.

## Now Playing Bar

Fixed at the bottom, visible in both sections. Shows current track, artist, progress, playback controls, volume, repeat mode.

## Global Handlers

These live at the top level (above both sections):

- **Keyboard:** Space = play/pause
- **Menu:** Cmd+I = import folder (scans + switches to Import)
- **Drag and drop:** Dropping a folder anywhere = scan + switch to Import

## State Ownership

All persistent state lives in the shared service layer (AppService in Swift, AppState store in Dioxus). The section bar and global handlers live in the top-level container view. Each section reads from the shared state but also maintains its own local view state (selection, scroll position, search text).

| State | Owner | Persists across switches? |
|-------|-------|--------------------------|
| Albums, artists | Shared service | Yes |
| Scan results, import statuses | Shared service | Yes |
| Playback state | Shared service | Yes |
| Selected artist, selected album | Library view local | Yes (view stays alive) |
| Selected candidate, search results | Import view local | Yes (view stays alive) |
| Active section | Top-level container | Yes |
