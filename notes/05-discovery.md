# Discovery Network

Every bae user who imports a release and matches it to an external ID (MusicBrainz or Discogs) creates a mapping:

```
External ID (universal -- "what this music IS")
        <->
Content hash / infohash (universal -- "the actual bytes")
```

This is a curation mapping -- someone verified that these bytes are this release. Sharing it publicly enables decentralized discovery without a central authority.

Discovery is off by default, opt-in per device and per release. It is not part of the v0 UI -- the protocol may be code-complete but will not be exposed until after v0.

## Three-layer lookup

```
External ID         -> content hashes (the curation mapping)
Content hash        -> peers who have it (the DHT)
Peer                -> actual bytes (BitTorrent)
```

## The DHT as rendezvous

The BitTorrent Mainline DHT is used for peer discovery, not as a database. For each external ID, derive a rendezvous key:

```
rendezvous = hash("bae:mbid:" + MBID_X)
rendezvous = hash("bae:discogs:" + DISCOGS_RELEASE_ID)
```

Every bae client that has a release matched to an external ID announces on that rendezvous key (standard DHT announce). A release matched to both MusicBrainz and Discogs announces on both keys.

## Forward lookup: "I want this album"

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

## Reverse lookup: "I have these files, what are they?"

```
User has files with infohash ABC
  -> DHT: find peers in the torrent swarm for infohash ABC
  -> connect, ask via BEP 10 extended messages: "what MBID is this?"
  -> peers respond with signed attestations
  -> now the user has proper metadata without manual tagging
```

## Why not a blockchain?

The attestation model doesn't need proof of work or consensus:

- **No financial stakes** -- worst case is a bad mapping, not stolen money
- **Identity-based** -- every attestation is signed by a keypair
- **Confidence = attestation count** -- more independent signers = higher trust
- **Bad mappings die naturally** -- zero corroboration, ignored

## Attestation properties

- **Signed**: every attestation is cryptographically signed by the author
- **Cached**: clients cache attestations locally, re-share to future queries -- knowledge spreads epidemically
- **Tamper-evident**: can't forge an attestation without the private key
- **No single writer**: no one controls the mapping, no one can censor it
- **Permissionless**: any bae client can participate

## Participation controls

Off by default. Enable in settings. Per-release opt-out. Attestation-only mode or full participation (attestations + seeding).
