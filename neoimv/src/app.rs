use std::collections::HashMap;
use std::ffi::CString;
use std::path::{Path, PathBuf};

use tracing::{error, info, warn};

use crate::config::settings::{cache_dir, config_dir, AppConfig};
use crate::config::ui::SettingsPanel;
use crate::core::animation::AnimationState;
use crate::core::background_loader::{BackgroundLoader, LoadResult, PluginDecoder};
use crate::core::filmstrip::Filmstrip;
use crate::core::fonts;
use crate::core::image_loader::{load_image_animated, LoadedImage};
use crate::core::image_ops::{rotate_rgba_ccw, rotate_rgba_cw};
use crate::core::navigator::Navigator;
use crate::core::path_picker::{OpenRequest, PathPicker};
use crate::core::renderer::{FitMode, ImageRenderer};
use crate::core::save::{save_as_dialog, save_image};
use crate::core::statusbar::{render_statusbar, StatusInfo};
use crate::core::undo::{Mutation, UndoStack};
use crate::core::window::WindowState;
use crate::keymap::action::ActionRegistry;
use crate::keymap::config::KeymapConfig;
use crate::plugin::context::ContextBuilder;
use crate::plugin::manager::PluginManager;
use crate::plugin::ui_bridge::render_ui_tree;

use neoimv_plugin_sdk::ffi_types::ImageBuffer;

pub struct NeoimvApp {
    // Config
    config: AppConfig,

    // Navigation
    navigator: Navigator,
    current_path: Option<PathBuf>,
    image_width: u32,
    image_height: u32,

    // Rendering
    renderer: ImageRenderer,
    error_message: Option<String>,
    loading: bool,

    // Background loading
    loader: BackgroundLoader,

    // Animation
    animation: Option<AnimationState>,

    // Undo / redo
    undo_stack: UndoStack,

    // Current pixel buffer (for undo/save/plugins)
    current_pixels: Vec<u8>,

    // Rename dialog
    rename_open: bool,
    rename_name: String,

    // Open-path picker
    path_picker: PathPicker,

    // Filmstrip
    filmstrip: Filmstrip,

    // Window state (position / fullscreen)
    window_state: WindowState,
    window_state_path: PathBuf,
    is_fullscreen: bool,
    last_viewport_size: egui::Vec2,

    // Keymap + actions
    keymap: KeymapConfig,
    action_registry: ActionRegistry,
    settings_panel: SettingsPanel,

    // Plugin manager
    plugin_manager: PluginManager,

    // Context builder (reused across frames to avoid repeated allocs)
    ctx_builder: ContextBuilder,
}

