# Sync

When a library has a cloud home configured, bae syncs metadata and files to it. Multiple devices sync through the same cloud home. Multiple users can share a library through signed changesets and a membership chain. Cloud sync also enables bae-server, which serves the library by pulling from the cloud home.

## Encryption

One symmetric encryption key per library, shared by all members. Everything that goes to cloud gets encrypted with it. This key is separate from per-user Ed25519 signing keys -- having the encryption key lets you read/write data but not sign changesets as someone else.

**Key fingerprint:** SHA-256 of the key, truncated. Stored in `config.yaml`. Lets us detect the wrong key immediately instead of silently producing garbage.

When cloud is configured, bae generates an encryption key and stores it in the OS keyring. On macOS, this prompts for keychain access -- the user should understand bae is storing the encryption key in the system's secure store, not asking for a bae password.

## The CloudHome Trait

The `CloudHome` trait is the core abstraction that makes bae backend-agnostic. Everything above this trait is universal -- the sync protocol, encryption, membership chain, cloud home layout, join flow. Everything below it adapts to the specific cloud provider.

### What's universal (above the trait)

- **Cloud home layout**: `changes/`, `heads/`, `snapshot.db.enc`, `images/`, `storage/` -- same logical paths regardless of backend
- **Encryption**: one master key per library, everything encrypted before it leaves the device
- **Sync protocol**: changesets, snapshots, conflict resolution via LWW -- same algorithm everywhere
- **Membership chain**: append-only log with Ed25519 signatures, encryption key wrapped to each member's pubkey
- **Join flow**: two-step code exchange (pubkey then invite code) -- same ceremony regardless of backend

### What varies (below the trait)

- **Storage API**: how files are actually read/written (S3 API vs Google Drive API vs Dropbox API)
- **Access management**: how a joiner gets storage access (folder sharing vs credential minting)
- **Authentication**: how the joiner authenticates (their own cloud account vs embedded credentials)
- **Change notifications**: consumer clouds have push notifications, S3 requires polling

### The trait

```rust
trait CloudHome {
    // storage -- same interface, different API underneath
    fn write(path, data) -> Result
    fn read(path) -> Result<Bytes>
    fn read_range(path, start, end) -> Result<Bytes>
    fn list(prefix) -> Result<Vec<String>>
    fn delete(path) -> Result
    fn exists(path) -> Result<bool>

    // access management -- varies by backend
    fn grant_access(member_email_or_id) -> Result<JoinInfo>
    fn revoke_access(member_email_or_id) -> Result
}
```

Implementations: S3 (via aws-sdk-s3), Google Drive, Dropbox, OneDrive, pCloud (via their REST APIs), iCloud Drive (via local filesystem).

### Storage operations

Every cloud storage service supports basic file operations -- the trait normalizes them into one interface:

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

| | Consumer cloud | S3 |
|---|---|---|
| **Grant access** | Share folder via provider API (Google Drive `permissions.create`, Dropbox `sharing/add_folder_member`, etc.) | Mint scoped IAM credentials (automated via provider API, or manually in the provider console) |
| **Revoke access** | Unshare folder via provider API | Delete minted credentials |
| **Joiner authenticates with** | Their own cloud account (OAuth) | Credentials embedded in invite code |
| **Per-user scoping** | Yes (provider-native) | Yes (per-user IAM) |

`JoinInfo` is what the joiner needs beyond the encryption key (which is always wrapped to their pubkey via the membership chain):
- **Consumer cloud**: provider type + folder ID. The joiner signs into their own account; the shared folder is already accessible.
- **S3**: bucket + region + endpoint + minted credentials.

### Change notifications

Consumer clouds support push-based change notifications (Google Drive `changes.watch`, Dropbox longpoll, OneDrive delta API) which enable faster sync than S3's polling model. The `CloudHome` trait can optionally support a `watch` method for backends that have it.

## Changeset sync

