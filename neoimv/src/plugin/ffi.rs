use libloading::{Library, Symbol};
use neoimv_plugin_sdk::*;
use std::ffi::CStr;
use std::os::raw::c_char;
use std::panic::{catch_unwind, AssertUnwindSafe};
use thiserror::Error;
use tracing::{error, warn};

#[derive(Error, Debug)]
pub enum PluginError {
    #[error("failed to load library: {0}")]
    LoadFailed(#[from] libloading::Error),
    #[error("missing required symbol: {0}")]
    MissingSymbol(String),
    #[error("plugin panicked during {0}")]
    Panicked(String),
    #[error("plugin returned error: {0}")]
    PluginError(String),
    #[error("incompatible API version: plugin={0}, supported={1:?}")]
    IncompatibleApi(u32, Vec<u32>),
}

/// A loaded plugin — wraps the dynamic library and provides safe call wrappers.
pub struct LoadedPlugin {
    _lib: Library,
    info: PluginInfo,
    name: String,

    // Function pointers (resolved at load time)
    fn_init: Option<unsafe extern "C" fn(*const PluginContext) -> PluginResult>,
    fn_shutdown: Option<unsafe extern "C" fn()>,
    fn_free: Option<unsafe extern "C" fn(*mut u8)>,

    // Decoder
    fn_extensions: Option<unsafe extern "C" fn() -> *const *const c_char>,
    fn_decode: Option<unsafe extern "C" fn(*const u8, usize) -> MutatedImage>,

    // Actions
    fn_actions: Option<unsafe extern "C" fn() -> ActionDefArray>,
    fn_on_action: Option<unsafe extern "C" fn(*const c_char, *const PluginContext) -> MutatedImage>,

    // UI
    fn_render_ui: Option<unsafe extern "C" fn(*const PluginContext) -> *const UiNode>,

    // Config
    fn_config_schema: Option<unsafe extern "C" fn() -> ConfigFieldArray>,
    fn_on_config_changed: Option<unsafe extern "C" fn(*const c_char)>,