impl NeoimvApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        initial_path: Option<PathBuf>,
        config: AppConfig,
    ) -> Self {
        // Configure fonts from config
        fonts::configure_fonts(&cc.egui_ctx, &config.core.font_override);

        let cfg_dir = config_dir();
        let cch_dir = cache_dir();

        // Load keymap
        let keymap = KeymapConfig::load(&cfg_dir.join("keymap.toml"));

        // Build action registry with core actions
        let mut action_registry = ActionRegistry::new();

        // Load plugins
        let plugin_dir = cfg_dir.join("plugins");
        let mut plugin_manager = PluginManager::new(plugin_dir);
        plugin_manager.load_all();

        // Register plugin actions in the action registry
        for plugin in plugin_manager.plugins() {
            for (id, label, _default_key, category) in plugin.get_actions() {
                let plugin_name = id.split(':').next().unwrap_or("plugin").to_string();
                action_registry.register(id, label, category, plugin_name);
            }
        }

        // Build plugin decoders from decoder_priority config
        let mut decoders: Vec<PluginDecoder> = Vec::new();
        let mut ext_map: HashMap<String, Vec<usize>> = HashMap::new();
        for plugin_name in &config.core.decoder_priority {
            if let Some(plugin) = plugin_manager.get(plugin_name) {
                if let Some(decode_fn) = plugin.decode_fn() {
                    let idx = decoders.len();
                    decoders.push(PluginDecoder {
                        decode_fn,
                        free_fn: plugin.free_fn(),
                    });
                    for ext in plugin.extensions() {
                        ext_map.entry(ext.to_lowercase()).or_default().push(idx);
                    }
                }
            }
        }
        let loader = BackgroundLoader::new(decoders, ext_map);

        // Undo stack
        let undo_stack = UndoStack::new(config.core.undo_max_depth, config.core.undo_memory_cap_mb);

        // Filmstrip
        let thumb_cache_dir = cch_dir.join("thumbnails");
        let filmstrip = Filmstrip::new(thumb_cache_dir);

        // Window state
        let window_state_path = cfg_dir.join("window_state.toml");
        let window_state = WindowState::load(&window_state_path);

        let mut app = Self {
            config,
            navigator: Navigator::new(),
            current_path: None,
            image_width: 0,
            image_height: 0,
            renderer: ImageRenderer::new(),
            error_message: None,
            loading: false,
            loader,
            animation: None,
            undo_stack,
            current_pixels: Vec::new(),
            rename_open: false,
            rename_name: String::new(),
            path_picker: PathPicker::new(),
            filmstrip,
            window_state,
            window_state_path,
            is_fullscreen: false,
            last_viewport_size: egui::Vec2::ZERO,
            keymap,
            action_registry,
            settings_panel: SettingsPanel::new(),
            plugin_manager,
            ctx_builder: ContextBuilder::new(),
        };

        if let Some(path) = initial_path {
            app.navigator.scan_directory(&path);
            if let Some(image_path) = app.navigator.current().cloned() {
                app.load_image_animated(&image_path);
            }
        }

        // Dispatch core:app_started to all plugins
        {
            let ctx = app.ctx_builder.build(
                None,
                app.current_path.as_deref(),
                app.navigator.count() as u32,
                app.navigator.current_index() as u32,
                None,
            );
            app.plugin_manager.broadcast_event("core:app_started", &ctx);
        }

        app
    }

    /// Request a background load with animated detection.
    fn load_image_animated(&mut self, path: &Path) {
        info!("Requesting animated load: {}", path.display());
        self.loading = true;
        self.error_message = None;
        self.animation = None;
        // Use background loader for the initial bytes; animation handled inline for now
        // For animated formats we load synchronously to get frame data
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        if ext == "gif" || ext == "webp" {
            match load_image_animated(path) {
                Ok(LoadedImage::Animated(frames)) => {
                    // Set renderer to first frame immediately
                    if let Some(first) = frames.first() {
                        self.current_pixels = first.pixels.clone();
                        self.image_width = first.width;
                        self.image_height = first.height;
                    }
                    self.animation = Some(AnimationState::new(frames));
                    self.current_path = Some(path.to_path_buf());
                    self.error_message = None;
                    self.loading = false;
                    return;
                }
                Ok(LoadedImage::Static(decoded)) => {
                    self.current_pixels = decoded.pixels;
                    self.image_width = decoded.width;
                    self.image_height = decoded.height;
                    self.animation = None;
                    self.current_path = Some(path.to_path_buf());
                    self.error_message = None;
                    self.loading = false;
                    return;
                }
                Err(e) => {
                    self.error_message = Some(format!("Failed to load: {}", e));
                    self.loading = false;
                    return;
                }
            }
        }

        // Non-animated: use background loader
        self.loader.request_load(path.to_path_buf());
    }

    fn window_title(&self) -> String {
        match &self.current_path {
            Some(path) => {
                let filename = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown");
                let count = self.navigator.count();
                if count > 0 {
                    format!(
                        "{} [{}/{}] - neoimv",
                        filename,
                        self.navigator.current_index() + 1,
                        count
                    )
                } else {
                    format!("{} - neoimv", filename)
                }
            }
            None => match self.navigator.location() {
                Some(dir) => format!("{} - neoimv", dir.display()),
                None => "neoimv".to_string(),
            },
        }
    }

    /// Handle dropped files. Returns true if a file was loaded.
    fn handle_dropped_files(&mut self, ctx: &egui::Context) -> bool {
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        if dropped.is_empty() {
            return false;
        }

        if let Some(path) = crate::core::drag_drop::first_image_from_dropped(&dropped) {
            self.navigator.scan_directory(&path);
            if let Some(current) = self.navigator.current().cloned() {
                self.load_image_animated(&current);
            }
            // Broadcast core:files_dropped to plugins
            let plugin_ctx = self.ctx_builder.build(
                None,
                Some(&path),
                self.navigator.count() as u32,
                self.navigator.current_index() as u32,
                None,
            );
            self.plugin_manager
                .broadcast_event("core:files_dropped", &plugin_ctx);
            return true;
        }
        false
    }

    /// Convert egui key + modifiers to the keymap string format used in keymap.toml.
    fn key_event_to_string(key: egui::Key, modifiers: egui::Modifiers) -> String {
        let mut parts = Vec::new();
        if modifiers.ctrl {
            parts.push("Ctrl".to_string());
        }
        if modifiers.alt {
            parts.push("Alt".to_string());
        }
        if modifiers.shift {
            parts.push("Shift".to_string());
        }
        parts.push(key_name(key));
        parts.join("+")
    }

    /// Process all key events this frame and dispatch actions.
    fn handle_input(&mut self, ctx: &egui::Context) {
        if self.rename_open
            || self.path_picker.is_open()
            || self.settings_panel.is_open()
            || ctx.wants_keyboard_input()
        {
            return;
        }

        let key_events: Vec<(egui::Key, egui::Modifiers)> = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|e| {
                    if let egui::Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } = e
                    {
                        Some((*key, *modifiers))
                    } else {
                        None
                    }
                })
                .collect()
        });

        for (key, modifiers) in key_events {
            let key_str = Self::key_event_to_string(key, modifiers);
            if let Some(action_id) = self.keymap.action_for_key(&key_str).map(str::to_string) {
                self.dispatch_action(ctx, &action_id);
            }
        }
    }

    /// Dispatch a named action (core or plugin).
    fn dispatch_action(&mut self, ctx: &egui::Context, action_id: &str) {
        match action_id {
            "core:prev_image" => {
                if let Some(path) = self.navigator.prev().cloned() {
                    self.load_image_animated(&path);
                }
            }
            "core:next_image" => {
                if let Some(path) = self.navigator.next().cloned() {
                    self.load_image_animated(&path);
                }
            }
            "core:zoom_in" => {
                self.renderer.zoom_in();
            }
            "core:zoom_out" => {
                self.renderer.zoom_out();
            }
            "core:fit_window" => {
                self.renderer.set_fit_mode(FitMode::FitWindow);
            }
            "core:fit_width" => {
                self.renderer.set_fit_mode(FitMode::FitWidth);
            }
            "core:actual_size" => {
                self.renderer.set_fit_mode(FitMode::ActualSize);
            }
            "core:fullscreen" => {
                self.is_fullscreen = !self.is_fullscreen;
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.is_fullscreen));
            }
            "core:quit" => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            "core:undo" => {
                if let Some(mutation) = self.undo_stack.undo() {
                    let pixels = mutation.pixels.clone();
                    let width = mutation.width;
                    let height = mutation.height;
                    self.renderer.set_image(ctx, &pixels, width, height);
                    self.current_pixels = pixels;
                    self.image_width = width;
                    self.image_height = height;
                    // Broadcast core:image_changed
                    let plugin_ctx = self.ctx_builder.build(
                        None,
                        self.current_path.as_deref(),
                        self.navigator.count() as u32,
                        self.navigator.current_index() as u32,
                        None,
                    );
                    self.plugin_manager
                        .broadcast_event("core:image_changed", &plugin_ctx);
                }
            }
            "core:redo" => {
                if let Some(mutation) = self.undo_stack.redo() {
                    let pixels = mutation.pixels.clone();
                    let width = mutation.width;
                    let height = mutation.height;
                    self.renderer.set_image(ctx, &pixels, width, height);
                    self.current_pixels = pixels;
                    self.image_width = width;
                    self.image_height = height;
                    // Broadcast core:image_changed
                    let plugin_ctx = self.ctx_builder.build(
                        None,
                        self.current_path.as_deref(),
                        self.navigator.count() as u32,
                        self.navigator.current_index() as u32,
                        None,
                    );
                    self.plugin_manager
                        .broadcast_event("core:image_changed", &plugin_ctx);
                }
            }
            "core:copy_image" => {
                self.copy_current_image(ctx);
            }
            "core:copy_path" => {
                self.copy_current_path(ctx);
            }
            "core:open_path" => {
                self.open_path_picker();
            }
            "core:rename_file" => {
                self.open_rename_dialog();
            }
            "core:rotate_cw" => {
                self.rotate_current(ctx, true);
            }
            "core:rotate_ccw" => {
                self.rotate_current(ctx, false);
            }
            "core:save" => {
                if let (Some(path), pixels) =
                    (&self.current_path.clone(), &self.current_pixels.clone())
                {
                    if !pixels.is_empty() {
                        if let Err(e) =
                            save_image(pixels, self.image_width, self.image_height, path)
                        {
                            error!("Save failed: {}", e);
                            self.error_message = Some(format!("Save failed: {}", e));
                        }
                    }
                }
            }
            "core:save_as" => {
                let current = self.current_path.as_deref();
                if let Some(save_path) = save_as_dialog(current) {
                    let pixels = self.current_pixels.clone();
                    if !pixels.is_empty() {
                        if let Err(e) =
                            save_image(&pixels, self.image_width, self.image_height, &save_path)
                        {
                            error!("Save As failed: {}", e);
                            self.error_message = Some(format!("Save As failed: {}", e));
                        }
                    }
                }
            }
            other => {
                // Plugin action — dispatch to matching plugin
                self.dispatch_plugin_action(ctx, other);
            }
        }
    }

    fn copy_current_image(&self, ctx: &egui::Context) {
        if let Some(frame) = self
            .animation
            .as_ref()
            .and_then(|anim| anim.current_frame())
        {
            ctx.copy_image(egui::ColorImage::from_rgba_unmultiplied(
                [frame.width as usize, frame.height as usize],
                &frame.pixels,
            ));
            return;
        }

        let expected_len = (self.image_width as usize)
            .checked_mul(self.image_height as usize)
            .and_then(|px| px.checked_mul(4));
        if expected_len == Some(self.current_pixels.len()) {
            ctx.copy_image(egui::ColorImage::from_rgba_unmultiplied(
                [self.image_width as usize, self.image_height as usize],
                &self.current_pixels,
            ));
        }
    }

    fn copy_current_path(&self, ctx: &egui::Context) {
        if let Some(path) = &self.current_path {
            ctx.copy_text(path.to_string_lossy().into_owned());
        }
    }

    /// Prefill with the active filter, else the current location (even an empty folder).
    fn open_path_picker(&mut self) {
        let base_dir = self
            .navigator
            .location()
            .map(Path::to_path_buf)
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_default();
        let input = match self.navigator.scope() {
            Some(scope) => scope.to_string(),
            None => {
                let mut dir = base_dir.to_string_lossy().into_owned();
                if !dir.ends_with(std::path::MAIN_SEPARATOR) {
                    dir.push(std::path::MAIN_SEPARATOR);
                }
                dir
            }
        };
        self.path_picker.open(input, base_dir);
    }

    fn open_request(&mut self, ctx: &egui::Context, request: OpenRequest) {
        match request {
            OpenRequest::Path(path) => {
                self.navigator.scan_directory(&path);
                if self.navigator.count() == 0 {
                    // Stay in the folder anyway so the next Ctrl+O starts from it.
                    self.renderer.clear();
                    self.animation = None;
                    self.current_path = None;
                    self.current_pixels.clear();
                    self.image_width = 0;
                    self.image_height = 0;
                    self.error_message = None;
                    self.loading = false;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.window_title()));
                    return;
                }
            }
            OpenRequest::Scoped {
                images,
                index,
                label,
            } => self.navigator.set_scoped(images, index, label),
        }
        if let Some(path) = self.navigator.current().cloned() {
            self.load_image_animated(&path);
        }
    }

    fn open_rename_dialog(&mut self) {
        if let Some(name) = self
            .current_path
            .as_ref()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
        {
            self.rename_name = name.to_string();
            self.rename_open = true;
        }
    }

    fn rename_current_file(&mut self, ctx: &egui::Context) {
        let Some(old_path) = self.current_path.clone() else {
            return;
        };
        let new_name = self.rename_name.trim();
        if new_name.is_empty() || new_name.contains('/') || new_name.contains('\\') {
            self.error_message = Some("Invalid file name".to_string());
            return;
        }
        let Some(parent) = old_path.parent() else {
            self.error_message = Some("Cannot rename file".to_string());
            return;
        };
        let new_path = parent.join(new_name);
        if new_path == old_path {
            self.rename_open = false;
            return;
        }
        let old_ext = old_path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("");
        let new_ext = new_path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("");
        if !old_ext.eq_ignore_ascii_case(new_ext) {
            self.error_message = Some("Keep the original file extension".to_string());
            return;
        }
        if new_path.exists() {
            self.error_message = Some("File already exists".to_string());
            return;
        }
        if let Err(e) = std::fs::rename(&old_path, &new_path) {
            self.error_message = Some(format!("Rename failed: {}", e));
            return;
        }
        self.current_path = Some(new_path.clone());
        if self.navigator.scope().is_some() {
            self.navigator.replace_current(new_path);
        } else {
            self.navigator.scan_directory(&new_path);
        }
        self.rename_open = false;
        self.error_message = None;
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.window_title()));
    }

    fn rotate_current(&mut self, ctx: &egui::Context, clockwise: bool) {
        if self.current_pixels.is_empty() {
            return;
        }
        if !self.undo_stack.can_undo() {
            self.undo_stack.push(Mutation {
                pixels: self.current_pixels.clone(),
                width: self.image_width,
                height: self.image_height,
                plugin_name: "core".to_string(),
            });
        }
        self.animation = None;
        let rotated = if clockwise {
            rotate_rgba_cw(&self.current_pixels, self.image_width, self.image_height)
        } else {
            rotate_rgba_ccw(&self.current_pixels, self.image_width, self.image_height)
        };
        let Some((pixels, width, height)) = rotated else {
            self.error_message = Some("Rotate failed".to_string());
            return;
        };

        self.renderer.set_image(ctx, &pixels, width, height);
        self.current_pixels = pixels;
        self.image_width = width;
        self.image_height = height;
        self.undo_stack.push(Mutation {
            pixels: self.current_pixels.clone(),
            width: self.image_width,
            height: self.image_height,
            plugin_name: "core".to_string(),
        });

        let plugin_ctx = self.ctx_builder.build(
            None,
            self.current_path.as_deref(),
            self.navigator.count() as u32,
            self.navigator.current_index() as u32,
            None,
        );
        self.plugin_manager
            .broadcast_event("core:image_changed", &plugin_ctx);
    }

    /// Dispatch a plugin action by action_id (e.g. "rotate:cw").
    fn dispatch_plugin_action(&mut self, egui_ctx: &egui::Context, action_id: &str) {
        let c_action = match CString::new(action_id) {
            Ok(s) => s,
            Err(_) => return,
        };

        // Build ImageBuffer for plugin context
        let image_buf: Option<ImageBuffer> = if !self.current_pixels.is_empty() {
            Some(ImageBuffer {
                pixels: self.current_pixels.as_ptr(),
                width: self.image_width,
                height: self.image_height,
                stride: self.image_width * 4,
            })
        } else {
            None
        };

        let plugin_name = action_id.split(':').next().unwrap_or("");

        // Find the plugin by name prefix
        let plugin_idx = self
            .plugin_manager
            .plugins()
            .iter()
            .position(|p| p.name() == plugin_name);

        if let Some(idx) = plugin_idx {
            // Build context - we need to use raw pointer approach since ImageBuffer may point
            // to current_pixels, which we own. Build ctx outside borrow.
            let config_str = self.config.plugin_config_toml(plugin_name);
            let ctx = self.ctx_builder.build(
                image_buf.as_ref(),
                self.current_path.as_deref(),
                self.navigator.count() as u32,
                self.navigator.current_index() as u32,
                config_str.as_deref(),
            );

            if let Some((pixels, width, height)) =
                self.plugin_manager.plugins()[idx].on_action(&c_action, &ctx)
            {
                // Push current state onto undo stack before applying mutation
                if !self.current_pixels.is_empty() {
                    self.undo_stack.push(Mutation {
                        pixels: self.current_pixels.clone(),
                        width: self.image_width,
                        height: self.image_height,
                        plugin_name: plugin_name.to_string(),
                    });
                }

                self.renderer.set_image(egui_ctx, &pixels, width, height);
                self.current_pixels = pixels;
                self.image_width = width;
                self.image_height = height;

                // Broadcast core:image_changed
                let change_ctx = self.ctx_builder.build(
                    None,
                    self.current_path.as_deref(),
                    self.navigator.count() as u32,
                    self.navigator.current_index() as u32,
                    None,
                );
                self.plugin_manager
                    .broadcast_event("core:image_changed", &change_ctx);
            }
        } else {
            warn!("No plugin found for action: {}", action_id);
        }
    }

    /// Poll the background loader for completed loads.
    fn poll_loader(&mut self, ctx: &egui::Context) {
        match self.loader.receiver.try_recv() {
            Ok(LoadResult::Loaded {
                path,
                pixels,
                width,
                height,
            }) => {
                self.renderer.set_image(ctx, &pixels, width, height);
                self.image_width = width;
                self.image_height = height;
                self.current_path = Some(path.clone());
                self.current_pixels = pixels;
                self.error_message = None;
                self.loading = false;
                self.animation = None;
                info!("Loaded image: {}", path.display());
                ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.window_title()));

                // Broadcast core:image_changed
                let plugin_ctx = self.ctx_builder.build(
                    None,
                    Some(&path),
                    self.navigator.count() as u32,
                    self.navigator.current_index() as u32,
                    None,
                );
                self.plugin_manager
                    .broadcast_event("core:image_changed", &plugin_ctx);
            }
            Ok(LoadResult::Error { path, error }) => {
                error!("Failed to load {}: {}", path.display(), error);
                self.error_message = Some(format!("Failed to load: {}", error));
                self.renderer.clear();
                self.image_width = 0;
                self.image_height = 0;
                self.current_pixels.clear();
                self.loading = false;
            }
            Err(_) => {
                if self.loading {
                    ctx.request_repaint();
                }
            }
        }
    }

    /// Tick animation; update texture if frame changed.
    fn tick_animation(&mut self, ctx: &egui::Context) {
        let frame_changed = if let Some(anim) = &mut self.animation {
            anim.tick()
        } else {
            false
        };

        if frame_changed {
            if let Some(anim) = &self.animation {
                if let Some(frame) = anim.current_frame() {
                    self.renderer
                        .set_image(ctx, &frame.pixels, frame.width, frame.height);
                }
            }
            ctx.request_repaint();
        } else if self
            .animation
            .as_ref()
            .map(|a| a.is_playing())
            .unwrap_or(false)
        {
            // Keep repainting while animation is running
            ctx.request_repaint();
        }
    }

    /// Render plugin UIs in a side panel.
    fn render_plugin_uis(&mut self, ctx: &egui::Context) {
        let ui_plugins: Vec<usize> = self
            .plugin_manager
            .plugins()
            .iter()
            .enumerate()
            .filter(|(_, p)| p.has_capability(neoimv_plugin_sdk::ffi_types::CAPABILITY_UI))
            .map(|(i, _)| i)
            .collect();

        if ui_plugins.is_empty() {
            return;
        }

        let config_str = self.current_path.as_ref().and_then(|p| {
            let name = p.file_name()?.to_str()?;
            // Use first plugin name as key — each plugin gets its own config
            Some(name.to_string())
        });
        let _ = config_str; // suppress unused warning

        egui::SidePanel::right("plugin_ui_panel").show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                for idx in &ui_plugins {
                    let image_buf: Option<ImageBuffer> = if !self.current_pixels.is_empty() {
                        Some(ImageBuffer {
                            pixels: self.current_pixels.as_ptr(),
                            width: self.image_width,
                            height: self.image_height,
                            stride: self.image_width * 4,
                        })
                    } else {
                        None
                    };

                    let plugin_name = self.plugin_manager.plugins()[*idx].name().to_string();
                    let cfg = self.config.plugin_config_toml(&plugin_name);

                    let plugin_ctx = self.ctx_builder.build(
                        image_buf.as_ref(),
                        self.current_path.as_deref(),
                        self.navigator.count() as u32,
                        self.navigator.current_index() as u32,
                        cfg.as_deref(),
                    );

                    if let Some(ui_root_ptr) =
                        self.plugin_manager.plugins()[*idx].render_ui(&plugin_ctx)
                    {
                        // SAFETY: pointer is valid for duration of this frame (plugin owns it)
                        let ui_root = unsafe { &*ui_root_ptr };
                        if let Some(action_id) = render_ui_tree(ui, ui_root) {
                            // Clone action_id before dispatching (avoids borrow issues)
                            let action_id_owned = action_id.clone();
                            let _ = ui_root;
                            // We can't call dispatch_action here because we're inside a closure
                            // that borrows ctx. Store it and dispatch after.
                            // For simplicity, dispatch plugin action directly.
                            let c_action = CString::new(action_id_owned.as_str()).unwrap();
                            let image_buf2: Option<ImageBuffer> = if !self.current_pixels.is_empty()
                            {
                                Some(ImageBuffer {
                                    pixels: self.current_pixels.as_ptr(),
                                    width: self.image_width,
                                    height: self.image_height,
                                    stride: self.image_width * 4,
                                })
                            } else {
                                None
                            };
                            let cfg2 = self.config.plugin_config_toml(&plugin_name);
                            let ctx2 = self.ctx_builder.build(
                                image_buf2.as_ref(),
                                self.current_path.as_deref(),
                                self.navigator.count() as u32,
                                self.navigator.current_index() as u32,
                                cfg2.as_deref(),
                            );
                            if let Some((pixels, width, height)) =
                                self.plugin_manager.plugins()[*idx].on_action(&c_action, &ctx2)
                            {
                                if !self.current_pixels.is_empty() {
                                    self.undo_stack.push(Mutation {
                                        pixels: self.current_pixels.clone(),
                                        width: self.image_width,
                                        height: self.image_height,
                                        plugin_name: plugin_name.clone(),
                                    });
                                }
                                self.image_width = width;
                                self.image_height = height;
                                self.current_pixels = pixels.clone();
                                self.renderer.set_image(ctx, &pixels, width, height);
                                let change_ctx = self.ctx_builder.build(
                                    None,
                                    self.current_path.as_deref(),
                                    self.navigator.count() as u32,
                                    self.navigator.current_index() as u32,
                                    None,
                                );
                                self.plugin_manager
                                    .broadcast_event("core:image_changed", &change_ctx);
                            }
                        }
                    }
                }
            });
        });
    }

    fn render_rename_dialog(&mut self, ctx: &egui::Context) {
        if !self.rename_open {
            return;
        }

        let mut rename = false;
        let mut cancel = false;
        egui::Window::new("Rename file")
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                let response = ui.text_edit_singleline(&mut self.rename_name);
                if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    rename = true;
                }
                ui.horizontal(|ui| {
                    if ui.button("Rename").clicked() {
                        rename = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    cancel = true;
                }
            });

        if rename {
            self.rename_current_file(ctx);
        }
        if cancel {
            self.rename_open = false;
        }
    }

    fn render_toolbar(&mut self, ctx: &egui::Context) {
        let mut action: Option<&'static str> = None;
        let has_image = !self.current_pixels.is_empty();
        let has_path = self.current_path.is_some();

        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui.button("Open").clicked() {
                    action = Some("core:open_path");
                }
                ui.separator();
                if ui.button("Prev").clicked() {
                    action = Some("core:prev_image");
                }
                if ui.button("Next").clicked() {
                    action = Some("core:next_image");
                }
                ui.separator();
                if ui.button("Zoom -").clicked() {
                    action = Some("core:zoom_out");
                }
                if ui.button("Zoom +").clicked() {
                    action = Some("core:zoom_in");
                }
                if ui.button("Fit").clicked() {
                    action = Some("core:fit_window");
                }
                if ui.button("Width").clicked() {
                    action = Some("core:fit_width");
                }
                if ui.button("1:1").clicked() {
                    action = Some("core:actual_size");
                }
                ui.separator();
                if ui
                    .add_enabled(has_image, egui::Button::new("Undo"))
                    .clicked()
                {
                    action = Some("core:undo");
                }
                if ui
                    .add_enabled(has_image, egui::Button::new("Redo"))
                    .clicked()
                {
                    action = Some("core:redo");
                }
                if ui
                    .add_enabled(has_image, egui::Button::new("Copy"))
                    .clicked()
                {
                    action = Some("core:copy_image");
                }
                if ui
                    .add_enabled(has_path, egui::Button::new("Path"))
                    .clicked()
                {
                    action = Some("core:copy_path");
                }
                if ui
                    .add_enabled(has_path, egui::Button::new("Rename"))
                    .clicked()
                {
                    action = Some("core:rename_file");
                }
                if ui
                    .add_enabled(has_image, egui::Button::new("Rot L"))
                    .clicked()
                {
                    action = Some("core:rotate_ccw");
                }
                if ui
                    .add_enabled(has_image, egui::Button::new("Rot R"))
                    .clicked()
                {
                    action = Some("core:rotate_cw");
                }
                if ui
                    .add_enabled(has_image && has_path, egui::Button::new("Save"))
                    .clicked()
                {
                    action = Some("core:save");
                }
                if ui
                    .add_enabled(has_image, egui::Button::new("Save As"))
                    .clicked()
                {
                    action = Some("core:save_as");
                }
                ui.separator();
                if ui.button("Settings").clicked() {
                    self.settings_panel.toggle();
                }
            });
        });

        if let Some(action) = action {
            self.dispatch_action(ctx, action);
        }
    }

    /// Detect viewport size changes and broadcast core:window_resized.
    fn check_viewport_resize(&mut self, ctx: &egui::Context) {
        let size = ctx.screen_rect().size();
        if (size - self.last_viewport_size).length() > 0.5 {
            self.last_viewport_size = size;
            let plugin_ctx = self.ctx_builder.build(
                None,
                self.current_path.as_deref(),
                self.navigator.count() as u32,
                self.navigator.current_index() as u32,
                None,
            );
            self.plugin_manager
                .broadcast_event("core:window_resized", &plugin_ctx);
        }
    }
}

