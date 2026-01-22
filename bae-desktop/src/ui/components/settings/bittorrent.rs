//! BitTorrent section wrapper - handles config state, delegates UI to BitTorrentSectionView

use crate::ui::app_service::use_app;
use bae_ui::stores::{AppStateStoreExt, ConfigStateStoreExt};
use bae_ui::{BitTorrentSectionView, BitTorrentSettings};
use dioxus::prelude::*;

#[component]
pub fn BitTorrentSection() -> Element {
    let app = use_app();

    // Read config from Store
    let config_store = app.state.config();
    let store_listen_port = *config_store.torrent_listen_port().read();
    let store_enable_upnp = *config_store.torrent_enable_upnp().read();
    let store_max_connections = *config_store.torrent_max_connections().read();
    let store_max_connections_per_torrent =
        *config_store.torrent_max_connections_per_torrent().read();
    let store_max_uploads = *config_store.torrent_max_uploads().read();
    let store_max_uploads_per_torrent = *config_store.torrent_max_uploads_per_torrent().read();
    let store_bind_interface = config_store.torrent_bind_interface().read().clone();

    let mut editing_section = use_signal(|| Option::<String>::None);
    let mut is_saving = use_signal(|| false);
    let mut save_error = use_signal(|| Option::<String>::None);

    // Edit state for listening port
    let mut listen_port =
        use_signal(move || store_listen_port.map(|p| p.to_string()).unwrap_or_default());
    let mut enable_upnp = use_signal(move || store_enable_upnp);

    // Edit state for connection limits
    let mut max_connections = use_signal(move || {
        store_max_connections
            .map(|c| c.to_string())
            .unwrap_or_default()
    });
    let mut max_connections_per_torrent = use_signal(move || {
        store_max_connections_per_torrent
            .map(|c| c.to_string())
            .unwrap_or_default()
    });
    let mut max_uploads =
        use_signal(move || store_max_uploads.map(|c| c.to_string()).unwrap_or_default());
    let mut max_uploads_per_torrent = use_signal(move || {
        store_max_uploads_per_torrent
            .map(|c| c.to_string())
            .unwrap_or_default()
    });

    // Edit state for network interface
    let original_bind = store_bind_interface.clone().unwrap_or_default();
    let initial_bind = original_bind.clone();
    let mut bind_interface = use_signal(move || initial_bind.clone());

    // Original values for change detection
    let original_port = store_listen_port.map(|p| p.to_string()).unwrap_or_default();
    let original_upnp = store_enable_upnp;
    let original_max_conn = store_max_connections
        .map(|c| c.to_string())
        .unwrap_or_default();
    let original_max_conn_torrent = store_max_connections_per_torrent
        .map(|c| c.to_string())
        .unwrap_or_default();
    let original_max_up = store_max_uploads.map(|c| c.to_string()).unwrap_or_default();
    let original_max_up_torrent = store_max_uploads_per_torrent
        .map(|c| c.to_string())
        .unwrap_or_default();

    let has_changes = match editing_section.read().as_deref() {
        Some("port") => {
            *listen_port.read() != original_port || *enable_upnp.read() != original_upnp
        }
        Some("limits") => {
            *max_connections.read() != original_max_conn
                || *max_connections_per_torrent.read() != original_max_conn_torrent
                || *max_uploads.read() != original_max_up
                || *max_uploads_per_torrent.read() != original_max_up_torrent
        }
        Some("interface") => *bind_interface.read() != original_bind,
        _ => false,
    };

    let settings = BitTorrentSettings {
        listen_port: store_listen_port,
        enable_upnp: store_enable_upnp,
        enable_natpmp: store_enable_upnp, // NAT-PMP follows UPnP setting
        max_connections: store_max_connections,
        max_connections_per_torrent: store_max_connections_per_torrent,
        max_uploads: store_max_uploads,
        max_uploads_per_torrent: store_max_uploads_per_torrent,
        bind_interface: store_bind_interface,
    };

    let save_changes = {
        let app = app.clone();
        move |_| {
            let section = editing_section.read().clone();

            let new_port: Option<u16> = listen_port.read().parse().ok();
            let new_upnp = *enable_upnp.read();
            let new_max_conn: Option<i32> = max_connections.read().parse().ok();
            let new_max_conn_torrent: Option<i32> = max_connections_per_torrent.read().parse().ok();
            let new_max_up: Option<i32> = max_uploads.read().parse().ok();
            let new_max_up_torrent: Option<i32> = max_uploads_per_torrent.read().parse().ok();
            let new_interface = bind_interface.read().clone();

            is_saving.set(true);
            save_error.set(None);

            app.save_config(move |config| match section.as_deref() {
                Some("port") => {
                    config.torrent_listen_port = new_port;
                    config.torrent_enable_upnp = new_upnp;
                    config.torrent_enable_natpmp = new_upnp;
                }
                Some("limits") => {
                    config.torrent_max_connections = new_max_conn;
                    config.torrent_max_connections_per_torrent = new_max_conn_torrent;
                    config.torrent_max_uploads = new_max_up;
                    config.torrent_max_uploads_per_torrent = new_max_up_torrent;
                }
                Some("interface") => {
                    config.torrent_bind_interface = if new_interface.is_empty() {
                        None
                    } else {
                        Some(new_interface)
                    };
                }
                _ => {}
            });

            is_saving.set(false);
            editing_section.set(None);
        }
    };

    let cancel_edit = move |_| {
        // Reset to original values
        listen_port.set(original_port.clone());
        enable_upnp.set(original_upnp);
        max_connections.set(original_max_conn.clone());
        max_connections_per_torrent.set(original_max_conn_torrent.clone());
        max_uploads.set(original_max_up.clone());
        max_uploads_per_torrent.set(original_max_up_torrent.clone());
        bind_interface.set(original_bind.clone());
        editing_section.set(None);
        save_error.set(None);
    };

    rsx! {
        BitTorrentSectionView {
            settings,
            editing_section: editing_section.read().clone(),
            edit_listen_port: listen_port.read().clone(),
            edit_enable_upnp: *enable_upnp.read(),
            edit_max_connections: max_connections.read().clone(),
            edit_max_connections_per_torrent: max_connections_per_torrent.read().clone(),
            edit_max_uploads: max_uploads.read().clone(),
            edit_max_uploads_per_torrent: max_uploads_per_torrent.read().clone(),
            edit_bind_interface: bind_interface.read().clone(),
            is_saving: *is_saving.read(),
            has_changes,
            save_error: save_error.read().clone(),
            on_edit_section: move |section: String| editing_section.set(Some(section)),
            on_cancel_edit: cancel_edit,
            on_save: save_changes,
            on_listen_port_change: move |val| listen_port.set(val),
            on_enable_upnp_change: move |val| enable_upnp.set(val),
            on_max_connections_change: move |val| max_connections.set(val),
            on_max_connections_per_torrent_change: move |val| max_connections_per_torrent.set(val),
            on_max_uploads_change: move |val| max_uploads.set(val),
            on_max_uploads_per_torrent_change: move |val| max_uploads_per_torrent.set(val),
            on_bind_interface_change: move |val| bind_interface.set(val),
        }
    }
}
