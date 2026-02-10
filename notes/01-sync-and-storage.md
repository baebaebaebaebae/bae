# Sync & Storage

bae starts local, scales to cloud, then to collaboration and a decentralized network. Progressive complexity -- each layer is independently useful, and you never pay for capabilities you don't use.

## Tiers

### Tier 1: Local (no setup)

- Install bae, import music from folders/CDs
- Files stored locally, plain SQLite DB, no encryption, no key
- Library lives at `~/.bae/`

### Tier 2: Cloud (two decisions)

Two independent capabilities -- they can be enabled separately or together:

**Sync (multi-device):** User configures a sync bucket (S3 credentials + bucket). bae generates an encryption key and stores it in the OS keyring. The sync bucket gets changesets, snapshots, and images -- everything needed for another device to join.

**Cloud file storage:** User creates a cloud storage profile (S3 credentials + bucket). Release files can be transferred there. This is separate from sync -- it's just a place to put files. The sync bucket itself can also serve as file storage (simplest setup: one bucket for everything).

- On macOS, iCloud Keychain syncs the encryption key to other devices automatically
- Files encrypt on upload, images encrypt in the sync bucket, DB snapshots encrypted for bootstrap
- The only decisions were "I want sync" and/or "I want cloud storage." Encryption followed automatically.
- The user never typed an encryption key. They might not even know they have one.

### Tier 3: Power user

- Multiple file storage profiles (different buckets, local + cloud mix)
  - e.g., fast S3 bucket for music you listen to often, cheap archival storage (S3 Glacier, Backblaze B2) for stuff you rarely access
- Export/import encryption key manually
- Run bae-server pointing at the sync bucket
- Key fingerprint visible in settings for verification

## What a Library Is

Desktop manages all libraries under `~/.bae/libraries/`. Each library is a directory:

```
~/.bae/
  active-library               # UUID of the active library
  libraries/
    {uuid}/                    # one directory per library
```

On first launch, bae creates the library home. The library home has a `storage_profiles` row in the DB (for file storage -- it holds release files like any other profile).

**`config.yaml`** -- device-specific settings (torrent ports, subsonic config, keyring hint flags, sync bucket configuration, device_id). Not synced. Only at the library home.

| Data | Tier 1 (local) | Tier 2+ (cloud) |
|------|----------------|-----------------|
| library.db | Plain SQLite | Plain locally, encrypted snapshot in sync bucket |
| Cover art | Plaintext | Encrypted in sync bucket |
| Release files | On their profile | Encrypted on cloud profiles |
| Encryption key | N/A | OS keyring (iCloud Keychain) |
| config.yaml | Local | Local (device-specific, not synced) |

Each library owns its buckets and directories exclusively -- no sharing between libraries.

## Encryption

One key per library. The key belongs to the library, not to individual storage profiles or the sync bucket. Everything that goes to cloud gets encrypted with it.

**Key fingerprint:** SHA-256 of the key, truncated. Stored in `config.yaml`. Lets us detect the wrong key immediately instead of silently producing garbage.

You shouldn't have to think about encryption, keys, or cloud storage until the moment you want cloud. And when you do, encryption just happens -- it's not a feature you configure, it's a consequence of going cloud.

## Sync

The design has four layers, each building on the previous:

1. **Changeset sync** -- incremental sync via SQLite session extension changesets pushed to a shared sync bucket
2. **Shared libraries** -- multiple users writing to the same library with signed changesets and a membership chain
3. **Cross-library sharing** -- share individual releases between libraries using derived encryption keys
4. **Public discovery** -- a decentralized MBID-to-content mapping via the BitTorrent DHT

Each layer is independently useful. A solo user benefits from layer 1. Friends sharing a library use layers 1-2. Sharing a single album uses layer 3. The public network is layer 4.

### Changeset sync (layer 1)

Use the SQLite session extension to capture exactly what changed, push the binary changeset to the sync bucket, pull and apply on other devices with a conflict handler. No coordination server. The sync bucket is a library-level concept -- one S3 bucket per library, configured in `config.yaml`, separate from storage profiles (which just hold release files).

Each device writes to its own keyspace on the shared bucket. No write contention by construction:

