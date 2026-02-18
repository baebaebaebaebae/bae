use crate::api;
use dioxus::prelude::*;
use wasm_bindgen_x::JsCast;

fn format_duration(secs: i64) -> String {
    let mins = secs / 60;
    let remaining = secs % 60;
    format!("{mins}:{remaining:02}")
}

// -- Cloud share helpers --

fn get_url_fragment() -> Option<String> {
    let window = web_sys_x::window()?;
    let hash = window.location().hash().ok()?;
    let hash = hash.strip_prefix('#')?;
    if hash.is_empty() {
        None
    } else {
        Some(hash.to_string())
    }
}

fn decode_share_key(fragment: &str) -> Result<[u8; 32], String> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(fragment)
        .map_err(|e| format!("Invalid share key: {e}"))?;
    bytes
        .try_into()
        .map_err(|_| "Share key must be 32 bytes".to_string())
}

fn decode_release_key(b64: &str) -> Result<[u8; 32], String> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|e| format!("Invalid release key: {e}"))?;
    bytes
        .try_into()
        .map_err(|_| "Release key must be 32 bytes".to_string())
}

fn mime_for_format(format: &str) -> &str {
    match format {
        "flac" => "audio/flac",
        "mp3" => "audio/mpeg",
        "ogg" => "audio/ogg",
        "wav" => "audio/wav",
        "aac" => "audio/aac",
        "m4a" => "audio/mp4",
        "opus" => "audio/opus",
        _ => "application/octet-stream",
    }
}

fn create_blob_url(data: &[u8], mime_type: &str) -> Result<String, String> {
    let uint8_array = js_sys_x::Uint8Array::from(data);
    let array = js_sys_x::Array::new();
    array.push(&uint8_array);

    let opts = web_sys_x::BlobPropertyBag::new();
    opts.set_type(mime_type);
    let blob = web_sys_x::Blob::new_with_u8_array_sequence_and_options(&array, &opts)
        .map_err(|e| format!("Failed to create blob: {e:?}"))?;

    web_sys_x::Url::create_object_url_with_blob(&blob)
        .map_err(|e| format!("Failed to create blob URL: {e:?}"))
}

fn revoke_blob_url(url: &str) {
    let _ = web_sys_x::Url::revoke_object_url(url);
}

fn trigger_download(blob_url: &str, filename: &str) {
    let Some(window) = web_sys_x::window() else {
        return;
    };
    let Some(document) = window.document() else {
        return;
    };
    let Ok(elem) = document.create_element("a") else {
        return;
    };
    let _ = elem.set_attribute("href", blob_url);
    let _ = elem.set_attribute("download", filename);
    let _ = elem.set_attribute("style", "display:none");
    let body = document.body().unwrap();
    let _ = body.append_child(&elem);
    if let Some(html_elem) = elem.dyn_ref::<web_sys_x::HtmlElement>() {
        html_elem.click();
    }
    let _ = body.remove_child(&elem);
}

async fn load_track_blob(
    share_id: &str,
    file_key: &str,
    release_key_b64: &str,
    format: &str,
) -> Result<String, String> {
    let release_key = decode_release_key(release_key_b64)?;
    let encrypted = api::fetch_share_file(share_id, file_key).await?;
    let decrypted = crate::crypto::decrypt(&release_key, &encrypted)?;
    create_blob_url(&decrypted, mime_for_format(format))
}

// -- Main dispatch --

#[component]
pub fn ShareView(token: String) -> Element {
    let fragment = get_url_fragment();

    if let Some(frag) = fragment {
        rsx! { CloudShareView { share_id: token, fragment: frag } }
    } else {
        rsx! {
            SharePageShell {
                div { class: "text-center",
                    p { class: "text-gray-400 text-lg mb-2", "Invalid share link" }
                    p { class: "text-gray-500 text-sm", "This link is missing the decryption key." }
                }
            }
        }
    }
}

// -- Cloud share --

