//! State presets for quick configuration switching

use super::registry::{ControlRegistry, ControlValue};
use dioxus::prelude::*;
use std::collections::HashMap;

/// A named preset with predefined control values
#[derive(Clone, PartialEq)]
pub struct Preset {
    pub name: &'static str,
    pub values: HashMap<String, ControlValue>,
}

impl Preset {
    /// Create a new preset with the given name
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            values: HashMap::new(),
        }
    }

    /// Set a boolean value in this preset
    pub fn set_bool(mut self, key: &'static str, value: bool) -> Self {
        self.values
            .insert(key.to_string(), ControlValue::Bool(value));
        self
    }

    /// Set a string/enum value in this preset
    pub fn set_string(mut self, key: &'static str, value: &'static str) -> Self {
        self.values
            .insert(key.to_string(), ControlValue::String(value.to_string()));
        self
    }

    /// Set an integer value in this preset
    pub fn set_int(mut self, key: &'static str, value: i32) -> Self {
        self.values
            .insert(key.to_string(), ControlValue::Int(value));
        self
    }

    /// Check if this preset matches the current registry state.
    /// A preset matches if all controls have their expected values:
    /// - Controls specified in the preset must match the preset's value
    /// - Controls not in the preset must be at their default value
    pub fn matches(&self, registry: &ControlRegistry) -> bool {
        for control in &registry.controls {
            let current_value = registry.values.get(control.key).map(|s| s.read());
            let current_value = current_value.as_deref();

            if let Some(preset_value) = self.values.get(control.key) {
                // Preset specifies this control - must match preset value
                if current_value != Some(preset_value) {
                    return false;
                }
            } else {
                // Preset doesn't specify this control - must be at default
                if current_value != Some(&control.default) {
                    return false;
                }
            }
        }
        true
    }
}
