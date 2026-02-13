# User Journey

How someone progresses from casual listener to connected music community.

## Stage 0: Click a share link

Someone sends you a link to a track. You click it, it plays in your browser. No app, no account, no friction. You like what you hear.

This is how most people first encounter bae -- not by seeking it out, but by receiving a link from a friend.

## Stage 1: Import & play

You download bae, create a library, import music from folders, CDs, or torrents. MusicBrainz matches provide metadata and cover art. You browse, play, and organize. Everything is local, single device.

No account, no server, no configuration beyond "point at your music."

## Stage 2: Sync across devices

You get a second machine or want cloud backup. Configure an S3-compatible bucket in settings. Your library syncs incrementally via changesets -- fast, bandwidth-efficient, no full re-uploads.

Same user, multiple devices, one library. Still solo.

## Stage 3: Share links

Now you're the one sending links. Right-click a track, "Copy Share Link," paste it anywhere. Your friends click and listen. The cycle from stage 0 repeats -- but now you're on the sending side.

## Stage 4: Follow a library

A friend who also has bae wants to browse your full catalog, not just one-off links. You generate a follow code -- a single token bundling bucket coordinates, read-only credentials, and the encryption key. They paste it into bae.

Now their app periodically pulls your catalog. It appears as a read-only library alongside their own. They can browse, search, and stream from it, but can't modify it. Their own library stays separate.

Follow is lightweight:
- No membership chain, no pubkey exchange
- One code, one paste
- Revoke by rotating the encryption key and re-issuing codes to people you still want
- Follower never pushes to the bucket -- pull only

## Stage 5: Join a shared library

For a spouse, roommate, or close collaborator who wants to contribute to the same library -- not just consume it. Both people import music, edit metadata, and curate together.

This is the full shared library: membership chain, signed changesets, mutual read+write access. The owner invites the joiner via a two-step code exchange (joiner shares their public key, owner sends back an invite code with bucket credentials and a wrapped encryption key).

Join is heavier than follow because it requires mutual trust:
- Membership chain tracks who's in and who's out
- Changesets are signed -- attribution for every edit
- Revocation rotates the encryption key and re-wraps to remaining members
- Both parties can push changes

## How they connect

```
Stage 0  Click a share link    (anyone with a browser)
Stage 1  Import & play         (downloads bae)
Stage 2  Sync across devices   (configures S3 bucket)
Stage 3  Share links           (sends links to others -- stage 0 for recipients)
Stage 4  Follow a library      (read-only pull of someone's catalog)
Stage 5  Join a shared library (close collaborator, mutual read+write)
```

The funnel: stage 0 converts to stage 1. Stage 3 feeds stage 0 for new people. Stage 4 is the natural social layer. Stage 5 is for the inner circle.

## What's built

- **Stage 1**: Complete -- import from folders, CDs, torrents. MusicBrainz + Discogs metadata. Full playback.
- **Stage 2**: Backend complete (changeset sync, S3 bucket client, snapshot bootstrap). Not yet wired into the desktop UI -- sync runs but there's no settings UI to configure a sync bucket.
- **Stage 3**: Complete -- share token generation, Subsonic API validation, web share page, copy-share-link in track menu, settings for base URL / expiry / key rotation.
- **Stage 4**: No work needed -- it's just stage 1 again.
- **Stage 5**: Not built. Needs: follow code generation/acceptance, read-only pull, displaying followed libraries in the UI.
- **Stage 6**: Backend exists (membership chain, invitation/revocation, changeset signing). No UI for creating invitations or the two-step code exchange. The "Join Shared" form requires manually entering 5 S3 fields.
