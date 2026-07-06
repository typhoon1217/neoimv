use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct KeymapConfig {
    #[serde(default)]
    pub core: HashMap<String, String>,
    #[serde(default)]
    pub plugin: HashMap<String, HashMap<String, String>>,
}

impl KeymapConfig {
    pub fn load(path: &Path) -> Self {
        if !path.exists() {
            info!("No keymap.toml found, using defaults");
            return Self::defaults();
        }

        match std::fs::read_to_string(path) {
            Ok(content) => {
                let mut loaded = toml::from_str(&content).unwrap_or_else(|e| {
                    warn!("Failed to parse keymap.toml: {}, using defaults", e);
                    Self::defaults()
                });
                loaded.merge_missing_core_defaults();
                loaded
            }
            Err(e) => {
                warn!("Failed to read keymap.toml: {}, using defaults", e);
                Self::defaults()
            }
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), std::io::Error> {
        let content = toml::to_string_pretty(self).map_err(std::io::Error::other)?;

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, content)
    }

    pub fn defaults() -> Self {
        let mut core = HashMap::new();
        core.insert("Left".into(), "core:prev_image".into());
        core.insert("Right".into(), "core:next_image".into());
        core.insert("Up".into(), "core:zoom_in".into());
        core.insert("Down".into(), "core:zoom_out".into());
        core.insert("f".into(), "core:fit_window".into());
        core.insert("q".into(), "core:quit".into());
        core.insert("Space".into(), "core:next_image".into());
        core.insert("F11".into(), "core:fullscreen".into());
        core.insert("Ctrl+z".into(), "core:undo".into());
        core.insert("Ctrl+Shift+z".into(), "core:redo".into());
        core.insert("Ctrl+c".into(), "core:copy_image".into());
        core.insert("Ctrl+Shift+c".into(), "core:copy_path".into());
        core.insert("F2".into(), "core:rename_file".into());
        core.insert("r".into(), "core:rotate_cw".into());
        core.insert("Shift+r".into(), "core:rotate_ccw".into());
        core.insert("Ctrl+s".into(), "core:save".into());
        core.insert("Ctrl+Shift+s".into(), "core:save_as".into());

        Self {
            core,
            plugin: HashMap::new(),
        }
    }

    fn merge_missing_core_defaults(&mut self) {
        for (key, action) in Self::defaults().core {
            if !self.core.contains_key(&key) && !self.core.values().any(|a| a == &action) {
                self.core.insert(key, action);
            }
        }
    }

    /// Look up action for a key string. Checks core first, then plugins.
    pub fn action_for_key(&self, key: &str) -> Option<&str> {
        if let Some(action) = self.core.get(key) {
            return Some(action);
        }
        for plugin_keys in self.plugin.values() {
            if let Some(action) = plugin_keys.get(key) {
                return Some(action);
            }
        }
        None
    }

    /// Merge plugin default keybindings (doesn't overwrite existing user bindings)
    pub fn merge_plugin_defaults(&mut self, plugin_name: &str, defaults: &[(String, String)]) {
        let all_bound_keys: Vec<String> = self
            .core
            .keys()
            .chain(self.plugin.values().flat_map(|m| m.keys()))
            .cloned()
            .collect();

        let existing = self.plugin.entry(plugin_name.to_string()).or_default();

        for (key, action) in defaults {
            if !all_bound_keys.contains(key) && !existing.values().any(|a| a == action) {
                existing.insert(key.clone(), action.clone());
            } else if !existing.values().any(|a| a == action) {
                warn!(
                    "Key '{}' already bound, skipping default for {}",
                    key, action
                );
            }
        }
    }

    /// Rebind a key to an action. Removes old binding for that action.
    pub fn rebind(&mut self, key: String, action: &str) {
        // Remove existing binding for this action
        self.core.retain(|_, v| v != action);
        for plugin_keys in self.plugin.values_mut() {
            plugin_keys.retain(|_, v| v != action);
        }

        // Determine which section to put it in
        if action.starts_with("core:") {
            self.core.insert(key, action.to_string());
        } else if let Some(plugin_name) = action.split(':').next() {
            self.plugin
                .entry(plugin_name.to_string())
                .or_default()
                .insert(key, action.to_string());
        }
    }

    /// Unbind a key (remove it from whichever section it's in).
    pub fn unbind_key(&mut self, key: &str) {
        self.core.remove(key);
        for plugin_keys in self.plugin.values_mut() {
            plugin_keys.remove(key);
        }
    }

    /// Bind an action to a key. Wrapper around rebind with (action, key) order.
    pub fn bind(&mut self, action: &str, key: &str) {
        self.rebind(key.to_string(), action);
    }

    /// Reverse lookup: find the key bound to an action. Returns first match.
    pub fn key_for_action(&self, action: &str) -> Option<String> {
        for (key, act) in &self.core {
            if act == action {
                return Some(key.clone());
            }
        }
        for plugin_keys in self.plugin.values() {
            for (key, act) in plugin_keys {
                if act == action {
                    return Some(key.clone());
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_defaults_have_arrow_keys() {
        let km = KeymapConfig::defaults();
        assert_eq!(km.action_for_key("Left"), Some("core:prev_image"));
        assert_eq!(km.action_for_key("Right"), Some("core:next_image"));
    }

    #[test]
    fn test_merge_plugin_defaults_no_conflict() {
        let mut km = KeymapConfig::defaults();
        km.merge_plugin_defaults("rotate", &[("Ctrl+r".into(), "rotate:cw".into())]);
        assert_eq!(km.action_for_key("Ctrl+r"), Some("rotate:cw"));
    }

    #[test]
    fn test_merge_plugin_defaults_conflict_skipped() {
        let mut km = KeymapConfig::defaults();
        // "Left" is already bound to core:prev_image
        km.merge_plugin_defaults("rotate", &[("Left".into(), "rotate:cw".into())]);
        // Should NOT overwrite
        assert_eq!(km.action_for_key("Left"), Some("core:prev_image"));
    }

    #[test]
    fn test_rebind() {
        let mut km = KeymapConfig::defaults();
        km.rebind("x".into(), "core:quit");
        assert_eq!(km.action_for_key("x"), Some("core:quit"));
        assert_eq!(km.action_for_key("q"), None); // old binding removed
    }

    #[test]
    fn test_roundtrip_toml() {
        let km = KeymapConfig::defaults();
        let toml_str = toml::to_string_pretty(&km).unwrap();
        let loaded: KeymapConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(loaded.action_for_key("Left"), Some("core:prev_image"));
    }

    #[test]
    fn test_merge_missing_core_defaults() {
        let mut km = KeymapConfig::default();
        km.core.insert("Left".into(), "core:prev_image".into());
        km.merge_missing_core_defaults();
        assert_eq!(km.action_for_key("Ctrl+c"), Some("core:copy_image"));
        assert_eq!(km.action_for_key("r"), Some("core:rotate_cw"));
    }
}
