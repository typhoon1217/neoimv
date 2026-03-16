/// Wrapper that makes a raw-pointer-containing type usable in static Mutex.
/// Safety: plugin statics are only accessed while holding the Mutex lock.
pub struct FfiSyncCell<T>(pub T);
unsafe impl<T> Send for FfiSyncCell<T> {}
unsafe impl<T> Sync for FfiSyncCell<T> {}

/// The trait plugin authors implement.
pub trait NeoimvPlugin: Send {
    fn info(&self) -> PluginInfoDef;
    fn capabilities(&self) -> u32;

    fn actions(&self) -> Vec<crate::types::Action> { vec![] }
    fn on_action(&mut self, _action: &str, _ctx: &crate::types::Context) -> Option<crate::types::Image> { None }

    fn render_ui(&mut self, _ctx: &crate::types::Context) -> Option<Vec<crate::ffi_types::UiNode>> { None }

    fn config_schema(&self) -> Vec<crate::types::ConfigFieldDef> { vec![] }
    fn on_config_changed(&mut self, _config: &str) {}

    fn on_event(&mut self, _event: &str, _ctx: &crate::types::Context) {}

    fn init(&mut self, _ctx: &crate::types::Context) -> Result<(), String> { Ok(()) }
    fn shutdown(&mut self) {}
}

/// Decoder trait — separate because it must be callable without &mut self
pub trait NeoimvDecoder: Send {
    fn extensions(&self) -> Vec<&'static str>;
    fn decode(&self, data: &[u8]) -> Result<crate::types::Image, String>;
}

/// Plugin info definition
pub struct PluginInfoDef {
    pub name: &'static str,
    pub version: &'static str,
    pub description: &'static str,
}