Use the SQLite session extension to capture exactly what changed, push the changeset to the cloud home, pull and apply on other devices with a conflict handler. No coordination server. The cloud home is a library-level concept -- one bucket/folder per library, configured in `config.yaml`. The cloud home can be an S3 bucket or a folder on a consumer cloud (Google Drive, Dropbox, etc.).

Each device writes to its own keyspace on the shared bucket. No write contention by construction:

```
cloud-home/
  snapshot.db.enc                  # full DB for bootstrapping new devices
  changes/{device_id}/{seq}.enc    # changeset blobs per device
  heads/{device_id}.json.enc       # "my latest seq is 42"
  images/ab/cd/{id}                # all library images (encrypted)
  storage/ab/cd/{file_id}          # release files (encrypted)
```

**Push** = grab changeset from the session, encrypt, upload to `changes/{your_device}/`, update `heads/{your_device}`.

**Pull** = list `heads/`, compare each device's seq to your local cursors. If anyone's ahead, fetch their new changesets, apply with conflict handler. Deterministic -- same changesets in the same order produce the same result.

**Polling is cheap** -- listing `heads/` is one S3 LIST call. If all seqs match your cursors, nothing to do. Check on app open + periodic timer.

### Why the session extension

We considered and rejected several alternatives:

- **Op log with method wrapping** -- requires intercepting ~63 write methods, maintaining a custom JSON op format, and building a custom merge algorithm
- **SQLite triggers** -- same maintenance burden moved to SQL; must enumerate every column of every synced table
- **CRDTs (Automerge/Loro)** -- hold full state in memory, don't scale to arbitrary entity counts
- **cr-sqlite** -- stalled project, too risky as a dependency

The session extension is built into SQLite. It tracks all changes (INSERT/UPDATE/DELETE) automatically at the C level. No triggers, no method wrapping, no column enumeration. The app writes normally. SQLite records what changed. We grab the changeset and push it.

### Changesets, not operations

The session extension produces a compact changeset that represents the diff between the database before and after. A changeset contains only the rows and columns that actually changed.

For an import that creates an album + release + 12 tracks + 12 files, a JSON op log approach would generate ~50 operations. The session extension produces a single changeset blob. Smaller, faster, and no custom serialization.

### Conflict resolution: row-level LWW

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

### The sync protocol

```
1. Start session (attach synced tables)
2. App writes normally...
3. Time to sync:
   a. Grab changeset from session
   b. End session
   c. Push changeset to cloud home
   d. Pull incoming changesets (NO session active)
   e. Apply incoming with conflict handler
   f. Start new session
```

**Key rule:** Never apply someone else's changeset while your session is recording. Otherwise your next outgoing changeset contains their changes as duplicates.

### Sync triggers

Sync pushes after `LibraryEvent::AlbumsChanged` (import, delete, edit) with debounce. If the cloud home is unreachable, sync is skipped and retried next time.

### Database architecture

The session extension attaches to a single connection and only captures changes made through that connection. The `Database` struct is refactored to use a dedicated write connection (with session attached) and a read pool. Write methods use the dedicated connection; read methods use the pool. This matches SQLite's single-writer-multiple-reader architecture.

### Schema evolution

The SQLite session extension identifies columns by index, not by name. This constrains how the schema can change once changeset sync is live.

**Additive changes are transparent.** Adding columns at the end of a table or adding new tables requires no coordination. Old changesets applied to a new schema just have fewer columns (extras keep defaults). New changesets applied to an old schema skip unknown columns. Devices on different schema versions interoperate seamlessly.

**Breaking changes require coordination.** Deleting, reordering, or renaming columns shifts column indices and corrupts changeset application. These changes bump a `min_schema_version` marker in the cloud home, splitting the changeset history into **epochs**. Within an epoch, all changesets are schema-compatible. Across epochs, no replay -- devices pull a fresh snapshot to jump forward. This means any schema change is possible (the snapshot IS the migrated state), but all devices must upgrade before syncing resumes.