```
s3://sync-bucket/
  snapshot.db.enc                  # full DB for bootstrapping new devices
  changes/{device_id}/{seq}.enc    # changeset blobs per device
  heads/{device_id}.json.enc       # "my latest seq is 42"
  images/ab/cd/{id}                # all library images (encrypted)
  storage/ab/cd/{file_id}          # release files (optional -- bucket can double as file storage)
```

**Push** = grab changeset from the session, encrypt, upload to `changes/{your_device}/`, update `heads/{your_device}`.

**Pull** = list `heads/`, compare each device's seq to your local cursors. If anyone's ahead, fetch their new changesets, apply with conflict handler. Deterministic -- same changesets in the same order produce the same result.

**Polling is cheap** -- listing `heads/` is one S3 LIST call. If all seqs match your cursors, nothing to do. Check on app open + periodic timer.

#### Why the session extension

We considered and rejected several alternatives:

- **Op log with method wrapping** -- requires intercepting ~63 write methods, maintaining a custom JSON op format, and building a custom merge algorithm
- **SQLite triggers** -- same maintenance burden moved to SQL; must enumerate every column of every synced table
- **CRDTs (Automerge/Loro)** -- hold full state in memory, don't scale to arbitrary entity counts
- **cr-sqlite** -- stalled project, too risky as a dependency

The session extension is built into SQLite. It tracks all changes (INSERT/UPDATE/DELETE) automatically at the C level. No triggers, no method wrapping, no column enumeration. The app writes normally. SQLite records what changed. We grab the binary changeset and push it.

#### Changesets, not operations

The session extension produces a compact binary changeset that represents the diff between the database before and after. A changeset contains only the rows and columns that actually changed.

For an import that creates an album + release + 12 tracks + 12 files, a JSON op log approach would generate ~50 operations. The session extension produces a single binary changeset blob. Smaller, faster, and no custom serialization.

#### Conflict resolution: row-level LWW

When two devices change the same row, the later `_updated_at` timestamp wins. Every synced table has an `_updated_at` column maintained by a Hybrid Logical Clock (HLC).

The session extension's `sqlite3changeset_apply()` calls a conflict handler for each conflicting operation. The handler compares `_updated_at` and returns REPLACE (accept incoming) or OMIT (keep local).

**Crucially, non-conflicting edits to different columns on the same row both survive.** A changeset for an UPDATE contains only the columns that changed. When we REPLACE, only those columns are overwritten -- the rest keep their local values.

**Example -- no conflict (different columns):**
```
Alice: edits title on rel-123 at T1
Bob:   edits year  on rel-123 at T2
Result: both title and year changes survive
```

**Example -- conflict (same column):**
```
Alice: edits title on rel-123 at T1
Bob:   edits title on rel-123 at T2
Result: Bob's title wins (later _updated_at)
```

**Example -- delete vs. edit:**
```
Bob:   edits genre on rel-123 at T1
Alice: deletes rel-123 at T2
Result: rel-123 is deleted (delete wins)
```

For a music library this is fine -- conflicts are rare and low-stakes. Worst case someone re-edits a field.

#### The sync protocol

```
1. Start session (attach synced tables)
2. App writes normally...
3. Time to sync:
   a. Grab changeset from session
   b. End session
   c. Push changeset to S3
   d. Pull incoming changesets (NO session active)
   e. Apply incoming with conflict handler
   f. Start new session
```

**Key rule:** Never apply someone else's changeset while your session is recording. Otherwise your next outgoing changeset contains their changes as duplicates.

#### Sync triggers

- After `LibraryEvent::AlbumsChanged` (import, delete) with debounce
- Manual "Sync Now" button in settings
- If the sync bucket is unreachable, sync is skipped and retried next time

#### Database architecture

The session extension attaches to a single connection and only captures changes made through that connection. The `Database` struct is refactored to use a dedicated write connection (with session attached) and a read pool. Write methods use the dedicated connection; read methods use the pool. This matches SQLite's single-writer-multiple-reader architecture.

#### Schema evolution

The SQLite session extension identifies columns by index, not by name. This constrains how the schema can change once changeset sync is live.

