use crate::api::{self, ShareInfo, SharedAlbum, SharedAlbumSong};
use dioxus::prelude::*;

fn cover_art_url(cover_art_id: &Option<String>, token: &str) -> Option<String> {
    cover_art_id
        .as_ref()
        .map(|id| format!("/rest/getCoverArt?id={id}&shareToken={token}"))
}

fn stream_url(track_id: &str, token: &str) -> String {
    format!("/rest/stream?id={track_id}&shareToken={token}")
}

fn download_url(track_id: &str, token: &str) -> String {
    format!("/rest/stream?id={track_id}&shareToken={token}&download=true")
}

fn format_duration(secs: i64) -> String {
    let mins = secs / 60;
    let remaining = secs % 60;
    format!("{mins}:{remaining:02}")
}

#[component]
pub fn ShareView(token: String) -> Element {
    let tok = token.clone();
    let data = use_resource(move || {
        let tok = tok.clone();
        async move { api::fetch_share_info(&tok).await }
    });
    let read = data.read();

    let result = match &*read {
        Some(Ok(info)) => Ok(info.clone()),
        Some(Err(e)) => Err(e.clone()),
        None => {
            return rsx! {
                SharePageShell {
                    div { class: "text-gray-400 text-sm", "Loading..." }
                }
            };
        }
    };
    drop(read);

    match result {
        Ok(ShareInfo::Track(track)) => {
            let cover_url = cover_art_url(&track.cover_art_id, &token);
            let audio_src = stream_url(&track.id, &token);
            let dl_url = download_url(&track.id, &token);

            rsx! {
                SharePageShell {
                    ShareCard {
                        cover_url,
                        primary_title: track.title.clone(),
                        secondary_line: track.artist.clone(),
                        tertiary_line: if track.album.is_empty() { None } else { Some(track.album.clone()) },
                        audio { class: "w-full mt-4", controls: true, src: audio_src }
                        div { class: "flex justify-center mt-3",
                            a {
                                class: "inline-flex items-center gap-2 px-4 py-2 rounded-lg bg-[var(--color-surface-input)] text-gray-300 hover:text-white hover:bg-[var(--color-hover)] transition-colors text-sm",
                                href: dl_url,
                                download: true,
                                DownloadIcon {}
                                "Download"
                            }
                        }
                    }
                }
            }
        }
        Ok(ShareInfo::Album(album)) => {
            rsx! {
                SharePageShell {
                    AlbumShareCard { album, token: token.clone() }
                }
            }
        }
        Err(e) => {
            rsx! {
                SharePageShell {
                    div { class: "text-center",
                        p { class: "text-gray-400 text-lg mb-2", "Link unavailable" }
                        p { class: "text-gray-500 text-sm", "{e}" }
                    }
                }
            }
        }
    }
}

/// Full-page shell: dark background, vertically + horizontally centered content.
#[component]
fn SharePageShell(children: Element) -> Element {
    rsx! {
        div { class: "min-h-screen bg-[var(--color-surface-base)] flex items-center justify-center p-4",
            {children}
        }
    }
}

/// A card with cover art, text lines, and an optional slot for audio controls or track list.
#[component]
fn ShareCard(
    cover_url: Option<String>,
    primary_title: String,
    secondary_line: String,
    tertiary_line: Option<String>,
    children: Element,
) -> Element {
    rsx! {
        div { class: "bg-[var(--color-surface-raised)] rounded-xl shadow-2xl max-w-md w-full p-6",
            // Cover art
            if let Some(url) = cover_url {
                img {
                    class: "w-full aspect-square object-cover rounded-lg mb-5",
                    src: "{url}",
                    alt: "Cover art",
                }
            } else {
                div { class: "w-full aspect-square bg-[var(--color-surface-input)] rounded-lg mb-5 flex items-center justify-center",
                    svg {
                        class: "w-16 h-16 text-gray-600",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "1.5",
                        view_box: "0 0 24 24",
                        // Simple image placeholder icon
                        path {
                            d: "M2.25 15.75l5.159-5.159a2.25 2.25 0 013.182 0l5.159 5.159m-1.5-1.5l1.409-1.409a2.25 2.25 0 013.182 0l2.909 2.909M3.75 21h16.5A2.25 2.25 0 0022.5 18.75V5.25A2.25 2.25 0 0020.25 3H3.75A2.25 2.25 0 001.5 5.25v13.5A2.25 2.25 0 003.75 21z",
                        }
                    }
                }
            }

            // Text
            div { class: "text-center mb-2",
                h1 { class: "text-white text-xl font-semibold truncate", "{primary_title}" }
                p { class: "text-gray-400 text-sm mt-1 truncate", "{secondary_line}" }
                if let Some(tertiary) = tertiary_line {
                    p { class: "text-gray-500 text-xs mt-0.5 truncate", "{tertiary}" }
                }
            }

            // Children slot (audio controls, track list, etc.)
            {children}
        }
    }
}