impl eframe::App for NeoimvApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 1. Poll background loader
        self.poll_loader(ctx);

        // 2. Tick animation (update texture if frame changed)
        self.tick_animation(ctx);

        // 3. Handle dropped files
        self.handle_dropped_files(ctx);

        // 4. Handle keyboard input → keymap → action dispatch
        self.handle_input(ctx);

        // 5. Detect viewport resize
        self.check_viewport_resize(ctx);

        // 6. Toolbar and settings/keymap window
        self.render_toolbar(ctx);
        if self.settings_panel.render(
            ctx,
            &mut self.config,
            &mut self.keymap,
            &self.action_registry,
            &self.plugin_manager,
        ) {
            let cfg_dir = config_dir();
            if let Err(e) = self.config.save(&cfg_dir.join("config.toml")) {
                self.error_message = Some(format!("Config save failed: {}", e));
            }
            if let Err(e) = self.keymap.save(&cfg_dir.join("keymap.toml")) {
                self.error_message = Some(format!("Keymap save failed: {}", e));
            }
        }

        // 7. Render plugin UIs (right side panel, only if plugins with UI are loaded)
        self.render_plugin_uis(ctx);

        self.render_rename_dialog(ctx);
        if let Some(request) = self.path_picker.render(ctx) {
            self.open_request(ctx, request);
        }

        // 8. Filmstrip (bottom panel, above statusbar)
        if self.config.core.filmstrip {
            let images: Vec<PathBuf> = self.navigator.images().to_vec();
            let current_index = self.navigator.current_index();
            let mut clicked: Option<usize> = None;

            egui::TopBottomPanel::bottom("filmstrip").show(ctx, |ui| {
                clicked = self.filmstrip.render(ui, ctx, &images, current_index);
            });

            if let Some(i) = clicked {
                if let Some(path) = self.navigator.go_to(i).cloned() {
                    self.load_image_animated(&path);
                }
            }
        }

        // 9. Status bar
        if self.config.core.statusbar {
            let filename = self
                .current_path
                .as_ref()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .unwrap_or("\u{2014}");

            let zoom = self.renderer.zoom_level();
            let index = self.navigator.current_index();
            let total = self.navigator.count();

            egui::TopBottomPanel::bottom("statusbar").show(ctx, |ui| {
                let info = StatusInfo {
                    filename,
                    width: self.image_width,
                    height: self.image_height,
                    zoom_percent: zoom,
                    image_index: index,
                    image_count: total,
                    scope: self.navigator.scope(),
                };
                render_statusbar(ui, &info);
            });
        }

        // 10. Central panel — image or loading/error state
        egui::CentralPanel::default().show(ctx, |ui| {
            if self.loading {
                ui.centered_and_justified(|ui| {
                    ui.label("Loading...");
                });
            } else if let Some(err) = &self.error_message.clone() {
                ui.centered_and_justified(|ui| {
                    ui.colored_label(egui::Color32::RED, err);
                });
            } else if self.navigator.count() == 0 && self.current_pixels.is_empty() {
                if let Some(dir) = self.navigator.location() {
                    ui.centered_and_justified(|ui| {
                        ui.weak(format!(
                            "No images in {}\nCtrl+O to open another path",
                            dir.display()
                        ));
                    });
                }
            } else {
                // Set texture from current frame if animated (renderer already updated by tick)
                // For static images renderer was already set during load
                self.renderer.render(ui);
            }
        });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        // Save window state on exit
        let rect = egui::Context::default().screen_rect();
        // We can't easily get the real window rect from on_exit without a stored value,
        // so save the last known viewport size
        let ws = WindowState {
            x: self.window_state.x,
            y: self.window_state.y,
            width: self.last_viewport_size.x.max(320.0),
            height: self.last_viewport_size.y.max(240.0),
            fullscreen: self.is_fullscreen,
        };
        ws.save(&self.window_state_path);
        info!("Window state saved on exit");
        let _ = rect; // suppress unused warning
    }
}