**Additive changes are transparent.** Adding columns at the end of a table or adding new tables requires no coordination. Old changesets applied to a new schema just have fewer columns (extras keep defaults). New changesets applied to an old schema skip unknown columns. Devices on different schema versions interoperate seamlessly.

**Breaking changes require coordination.** Deleting, reordering, or renaming columns shifts column indices and corrupts changeset application. These changes bump a `min_schema_version` marker in the sync bucket, splitting the changeset history into **epochs**. Within an epoch, all changesets are schema-compatible. Across epochs, no replay -- devices pull a fresh snapshot to jump forward. This means any schema change is possible (the snapshot IS the migrated state), but all devices must upgrade before syncing resumes.

Every changeset envelope carries a `schema_version` integer so receivers know what schema produced it. In practice, schema changes for a music library are almost always additive (new fields), making breaking migrations rare. See `plans/sync-and-network/roadmap.md` for the full protocol.

#### Snapshots

The changeset log grows forever without intervention. Periodically, any device writes a snapshot -- a full DB `VACUUM INTO`:

```
snapshot.db.enc   # overwritten each time
```

New devices start from the snapshot, then replay only changesets after it. Old changesets can be garbage collected after a grace period (30 days).

#### What this replaces

The `MetadataReplicator` -- which pushed a full `VACUUM INTO` snapshot plus all images to every non-home profile on every mutation -- is eliminated entirely. It is not reduced to local-only; it is removed. Sync goes through the single sync bucket. Storage profiles (including external drives) hold release files only -- no DB, no images, no manifest.

### Shared libraries (layer 2)

Currently a library has one writer (desktop). Adding users -- multiple people reading and writing the same library -- requires identity, authorization, and a trust model.

#### Identity = a keypair

Each user generates a keypair locally (Ed25519 for signing, X25519 for encryption). No accounts, no server, no signup. Your public key is your identity. The keypair is global (not per-library) so attestations in layer 4 accumulate under one identity.

#### Bucket layout with users

```
s3://sync-bucket/
  membership/{pubkey}/{seq}.enc     # signed membership entries (per author, avoids S3 overwrite races)
  keys/{user_pubkey}.enc            # library key wrapped to each member's public key
  heads/{device_id}.json.enc        # per-device head pointer (unchanged from layer 1)
  changes/{device_id}/{seq}.enc     # per-device changeset stream (signed)
  snapshot.db.enc
  images/ab/cd/{id}
  storage/ab/cd/{file_id}
```

Changesets stay keyed by device_id (a user may have multiple devices). Authorship is established cryptographically: each changeset envelope includes `author_pubkey` and a signature over the changeset bytes.

#### Membership chain

An append-only log of membership changes, stored as individual files to avoid S3 overwrite races. Each entry is signed by an owner.

```json
{ "action": "add", "user_pubkey": "...", "role": "owner",
  "ts": "2026-01-01T...", "author_pubkey": "...", "sig": "..." }
```

On read, clients download all membership entries, order by timestamp, and validate the chain.

#### Invitation flow

```
Owner invites Alice:
  1. Alice generates a keypair, sends her public key to the owner
  2. Owner wraps the library encryption key to Alice's public key
     -> uploads keys/alice.enc
  3. Owner writes membership entry: { action: "add", user: alice }
  4. Gives Alice the bucket coordinates

Alice's first sync:
  1. Downloads keys/alice.enc -> unwraps library key
  2. Downloads and validates membership entries
  3. Downloads snapshot, pulls changesets -> applies -> has the full library
  4. Can now push her own signed changesets
```

#### Changeset validation on pull

Before applying any changeset:
1. Verify the signature against `author_pubkey`
2. Was the author a valid member at that time?
3. If either fails -> discard

#### Revocation

Owner writes a Remove membership entry, generates a new encryption key, re-wraps to remaining members. Old data: Bob had the old key, accept it pragmatically. New data is protected.

#### Attribution

Every changeset envelope carries `author_pubkey`. "Alice added this release," "Bob changed the cover." Free audit trail.

### Cross-library sharing (layer 3)

