//! General UI state store (overlays, sidebar, search)

use dioxus::prelude::*;

// =============================================================================
// Overlay System
// =============================================================================

/// A single overlay layer in the stack
#[derive(Clone, Debug, PartialEq)]
pub struct OverlayLayer {
    /// Unique identifier for this overlay
    pub id: String,
    /// The content to render
    pub content: OverlayContent,
    /// Whether clicking the backdrop dismisses this overlay
    pub dismiss_on_backdrop: bool,
    /// Whether the backdrop blocks pointer events
    pub blocks_pointer_events: bool,
}

/// Content types that can be rendered as overlays
#[derive(Clone, Debug, PartialEq)]
pub enum OverlayContent {
    /// Confirmation dialog (delete album, delete release, etc.)
    ConfirmDialog {
        title: String,
        message: String,
        confirm_label: String,
        cancel_label: String,
        on_confirm_action: ConfirmAction,
    },
    /// Release info modal
    ReleaseInfoModal { release_id: String },
    /// Dropdown menu anchored to an element
    Dropdown {
        anchor_id: String,
        menu_type: DropdownMenuType,
    },
}

/// Actions that can be triggered by confirmation dialogs
#[derive(Clone, Debug, PartialEq)]
pub enum ConfirmAction {
    DeleteAlbum { album_id: String },
    DeleteRelease { release_id: String },
    DeleteStorageProfile { profile_id: String },
}

/// Types of dropdown menus
#[derive(Clone, Debug, PartialEq)]
pub enum DropdownMenuType {
    /// Album card context menu (play, add to queue, etc.)
    AlbumCard { album_id: String },
    /// Track row context menu (export, play next, add to queue)
    TrackRow {
        track_id: String,
        release_id: String,
    },
    /// Release tab context menu (view files, delete, export)
    ReleaseTab { release_id: String },
    /// Play album button menu (play, add to queue)
    PlayAlbum { track_ids: Vec<String> },
    /// Album cover section menu (export, delete, view info)
    AlbumCover {
        album_id: String,
        release_id: Option<String>,
    },
}

impl OverlayLayer {
    /// Create a confirmation dialog overlay
    pub fn confirm_dialog(
        id: impl Into<String>,
        title: impl Into<String>,
        message: impl Into<String>,
        confirm_label: impl Into<String>,
        cancel_label: impl Into<String>,
        action: ConfirmAction,
    ) -> Self {
        Self {
            id: id.into(),
            content: OverlayContent::ConfirmDialog {
                title: title.into(),
                message: message.into(),
                confirm_label: confirm_label.into(),
                cancel_label: cancel_label.into(),
                on_confirm_action: action,
            },
            dismiss_on_backdrop: true,
            blocks_pointer_events: true,
        }
    }

    /// Create a modal overlay
    pub fn modal(id: impl Into<String>, content: OverlayContent) -> Self {
        Self {
            id: id.into(),
            content,
            dismiss_on_backdrop: true,
            blocks_pointer_events: true,
        }
    }

    /// Create a dropdown overlay
    pub fn dropdown(
        id: impl Into<String>,
        anchor_id: impl Into<String>,
        menu_type: DropdownMenuType,
    ) -> Self {
        Self {
            id: id.into(),
            content: OverlayContent::Dropdown {
                anchor_id: anchor_id.into(),
                menu_type,
            },
            dismiss_on_backdrop: true,
            blocks_pointer_events: false, // Dropdowns don't block pointer events on backdrop
        }
    }
}

// =============================================================================
// Other UI State
// =============================================================================

/// State for the queue sidebar
#[derive(Clone, Debug, Default, PartialEq, Store)]
pub struct SidebarState {
    pub is_open: bool,
}

/// State for library search
#[derive(Clone, Debug, Default, PartialEq, Store)]
pub struct SearchState {
    pub query: String,
}

// =============================================================================
// Combined UI State
// =============================================================================

/// Combined UI state
#[derive(Clone, Debug, Default, PartialEq, Store)]
pub struct UiState {
    /// Stack of overlay layers (dialogs, modals, dropdowns)
    pub overlays: Vec<OverlayLayer>,
    /// Queue sidebar state
    pub sidebar: SidebarState,
    /// Library search state
    pub search: SearchState,
}