#[component]
fn CloudShareView(share_id: String, fragment: String) -> Element {
    let share_id_clone = share_id.clone();
    let frag_clone = fragment.clone();

    let data = use_resource(move || {
        let sid = share_id_clone.clone();
        let frag = frag_clone.clone();
        async move {
            let key = decode_share_key(&frag)?;
            let encrypted = api::fetch_share_meta_encrypted(&sid).await?;
            let decrypted = crate::crypto::decrypt(&key, &encrypted)?;
            let meta: api::CloudShareMeta = serde_json::from_slice(&decrypted)
                .map_err(|e| format!("Invalid share metadata: {e}"))?;
            Ok::<_, String>(meta)
        }
    });

    let read = data.read();
    match &*read {
        None => rsx! {
            SharePageShell {
                div { class: "text-gray-400 text-sm", "Loading..." }
            }
        },
        Some(Err(e)) => rsx! {
            SharePageShell {
                div { class: "text-center",
                    p { class: "text-gray-400 text-lg mb-2", "Link unavailable" }
                    p { class: "text-gray-500 text-sm", "{e}" }
                }
            }
        },
        Some(Ok(meta)) => {
            let meta = meta.clone();
            rsx! {
                CloudAlbumView { share_id, meta }
            }
        }
    }
}

#[component]
fn CloudAlbumView(share_id: String, meta: api::CloudShareMeta) -> Element {
    let mut current_track_idx: Signal<Option<usize>> = use_signal(|| None);
    let mut audio_blob_url: Signal<Option<String>> = use_signal(|| None);
    let mut loading_track: Signal<bool> = use_signal(|| false);
    let mut cover_blob_url: Signal<Option<String>> = use_signal(|| None);

    // Clean up blob URLs on unmount
    use_drop({
        let cover = cover_blob_url;
        let audio = audio_blob_url;
        move || {
            if let Some(u) = cover.peek().as_ref() {
                revoke_blob_url(u);
            }
            if let Some(u) = audio.peek().as_ref() {
                revoke_blob_url(u);
            }
        }
    });

    // Load cover art
    let cover_key = meta.cover_image_key.clone();
    let sid_cover = share_id.clone();
    let release_key_b64 = meta.release_key_b64.clone();

    use_effect(move || {
        let cover_key = cover_key.clone();
        let sid = sid_cover.clone();
        let rk_b64 = release_key_b64.clone();
        spawn(async move {
            if let Some(key) = cover_key {
                if let Ok(release_key) = decode_release_key(&rk_b64) {
                    if let Ok(encrypted) = api::fetch_share_file(&sid, &key).await {
                        if let Ok(decrypted) = crate::crypto::decrypt(&release_key, &encrypted) {
                            if let Ok(url) = create_blob_url(&decrypted, "image/jpeg") {
                                cover_blob_url.set(Some(url));
                            }
                        }
                    }
                }
            }
        });
    });

    let tertiary = meta.year.map(|y| format!("({y})"));

    rsx! {
        SharePageShell {
            ShareCard {
                cover_url: cover_blob_url(),
                primary_title: meta.album_name.clone(),
                secondary_line: meta.artist.clone(),
                tertiary_line: tertiary,
                div { class: "mt-4 border-t border-[var(--color-border-subtle)]",
                    for (idx, track) in meta.tracks.iter().enumerate() {
                        CloudTrackRow {
                            idx,
                            track: track.clone(),
                            share_id: share_id.clone(),
                            release_key_b64: meta.release_key_b64.clone(),
                            is_playing: current_track_idx() == Some(idx),
                            is_loading: *loading_track.read() && current_track_idx() == Some(idx),
                            on_click: {
                                let share_id = share_id.clone();
                                let rk_b64 = meta.release_key_b64.clone();
                                let file_key = track.file_key.clone();
                                let format = track.format.clone();
                                move |clicked_idx: usize| {
                                    let share_id = share_id.clone();
                                    let rk_b64 = rk_b64.clone();
                                    let file_key = file_key.clone();
                                    let format = format.clone();
                                    if let Some(old_url) = audio_blob_url.peek().clone() {
                                        revoke_blob_url(&old_url);
                                    }
                                    audio_blob_url.set(None);
                                    current_track_idx.set(Some(clicked_idx));
                                    loading_track.set(true);
                                    spawn(async move {
                                        match load_track_blob(&share_id, &file_key, &rk_b64, &format).await {
                                            Ok(url) => {
                                                audio_blob_url.set(Some(url));
                                                loading_track.set(false);
                                            }
                                            Err(_) => {
                                                loading_track.set(false);
                                            }
                                        }
                                    });
                                }
                            },
                        }
                    }
                }
                if let Some(url) = audio_blob_url() {
                    audio {
                        class: "w-full mt-3",
                        controls: true,
                        autoplay: true,
                        key: "{url}",
                        src: "{url}",
                        onended: {
                            let share_id = share_id.clone();
                            let meta = meta.clone();
                            move |_| {
                                if let Some(current) = current_track_idx() {
                                    let next = current + 1;
                                    if next < meta.tracks.len() {
                                        let track = &meta.tracks[next];
                                        let share_id = share_id.clone();
                                        let file_key = track.file_key.clone();
                                        let rk_b64 = meta.release_key_b64.clone();
                                        let format = track.format.clone();
                                        if let Some(old_url) = audio_blob_url.peek().clone() {
                                            revoke_blob_url(&old_url);
                                        }
                                        audio_blob_url.set(None);
                                        current_track_idx.set(Some(next));
                                        loading_track.set(true);
                                        spawn(async move {
                                            match load_track_blob(&share_id, &file_key, &rk_b64, &format).await {
                                                Ok(url) => {
                                                    audio_blob_url.set(Some(url));
                                                    loading_track.set(false);
                                                }
                                                Err(_) => {
                                                    loading_track.set(false);
                                                }
                                            }
                                        });
                                    } else {
                                        if let Some(old_url) = audio_blob_url.peek().clone() {
                                            revoke_blob_url(&old_url);
                                        }
                                        current_track_idx.set(None);
                                        audio_blob_url.set(None);
                                    }
                                }
                            }
                        },
                    }
                } else if *loading_track.read() {
                    div { class: "flex justify-center mt-3 text-gray-400 text-sm py-2",
                        "Loading track..."
                    }
                }
            }
        }
    }
}

