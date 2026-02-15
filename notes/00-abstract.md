# Abstract

Owning and managing a digital music collection is difficult. bae is a music library manager that uses decentralized identity and zero-knowledge encryption over pluggable storage to enable multi-device sync, collaborative curation, and discovery.

- **Identity**: each user has a locally generated keypair (Ed25519/X25519). Public keys are identities. There is no central identity server.
- **Encryption**: one symmetric key per library, shared by all members. Everything in the cloud home is encrypted. The storage provider sees opaque blobs.
- **Storage**: pluggable -- Google Drive, Dropbox, OneDrive, pCloud, any S3-compatible bucket, or local-only.
- **Sync and collaboration**: devices sync through the cloud home via encrypted changesets. Multiple users can share a library through a membership chain and signed changesets.
- **Discovery**: users who match releases to MusicBrainz/Discogs IDs create mappings from metadata to content hashes. These mappings are shared over a DHT, enabling decentralized search and download.