/// Album share card with a track list and switchable audio source.
#[component]
fn AlbumShareCard(album: SharedAlbum, token: String) -> Element {
    let mut current_track_id: Signal<Option<String>> = use_signal(|| None);
    let cover_url = cover_art_url(&album.cover_art_id, &token);

    let tertiary = album.year.map(|y| format!("({y})"));

    rsx! {
        ShareCard {
            cover_url,
            primary_title: album.name.clone(),
            secondary_line: album.artist.clone(),
            tertiary_line: tertiary,

            // Track list
            div { class: "mt-4 border-t border-[var(--color-border-subtle)]",
                for song in &album.songs {
                    TrackRow {
                        download_href: download_url(&song.id, &token),
                        song: song.clone(),
                        is_playing: current_track_id().as_deref() == Some(&song.id),
                        on_click: move |id: String| {
                            current_track_id.set(Some(id));
                        },
                    }
                }
            }

            // Audio element — src set when a track is selected
            if let Some(track_id) = current_track_id() {
                audio {
                    class: "w-full mt-3",
                    controls: true,
                    autoplay: true,
                    key: "{track_id}",
                    src: stream_url(&track_id, &token),
                    onended: move |_| {
                        // Advance to next track
                        if let Some(current) = current_track_id() {
                            let pos = album.songs.iter().position(|s| s.id == current);
                            if let Some(idx) = pos {
                                if idx + 1 < album.songs.len() {
                                    current_track_id.set(Some(album.songs[idx + 1].id.clone()));
                                } else {
                                    current_track_id.set(None);
                                }
                            }
                        }
                    },
                }
            }
        }
    }
}

/// A single row in the album track list.
#[component]
fn TrackRow(
    song: SharedAlbumSong,
    is_playing: bool,
    on_click: EventHandler<String>,
    download_href: String,
) -> Element {
    let id = song.id.clone();
    let highlight = if is_playing {
        "text-[var(--color-accent)]"
    } else {
        "text-gray-300 hover:text-white"
    };

    rsx! {
        div { class: "flex items-center gap-1 hover:bg-[var(--color-hover)] rounded",
            button {
                class: "flex-1 flex items-center gap-3 px-2 py-2.5 text-left transition-colors cursor-pointer {highlight} rounded min-w-0",
                onclick: move |_| on_click.call(id.clone()),
                // Track number
                span { class: "w-6 text-right text-xs text-gray-500 shrink-0",
                    if let Some(n) = song.track_number {
                        "{n}"
                    }
                }
                // Title
                span { class: "flex-1 text-sm truncate", "{song.title}" }
                // Duration
                if let Some(secs) = song.duration_secs {
                    span { class: "text-xs text-gray-500 shrink-0", "{format_duration(secs)}" }
                }
            }
            a {
                class: "p-2 text-gray-500 hover:text-white transition-colors shrink-0",
                href: download_href,
                download: true,
                title: "Download",
                onclick: move |e: Event<MouseData>| e.stop_propagation(),
                DownloadIcon {}
            }
        }
    }
}

#[component]
fn DownloadIcon() -> Element {
    rsx! {
        svg {
            class: "w-4 h-4",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "2",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            view_box: "0 0 24 24",
            path { d: "M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" }
            polyline { points: "7 10 12 15 17 10" }
            line { x1: "12", y1: "15", x2: "12", y2: "3" }
        }
    }
}
