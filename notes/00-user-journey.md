# User Journey

## Stage 0: Click a share link

Many people first encounter bae by receiving a link from a friend.

Someone sends you a link to a track. You click it, it plays in your browser. The share page prompts you to save the track/album to your own bae library.

## Stage 1: Import & play

You download bae, create a library, import music from folders, CDs, torrents, or share links. MusicBrainz matches provide metadata and cover art. You browse, play, and organize.

Everything is local to the machine.

## Stage 2: Sync across devices

You get a second machine or want cloud backup. Sign in with a cloud provider (Google Drive, Dropbox, OneDrive, pCloud) or configure an S3-compatible bucket. This creates your cloud home -- one location that holds everything.

Consumer clouds are the primary path (OAuth sign-in). S3-compatible providers (Backblaze B2, Cloudflare R2, AWS, Wasabi, etc.) are for more technical users.

Your library syncs incrementally via changesets -- fast, bandwidth-efficient. Same user, multiple devices, one library. Still solo.

## Stage 3: Share links

Leave bae-desktop running, or run bae-server, and you can generate share links -- right-click a track, "Copy Share Link," paste it anywhere. Recipients click and listen in their browser (stage 0).

## Stage 4: Follow

A friend who also has bae wants to browse your full catalog, not just one-off links. You give them access to your bae-desktop or bae-server -- a URL and credentials. Their bae app connects and your library appears alongside their own as a read-only remote library.

Follow goes through bae-desktop or bae-server. The follower streams through your server, never touches the cloud home.

## Stage 5: Join

For a close collaborator who wants to contribute to the same library -- not just consume it. Both people import music, edit metadata, and curate together.

The joiner shares their public key, the owner sends back an invite code. bae handles the rest -- grants storage access (shares the cloud home folder on consumer clouds, mints credentials on S3), wraps the encryption key to the joiner's pubkey, and bundles everything into the code. The joiner pastes it and syncs.

Under the hood: membership chain, signed changesets, mutual read+write access. The two-step code exchange is the same regardless of cloud backend -- what adapts is how storage access is granted. See the CloudHome trait in `02-sync-and-storage.md`.