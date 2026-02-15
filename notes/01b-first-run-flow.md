# First-Run Flows

## New library

On first run (no `~/.bae/active-library`), desktop shows a welcome screen. User picks "Create new library":

1. Generate a library UUID (e.g., `lib-111`)
2. Create `~/.bae/libraries/lib-111/`
3. Create empty `library.db`
4. Write `config.yaml`, write `~/.bae/active-library` -> `lib-111`
5. Re-exec binary -- desktop launches normally

`storage/` is empty -- user imports their first album, files go into `storage/ab/cd/{file_id}`.

## Restore from cloud home

User picks "Restore from cloud home" and provides cloud home credentials + encryption key:

1. Download + decrypt `snapshot.db.enc` (validates the key -- if decryption fails, wrong key)
2. Create `~/.bae/libraries/{library_id}/`
3. Write `config.yaml` (with cloud home config), keyring entries, `~/.bae/active-library` -> `{library_id}`
4. Download images from the cloud home
5. Pull and apply any changesets newer than the snapshot
6. Re-exec binary

Local `storage/` is empty -- release files stream from the cloud home. The user can optionally download files locally for offline playback.

## Going from local to cloud

1. User signs in with a cloud provider (OAuth) or enters S3 credentials
2. bae creates the cloud home folder/bucket (or uses an existing one)
3. bae generates encryption key if one doesn't exist, stores in keyring
4. bae pushes a full snapshot + all images + release files to the cloud home
5. Subsequent mutations push incremental changesets
6. Another device can now join from the cloud home
