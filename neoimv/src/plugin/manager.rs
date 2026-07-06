use super::ffi::{LoadedPlugin, PluginError};
use neoimv_plugin_sdk::*;
use std::path::{Path, PathBuf};
use tracing::{error, info};

const SUPPORTED_API_VERSIONS: &[u32] = &[1];

pub struct PluginManager {
    plugins: Vec<LoadedPlugin>,
    plugin_dir: PathBuf,
}

impl PluginManager {
    pub fn new(plugin_dir: PathBuf) -> Self {
        Self {
            plugins: Vec::new(),
            plugin_dir,
        }
    }

    /// Load all plugins from the plugin directory.
    pub fn load_all(&mut self) {
        if !self.plugin_dir.exists() {
            info!(
                "Plugin directory does not exist: {}",
                self.plugin_dir.display()
            );
            return;
        }

        let mut entries: Vec<PathBuf> = std::fs::read_dir(&self.plugin_dir)
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| is_plugin_file(p))
            .collect();

        // Alphabetical order for deterministic loading
        entries.sort();

        for path in entries {
            match self.load_plugin(&path) {
                Ok(()) => info!("Loaded plugin: {}", path.display()),
                Err(e) => error!("Failed to load {}: {}", path.display(), e),
            }
        }
    }

    pub fn load_plugin(&mut self, path: &Path) -> Result<(), PluginError> {
        let plugin = unsafe { LoadedPlugin::load(path, SUPPORTED_API_VERSIONS)? };

        // Initialize with empty context
        let empty_ctx = PluginContext {
            image: std::ptr::null(),
            image_path: std::ptr::null(),
            image_count: 0,
            image_index: 0,
            config: std::ptr::null(),
        };
        plugin.init(&empty_ctx)?;

        self.plugins.push(plugin);
        Ok(())
    }

    pub fn unload_plugin(&mut self, name: &str) -> bool {
        if let Some(pos) = self.plugins.iter().position(|p| p.name() == name) {
            self.plugins.remove(pos); // Drop calls shutdown()
            true
        } else {
            false
        }
    }

    pub fn plugins(&self) -> &[LoadedPlugin] {
        &self.plugins
    }

    pub fn plugins_mut(&mut self) -> &mut [LoadedPlugin] {
        &mut self.plugins
    }

    /// Find decoder plugins that handle a given extension.
    pub fn decoders_for_extension(&self, ext: &str) -> Vec<&LoadedPlugin> {
        self.plugins
            .iter()
            .filter(|p| p.has_capability(CAPABILITY_DECODER))
            .filter(|p| p.extensions().iter().any(|e| e.eq_ignore_ascii_case(ext)))
            .collect()
    }

    /// Get all plugins with UI capability.
    pub fn ui_plugins(&self) -> Vec<&LoadedPlugin> {
        self.plugins
            .iter()
            .filter(|p| p.has_capability(CAPABILITY_UI))
            .collect()
    }

    /// Get all plugins with Actions capability.
    pub fn action_plugins(&self) -> Vec<&LoadedPlugin> {
        self.plugins
            .iter()
            .filter(|p| p.has_capability(CAPABILITY_ACTIONS))
            .collect()
    }

    /// Get a plugin by name.
    pub fn get(&self, name: &str) -> Option<&LoadedPlugin> {
        self.plugins.iter().find(|p| p.name() == name)
    }

    /// Get all loaded plugin names.
    pub fn plugin_names(&self) -> Vec<String> {
        self.plugins.iter().map(|p| p.name().to_string()).collect()
    }

    /// Get config schema for a plugin as Rust-friendly types.
    pub fn config_schema(&self, name: &str) -> Option<Vec<ConfigFieldInfo>> {
        let plugin = self.get(name)?;
        Some(plugin.config_schema())
    }

    /// Broadcast an event to all loaded plugins.
    pub fn broadcast_event(&self, event_id: &str, ctx: &PluginContext) {
        let c_event = std::ffi::CString::new(event_id).unwrap();
        for plugin in &self.plugins {
            plugin.on_event(&c_event, ctx);
        }
    }
}

/// Default plugin search path for the current platform.
pub fn default_plugin_dir() -> PathBuf {
    #[cfg(target_os = "linux")]
    {
        // Use $XDG_DATA_HOME if set, otherwise ~/.local/share
        let base = if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
            if !xdg.is_empty() {
                PathBuf::from(xdg)
            } else {
                xdg_data_home_fallback()
            }
        } else {
            xdg_data_home_fallback()
        };
        base.join("neoimv").join("plugins")
    }
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var("HOME").unwrap_or_default();
        PathBuf::from(home).join("Library/Application Support/neoimv/plugins")
    }
    #[cfg(target_os = "windows")]
    {
        let appdata = std::env::var("APPDATA").unwrap_or_default();
        PathBuf::from(appdata).join("neoimv/plugins")
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        PathBuf::from("plugins")
    }
}

#[cfg(target_os = "linux")]
fn xdg_data_home_fallback() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(home).join(".local/share")
}

/// Rust-friendly representation of a plugin config field (converted from FFI)
#[derive(Debug, Clone)]
pub struct ConfigFieldInfo {
    pub key: String,
    pub label: String,
    pub description: String,
    pub field_type: ConfigFieldType,
    pub default_value: String,
    pub enum_variants: Vec<String>,
}

fn is_plugin_file(path: &Path) -> bool {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    #[cfg(target_os = "linux")]
    {
        ext == "so"
    }
    #[cfg(target_os = "macos")]
    {
        ext == "dylib"
    }
    #[cfg(target_os = "windows")]
    {
        ext == "dll"
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        false
    }
}
