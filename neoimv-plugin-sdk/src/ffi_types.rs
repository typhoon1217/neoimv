use std::os::raw::c_char;

pub const CAPABILITY_DECODER: u32 = 1;
pub const CAPABILITY_UI: u32 = 2;
pub const CAPABILITY_ACTIONS: u32 = 4;
pub const CAPABILITY_CONFIG: u32 = 8;

#[repr(C)]
pub struct PluginInfo {
    pub name: *const c_char,
    pub version: *const c_char,
    pub description: *const c_char,
    pub capabilities: u32,
    pub api_version: u32,
}

#[repr(C)]
pub struct ImageBuffer {
    pub pixels: *const u8,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
}

#[repr(C)]
pub struct ActionDef {
    pub id: *const c_char,
    pub label: *const c_char,
    pub default_key: *const c_char,
    pub category: *const c_char,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct ActionDefArray {
    pub items: *const ActionDef,
    pub count: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConfigFieldType {
    Bool = 0,
    Int = 1,
    Float = 2,
    String = 3,
    Color = 4,
    Enum = 5,
}

#[repr(C)]
pub struct ConfigField {
    pub key: *const c_char,
    pub label: *const c_char,
    pub description: *const c_char,
    pub field_type: ConfigFieldType,
    pub default_value: *const c_char,
    pub enum_variants: *const *const c_char,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct ConfigFieldArray {
    pub items: *const ConfigField,
    pub count: u32,
}

#[repr(C)]
pub struct PluginContext {
    pub image: *const ImageBuffer,
    pub image_path: *const c_char,
    pub image_count: u32,
    pub image_index: u32,
    pub config: *const c_char,
}

#[repr(C)]
#[derive(Clone, Copy, PartialEq)]
pub enum PluginResultCode {
    Ok = 0,
    Error = 1,
    Unsupported = 2,
}

#[repr(C)]
pub struct PluginResult {
    pub code: PluginResultCode,
    pub error_message: *const c_char,
}

#[repr(C)]
pub struct MutatedImage {
    pub result: PluginResultCode,
    pub error_message: *const c_char,
    pub pixels: *mut u8,
    pub width: u32,
    pub height: u32,
}

// --- Declarative UI types ---

#[repr(C)]
#[derive(Clone, Copy, PartialEq)]
pub enum UiNodeType {
    Panel = 0,
    Button = 1,
    Label = 2,
    Slider = 3,
    ColorPicker = 4,
    Separator = 5,
    Image = 6,
    Toggle = 7,
}

#[repr(C)]
pub struct UiProps {
    pub id: *const c_char,
    pub label: *const c_char,
    pub action_id: *const c_char,
    pub value_float: f32,
    pub value_bool: u8,
    pub min: f32,
    pub max: f32,
    pub color: [u8; 4],
    pub visible: u8,
}

#[repr(C)]
pub struct UiNode {
    pub node_type: UiNodeType,
    pub children: *const UiNode,
    pub children_count: u32,
    pub props: UiProps,
}

// Safety helpers for constructing PluginResult
impl PluginResult {
    pub fn ok() -> Self {
        Self { code: PluginResultCode::Ok, error_message: std::ptr::null() }
    }
    pub fn error(msg: *const c_char) -> Self {
        Self { code: PluginResultCode::Error, error_message: msg }
    }
}

impl MutatedImage {
    pub fn none() -> Self {
        Self {
            result: PluginResultCode::Ok,
            error_message: std::ptr::null(),
            pixels: std::ptr::null_mut(),
            width: 0,
            height: 0,
        }
    }
}

// Ensure types are Send (needed for background decode thread)
unsafe impl Send for PluginInfo {}
unsafe impl Send for MutatedImage {}
unsafe impl Send for PluginResult {}

// FFI pointer types used in Mutex<> statics inside declare_plugin! macro.
// Safety: these are only accessed behind Mutex guards in the plugin — never
// aliased across threads without synchronisation.
unsafe impl Send for ActionDef {}
unsafe impl Sync for ActionDef {}
unsafe impl Send for ActionDefArray {}
unsafe impl Sync for ActionDefArray {}
unsafe impl Send for ConfigField {}
unsafe impl Sync for ConfigField {}
unsafe impl Send for ConfigFieldArray {}
unsafe impl Sync for ConfigFieldArray {}
unsafe impl Send for UiNode {}
unsafe impl Sync for UiNode {}
unsafe impl Send for UiProps {}
unsafe impl Sync for UiProps {}
