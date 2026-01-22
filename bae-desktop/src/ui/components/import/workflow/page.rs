#[cfg(feature = "cd-rip")]
use super::cd_import::CdImport;
use super::folder_import::FolderImport;
#[cfg(feature = "torrent")]
use super::torrent_import::TorrentImport;
use crate::ui::app_service::use_app;
use crate::ui::components::dialog_context::DialogContext;
use crate::ui::import_helpers::try_switch_import_source;
use bae_ui::stores::AppStateStoreExt;
use bae_ui::{ImportSource, ImportView};
use dioxus::prelude::*;

#[component]
pub fn ImportPage() -> Element {
    let app = use_app();
    let dialog = use_context::<DialogContext>();

    let import_store = app.state.import();
    let selected_source = import_store.read().selected_import_source;

    let on_source_select = {
        let app = app.clone();
        move |source: ImportSource| {
            try_switch_import_source(&app, &dialog, source);
        }
    };

    rsx! {
        ImportView { selected_source, on_source_select,
            match selected_source {
                ImportSource::Folder => rsx! {
                    FolderImport {}
                },
                #[cfg(feature = "torrent")]
                ImportSource::Torrent => rsx! {
                    TorrentImport {}
                },
                #[cfg(feature = "cd-rip")]
                ImportSource::Cd => rsx! {
                    CdImport {}
                },
                #[cfg(not(all(feature = "torrent", feature = "cd-rip")))]
                _ => rsx! {
                    div { class: "p-4 text-red-500", "This import source is not available" }
                },
            }
        }
    }
}
