# App Layout

This document describes the top-level layout shared across all desktop/native apps (Dioxus desktop, macOS, Windows, Linux). Mobile apps (iOS, Android) have their own navigation patterns.

## Two Persistent Sections

The app has two top-level sections: **Library** and **Import**. These are not routes or tabs in the platform sense — they're two views that both stay alive in memory, with only one visible at a time.

```
+----------------------------------------------+
|  [Library | Import]       [sync] [gear] [🔍] |
+----------------------------------------------+
|                                              |
|          (active section content)            |
|                                              |
+----------------------------------------------+
|  Now Playing Bar                             |
+----------------------------------------------+
```

### Why both stay alive

Import is a multi-step workflow: scan folder, search metadata, pick a match, import. If switching to Library destroyed the import view, the user would lose their search results and selection. Both sections share the same backing state (AppService / AppState), but each section also has local view state (selected candidate, search results) that must survive section switches.

### Implementation pattern

Use whatever the platform's equivalent of "keep both views mounted but show one" is:

- **Dioxus (bae-desktop):** Router with separate routes (`/` for Library, `/import` for Import). Dioxus preserves component state across route changes via the global Store.
- **SwiftUI (macOS):** ZStack with opacity + allowsHitTesting. Both views exist simultaneously; the inactive one is invisible and non-interactive.
- **Future platforms:** Same idea — don't destroy/recreate views on section switch.

## Window Toolbar

The section switcher and action buttons live in the window's native toolbar (title bar area), not in a custom bar below it.

**Center:**
- Segmented control: Library / Import

**Trailing:**
- Sync button (opens sync settings sheet)
- Settings button (opens settings sheet)
- Search field (platform search, filters library content)

No "Import Folder" button in the toolbar. Importing is done via:
- Cmd+I menu shortcut
- Drag-and-drop folders onto the window
- The "+" button shown in the Import section's empty state

## Library Section

Flat album grid. No artist sidebar — search handles filtering.

- **Album grid** — all albums, with cover art thumbnails. Click to open detail, double-click to play.
- **Album detail** — opens as a sheet/modal. Shows tracks, metadata, cover art, storage info, share link.
- **Search** — filters across artists, albums, and tracks. Results replace the grid while typing.

### Platform variations

- **bae-desktop (Dioxus):** Three-column layout with artist sidebar, album grid, album detail. This works well with Dioxus's layout system.
- **macOS (SwiftUI):** Flat grid + sheet detail. NavigationSplitView was rejected — it wastes space and has poor UX for this use case.
- **Future platforms:** Choose whichever pattern works best natively. The artist sidebar is optional — search covers the same use case.

## Import Section

### Empty state

When no folders have been scanned, show a centered "+" button with prompt text. No panes.

### Active state (folders scanned)

Two-column layout:

1. **Candidate sidebar** — scanned folders presented as folder items (folder icon + folder basename). Status indicators: pending (folder icon), importing (spinner), done (checkmark), incomplete (dimmed + warning text). Incomplete folders are not selectable.

2. **Main content area** — for the selected candidate:
   - **Header** — folder name
   - **File display** — categorized files grouped as Audio, Images, Documents (collapsible sections). Shows file names and sizes. Images help identify the release (catalog numbers on spines, label logos, disc art).
   - **Search form** — tabbed: General (artist + album), Catalog Number, Barcode. Each tab has the appropriate fields and a search button.
   - **Search results** — list of metadata matches from MusicBrainz/Discogs. Each result shows title, artist, year, format, label. "Import" button per result.

The import section starts empty. Scanning a folder (via Cmd+I, drag-and-drop, or the empty state button) populates the candidate list and auto-switches to the Import section.

## Now Playing Bar

Fixed at the bottom, visible in both sections. Shows current track, artist, progress, playback controls, volume, repeat mode.

## Global Handlers

These live at the top level (above both sections):

- **Keyboard:** Space = play/pause
- **Menu:** Cmd+I = import folder (scans + switches to Import)
- **Drag and drop:** Dropping a folder anywhere = scan + switch to Import

## State Ownership

All persistent state lives in the shared service layer (AppService in Swift, AppState store in Dioxus). The section switcher and global handlers live in the top-level container view. Each section reads from the shared state but also maintains its own local view state.

| State | Owner | Persists across switches? |
|-------|-------|--------------------------|
| Albums, artists | Shared service | Yes |
| Scan results, import statuses | Shared service | Yes |
| Playback state | Shared service | Yes |
| Selected album, search text | Library view local | Yes (view stays alive) |
| Selected candidate, search results, search tab | Import view local | Yes (view stays alive) |
| Active section | Top-level container | Yes |