Libraries are islands. If Alice wants to share one album with Bob (who isn't in her library), she'd have to give him the entire library encryption key. All or nothing.

#### Derived keys

Replace the flat encryption model with a key hierarchy:

```
master_key (per library, in keyring)
  -> derive(master_key, release_id) -> release_key
      -> encrypts that release's files
```

HKDF-SHA256 with the release ID as context. Each release effectively has its own key, derived from the master. Library members have the master key and can derive any release key. A single release key can be shared without exposing the master.

#### Sharing a release

```
Alice wants to share "Kind of Blue" with Bob (not in her library):

  1. Alice derives: release_key = derive(master_key, "rel-123")
  2. Alice wraps release_key + optional S3 creds to Bob's public key
  3. Alice signs a share grant
  4. Sends the grant to Bob (any channel)

Bob's client:
  1. Unwraps the release key and creds with his private key
  2. Fetches the release's files from Alice's bucket
  3. Decrypts with the release key
  4. Plays
```

No server in the loop. Bob reads directly from Alice's S3 bucket with just enough key material for one release.

#### Aggregated view

A user's client aggregates all their access into one view:

```
You (keypair)
  -- your library (master key -> full access)
  -- friend's library (master key -> full member)
  -- share from Alice (release key -> one album)
  -- public catalog follows (metadata only, no keys)
```

Your music, shared libraries, individual grants -- resolved at play time from different buckets.

### Discovery network (layer 4)

Every bae user who imports a release and matches it to a MusicBrainz ID creates a mapping:

```
MusicBrainz ID (universal -- "what this music IS")
        <->
Content hash / infohash (universal -- "the actual bytes")
```

This mapping is valuable. It's curation -- someone verified that these bytes are this release. Sharing it publicly enables decentralized music discovery without a central authority.

#### Three-layer lookup

```
MBID                -> content hashes (the curation mapping)
Content hash        -> peers who have it (the DHT)
Peer                -> actual bytes (BitTorrent)
```

#### The DHT as rendezvous

The BitTorrent Mainline DHT is used for peer discovery, not as a database. For each MBID, derive a rendezvous key:

```
rendezvous = hash("bae:mbid:" + MBID_X)
```

Every bae client that has a release matched to MBID X announces on that rendezvous key (standard DHT announce).

#### Forward lookup: "I want Kind of Blue"

```
User knows MBID X (from MusicBrainz search)
  -> DHT: find peers announcing hash("bae:mbid:" + MBID_X)
  -> discovers Alice, Bob, Carlos are online
  -> connects peer-to-peer
  -> each peer sends their signed attestation:
      Alice: { mbid: X, infohash: ABC, sig: "..." }
      Bob:   { mbid: X, infohash: ABC, sig: "..." }
      Carlos: { mbid: X, infohash: DEF, sig: "..." }
  -> aggregate locally:
      infohash ABC -- 2 attestations (probably the common CD rip)
      infohash DEF -- 1 attestation (maybe a remaster)
  -> pick one, use standard BitTorrent to download
```

#### Reverse lookup: "I have these files, what are they?"

```
User has files with infohash ABC
  -> DHT: find peers in the torrent swarm for infohash ABC
  -> connect, ask via BEP 10 extended messages: "what MBID is this?"
  -> peers respond with signed attestations
  -> now the user has proper metadata without manual tagging
```

#### Why not a blockchain?

The attestation model doesn't need proof of work or consensus:

- **No financial stakes** -- worst case is a bad mapping, not stolen money
- **Identity-based** -- every attestation is signed by a keypair
- **Confidence = attestation count** -- more independent signers = higher trust
- **Bad mappings die naturally** -- zero corroboration, ignored

#### Attestation properties

- **Signed**: every attestation is cryptographically signed by the author
- **Cached**: clients cache attestations locally, re-share to future queries -- knowledge spreads epidemically
- **Tamper-evident**: can't forge an attestation without the private key
- **No single writer**: no one controls the mapping, no one can censor it
- **Permissionless**: any bae client can participate

#### Participation controls

Off by default. Enable in settings. Per-release opt-out. Attestation-only mode or full participation (attestations + seeding).

## Storage Profiles

How release files are stored and managed across local and cloud locations. See `02-storage-profiles.md` for the full design.

## bae-server

`bae-server` -- a headless, read-only Subsonic API server.

- Given sync bucket URL + encryption key: downloads `snapshot.db.enc`, applies changesets, caches DB + images locally
- Streams audio from whatever storage location files are on, decrypting on the fly
- Optional `--web-dir` serves the bae-web frontend alongside the API
- `--recovery-key` for encrypted libraries, `--refresh` to re-pull from sync bucket
- Stateless -- no writes, no migrations, ephemeral cache rebuilt from the sync bucket

## First-Run Flows

### New library

On first run (no `~/.bae/active-library`), desktop shows a welcome screen. User picks "Create new library":

1. Generate a library UUID (e.g., `lib-111`) and a profile UUID (e.g., `prof-aaa`)
2. Create `~/.bae/libraries/lib-111/`
3. Create empty `library.db`, insert `storage_profiles` row:
   | profile_id | location | location_path |
   |---|---|---|
   | `prof-aaa` | local | `~/.bae/libraries/lib-111/` |
4. Write `config.yaml`, write `~/.bae/active-library` -> `lib-111`
5. Re-exec binary -- desktop launches normally

The library home is now a storage profile. `storage/` is empty -- user imports their first album, files go into `storage/ab/cd/{file_id}`.

### Restore from sync bucket

User picks "Restore from sync bucket" and provides an S3 bucket + creds + encryption key:

1. Download + decrypt `snapshot.db.enc` from the bucket (validates the key -- if decryption fails, wrong key)
2. Generate a new profile UUID (`prof-ccc`), create `~/.bae/libraries/{library_id}/`
3. Insert a new `storage_profiles` row:
   | profile_id | location | location_path |
   |---|---|---|
   | `prof-ccc` | local | `~/.bae/libraries/{library_id}/` |
4. Write `config.yaml` (with sync bucket config), keyring entries, `~/.bae/active-library` -> `{library_id}`
5. Download images from the bucket
6. Pull and apply any changesets newer than the snapshot
7. Re-exec binary

The new library home is `prof-ccc`. Its `storage/` is empty -- release files still live on their original storage profiles. The user can stream from cloud profiles or transfer releases to `prof-ccc`.

### Going from local to cloud

Sync and file storage are independent. Either can be enabled first.

**Enabling sync:**
1. User provides S3 credentials for a sync bucket (bucket must be empty)
2. bae generates encryption key if one doesn't exist, stores in keyring
3. bae pushes a full snapshot + all images to the sync bucket
4. Subsequent mutations push incremental changesets
5. Another device can now join from the sync bucket

**Adding cloud file storage:**
1. User creates a cloud storage profile (provides S3 credentials, bucket must be empty)
2. Release files can be transferred to the cloud profile
3. No metadata is replicated to the storage profile -- it just holds files

**Simplest setup: one bucket for everything.** The sync bucket can also serve as a file storage location. Release files go under `storage/` in the same bucket alongside the sync data. One bucket, one set of credentials. For many users, this is all they need.

**Separate buckets.** Power users can have the sync bucket on fast storage and file storage on cheap archival buckets. Or file storage on an external drive. The sync bucket only holds changesets, snapshots, and images -- it stays small.

## How the Layers Compose

**Solo user, local only** (today):
- Layer 1 replaces full-snapshot sync with incremental changesets to the sync bucket
- Faster, uses less bandwidth

**Solo user, multiple devices**:
- Layer 1 syncs between devices via the shared sync bucket
- Same user, different device IDs, merge via LWW

**Friends sharing a library**:
- Layers 1 + 2: multiple users, signed changesets, membership chain
- Everyone has the master key, full read/write access

**Sharing one album with someone**:
- Layer 3: derived key for that release, wrapped to recipient's public key
- No library membership needed, no server needed

**Public music discovery**:
- Layer 4: MBID-to-infohash mapping via DHT
- Participate by announcing your releases, benefit by discovering metadata

Each layer is opt-in. A user who never wants collaboration still benefits from incremental sync. A user who never wants public participation still benefits from shared libraries and private sharing.

## What's Not Built Yet

- Bidirectional sync / conflict resolution (Phase 1 of roadmap)
- Periodic auto-upload
- Write lock to prevent two desktops writing to the same library

## Open Questions

- Managed storage (we host S3) or always BYO-bucket?
- Second-device setup when iCloud Keychain is off -- QR code? Paste key?
- Key rotation -- probably YAGNI for now
