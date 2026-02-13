# MusicBrainz API Usage

All requests use `Accept: application/json` header. Rate-limited to 1 req/sec via shared `tokio::sync::Mutex<Instant>`.

## `lookup_by_discid`

| | |
|---|---|
| **Path** | `/ws/2/discid/{discid}` |
| **inc** | `recordings`, `artist-credits`, `release-groups`, `url-rels`, `labels` |
| **Returns** | `(Vec<MbRelease>, ExternalUrls)` |
| **Call sites** | `lookup_discid` wrapper (`import_helpers.rs:251`), called from `load_selected_release` (`import_helpers.rs:951`) |

## `fetch_release_group_with_relations`

| | |
|---|---|
| **Path** | `/ws/2/release-group/{id}` |
| **inc** | `url-rels` |
| **Returns** | `serde_json::Value` |
| **Call sites** | Internal, within `lookup_release_by_id` (`musicbrainz.rs:459`) |

## `lookup_release_by_id`

| | |
|---|---|
| **Path** | `/ws/2/release/{id}` |
| **inc** | `recordings`, `artist-credits`, `release-groups`, `release-group-rels`, `url-rels`, `labels`, `media` |
| **Returns** | `(MbRelease, ExternalUrls, serde_json::Value)` |
| **Call sites** | Pre-import validation (`import_helpers.rs:700`), `fetch_and_parse_mb_release` (`musicbrainz_parser.rs:28`) — indirectly from folder/torrent/CD import (`handle.rs:241,416,516`) |

## `search_releases_with_params`

| | |
|---|---|
| **Path** | `/ws/2/release` |
| **query** | `{text}` |
| **limit** | `25` |
| **inc** | `recordings`, `artist-credits`, `release-groups`, `labels`, `media`, `url-rels` |
| **Returns** | `Vec<MbRelease>` |
| **Call sites** | Folder/torrent text search (`import_helpers.rs:442`) |
