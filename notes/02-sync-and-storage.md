# Sync & Storage

bae starts local, scales to cloud, then to collaboration and a decentralized network. Progressive complexity -- each layer is independently useful, and you never pay for capabilities you don't use.

## Tiers

### Tier 1: Local (no setup)

- Install bae, import music from folders/CDs
- Files stored locally, plain SQLite DB, no encryption, no key
- Library lives at `~/.bae/`

### Tier 2: Cloud (one decision)

Sign in with a cloud provider (Google Drive, Dropbox, OneDrive, pCloud) or configure an S3-compatible bucket. This creates your **cloud home** -- a single cloud location that holds everything: changesets, snapshots, images, and your release files. There is exactly one cloud home per library.

Consumer clouds are the primary path -- OAuth sign-in, zero configuration, everyone already has an account. S3-compatible providers (Backblaze B2, Cloudflare R2, AWS, MinIO, Wasabi, etc.) are for more technical users who want full control or self-hosting.

Your cloud home is your default cloud storage. When you import music, files go there. When another device joins, it pulls everything from there. One location, one sign-in.

- bae generates an encryption key and stores it in the OS keyring
- On macOS, iCloud Keychain syncs the encryption key to other devices automatically
- Files encrypt on upload, images encrypt in the cloud home, DB snapshots encrypted for bootstrap
- The user never typed an encryption key. They might not even know they have one.

### Tier 3: More technical users

- Export/import encryption key manually
- Run bae-server pointing at the cloud home
- Key fingerprint visible in settings for verification

## What a Library Is

Desktop manages all libraries under `~/.bae/libraries/`. Each library is a directory:

```
~/.bae/
  active-library               # UUID of the active library
  libraries/
    {uuid}/                    # one directory per library
```

On first launch, bae creates the library home.

**`config.yaml`** -- device-specific settings (torrent ports, subsonic config, keyring hint flags, cloud home configuration, device_id). Not synced. Only at the library home.

| Data | Tier 1 (local) | Tier 2+ (cloud) |
|------|----------------|-----------------|
| library.db | Plain SQLite | Plain locally, encrypted snapshot in cloud home |
| Cover art | Plaintext | Encrypted in cloud home |
| Release files | Local in library home | Encrypted in cloud home |
| Encryption key | N/A | OS keyring (iCloud Keychain) |
| config.yaml | Local | Local (device-specific, not synced) |

## Encryption

One key per library. Everything that goes to cloud gets encrypted with it.

**Key fingerprint:** SHA-256 of the key, truncated. Stored in `config.yaml`. Lets us detect the wrong key immediately instead of silently producing garbage.

You shouldn't have to think about encryption, keys, or cloud storage until the moment you want cloud. And when you do, encryption just happens -- it's not a feature you configure, it's a consequence of going cloud.

## The CloudHome Trait

The `CloudHome` trait is the core abstraction that makes bae backend-agnostic. Everything above this trait is universal — the sync protocol, encryption, membership chain, cloud home layout, join flow. Everything below it adapts to the specific cloud provider.

### What's universal (above the trait)

- **Cloud home layout**: `changes/`, `heads/`, `snapshot.db.enc`, `images/`, `storage/` — same logical paths regardless of backend
- **Encryption**: one master key per library, everything encrypted before it leaves the device
- **Sync protocol**: changesets, snapshots, conflict resolution via LWW — same algorithm everywhere
- **Membership chain**: append-only log with Ed25519 signatures, encryption key wrapped to each member's pubkey
- **Join flow**: two-step code exchange (pubkey then invite code) — same ceremony regardless of backend

### What varies (below the trait)

- **Storage API**: how files are actually read/written (S3 API vs Google Drive API vs Dropbox API)
- **Access management**: how a joiner gets storage access (folder sharing vs credential minting vs manual)
- **Authentication**: how the joiner authenticates (their own cloud account vs embedded credentials)
- **Change notifications**: consumer clouds have push notifications, S3 requires polling

### The trait

```rust
trait CloudHome {
    // storage — same interface, different API underneath
    fn write(path, data) -> Result
    fn read(path) -> Result<Bytes>
    fn read_range(path, start, end) -> Result<Bytes>
    fn list(prefix) -> Result<Vec<String>>
    fn delete(path) -> Result
    fn exists(path) -> Result<bool>

    // access management — varies by backend
    fn grant_access(member_email_or_id) -> Result<JoinInfo>
    fn revoke_access(member_email_or_id) -> Result
}
```

Implementations: S3 (via aws-sdk-s3), Google Drive, Dropbox, OneDrive, pCloud (via their REST APIs).

### Storage operations

Every cloud storage service supports basic file operations — the trait normalizes them into one interface:

