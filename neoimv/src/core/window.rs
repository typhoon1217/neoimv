use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::warn;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowState {
    pub x: Option<f32>,
    pub y: Option<f32>,
    pub width: f32,
    pub height: f32,
    pub fullscreen: bool,
}

impl WindowState {
    /// Default to 80% of primary monitor size, or 800x600 minimum.
    pub fn default_for_monitor(monitor_width: f32, monitor_height: f32) -> Self {
        Self {
            x: None,
            y: None,
            width: (monitor_width * 0.8).max(800.0),
            height: (monitor_height * 0.8).max(600.0),
            fullscreen: false,
        }
    }

    pub fn load(path: &Path) -> Self {
        if !path.exists() {
            return Self::default();
        }
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(content) = toml::to_string_pretty(self) {
            if let Err(e) = std::fs::write(path, content) {
                warn!("Failed to save window state: {}", e);
            }
        }
    }
}

impl Default for WindowState {
    fn default() -> Self {
        // Fallback if monitor size unknown; callers should prefer default_for_monitor().
        Self {
            x: None,
            y: None,
            width: 1024.0,
            height: 768.0,
            fullscreen: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_for_monitor() {
        let state = WindowState::default_for_monitor(1920.0, 1080.0);
        assert_eq!(state.width, 1920.0 * 0.8);
        assert_eq!(state.height, 1080.0 * 0.8);
        assert!(!state.fullscreen);
    }

    #[test]
    fn test_default_for_small_monitor_clamps() {
        let state = WindowState::default_for_monitor(100.0, 100.0);
        assert_eq!(state.width, 800.0);
        assert_eq!(state.height, 600.0);
    }

    #[test]
    fn test_save_and_load_roundtrip() {
        let path = std::env::temp_dir().join("neoimv_window_state_test.toml");
        let original = WindowState {
            x: Some(100.0),
            y: Some(200.0),
            width: 1280.0,
            height: 720.0,
            fullscreen: false,
        };
        original.save(&path);
        let loaded = WindowState::load(&path);
        assert_eq!(loaded.x, original.x);
        assert_eq!(loaded.y, original.y);
        assert_eq!(loaded.width, original.width);
        assert_eq!(loaded.height, original.height);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_load_nonexistent_returns_default() {
        let path = std::env::temp_dir().join("neoimv_window_state_nonexistent.toml");
        let _ = std::fs::remove_file(&path);
        let state = WindowState::load(&path);
        assert_eq!(state.width, 1024.0);
        assert_eq!(state.height, 768.0);
    }
}
