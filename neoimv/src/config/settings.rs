use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreConfig {
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_fit")]
    pub default_fit: String,
    #[serde(default = "default_true")]
    pub filmstrip: bool,
    #[serde(default = "default_filmstrip_position")]
    pub filmstrip_position: String,
    #[serde(default = "default_filmstrip_height")]
    pub filmstrip_height: u32,
    #[serde(default = "default_true")]
    pub statusbar: bool,
    #[serde(default = "default_true")]
    pub window_remember_position: bool,
    #[serde(default = "default_undo_depth")]
    pub undo_max_depth: usize,
    #[serde(default = "default_undo_memory")]
    pub undo_memory_cap_mb: usize,
    #[serde(default = "default_true")]
    pub thumbnail_cache: bool,
    /// Ordered list of decoder plugin names to try before built-in.
    /// Example: ["avif-decoder", "heif-decoder"]
    /// Empty = built-in only. Plugins listed first get priority.
    #[serde(default)]
    pub decoder_priority: Vec<String>,
    /// Override font path. "system" = auto-detect, otherwise absolute path to .ttf/.otf.
    #[serde(default = "default_font")]
    pub font_override: String,
}

fn default_theme() -> String { "dark".into() }
fn default_fit() -> String { "fit_window".into() }
fn default_true() -> bool { true }
fn default_filmstrip_position() -> String { "bottom".into() }
fn default_filmstrip_height() -> u32 { 80 }
fn default_font() -> String { "system".into() }
fn default_undo_depth() -> usize { 50 }
fn default_undo_memory() -> usize { 512 }

impl Default for CoreConfig {
    fn default() -> Self {
        Self {
            theme: default_theme(),
            default_fit: default_fit(),
            filmstrip: true,
            filmstrip_position: default_filmstrip_position(),
            filmstrip_height: default_filmstrip_height(),
            statusbar: true,
            window_remember_position: true,
            undo_max_depth: default_undo_depth(),
            undo_memory_cap_mb: default_undo_memory(),
            thumbnail_cache: true,
            decoder_priority: Vec::new(),
            font_override: default_font(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    #[serde(default)]
    pub core: CoreConfig,
    #[serde(default)]
    pub plugin: std::collections::HashMap<String, toml::Table>,
}

impl AppConfig {
    pub fn load(path: &Path) -> Self {
        if !path.exists() {
            info!("No config.toml found, using defaults");
            return Self::default();
        }

        match std::fs::read_to_string(path) {
            Ok(content) => toml::from_str(&content).unwrap_or_else(|e| {
                warn!("Failed to parse config.toml: {}, using defaults", e);
                Self::default()
            }),
            Err(e) => {
                warn!("Failed to read config.toml: {}, using defaults", e);
                Self::default()
            }
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), std::io::Error> {
        let content = toml::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, content)
    }

    pub fn plugin_config_toml(&self, plugin_name: &str) -> Option<String> {
        self.plugin.get(plugin_name)
            .and_then(|t| toml::to_string(t).ok())
    }
}

/// Resolve the config directory using the `directories` crate.
pub fn config_dir() -> PathBuf {
    directories::ProjectDirs::from("", "", "neoimv")
        .map(|d| d.config_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from(".config/neoimv"))
}

/// Resolve the cache directory.
pub fn cache_dir() -> PathBuf {
    directories::ProjectDirs::from("", "", "neoimv")
        .map(|d| d.cache_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from(".cache/neoimv"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AppConfig::default();
        assert_eq!(config.core.theme, "dark");
        assert_eq!(config.core.undo_max_depth, 50);
        assert_eq!(config.core.undo_memory_cap_mb, 512);
    }

    #[test]
    fn test_roundtrip_toml() {
        let config = AppConfig::default();
        let toml_str = toml::to_string_pretty(&config).unwrap();
        let loaded: AppConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(loaded.core.theme, "dark");
    }
}