/// Macro that generates all extern "C" entry points from a NeoimvPlugin impl.
/// Requires: $plugin_type must implement Default + NeoimvPlugin.
/// Optionally implements NeoimvDecoder for decoder capability.
#[macro_export]
macro_rules! declare_plugin {
    ($plugin_type:ty) => {
        use std::sync::Mutex;
        use std::ffi::CString;
        use std::os::raw::c_char;

        static INSTANCE: Mutex<Option<Box<$plugin_type>>> = Mutex::new(None);
        static LEAKED_STRINGS: Mutex<$crate::macros::FfiSyncCell<Vec<*const c_char>>> =
            Mutex::new($crate::macros::FfiSyncCell(Vec::new()));

        // Track Vec allocations: (ptr, len, capacity) for correct deallocation
        static ALLOC_REGISTRY: Mutex<Vec<(usize, usize, usize)>> = Mutex::new(Vec::new());

        // Poison-safe lock: if a prior panic poisoned the mutex, recover the inner data.
        // This prevents a single plugin panic from permanently disabling the plugin.
        fn lock_or_recover<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
            m.lock().unwrap_or_else(|e| e.into_inner())
        }

        fn leak_cstr(s: &str) -> *const c_char {
            let c = CString::new(s).unwrap_or_else(|_| CString::new("").unwrap());
            let ptr = c.into_raw() as *const c_char;
            lock_or_recover(&LEAKED_STRINGS).0.push(ptr);
            ptr
        }

        fn ctx_from_ffi(ctx: *const $crate::ffi_types::PluginContext) -> $crate::types::Context {
            if ctx.is_null() {
                return $crate::types::Context {
                    image_pixels: None, image_width: 0, image_height: 0,
                    image_path: None, image_count: 0, image_index: 0, config: None,
                };
            }
            let c = unsafe { &*ctx };
            let image_path = if c.image_path.is_null() { None }
                else { Some(unsafe { std::ffi::CStr::from_ptr(c.image_path) }.to_string_lossy().into()) };
            let config = if c.config.is_null() { None }
                else { Some(unsafe { std::ffi::CStr::from_ptr(c.config) }.to_string_lossy().into()) };
            let (image_pixels, image_width, image_height) = if c.image.is_null() {
                (None, 0, 0)
            } else {
                let img = unsafe { &*c.image };
                let size = (img.width * img.height * 4) as usize;
                let pixels = unsafe { std::slice::from_raw_parts(img.pixels, size) }.to_vec();
                (Some(pixels), img.width, img.height)
            };
            $crate::types::Context {
                image_pixels, image_width, image_height,
                image_path, image_count: c.image_count, image_index: c.image_index, config,
            }
        }

        fn image_to_mutated(img: Option<$crate::types::Image>) -> $crate::ffi_types::MutatedImage {
            match img {
                None => $crate::ffi_types::MutatedImage::none(),
                Some(image) => {
                    let mut pixels = image.pixels;
                    let ptr = pixels.as_mut_ptr();
                    let len = pixels.len();
                    let cap = pixels.capacity();
                    std::mem::forget(pixels); // prevent drop — core will call plugin_free
                    lock_or_recover(&ALLOC_REGISTRY).push((ptr as usize, len, cap));
                    $crate::ffi_types::MutatedImage {
                        result: $crate::ffi_types::PluginResultCode::Ok,
                        error_message: std::ptr::null(),
                        pixels: ptr, width: image.width, height: image.height,
                    }
                }
            }
        }

        // --- Required entry points ---

        #[no_mangle]
        pub extern "C" fn plugin_info() -> $crate::ffi_types::PluginInfo {
            let temp = <$plugin_type as Default>::default();
            let info = $crate::macros::NeoimvPlugin::info(&temp);
            $crate::ffi_types::PluginInfo {
                name: leak_cstr(info.name),
                version: leak_cstr(info.version),
                description: leak_cstr(info.description),
                capabilities: $crate::macros::NeoimvPlugin::capabilities(&temp),
                api_version: 1,
            }
        }

        #[no_mangle]
        pub extern "C" fn plugin_init(ctx: *const $crate::ffi_types::PluginContext) -> $crate::ffi_types::PluginResult {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let mut instance = Box::new(<$plugin_type as Default>::default());
                let rust_ctx = ctx_from_ffi(ctx);
                match $crate::macros::NeoimvPlugin::init(&mut *instance, &rust_ctx) {
                    Ok(()) => { *lock_or_recover(&INSTANCE) = Some(instance); $crate::ffi_types::PluginResult::ok() }
                    Err(msg) => $crate::ffi_types::PluginResult::error(leak_cstr(&msg)),
                }
            })).unwrap_or_else(|_| $crate::ffi_types::PluginResult::error(leak_cstr("panic in plugin_init")))
        }

        #[no_mangle]
        pub extern "C" fn plugin_shutdown() {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                if let Some(mut inst) = lock_or_recover(&INSTANCE).take() {
                    $crate::macros::NeoimvPlugin::shutdown(&mut *inst);
                }
                for ptr in lock_or_recover(&LEAKED_STRINGS).0.drain(..) {
                    unsafe { drop(CString::from_raw(ptr as *mut c_char)); }
                }
                lock_or_recover(&ALLOC_REGISTRY).clear();
            }));
        }

        #[no_mangle]
        pub extern "C" fn plugin_free(ptr: *mut u8) {
            if ptr.is_null() { return; }
            // Reconstruct the Vec with correct (ptr, len, capacity) to free properly
            let mut registry = lock_or_recover(&ALLOC_REGISTRY);
            if let Some(pos) = registry.iter().position(|(p, _, _)| *p == ptr as usize) {
                let (_, len, cap) = registry.remove(pos);
                unsafe { drop(Vec::from_raw_parts(ptr, len, cap)); }
            }
        }

        // --- Actions entry points ---

        // Cached actions array — built once, returned on subsequent calls
        static ACTIONS_CACHE: Mutex<Option<($crate::ffi_types::ActionDefArray, Vec<$crate::ffi_types::ActionDef>)>> = Mutex::new(None);

        #[no_mangle]
        pub extern "C" fn plugin_actions() -> $crate::ffi_types::ActionDefArray {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                // Return cached if already built
                let cache = lock_or_recover(&ACTIONS_CACHE);
                if let Some((array, _)) = cache.as_ref() {
                    return *array;
                }
                drop(cache);

                let guard = lock_or_recover(&INSTANCE);
                let Some(inst) = guard.as_ref() else {
                    return $crate::ffi_types::ActionDefArray { items: std::ptr::null(), count: 0 };
                };
                let actions = $crate::macros::NeoimvPlugin::actions(&**inst);
                let defs: Vec<$crate::ffi_types::ActionDef> = actions.iter().map(|a| {
                    $crate::ffi_types::ActionDef {
                        id: leak_cstr(a.id), label: leak_cstr(a.label),
                        default_key: a.default_key.map(|k| leak_cstr(k)).unwrap_or(std::ptr::null()),
                        category: leak_cstr(a.category),
                    }
                }).collect();
                let count = defs.len() as u32;
                let ptr = defs.as_ptr();
                let array = $crate::ffi_types::ActionDefArray { items: ptr, count };
                // Cache the Vec (keeps allocation alive) and the array struct
                *lock_or_recover(&ACTIONS_CACHE) = Some((array, defs));
                array
            })).unwrap_or($crate::ffi_types::ActionDefArray { items: std::ptr::null(), count: 0 })
        }

        #[no_mangle]
        pub extern "C" fn plugin_on_action(
            action_id: *const c_char, ctx: *const $crate::ffi_types::PluginContext,
        ) -> $crate::ffi_types::MutatedImage {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let id = unsafe { std::ffi::CStr::from_ptr(action_id) }.to_string_lossy();
                let rust_ctx = ctx_from_ffi(ctx);
                let mut guard = lock_or_recover(&INSTANCE);
                let Some(inst) = guard.as_mut() else { return $crate::ffi_types::MutatedImage::none(); };
                image_to_mutated($crate::macros::NeoimvPlugin::on_action(&mut **inst, &id, &rust_ctx))
            })).unwrap_or($crate::ffi_types::MutatedImage::none())
        }

        // --- UI entry point ---

        static UI_CACHE: Mutex<Vec<$crate::ffi_types::UiNode>> = Mutex::new(Vec::new());

        #[no_mangle]
        pub extern "C" fn plugin_render_ui(
            ctx: *const $crate::ffi_types::PluginContext,
        ) -> *const $crate::ffi_types::UiNode {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let rust_ctx = ctx_from_ffi(ctx);
                let mut guard = lock_or_recover(&INSTANCE);
                let Some(inst) = guard.as_mut() else { return std::ptr::null(); };
                match $crate::macros::NeoimvPlugin::render_ui(&mut **inst, &rust_ctx) {
                    Some(nodes) => {
                        let mut cache = lock_or_recover(&UI_CACHE);
                        *cache = nodes;
                        cache.as_ptr()
                    }
                    None => std::ptr::null(),
                }
            })).unwrap_or(std::ptr::null())
        }

        // --- Config entry points ---

        // Cached config schema — built once
        static CONFIG_CACHE: Mutex<Option<($crate::ffi_types::ConfigFieldArray, Vec<$crate::ffi_types::ConfigField>)>> = Mutex::new(None);
        // Keep enum variant pointer arrays alive alongside the cache
        static ENUM_VARIANT_PTRS: Mutex<$crate::macros::FfiSyncCell<Vec<Vec<*const c_char>>>> =
            Mutex::new($crate::macros::FfiSyncCell(Vec::new()));

        #[no_mangle]
        pub extern "C" fn plugin_config_schema() -> $crate::ffi_types::ConfigFieldArray {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let cache = lock_or_recover(&CONFIG_CACHE);
                if let Some((array, _)) = cache.as_ref() {
                    return *array;
                }
                drop(cache);

                let guard = lock_or_recover(&INSTANCE);
                let Some(inst) = guard.as_ref() else {
                    return $crate::ffi_types::ConfigFieldArray { items: std::ptr::null(), count: 0 };
                };
                let schema = $crate::macros::NeoimvPlugin::config_schema(&**inst);
                let mut variant_arrays = lock_or_recover(&ENUM_VARIANT_PTRS);
                let fields: Vec<$crate::ffi_types::ConfigField> = schema.iter().map(|f| {
                    // Convert enum_variants to null-terminated *const c_char array
                    let enum_variants = if let Some(variants) = &f.enum_variants {
                        let mut ptrs: Vec<*const c_char> = variants.iter()
                            .map(|v| leak_cstr(v))
                            .collect();
                        ptrs.push(std::ptr::null()); // null terminator
                        let ptr = ptrs.as_ptr();
                        variant_arrays.0.push(ptrs); // keep alive
                        ptr
                    } else {
                        std::ptr::null()
                    };
                    $crate::ffi_types::ConfigField {
                        key: leak_cstr(f.key), label: leak_cstr(f.label),
                        description: leak_cstr(f.description),
                        field_type: f.field_type, default_value: leak_cstr(f.default_value),
                        enum_variants,
                    }
                }).collect();
                drop(variant_arrays);
                let count = fields.len() as u32;
                let ptr = fields.as_ptr();
                let array = $crate::ffi_types::ConfigFieldArray { items: ptr, count };
                *lock_or_recover(&CONFIG_CACHE) = Some((array, fields));
                array
            })).unwrap_or($crate::ffi_types::ConfigFieldArray { items: std::ptr::null(), count: 0 })
        }

        #[no_mangle]
        pub extern "C" fn plugin_on_config_changed(config: *const c_char) {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let cfg = unsafe { std::ffi::CStr::from_ptr(config) }.to_string_lossy();
                let mut guard = lock_or_recover(&INSTANCE);
                if let Some(inst) = guard.as_mut() {
                    $crate::macros::NeoimvPlugin::on_config_changed(&mut **inst, &cfg);
                }
            }));
        }

        // --- Event entry point ---

        #[no_mangle]
        pub extern "C" fn plugin_on_event(
            event_id: *const c_char, ctx: *const $crate::ffi_types::PluginContext,
        ) -> $crate::ffi_types::PluginResult {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let id = unsafe { std::ffi::CStr::from_ptr(event_id) }.to_string_lossy();
                let rust_ctx = ctx_from_ffi(ctx);
                let mut guard = lock_or_recover(&INSTANCE);
                if let Some(inst) = guard.as_mut() {
                    $crate::macros::NeoimvPlugin::on_event(&mut **inst, &id, &rust_ctx);
                }
                $crate::ffi_types::PluginResult::ok()
            })).unwrap_or_else(|_| $crate::ffi_types::PluginResult::error(leak_cstr("panic in plugin_on_event")))
        }
    };

    // Variant for plugins that also implement NeoimvDecoder (decoder capability)
    ($plugin_type:ty, decoder) => {
        // Generate all standard entry points
        $crate::declare_plugin!($plugin_type);

        // --- Decoder entry points (pure functions, safe to call from any thread) ---

        // Cached extensions — built once
        static EXTENSIONS_CACHE: Mutex<Option<(Vec<*const c_char>, *const *const c_char)>> = Mutex::new(None);

        #[no_mangle]
        pub extern "C" fn plugin_extensions() -> *const *const c_char {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let cache = lock_or_recover(&EXTENSIONS_CACHE);
                if let Some((_, ptr)) = cache.as_ref() {
                    return *ptr;
                }
                drop(cache);

                let temp = <$plugin_type as Default>::default();
                let exts = $crate::macros::NeoimvDecoder::extensions(&temp);
                let mut ptrs: Vec<*const c_char> = exts.iter()
                    .map(|e| leak_cstr(e))
                    .collect();
                ptrs.push(std::ptr::null()); // null terminator
                let ptr = ptrs.as_ptr();
                *lock_or_recover(&EXTENSIONS_CACHE) = Some((ptrs, ptr));
                ptr
            })).unwrap_or(std::ptr::null())
        }

        /// Pure decoder function — no instance state, safe for background threads.
        #[no_mangle]
        pub extern "C" fn plugin_decode(
            data: *const u8, len: usize,
        ) -> $crate::ffi_types::MutatedImage {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                if data.is_null() || len == 0 {
                    return $crate::ffi_types::MutatedImage::none();
                }
                let slice = unsafe { std::slice::from_raw_parts(data, len) };
                // Create a temporary instance — decode is stateless per spec
                let decoder = <$plugin_type as Default>::default();
                match $crate::macros::NeoimvDecoder::decode(&decoder, slice) {
                    Ok(image) => image_to_mutated(Some(image)),
                    Err(msg) => {
                        let mut result = $crate::ffi_types::MutatedImage::none();
                        result.result = $crate::ffi_types::PluginResultCode::Error;
                        result.error_message = leak_cstr(&msg);
                        result
                    }
                }
            })).unwrap_or($crate::ffi_types::MutatedImage::none())
        }
    };
}
