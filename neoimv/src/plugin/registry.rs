use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginEntry {
    pub name: String,
    pub version: String,
    pub source: String,       // GitHub repo URL
    pub enabled: bool,
    pub api_version: u32,
    pub lib_filename: String, // e.g. "libneoimv_rotate.so"
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PluginRegistry {
    #[serde(default)]
    pub plugins: Vec<PluginEntry>,
}

impl PluginRegistry {
    pub fn load(path: &Path) -> Self {
        if !path.exists() {
            return Self::default();
        }
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_else(|| {
                warn!("Failed to parse plugin registry at {}", path.display());
                Self::default()
            })
    }

    pub fn save(&self, path: &Path) -> Result<(), std::io::Error> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(path, content)?;
        info!("Saved plugin registry to {}", path.display());
        Ok(())
    }

    pub fn find(&self, name: &str) -> Option<&PluginEntry> {
        self.plugins.iter().find(|p| p.name == name)
    }

    pub fn find_mut(&mut self, name: &str) -> Option<&mut PluginEntry> {
        self.plugins.iter_mut().find(|p| p.name == name)
    }

    pub fn remove(&mut self, name: &str) -> bool {
        let len = self.plugins.len();
        self.plugins.retain(|p| p.name != name);
        self.plugins.len() < len
    }

    pub fn enabled_plugins(&self) -> Vec<&PluginEntry> {
        self.plugins.iter().filter(|p| p.enabled).collect()
    }
}