#[component]
fn CloudTrackRow(
    idx: usize,
    track: api::CloudShareTrack,
    share_id: String,
    release_key_b64: String,
    is_playing: bool,
    is_loading: bool,
    on_click: EventHandler<usize>,
) -> Element {
    let mut downloading = use_signal(|| false);
    let highlight = if is_playing {
        "text-[var(--color-accent)]"
    } else {
        "text-gray-300 hover:text-white"
    };

    let number = track.number;
    let title = track.title.clone();
    let duration = track.duration_secs;

    rsx! {
        div { class: "flex items-center gap-1 hover:bg-[var(--color-hover)] rounded",
            button {
                class: "flex-1 flex items-center gap-3 px-2 py-2.5 text-left transition-colors cursor-pointer {highlight} rounded min-w-0",
                onclick: move |_| on_click.call(idx),
                span { class: "w-6 text-right text-xs text-gray-500 shrink-0",
                    if let Some(n) = number {
                        "{n}"
                    }
                }
                span { class: "flex-1 text-sm truncate", "{title}" }
                if is_loading {
                    span { class: "text-xs text-gray-500 shrink-0", "..." }
                } else if let Some(secs) = duration {
                    span { class: "text-xs text-gray-500 shrink-0", "{format_duration(secs)}" }
                }
            }
            button {
                class: "p-2 text-gray-500 hover:text-white transition-colors shrink-0 cursor-pointer",
                title: "Download",
                disabled: *downloading.read(),
                onclick: {
                    let file_key = track.file_key.clone();
                    let format = track.format.clone();
                    let track_title = track.title.clone();
                    let share_id = share_id.clone();
                    let rk_b64 = release_key_b64.clone();
                    move |e: Event<MouseData>| {
                        e.stop_propagation();
                        let file_key = file_key.clone();
                        let format = format.clone();
                        let track_title = track_title.clone();
                        let share_id = share_id.clone();
                        let rk_b64 = rk_b64.clone();
                        downloading.set(true);
                        spawn(async move {
                            if let Ok(url) = load_track_blob(&share_id, &file_key, &rk_b64, &format).await {
                                trigger_download(&url, &format!("{track_title}.{format}"));
                                revoke_blob_url(&url);
                            }
                            downloading.set(false);
                        });
                    }
                },
                DownloadIcon {}
            }
        }
    }
}

// -- Shared components --

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
                        path {
                            d: "M2.25 15.75l5.159-5.159a2.25 2.25 0 013.182 0l5.159 5.159m-1.5-1.5l1.409-1.409a2.25 2.25 0 013.182 0l2.909 2.909M3.75 21h16.5A2.25 2.25 0 0022.5 18.75V5.25A2.25 2.25 0 0020.25 3H3.75A2.25 2.25 0 001.5 5.25v13.5A2.25 2.25 0 003.75 21z",
                        }
                    }
                }
            }
            div { class: "text-center mb-2",
                h1 { class: "text-white text-xl font-semibold truncate", "{primary_title}" }
                p { class: "text-gray-400 text-sm mt-1 truncate", "{secondary_line}" }
                if let Some(tertiary) = tertiary_line {
                    p { class: "text-gray-500 text-xs mt-0.5 truncate", "{tertiary}" }
                }
            }
            {children}
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