| Operation | S3 | Google Drive | Dropbox |
|---|---|---|---|
| Write | `PutObject` | `files.create` | `files/upload` |
| Read | `GetObject` | `files.get` (media) | `files/download` |
| Read byte range | `Range` header | `Range` header | `Range` header |
| List by prefix | `ListObjectsV2(prefix=)` | `files.list(q=)` on folder | `files/list_folder` |
| Delete | `DeleteObject` | `files.delete` | `files/delete` |
| Exists | `HeadObject` | `files.get(fields=id)` | `files/get_metadata` |

Cloud home paths like `changes/{device_id}/{seq}.enc` map to flat object keys on S3 and folder hierarchy on consumer clouds. Same logical paths, different native representations. Callers (sync engine, image uploader, file storage) don't know or care which backend they're talking to.

### Access management

How `grant_access` and `revoke_access` work depends on the backend:

| | Consumer cloud | S3 with minting | S3 without minting |
|---|---|---|---|
| **Grant access** | Share folder via provider API (Google Drive `permissions.create`, Dropbox `sharing/add_folder_member`, etc.) | Mint scoped IAM credentials via provider API | Owner creates credentials manually in provider console |
| **Revoke access** | Unshare folder via provider API | Delete minted credentials | Rotate shared credentials (all-or-nothing) |
| **Joiner authenticates with** | Their own cloud account (OAuth) | Credentials embedded in invite code | Credentials embedded in invite code |
| **Per-user scoping** | Yes (provider-native) | Yes (per-user IAM) | No (shared credentials) |

`JoinInfo` is what the joiner needs beyond the encryption key (which is always wrapped to their pubkey via the membership chain):
- **Consumer cloud**: provider type + folder ID. The joiner signs into their own account; the shared folder is already accessible.
- **S3**: bucket + region + endpoint + credentials.

### The join flow

The same two-step code exchange regardless of backend:

```
1. Joiner shares their public key with the owner
2. Owner's bae:
   a. Calls grant_access (shares folder or mints credentials)
   b. Wraps the encryption key to the joiner's pubkey (membership chain)
   c. Bundles JoinInfo + wrapped key into an invite code
3. Joiner pastes the invite code
   a. Consumer cloud: signs into their own account, bae opens the shared folder
   b. S3: bae uses the embedded credentials
   c. Both: unwraps the encryption key, pulls snapshot, syncs
```

The membership chain is the same everywhere -- it's how the encryption key is shared securely (never in the clear). What adapts is the storage access layer.

### Change notifications

Consumer clouds support push-based change notifications (Google Drive `changes.watch`, Dropbox longpoll, OneDrive delta API) which enable faster sync than S3's polling model. The `CloudHome` trait can optionally support a `watch` method for backends that have it.


## Sync

The design has four layers, each building on the previous:

1. **Changeset sync** -- incremental sync via SQLite session extension changesets pushed to a shared cloud home
2. **Shared libraries** -- multiple users writing to the same library with signed changesets and a membership chain

Each layer is independently useful. A solo user benefits from layer 1. Friends sharing a library use layers 1-2.

### Changeset sync (layer 1)

Use the SQLite session extension to capture exactly what changed, push the binary changeset to the cloud home, pull and apply on other devices with a conflict handler. No coordination server. The cloud home is a library-level concept -- one bucket/folder per library, configured in `config.yaml`. The cloud home can be an S3 bucket or a folder on a consumer cloud (Google Drive, Dropbox, etc.).

Each device writes to its own keyspace on the shared bucket. No write contention by construction:

```
cloud-home/
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
- If the cloud home is unreachable, sync is skipped and retried next time

#### Database architecture

The session extension attaches to a single connection and only captures changes made through that connection. The `Database` struct is refactored to use a dedicated write connection (with session attached) and a read pool. Write methods use the dedicated connection; read methods use the pool. This matches SQLite's single-writer-multiple-reader architecture.

#### Schema evolution

The SQLite session extension identifies columns by index, not by name. This constrains how the schema can change once changeset sync is live.

**Additive changes are transparent.** Adding columns at the end of a table or adding new tables requires no coordination. Old changesets applied to a new schema just have fewer columns (extras keep defaults). New changesets applied to an old schema skip unknown columns. Devices on different schema versions interoperate seamlessly.

**Breaking changes require coordination.** Deleting, reordering, or renaming columns shifts column indices and corrupts changeset application. These changes bump a `min_schema_version` marker in the cloud home, splitting the changeset history into **epochs**. Within an epoch, all changesets are schema-compatible. Across epochs, no replay -- devices pull a fresh snapshot to jump forward. This means any schema change is possible (the snapshot IS the migrated state), but all devices must upgrade before syncing resumes.

Every changeset envelope carries a `schema_version` integer so receivers know what schema produced it. In practice, schema changes for a music library are almost always additive (new fields), making breaking migrations rare.

#### Snapshots

The changeset log grows forever without intervention. Periodically, any device writes a snapshot -- a full DB `VACUUM INTO`:

```
snapshot.db.enc   # overwritten each time
```

New devices start from the snapshot, then replay only changesets after it. Old changesets can be garbage collected after a grace period (30 days).

#### What this replaces

The `MetadataReplicator` -- which pushed a full `VACUUM INTO` snapshot plus all images to every non-home profile on every mutation -- is eliminated entirely. It is not reduced to local-only; it is removed. Sync goes through the single cloud home. Storage profiles (including external drives) hold release files only -- no DB, no images, no manifest.

### Shared libraries (layer 2)

Currently a library has one writer (desktop). Adding users -- multiple people reading and writing the same library -- requires identity, authorization, and a trust model.

#### Identity = a keypair

Each user generates a keypair locally (Ed25519 for signing, X25519 for encryption). No accounts, no server, no signup. Your public key is your identity. The keypair is global (not per-library) so attestations in layer 4 accumulate under one identity.

#### Bucket layout with users

```
cloud-home/
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

