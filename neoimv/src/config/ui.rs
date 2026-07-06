use crate::config::settings::{AppConfig, CoreConfig};
use crate::keymap::action::ActionRegistry;
use crate::keymap::config::KeymapConfig;
use crate::keymap::ui::KeymapEditor;
use crate::plugin::manager::PluginManager;
use egui::Ui;
use neoimv_plugin_sdk::ConfigFieldType;

pub struct SettingsPanel {
    open: bool,
    active_tab: SettingsTab,
    keymap_editor: KeymapEditor,
}

#[derive(PartialEq)]
enum SettingsTab {
    Core,
    Keybindings,
    Plugins,
    PluginConfig(String),
}

impl SettingsPanel {
    pub fn new() -> Self {
        Self {
            open: false,
            active_tab: SettingsTab::Core,
            keymap_editor: KeymapEditor::new(),
        }
    }

    pub fn toggle(&mut self) {
        self.open = !self.open;
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn render(
        &mut self,
        ctx: &egui::Context,
        config: &mut AppConfig,
        keymap: &mut KeymapConfig,
        registry: &ActionRegistry,
        plugin_manager: &PluginManager,
    ) -> bool {
        if !self.open {
            return false;
        }
        let mut changed = false;

        egui::Window::new("Settings")
            .open(&mut self.open)
            .default_width(600.0)
            .show(ctx, |ui| {
                // Tab bar
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.active_tab, SettingsTab::Core, "Core");
                    ui.selectable_value(
                        &mut self.active_tab,
                        SettingsTab::Keybindings,
                        "Keybindings",
                    );
                    ui.selectable_value(&mut self.active_tab, SettingsTab::Plugins, "Plugins");
                    // Per-plugin config tabs
                    for name in plugin_manager.plugin_names() {
                        let tab = SettingsTab::PluginConfig(name.clone());
                        ui.selectable_value(&mut self.active_tab, tab, &name);
                    }
                });
                ui.separator();

                match &self.active_tab {
                    SettingsTab::Core => {
                        changed |= render_core_settings(ui, &mut config.core);
                    }
                    SettingsTab::Keybindings => {
                        changed |= self.keymap_editor.render(ui, keymap, registry);
                    }
                    SettingsTab::Plugins => {
                        // Plugin list with enable/disable — details in Task 22
                        ui.label("Plugin management (see Task 22)");
                    }
                    SettingsTab::PluginConfig(name) => {
                        let name = name.clone();
                        changed |= render_plugin_config(ui, &name, plugin_manager, config);
                    }
                }
            });
        changed
    }
}

fn render_core_settings(ui: &mut Ui, core: &mut CoreConfig) -> bool {
    let mut changed = false;
    if ui.checkbox(&mut core.filmstrip, "Show filmstrip").changed() {
        changed = true;
    }
    if ui
        .checkbox(&mut core.statusbar, "Show status bar")
        .changed()
    {
        changed = true;
    }
    if ui
        .checkbox(
            &mut core.window_remember_position,
            "Remember window position",
        )
        .changed()
    {
        changed = true;
    }
    if ui
        .checkbox(&mut core.thumbnail_cache, "Cache thumbnails to disk")
        .changed()
    {
        changed = true;
    }

    ui.horizontal(|ui| {
        ui.label("Theme:");
        egui::ComboBox::from_id_salt("theme")
            .selected_text(&core.theme)
            .show_ui(ui, |ui| {
                for t in &["dark", "light"] {
                    if ui.selectable_label(core.theme == *t, *t).clicked() {
                        core.theme = t.to_string();
                        changed = true;
                    }
                }
            });
    });

    ui.horizontal(|ui| {
        ui.label("Default fit:");
        egui::ComboBox::from_id_salt("fit")
            .selected_text(&core.default_fit)
            .show_ui(ui, |ui| {
                for f in &["fit_window", "fit_width", "actual_size"] {
                    if ui.selectable_label(core.default_fit == *f, *f).clicked() {
                        core.default_fit = f.to_string();
                        changed = true;
                    }
                }
            });
    });

    if ui
        .add(egui::Slider::new(&mut core.undo_max_depth, 1..=200).text("Undo depth"))
        .changed()
    {
        changed = true;
    }
    if ui
        .add(
            egui::Slider::new(&mut core.undo_memory_cap_mb, 64..=2048).text("Undo memory cap (MB)"),
        )
        .changed()
    {
        changed = true;
    }

    changed
}

