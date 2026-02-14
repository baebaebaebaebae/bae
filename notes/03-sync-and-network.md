# Sync, Collaboration & Discovery Network

How bae evolves from single-device sync to a decentralized music network.

## Layers

The design has four layers, each building on the previous:

1. **Changeset sync** -- incremental sync via SQLite session extension changesets pushed to a shared cloud home
2. **Shared libraries** -- multiple users writing to the same library with signed changesets and a membership chain

Each layer is independently useful. A solo user benefits from layer 1. Friends sharing a library use layers 1-2.

---

## Layer 1: Changeset sync

### The problem

The current sync model pushes a full DB snapshot + all images to every profile on every mutation. This doesn't scale -- a single track rename re-uploads the entire library.

### The model

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

### Why the session extension

We considered and rejected several alternatives:

- **Op log with method wrapping** -- requires intercepting ~63 write methods, maintaining a custom JSON op format, and building a custom merge algorithm
- **SQLite triggers** -- same maintenance burden moved to SQL; must enumerate every column of every synced table
- **CRDTs (Automerge/Loro)** -- hold full state in memory, don't scale to arbitrary entity counts
- **cr-sqlite** -- stalled project, too risky as a dependency

The session extension is built into SQLite. It tracks all changes (INSERT/UPDATE/DELETE) automatically at the C level. No triggers, no method wrapping, no column enumeration. The app writes normally. SQLite records what changed. We grab the binary changeset and push it.

### Changesets, not operations

The session extension produces a compact binary changeset that represents the diff between the database before and after. A changeset contains only the rows and columns that actually changed.

For an import that creates an album + release + 12 tracks + 12 files, a JSON op log approach would generate ~50 operations. The session extension produces a single binary changeset blob. Smaller, faster, and no custom serialization.

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
   c. Push changeset to S3
   d. Pull incoming changesets (NO session active)
   e. Apply incoming with conflict handler
   f. Start new session
```

**Key rule:** Never apply someone else's changeset while your session is recording. Otherwise your next outgoing changeset contains their changes as duplicates.

### Database architecture

The session extension attaches to a single connection and only captures changes made through that connection. The `Database` struct is refactored to use a dedicated write connection (with session attached) and a read pool. Write methods use the dedicated connection; read methods use the pool. This matches SQLite's single-writer-multiple-reader architecture.

### Schema evolution

The SQLite session extension identifies columns by index, not by name. This constrains how the schema can change once changeset sync is live.

**Additive changes are transparent.** Adding columns at the end of a table or adding new tables requires no coordination. Old changesets applied to a new schema just have fewer columns (extras keep defaults). New changesets applied to an old schema skip unknown columns. Devices on different schema versions interoperate seamlessly.

**Breaking changes require coordination.** Deleting, reordering, or renaming columns shifts column indices and corrupts changeset application. These changes bump a `min_schema_version` marker in the cloud home, splitting the changeset history into **epochs**. Within an epoch, all changesets are schema-compatible. Across epochs, no replay -- devices pull a fresh snapshot to jump forward. This means any schema change is possible (the snapshot IS the migrated state), but all devices must upgrade before syncing resumes.

Every changeset envelope carries a `schema_version` integer so receivers know what schema produced it. In practice, schema changes for a music library are almost always additive (new fields), making breaking migrations rare. See the roadmap (1h) for the full protocol.

### Snapshots

The changeset log grows forever without intervention. Periodically, any device writes a snapshot -- a full DB `VACUUM INTO`:

```
snapshot.db.enc   # overwritten each time
```

New devices start from the snapshot, then replay only changesets after it. Old changesets can be garbage collected after a grace period (30 days).

### What this replaces

The current `MetadataReplicator` -- which pushes a full `VACUUM INTO` snapshot plus all images to every non-home profile on every mutation -- is eliminated entirely. It is not reduced to local-only; it is removed. Sync goes through the single cloud home. Storage profiles (including external drives) hold release files only -- no DB, no images, no manifest.

---

## Layer 2: Shared libraries

### The problem

Currently a library has one writer (desktop). Adding users -- multiple people reading and writing the same library -- requires identity, authorization, and a trust model.

### Identity = a keypair

Each user generates a keypair locally (Ed25519 for signing, X25519 for encryption). No accounts, no server, no signup. Your public key is your identity. The keypair is global (not per-library) so attestations in layer 4 accumulate under one identity.

### Bucket layout with users

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

### Membership chain

An append-only log of membership changes, stored as individual files to avoid S3 overwrite races. Each entry is signed by an owner.

```json
{ "action": "add", "user_pubkey": "...", "role": "owner",
  "ts": "2026-01-01T...", "author_pubkey": "...", "sig": "..." }
```

On read, clients download all membership entries, order by timestamp, and validate the chain.

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

The invite code contains everything Alice needs except the encryption key (which is wrapped to her pubkey in the cloud home). What's in the code depends on the backend -- see the CloudHome trait in `02-sync-and-storage.md`.

### Changeset validation on pull

Before applying any changeset:
1. Verify the signature against `author_pubkey`
2. Was the author a valid member at that time?
3. If either fails -> discard

### Revocation

Owner writes a Remove membership entry, calls `CloudHome::revoke_access` (unshares folder or deletes credentials), generates a new encryption key, re-wraps to remaining members. Old data: Bob had the old key, accept it pragmatically. New data is protected.

### Attribution

Every changeset envelope carries `author_pubkey`. "Alice added this release," "Bob changed the cover." Free audit trail.

---

## Discovery network

### The idea

Every bae user who imports a release and matches it to a MusicBrainz ID creates a mapping:

```
MusicBrainz ID (universal -- "what this music IS")
        <->
Content hash / infohash (universal -- "the actual bytes")
```

This mapping is valuable. It's curation -- someone verified that these bytes are this release. Sharing it publicly enables decentralized music discovery without a central authority.

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

---

## How the layers compose

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