Every changeset envelope carries a `schema_version` integer so receivers know what schema produced it. In practice, schema changes for a music library are almost always additive (new fields), making breaking migrations rare.

### Snapshots

The changeset log grows forever without intervention. Periodically, any device writes a snapshot -- a full DB `VACUUM INTO`:

```
snapshot.db.enc   # overwritten each time
```

New devices start from the snapshot, then replay only changesets after it. Old changesets can be garbage collected after a grace period (30 days).

## Shared libraries

Currently a library has one writer (desktop). Adding users -- multiple people reading and writing the same library -- requires identity, authorization, and a trust model.

### Identity = a keypair

Each user generates a keypair locally (Ed25519 for signing, X25519 for encryption). No accounts, no server, no signup. Your public key is your identity. The keypair is global (not per-library) -- the same identity across all libraries a user participates in.

### Bucket layout with users

```
cloud-home/
  membership/{pubkey}/{seq}.enc     # signed membership entries (per author, avoids S3 overwrite races)
  keys/{user_pubkey}.enc            # library key wrapped to each member's public key
  heads/{device_id}.json.enc        # per-device head pointer (unchanged)
  changes/{device_id}/{seq}.enc     # per-device changeset stream (signed)
  snapshot.db.enc
  images/ab/cd/{id}
  storage/ab/cd/{file_id}
```

Changesets stay keyed by device_id (a user may have multiple devices). Authorship is established cryptographically: each changeset envelope includes `author_pubkey` and a signature over the changeset bytes.

### Membership chain

An append-only log of membership changes, stored as individual files to avoid S3 overwrite races. Each entry is signed by an owner. The chain serves as the library's collective keychain -- it's the authoritative record of who is a member and what their public key is.

```json
{ "action": "add", "user_pubkey": "...", "role": "owner",
  "ts": "2026-01-01T...", "author_pubkey": "...", "sig": "..." }
```

On read, clients download all membership entries, order by timestamp, and validate the chain. Changeset signatures are verified against public keys from this chain.

### Invitation flow

```
Owner invites Alice:
  1. Alice generates a keypair, sends her public key to the owner
  2. Owner's bae calls CloudHome::grant_access (shares folder or mints credentials)
  3. Owner wraps the library encryption key to Alice's public key
     -> uploads keys/alice.enc
  4. Owner writes membership entry: { action: "add", user: alice }
  5. Bundles JoinInfo + wrapped key into an invite code -> sends to Alice

Alice's first sync:
  1. Pastes invite code
  2. Consumer cloud: signs into her own account, bae opens the shared folder
     S3: bae uses the embedded credentials
  3. Downloads keys/alice.enc -> unwraps library key
  4. Downloads and validates membership entries
  5. Downloads snapshot, pulls changesets -> applies -> has the full library
  6. Can now push her own signed changesets
```

The invite code contains everything Alice needs except the encryption key (which is wrapped to her pubkey in the cloud home). What's in the code depends on the backend.

### Changeset validation on pull

Before applying any changeset:
1. Verify the signature against `author_pubkey`
2. Was the author a valid member at that time?
3. If either fails -> discard

### Revocation

Owner writes a Remove membership entry and calls `CloudHome::revoke_access` (unshares folder or deletes credentials). The encryption key is not rotated -- the revoked member had the key but can no longer access the cloud home. For a music library this is sufficient; the threat model is "someone left the group," not adversarial.

### Attribution

Every changeset envelope carries `author_pubkey`, so changes are attributed to the user who made them.

## Discovery network

Every bae user who imports a release and matches it to a MusicBrainz ID creates a mapping:

```
MusicBrainz ID (universal -- "what this music IS")
        <->
Content hash / infohash (universal -- "the actual bytes")
```

This is a curation mapping -- someone verified that these bytes are this release. Sharing it publicly enables decentralized discovery without a central authority.