/// Auto-generate config UI from plugin's ConfigFieldArray
fn render_plugin_config(
    ui: &mut Ui,
    plugin_name: &str,
    plugin_manager: &PluginManager,
    config: &mut AppConfig,
) -> bool {
    let mut changed = false;
    let Some(schema) = plugin_manager.config_schema(plugin_name) else {
        ui.label("No configurable options.");
        return false;
    };

    // Get or create plugin config section
    let plugin_conf = config
        .plugin
        .entry(plugin_name.to_string())
        .or_insert_with(toml::Table::new);

    for field in &schema {
        match field.field_type {
            ConfigFieldType::Bool => {
                let mut val = plugin_conf
                    .get(&field.key)
                    .and_then(|v| v.as_bool())
                    .unwrap_or(field.default_value == "true");
                if ui.checkbox(&mut val, &field.label).changed() {
                    plugin_conf.insert(field.key.clone(), toml::Value::Boolean(val));
                    changed = true;
                }
            }
            ConfigFieldType::Int => {
                let mut val = plugin_conf
                    .get(&field.key)
                    .and_then(|v| v.as_integer())
                    .unwrap_or_else(|| field.default_value.parse().unwrap_or(0));
                ui.horizontal(|ui| {
                    if ui.add(egui::DragValue::new(&mut val).speed(1)).changed() {
                        plugin_conf.insert(field.key.clone(), toml::Value::Integer(val));
                        changed = true;
                    }
                    ui.label(&field.label);
                });
            }
            ConfigFieldType::Float => {
                let mut val = plugin_conf
                    .get(&field.key)
                    .and_then(|v| v.as_float())
                    .unwrap_or_else(|| field.default_value.parse().unwrap_or(0.0));
                if ui
                    .add(egui::Slider::new(&mut val, 0.0..=100.0).text(&field.label))
                    .changed()
                {
                    plugin_conf.insert(field.key.clone(), toml::Value::Float(val));
                    changed = true;
                }
            }
            ConfigFieldType::String | ConfigFieldType::Color => {
                let mut val = plugin_conf
                    .get(&field.key)
                    .and_then(|v| v.as_str())
                    .unwrap_or(&field.default_value)
                    .to_string();
                ui.horizontal(|ui| {
                    ui.label(&field.label);
                    if ui.text_edit_singleline(&mut val).changed() {
                        plugin_conf.insert(field.key.clone(), toml::Value::String(val));
                        changed = true;
                    }
                });
            }
            ConfigFieldType::Enum => {
                let current = plugin_conf
                    .get(&field.key)
                    .and_then(|v| v.as_str())
                    .unwrap_or(&field.default_value)
                    .to_string();
                ui.horizontal(|ui| {
                    ui.label(&field.label);
                    egui::ComboBox::from_id_salt(&field.key)
                        .selected_text(&current)
                        .show_ui(ui, |ui| {
                            for variant in &field.enum_variants {
                                if ui.selectable_label(current == *variant, variant).clicked() {
                                    plugin_conf.insert(
                                        field.key.clone(),
                                        toml::Value::String(variant.clone()),
                                    );
                                    changed = true;
                                }
                            }
                        });
                });
            }
        }
        if !field.description.is_empty() {
            ui.label(egui::RichText::new(&field.description).weak().small());
        }
    }
    changed
}
