# PR 2: Library directory restructuring

**Branch:** `encryption-ux` (continuing from PR 1)

## Goal

`~/.bae/libraries/<library-id>/` replaces flat `~/.bae/`. No migration — clean break.

## Target layout

```
~/.bae/
├── library                          # pointer file (always exists, contains path)
└── libraries/
    └── <library-id>/
        ├── config.yaml
        └── library.db
```

## Changes

### `bae-core/src/config.rs`

**`Config.library_path: PathBuf`** (was `Option<PathBuf>`)

- Remove `get_library_path()` — use `self.library_path` directly
- Remove `set_library_path()` — direct field assignment

**`from_config_file()`** — resolve library path:
1. Pointer file exists → use it
2. No pointer file → generate ID, path = `~/.bae/libraries/<id>/`, write pointer

**`save_library_path()`** — drop the `Option` unwrap, just write `self.library_path`

**`save_to_config_yaml()`** — `self.library_path` instead of `self.get_library_path()`

**`from_env()`** — `BAE_LIBRARY_PATH` env var or default `~/.bae/libraries/<generated-id>/`

### `bae-desktop/src/main.rs`

- `create_database()` — `config.library_path` instead of `config.get_library_path()`

### `bae-desktop/src/ui/window_activation/macos_window.rs`

- Menu handler: `config.library_path = path` instead of `config.set_library_path(path)`

## Verification

- `cargo clippy -p bae-desktop && cargo clippy -p bae-mocks` clean
- `cargo test -p bae-core` passes
- Run app: creates `~/.bae/libraries/<id>/` with config.yaml and library.db
