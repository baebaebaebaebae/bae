//! URL state persistence for mock pages
//!
//! Serializes control state as base64-encoded JSON in the query string,
//! keeping URLs opaque and avoiding conflicts with query parameter names.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use std::collections::BTreeMap;

/// Decode a state string from a URL query parameter into key-value pairs.
pub fn parse_state(encoded: &str) -> Vec<(String, String)> {
    if encoded.is_empty() {
        return Vec::new();
    }

    let json_bytes = match URL_SAFE_NO_PAD.decode(encoded) {
        Ok(b) => b,
        Err(_) => return Vec::new(),
    };

    let map: BTreeMap<String, String> = match serde_json::from_slice(&json_bytes) {
        Ok(m) => m,
        Err(_) => return Vec::new(),
    };

    map.into_iter().collect()
}

/// Encode key-value pairs into a base64 state string for the URL.
pub fn build_state(pairs: &[(String, String)]) -> String {
    let map: BTreeMap<&str, &str> = pairs
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let json = serde_json::to_string(&map).expect("state map is always serializable");
    URL_SAFE_NO_PAD.encode(json.as_bytes())
}

/// Builder to collect state changes and produce an encoded state string
pub struct StateBuilder {
    pairs: Vec<(String, String)>,
}

impl StateBuilder {
    pub fn new() -> Self {
        Self { pairs: Vec::new() }
    }

    pub fn set_bool(&mut self, key: &str, value: bool, default: bool) {
        if value != default {
            self.pairs
                .push((key.to_string(), if value { "1" } else { "0" }.to_string()));
        }
    }

    pub fn set_string(&mut self, key: &str, value: &str) {
        self.pairs.push((key.to_string(), value.to_string()));
    }

    pub fn build(self) -> String {
        build_state(&self.pairs)
    }

    pub fn build_option(self) -> Option<String> {
        if self.pairs.is_empty() {
            None
        } else {
            Some(self.build())
        }
    }
}

impl Default for StateBuilder {
    fn default() -> Self {
        Self::new()
    }
}
