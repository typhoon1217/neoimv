/// Rust-friendly action definition (converted to ActionDef by macro)
pub struct Action {
    pub id: &'static str,
    pub label: &'static str,
    pub default_key: Option<&'static str>,
    pub category: &'static str,
}

/// Rust-friendly config field
pub struct ConfigFieldDef {
    pub key: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub field_type: crate::ffi_types::ConfigFieldType,
    pub default_value: &'static str,
    pub enum_variants: Option<&'static [&'static str]>,
}

/// Capabilities bitflags
pub struct Capabilities;

impl Capabilities {
    pub const DECODER: u32 = crate::ffi_types::CAPABILITY_DECODER;
    pub const UI: u32 = crate::ffi_types::CAPABILITY_UI;
    pub const ACTIONS: u32 = crate::ffi_types::CAPABILITY_ACTIONS;
    pub const CONFIG: u32 = crate::ffi_types::CAPABILITY_CONFIG;
}

/// Rust-friendly image for plugin returns
pub struct Image {
    pub pixels: Vec<u8>,  // RGBA8
    pub width: u32,
    pub height: u32,
}

/// Read-only context passed to plugin methods
pub struct Context {
    pub image_pixels: Option<Vec<u8>>,
    pub image_width: u32,
    pub image_height: u32,
    pub image_path: Option<String>,
    pub image_count: u32,
    pub image_index: u32,
    pub config: Option<String>,
}

/// Plugin info definition (re-exported from macros for convenience)
pub use crate::macros::PluginInfoDef;
