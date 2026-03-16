use neoimv_plugin_sdk::*;
use std::ffi::CString;
use std::path::Path;

/// Builds a PluginContext for passing to plugin calls.
/// Owns the C strings so they remain valid for the call duration.
pub struct ContextBuilder {
    image_path_c: Option<CString>,
    config_c: Option<CString>,
}

impl ContextBuilder {
    pub fn new() -> Self {
        Self {
            image_path_c: None,
            config_c: None,
        }
    }

    pub fn build(
        &mut self,
        image: Option<&ImageBuffer>,
        image_path: Option<&Path>,
        image_count: u32,
        image_index: u32,
        config: Option<&str>,
    ) -> PluginContext {
        self.image_path_c = image_path
            .and_then(|p| p.to_str())
            .and_then(|s| CString::new(s).ok());

        self.config_c = config.and_then(|s| CString::new(s).ok());

        PluginContext {
            image: image.map(|i| i as *const _).unwrap_or(std::ptr::null()),
            image_path: self
                .image_path_c
                .as_ref()
                .map(|c| c.as_ptr())
                .unwrap_or(std::ptr::null()),
            image_count,
            image_index,
            config: self
                .config_c
                .as_ref()
                .map(|c| c.as_ptr())
                .unwrap_or(std::ptr::null()),
        }
    }
}

impl Default for ContextBuilder {
    fn default() -> Self {
        Self::new()
    }
}
