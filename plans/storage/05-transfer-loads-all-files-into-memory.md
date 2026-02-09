# Transfer loads all files into memory

## Problem

The transfer service reads every file of a release into memory before writing any of them to the destination.

## What happens today

In `bae-core/src/storage/transfer.rs:187-216`, `do_transfer()` collects all file data into a `Vec<(String, Vec<u8>)>`:

```rust
let mut file_data: Vec<(String, Vec<u8>)> = Vec::with_capacity(old_files.len());
for (i, file) in old_files.iter().enumerate() {
    let raw_data = read_file_data(file, source_reader.as_ref()).await?;
    let data = if source_encrypted { enc.decrypt(&raw_data)? } else { raw_data };
    file_data.push((file.original_filename.clone(), data));
}
```

Then it writes them all in a second loop. For a typical FLAC album (400-700 MB), this means holding the entire release in memory. For encrypted sources, there's a transient doubling during decrypt (encrypted + decrypted copies overlap briefly).

## Why it's done this way

The current code deletes old file records before writing new ones (`delete_files_for_release` + `delete_release_storage` at line 222-223). If writing happened interleaved with reading, a failure mid-transfer could leave the release in a half-migrated state with some file records deleted and new ones partially written.

## Fix

Stream file-by-file: read one file, write it to destination, then move to the next. The atomicity concern can be handled by:

1. Writing all new files first (with temporary `source_path` values or into a staging area)
2. Updating the DB records in a single transaction (delete old file rows, insert new ones, update `release_storage`)
3. Queuing old files for deferred deletion (which already exists via `pending_deletions.json`)

This keeps peak memory at one file at a time (~30-50 MB for a single FLAC track).
