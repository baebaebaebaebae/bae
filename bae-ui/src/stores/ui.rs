//! General UI state store (sidebar, search, library sort)

use crate::display_types::{LibrarySortField, LibraryViewMode, SortCriterion, SortDirection};
use dioxus::prelude::*;

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

/// Persisted sort/view state for the library page
#[derive(Clone, Debug, PartialEq, Store)]
pub struct LibrarySortState {
    pub sort_criteria: Vec<SortCriterion>,
    pub view_mode: LibraryViewMode,
}

impl Default for LibrarySortState {
    fn default() -> Self {
        Self {
            sort_criteria: vec![SortCriterion {
                field: LibrarySortField::DateAdded,
                direction: SortDirection::Descending,
            }],
            view_mode: LibraryViewMode::Albums,
        }
    }
}

/// Combined UI state
#[derive(Clone, Debug, Default, PartialEq, Store)]
pub struct UiState {
    /// Queue sidebar state
    pub sidebar: SidebarState,
    /// Library search state
    pub search: SearchState,
    /// Library sort/view state (persisted across tab switches)
    pub library_sort: LibrarySortState,
}