### Discovery network

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

## bae-server

`bae-server` -- a headless, read-only Subsonic API server.

- Given cloud home URL + encryption key: downloads `snapshot.db.enc`, applies changesets, caches DB + images locally
- Streams audio from the cloud home, decrypting on the fly
- Optional `--web-dir` serves the bae-web frontend alongside the API
- `--recovery-key` for encrypted libraries, `--refresh` to re-pull from cloud home
- Stateless -- no writes, no migrations, ephemeral cache rebuilt from the cloud home

## First-Run Flows

### New library

On first run (no `~/.bae/active-library`), desktop shows a welcome screen. User picks "Create new library":

1. Generate a library UUID (e.g., `lib-111`)
2. Create `~/.bae/libraries/lib-111/`
3. Create empty `library.db`
4. Write `config.yaml`, write `~/.bae/active-library` -> `lib-111`
5. Re-exec binary -- desktop launches normally

`storage/` is empty -- user imports their first album, files go into `storage/ab/cd/{file_id}`.

### Restore from cloud home

User picks "Restore from cloud home" and provides cloud home credentials + encryption key:

1. Download + decrypt `snapshot.db.enc` (validates the key -- if decryption fails, wrong key)
2. Create `~/.bae/libraries/{library_id}/`
3. Write `config.yaml` (with cloud home config), keyring entries, `~/.bae/active-library` -> `{library_id}`
4. Download images from the cloud home
5. Pull and apply any changesets newer than the snapshot
6. Re-exec binary

Local `storage/` is empty -- release files stream from the cloud home. The user can optionally download files locally for offline playback.

### Going from local to cloud

1. User signs in with a cloud provider (OAuth) or enters S3 credentials
2. bae creates the cloud home folder/bucket (or uses an existing one)
3. bae generates encryption key if one doesn't exist, stores in keyring
4. bae pushes a full snapshot + all images + release files to the cloud home
5. Subsequent mutations push incremental changesets
6. Another device can now join from the cloud home

One sign-in, one location. Everything lives in the cloud home.

## How the Layers Compose

**Solo user, local only** (today):
- Layer 1 replaces full-snapshot sync with incremental changesets to the cloud home
- Faster, uses less bandwidth

**Solo user, multiple devices**:
- Layer 1 syncs between devices via the shared cloud home
- Same user, different device IDs, merge via LWW

**Friends sharing a library**:
- Layers 1 + 2: multiple users, signed changesets, membership chain
- Everyone has the master key, full read/write access

Each layer is opt-in. A user who never wants collaboration still benefits from incremental sync.

## Constraints

### Access control varies by backend

See the CloudHome trait's access management table above. The three tiers (consumer cloud, S3 with minting, S3 without minting) have different capabilities for per-user scoping and revocation, but the join flow and membership chain work the same across all of them.

**Bandwidth/request limits are the provider's problem.** bae can't enforce quotas regardless of backend.

### No coordination server

bae has no central server. All sync, membership, and sharing happens through the storage backend and peer-to-peer exchanges (code pasting, link sharing). This means:

- **No push notifications for S3.** Devices poll on a timer. Consumer clouds support change notifications (Google Drive changes.watch, Dropbox longpoll) which are faster.
- **No NAT traversal service.** Peer-to-peer connections (layer 4) depend on DHT and UPnP.
- **No account recovery.** Lose your keypair and encryption key, lose access. iCloud Keychain helps but isn't guaranteed.
- **No global directory.** You can't "search for a user" -- you need their public key or a follow/invite code exchanged out-of-band.

## What's Not Built Yet

- Bidirectional sync / conflict resolution (Phase 1 of roadmap)
- Periodic auto-upload
- Write lock to prevent two desktops writing to the same library

## Open Questions

- Second-device setup when iCloud Keychain is off -- QR code? Paste key?
- Key rotation -- probably YAGNI for now
- bae Cloud managed offering -- storage + always-on bae-server + `yourname.bae.fm` subdomain