    // Events
    fn_on_event: Option<unsafe extern "C" fn(*const c_char, *const PluginContext) -> PluginResult>,
}

impl LoadedPlugin {
    /// Load a plugin from a dynamic library path.
    /// Resolves all symbols, checks API version, calls plugin_init.
    pub unsafe fn load(
        path: &std::path::Path,
        supported_api_versions: &[u32],
    ) -> Result<Self, PluginError> {
        let lib = unsafe { Library::new(path)? };

        // Required: plugin_info
        let fn_info: Symbol<unsafe extern "C" fn() -> PluginInfo> =
            unsafe { lib.get(b"plugin_info\0") }
                .map_err(|_| PluginError::MissingSymbol("plugin_info".into()))?;

        let info = catch_unwind(AssertUnwindSafe(|| unsafe { fn_info() }))
            .map_err(|_| PluginError::Panicked("plugin_info".into()))?;

        // Check API version
        if !supported_api_versions.contains(&info.api_version) {
            return Err(PluginError::IncompatibleApi(
                info.api_version,
                supported_api_versions.to_vec(),
            ));
        }

        let name = unsafe { CStr::from_ptr(info.name) }
            .to_str()
            .unwrap_or("unknown")
            .to_string();

        // Resolve optional symbols
        let fn_init = unsafe { lib.get(b"plugin_init\0").ok().map(|s: Symbol<_>| *s) };
        let fn_shutdown = unsafe { lib.get(b"plugin_shutdown\0").ok().map(|s: Symbol<_>| *s) };
        let fn_free = unsafe { lib.get(b"plugin_free\0").ok().map(|s: Symbol<_>| *s) };
        let fn_extensions = unsafe { lib.get(b"plugin_extensions\0").ok().map(|s: Symbol<_>| *s) };
        let fn_decode = unsafe { lib.get(b"plugin_decode\0").ok().map(|s: Symbol<_>| *s) };
        let fn_actions = unsafe { lib.get(b"plugin_actions\0").ok().map(|s: Symbol<_>| *s) };
        let fn_on_action = unsafe { lib.get(b"plugin_on_action\0").ok().map(|s: Symbol<_>| *s) };
        let fn_render_ui = unsafe { lib.get(b"plugin_render_ui\0").ok().map(|s: Symbol<_>| *s) };
        let fn_config_schema =
            unsafe { lib.get(b"plugin_config_schema\0").ok().map(|s: Symbol<_>| *s) };
        let fn_on_config_changed =
            unsafe { lib.get(b"plugin_on_config_changed\0").ok().map(|s: Symbol<_>| *s) };
        let fn_on_event = unsafe { lib.get(b"plugin_on_event\0").ok().map(|s: Symbol<_>| *s) };

        Ok(Self {
            _lib: lib,
            info,
            name,
            fn_init,
            fn_shutdown,
            fn_free,
            fn_extensions,
            fn_decode,
            fn_actions,
            fn_on_action,
            fn_render_ui,
            fn_config_schema,
            fn_on_config_changed,
            fn_on_event,
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn capabilities(&self) -> u32 {
        self.info.capabilities
    }

    pub fn has_capability(&self, cap: u32) -> bool {
        self.info.capabilities & cap != 0
    }

    /// Get raw decode function pointer (Copy+Send, safe to pass to background thread)
    pub fn decode_fn(&self) -> Option<unsafe extern "C" fn(*const u8, usize) -> MutatedImage> {
        self.fn_decode
    }

    /// Get raw free function pointer (Copy+Send)
    pub fn free_fn(&self) -> Option<unsafe extern "C" fn(*mut u8)> {
        self.fn_free
    }

    /// Call plugin_init. Returns Ok(()) or error.
    pub fn init(&self, ctx: &PluginContext) -> Result<(), PluginError> {
        if let Some(f) = self.fn_init {
            let result = catch_unwind(AssertUnwindSafe(|| unsafe { f(ctx as *const _) }))
                .map_err(|_| PluginError::Panicked("plugin_init".into()))?;
            if result.code != PluginResultCode::Ok {
                let msg = if !result.error_message.is_null() {
                    unsafe { CStr::from_ptr(result.error_message) }
                        .to_string_lossy()
                        .to_string()
                } else {
                    "init failed".into()
                };
                return Err(PluginError::PluginError(msg));
            }
        }
        Ok(())
    }

    /// Call plugin_shutdown.
    pub fn shutdown(&self) {
        if let Some(f) = self.fn_shutdown {
            let _ = catch_unwind(AssertUnwindSafe(|| unsafe { f() }));
        }
    }

    /// Call plugin_free to release plugin-allocated memory.
    pub fn free_ptr(&self, ptr: *mut u8) {
        if let Some(f) = self.fn_free {
            if !ptr.is_null() {
                let _ = catch_unwind(AssertUnwindSafe(|| unsafe { f(ptr) }));
            }
        }
    }

    /// Get file extensions this decoder supports.
    pub fn extensions(&self) -> Vec<String> {
        let Some(f) = self.fn_extensions else {
            return vec![];
        };

        let ptr = match catch_unwind(AssertUnwindSafe(|| unsafe { f() })) {
            Ok(p) => p,
            Err(_) => {
                error!("Plugin {} panicked in plugin_extensions", self.name);
                return vec![];
            }
        };

        if ptr.is_null() {
            return vec![];
        }

        let mut exts = Vec::new();
        let mut i = 0;
        loop {
            let ext_ptr = unsafe { *ptr.add(i) };
            if ext_ptr.is_null() {
                break;
            }
            let ext = unsafe { CStr::from_ptr(ext_ptr) }
                .to_string_lossy()
                .to_string();
            exts.push(ext);
            i += 1;
        }
        exts
    }

    /// Decode image data (PURE FUNCTION — safe to call from background thread).
    pub fn decode(&self, data: &[u8]) -> Option<MutatedImage> {
        let f = self.fn_decode?;
        match catch_unwind(AssertUnwindSafe(|| unsafe { f(data.as_ptr(), data.len()) })) {
            Ok(result)
                if result.result == PluginResultCode::Ok && !result.pixels.is_null() =>
            {
                Some(result)
            }
            Ok(result) => {
                if result.result == PluginResultCode::Error && !result.error_message.is_null() {
                    let msg =
                        unsafe { CStr::from_ptr(result.error_message) }.to_string_lossy();
                    warn!("Plugin {} decode error: {}", self.name, msg);
                }
                None
            }
            Err(_) => {
                error!("Plugin {} panicked in plugin_decode", self.name);
                None
            }
        }
    }

    /// Get registered actions.
    pub fn get_actions(&self) -> Vec<(String, String, Option<String>, String)> {
        let Some(f) = self.fn_actions else {
            return vec![];
        };

        let array = match catch_unwind(AssertUnwindSafe(|| unsafe { f() })) {
            Ok(a) => a,
            Err(_) => {
                error!("Plugin {} panicked in plugin_actions", self.name);
                return vec![];
            }
        };

        if array.items.is_null() || array.count == 0 {
            return vec![];
        }

        let mut actions = Vec::new();
        for i in 0..array.count {
            let def = unsafe { &*array.items.add(i as usize) };
            let id = unsafe { CStr::from_ptr(def.id) }
                .to_string_lossy()
                .to_string();
            let label = unsafe { CStr::from_ptr(def.label) }
                .to_string_lossy()
                .to_string();
            let default_key = if def.default_key.is_null() {
                None
            } else {
                Some(
                    unsafe { CStr::from_ptr(def.default_key) }
                        .to_string_lossy()
                        .to_string(),
                )
            };
            let category = unsafe { CStr::from_ptr(def.category) }
                .to_string_lossy()
                .to_string();
            actions.push((id, label, default_key, category));
        }
        actions
    }

    /// Call plugin_on_action. Returns copied pixel buffer if mutation occurred.
    pub fn on_action(
        &self,
        action_id: &std::ffi::CString,
        ctx: &PluginContext,
    ) -> Option<(Vec<u8>, u32, u32)> {
        let f = self.fn_on_action?;

        let result =
            match catch_unwind(AssertUnwindSafe(|| unsafe { f(action_id.as_ptr(), ctx as *const _) })) {
                Ok(r) => r,
                Err(_) => {
                    error!("Plugin {} panicked in plugin_on_action", self.name);
                    return None;
                }
            };

        if result.result != PluginResultCode::Ok || result.pixels.is_null() {
            if result.result == PluginResultCode::Error && !result.error_message.is_null() {
                let msg =
                    unsafe { CStr::from_ptr(result.error_message) }.to_string_lossy();
                warn!("Plugin {} action error: {}", self.name, msg);
            }
            return None;
        }

        // CRITICAL: Copy pixels immediately, then free plugin allocation
        let size = (result.width * result.height * 4) as usize;
        let pixels = unsafe { std::slice::from_raw_parts(result.pixels, size) }.to_vec();
        self.free_ptr(result.pixels);

        Some((pixels, result.width, result.height))
    }

    /// Get the UI tree (plugin-cached, do not free).
    pub fn render_ui(&self, ctx: &PluginContext) -> Option<*const UiNode> {
        let f = self.fn_render_ui?;
        match catch_unwind(AssertUnwindSafe(|| unsafe { f(ctx as *const _) })) {
            Ok(ptr) if !ptr.is_null() => Some(ptr),
            Ok(_) => None,
            Err(_) => {
                error!("Plugin {} panicked in plugin_render_ui", self.name);
                None
            }
        }
    }

    /// Send event notification.
    pub fn on_event(&self, event_id: &std::ffi::CString, ctx: &PluginContext) {
        let Some(f) = self.fn_on_event else { return };
        let _ =
            catch_unwind(AssertUnwindSafe(|| unsafe { f(event_id.as_ptr(), ctx as *const _) }));
    }

    /// Call plugin_on_config_changed with a JSON config string.
    pub fn on_config_changed(&self, config_json: &std::ffi::CString) {
        let Some(f) = self.fn_on_config_changed else { return };
        let _ = catch_unwind(AssertUnwindSafe(|| unsafe { f(config_json.as_ptr()) }));
    }

    /// Get config schema as Rust-friendly types.
    pub fn config_schema(&self) -> Vec<super::manager::ConfigFieldInfo> {
        let Some(f) = self.fn_config_schema else {
            return vec![];
        };
        let array = match catch_unwind(AssertUnwindSafe(|| unsafe { f() })) {
            Ok(a) => a,
            Err(_) => {
                error!("Plugin {} panicked in plugin_config_schema", self.name);
                return vec![];
            }
        };
        if array.items.is_null() || array.count == 0 {
            return vec![];
        }

        let mut fields = Vec::new();
        for i in 0..array.count {
            let field = unsafe { &*array.items.add(i as usize) };
            let key = unsafe { CStr::from_ptr(field.key) }
                .to_string_lossy()
                .to_string();
            let label = unsafe { CStr::from_ptr(field.label) }
                .to_string_lossy()
                .to_string();
            let description = unsafe { CStr::from_ptr(field.description) }
                .to_string_lossy()
                .to_string();
            let default_value = unsafe { CStr::from_ptr(field.default_value) }
                .to_string_lossy()
                .to_string();

            // Parse null-terminated enum_variants array
            let mut enum_variants = Vec::new();
            if !field.enum_variants.is_null() {
                let mut j = 0;
                loop {
                    let vptr = unsafe { *field.enum_variants.add(j) };
                    if vptr.is_null() {
                        break;
                    }
                    enum_variants.push(
                        unsafe { CStr::from_ptr(vptr) }
                            .to_string_lossy()
                            .to_string(),
                    );
                    j += 1;
                }
            }

            fields.push(super::manager::ConfigFieldInfo {
                key,
                label,
                description,
                field_type: field.field_type,
                default_value,
                enum_variants,
            });
        }
        fields
    }
}

impl Drop for LoadedPlugin {
    fn drop(&mut self) {
        self.shutdown();
    }
}
