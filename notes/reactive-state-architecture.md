# Reactive State Architecture

This document explains how we structure and pass reactive state through the UI layer using Dioxus stores and lenses.

## Core Principle: Pass Lenses Down, Read at Leaf

**Never call `.read()` until you actually need to render a value.**

When you call `.read()` on a signal, that component subscribes to ALL changes to the signal - Dioxus can't track which fields you access. So:

```rust
// BAD: Parent re-renders on ANY state change
fn Parent(state: ReadSignal<ImportState>) {
    let value = state.read().some_field;  // subscribes parent to everything
    rsx! { Child { value } }
}

// GOOD: Only leaf re-renders when its data changes
fn Parent(state: ReadSignal<ImportState>) {
    rsx! { Child { state } }  // just pass through
}

fn Child(state: ReadSignal<ImportState>) {
    let value = state.read().some_field;  // read at leaf
    rsx! { div { "{value}" } }
}
```

## Overview

The UI uses Dioxus's Store system for fine-grained reactivity. Instead of passing cloned data down the component tree (which causes parent components to re-render on every change), we pass *lenses* that allow child components to subscribe directly to the data they need.

## AppState: The Root Store

All application state lives in `AppState`, defined in `bae-ui/src/stores/app.rs`:

```rust
#[derive(Clone, Debug, Default, PartialEq, Store)]
pub struct AppState {
    pub import: ImportState,
    pub library: LibraryState,
    pub playback: PlaybackUiState,
    pub config: ConfigState,
    pub storage_profiles: StorageProfilesState,
    // ...
}
```

The `#[derive(Store)]` macro generates lens methods for each field. These methods return `Store<T, Lens>` objects that implement `Readable` and `Writable` traits, allowing fine-grained access to nested data.

## Lenses Enable Granular Reactivity

A lens is a zero-cost abstraction that "zooms in" on a portion of the state. When you call `.read()` on a lens, only that specific path is subscribed to changes.

```rust
// This subscribes to storage_profiles.profiles specifically
let profiles = app.state.storage_profiles().profiles();

// When profiles change, only components that called .read() on this lens re-render
```

## Pattern: Pass Lenses as Props

Instead of reading state in a parent and passing cloned data down:

```rust
// BAD: Parent subscribes to storage_profiles changes
fn Parent() -> Element {
    let profiles: Vec<StorageProfile> = app.state
        .storage_profiles()
        .read()  // <-- subscribes THIS component
        .profiles
        .clone();
    
    rsx! { Child { profiles } }
}
```

Pass the lens directly:

```rust
// GOOD: Only Child subscribes
fn Parent() -> Element {
    let profiles = app.state.storage_profiles().profiles();
    rsx! { Child { profiles } }
}

#[component]
fn Child(profiles: ReadSignal<Vec<StorageProfile>>) -> Element {
    // Only THIS component re-renders when profiles change
    for profile in profiles.read().iter() {
        // ...
    }
}
```

At component boundaries, store lenses automatically decay to `ReadSignal` or `ReadStore` as needed by the prop type.

## Extension Traits

The Store derive macro generates extension traits that must be imported to use lens methods:

```rust
use bae_ui::stores::{AppStateStoreExt, StorageProfilesStateStoreExt};

// Now you can call:
app.state.storage_profiles().profiles()
```

These are re-exported from `bae_ui::stores` and follow the naming pattern `{TypeName}StoreExt`.

## Reactive Collections

For collections like `Vec<T>` and `HashMap<K, V>`, stores provide per-entry reactivity:

```rust
#[derive(Store)]
struct StorageProfilesState {
    pub profiles: Vec<StorageProfile>,
}

// Iterating with .iter() gives you lenses to individual entries
for (index, profile) in app.state.storage_profiles().profiles().iter() {
    // profile is a lens, not a clone
    ProfileItem { profile }
}
```

When you insert/remove/modify entries, only affected list items re-render.

## Nested Stores

For stores to "see through" nested types, those types must also derive `Store`:

```rust
#[derive(Store)]
pub struct StorageProfile {
    pub id: String,
    pub name: String,
    // ...
}

// Now we can lens into individual profile fields
let name = app.state.storage_profiles().profiles()[0].name();
```

## Don't Create Duplicate "Info" Types

A common anti-pattern is creating simplified "display" versions of types:

```rust
// BAD: Creates mapping overhead and breaks reactivity
struct StorageProfileInfo { id, name, is_default }

let profiles: Vec<StorageProfileInfo> = app.state
    .storage_profiles()
    .read()
    .profiles
    .iter()
    .map(|p| StorageProfileInfo { ... })
    .collect();
```

Instead, use the full type and let components access only the fields they need:

```rust
// GOOD: Pass the lens, components ignore fields they don't use
let profiles = app.state.storage_profiles().profiles();
```

## Summary

1. **Never call `.read()` until you render** - pass signals through intermediate components
2. **AppState is the single source of truth** for UI state
3. **Pass lenses, not cloned data** to preserve granular reactivity
4. **Only leaf components subscribe** to the data they actually render
5. **Import extension traits** to use generated lens methods
6. **Derive Store on nested types** to enable deep lensing
7. **Use full types** instead of creating subset "info" variants