### Three-layer lookup

```
MBID                -> content hashes (the curation mapping)
Content hash        -> peers who have it (the DHT)
Peer                -> actual bytes (BitTorrent)
```

### The DHT as rendezvous

The BitTorrent Mainline DHT is used for peer discovery, not as a database. For each MBID, derive a rendezvous key:

```
rendezvous = hash("bae:mbid:" + MBID_X)
```

Every bae client that has a release matched to MBID X announces on that rendezvous key (standard DHT announce).

### Forward lookup: "I want Kind of Blue"

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

### Reverse lookup: "I have these files, what are they?"

```
User has files with infohash ABC
  -> DHT: find peers in the torrent swarm for infohash ABC
  -> connect, ask via BEP 10 extended messages: "what MBID is this?"
  -> peers respond with signed attestations
  -> now the user has proper metadata without manual tagging
```

### Why not a blockchain?

The attestation model doesn't need proof of work or consensus:

- **No financial stakes** -- worst case is a bad mapping, not stolen money
- **Identity-based** -- every attestation is signed by a keypair
- **Confidence = attestation count** -- more independent signers = higher trust
- **Bad mappings die naturally** -- zero corroboration, ignored

### Attestation properties

- **Signed**: every attestation is cryptographically signed by the author
- **Cached**: clients cache attestations locally, re-share to future queries -- knowledge spreads epidemically
- **Tamper-evident**: can't forge an attestation without the private key
- **No single writer**: no one controls the mapping, no one can censor it
- **Permissionless**: any bae client can participate

### Participation controls

Off by default. Enable in settings. Per-release opt-out. Attestation-only mode or full participation (attestations + seeding).

## bae-server

`bae-server` -- a headless, read-only server that pulls from the cloud home.

- Given cloud home URL + encryption key: downloads `snapshot.db.enc`, applies changesets, caches DB + images locally
- Streams audio from the cloud home, decrypting on the fly
- Optional `--web-dir` serves the bae-web frontend alongside the API
- `--recovery-key` for encrypted libraries, `--refresh` to re-pull from cloud home
- Stateless -- no writes, no migrations, ephemeral cache rebuilt from the cloud home

## How It Composes

**Solo user, local only**:
- Changeset sync replaces full-snapshot sync with incremental changesets to the cloud home
- Faster, uses less bandwidth

**Solo user, multiple devices**:
- Changeset sync syncs between devices via the shared cloud home
- Same user, different device IDs, merge via LWW

**Friends sharing a library**:
- Changeset sync + shared libraries: multiple users, signed changesets, membership chain
- Everyone has the master key, full read/write access

Each capability is independent. Solo users only need changeset sync.

## Constraints

### Access control varies by backend

See the CloudHome trait's access management table above. Consumer clouds and S3 have different mechanisms for access management, but the join flow and membership chain work the same across all of them. S3 always uses per-user credential minting — the difference is whether minting is automated (provider API) or manual (owner creates credentials in the provider console).

**Bandwidth/request limits are the provider's problem.** bae can't enforce quotas regardless of backend.

### No coordination server

bae has no central server. All sync, membership, and sharing happens through the storage backend and peer-to-peer exchanges (code pasting, link sharing). This means:

- **No push notifications for S3.** Devices poll on a timer. Consumer clouds support change notifications (Google Drive changes.watch, Dropbox longpoll) which are faster.
- **No NAT traversal service.** Peer-to-peer connections depend on DHT and UPnP.
- **No account recovery.** Lose your keypair and encryption key, lose access. iCloud Keychain helps but isn't guaranteed.
- **No global directory.** You can't "search for a user" -- you need their public key or a follow/invite code exchanged out-of-band.

## Open Questions

- Second-device setup when iCloud Keychain is off -- QR code? Paste key?
- bae Cloud managed offering -- storage + always-on bae-server + `yourname.bae.fm` subdomain