/// Convert an egui key to the string representation used in keymap.toml.
fn key_name(key: egui::Key) -> String {
    match key {
        egui::Key::ArrowLeft => "Left".into(),
        egui::Key::ArrowRight => "Right".into(),
        egui::Key::ArrowUp => "Up".into(),
        egui::Key::ArrowDown => "Down".into(),
        egui::Key::Escape => "Escape".into(),
        egui::Key::Space => "Space".into(),
        egui::Key::Enter => "Enter".into(),
        egui::Key::Backspace => "Backspace".into(),
        egui::Key::Delete => "Delete".into(),
        egui::Key::Tab => "Tab".into(),
        egui::Key::Home => "Home".into(),
        egui::Key::End => "End".into(),
        egui::Key::PageUp => "PageUp".into(),
        egui::Key::PageDown => "PageDown".into(),
        egui::Key::Insert => "Insert".into(),
        egui::Key::F1 => "F1".into(),
        egui::Key::F2 => "F2".into(),
        egui::Key::F3 => "F3".into(),
        egui::Key::F4 => "F4".into(),
        egui::Key::F5 => "F5".into(),
        egui::Key::F6 => "F6".into(),
        egui::Key::F7 => "F7".into(),
        egui::Key::F8 => "F8".into(),
        egui::Key::F9 => "F9".into(),
        egui::Key::F10 => "F10".into(),
        egui::Key::F11 => "F11".into(),
        egui::Key::F12 => "F12".into(),
        egui::Key::A => "a".into(),
        egui::Key::B => "b".into(),
        egui::Key::C => "c".into(),
        egui::Key::D => "d".into(),
        egui::Key::E => "e".into(),
        egui::Key::F => "f".into(),
        egui::Key::G => "g".into(),
        egui::Key::H => "h".into(),
        egui::Key::I => "i".into(),
        egui::Key::J => "j".into(),
        egui::Key::K => "k".into(),
        egui::Key::L => "l".into(),
        egui::Key::M => "m".into(),
        egui::Key::N => "n".into(),
        egui::Key::O => "o".into(),
        egui::Key::P => "p".into(),
        egui::Key::Q => "q".into(),
        egui::Key::R => "r".into(),
        egui::Key::S => "s".into(),
        egui::Key::T => "t".into(),
        egui::Key::U => "u".into(),
        egui::Key::V => "v".into(),
        egui::Key::W => "w".into(),
        egui::Key::X => "x".into(),
        egui::Key::Y => "y".into(),
        egui::Key::Z => "z".into(),
        egui::Key::Num0 => "0".into(),
        egui::Key::Num1 => "1".into(),
        egui::Key::Num2 => "2".into(),
        egui::Key::Num3 => "3".into(),
        egui::Key::Num4 => "4".into(),
        egui::Key::Num5 => "5".into(),
        egui::Key::Num6 => "6".into(),
        egui::Key::Num7 => "7".into(),
        egui::Key::Num8 => "8".into(),
        egui::Key::Num9 => "9".into(),
        other => format!("{:?}", other),
    }
}
