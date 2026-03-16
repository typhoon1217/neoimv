use egui::{Color32, RichText, Ui};
use crate::keymap::config::KeymapConfig;
use crate::keymap::action::ActionRegistry;

pub struct KeymapEditor {
    /// When Some(action_id), we are capturing next keypress for rebinding
    rebinding: Option<String>,
    filter_text: String,
}

impl KeymapEditor {
    pub fn new() -> Self {
        Self { rebinding: None, filter_text: String::new() }
    }

    pub fn render(
        &mut self, ui: &mut Ui,
        keymap: &mut KeymapConfig,
        registry: &ActionRegistry,
    ) -> bool {
        let mut changed = false;

        // Search/filter
        ui.horizontal(|ui| {
            ui.label("Filter:");
            ui.text_edit_singleline(&mut self.filter_text);
            if ui.button("Reset All to Defaults").clicked() {
                *keymap = KeymapConfig::defaults();
                changed = true;
            }
        });
        ui.separator();

        // If rebinding, capture next keypress via raw input
        if let Some(ref action_id) = self.rebinding.clone() {
            ui.colored_label(
                Color32::YELLOW,
                format!("Press a key for '{}'... (Esc to cancel)", action_id),
            );
            let events = ui.input(|i| i.events.clone());
            for event in &events {
                if let egui::Event::Key { key, pressed: true, modifiers, .. } = event {
                    if *key == egui::Key::Escape {
                        self.rebinding = None;
                    } else {
                        let key_str = format_key_combo(*modifiers, *key);
                        // Check for conflicts
                        if let Some(conflict) = keymap.action_for_key(&key_str) {
                            if conflict != *action_id {
                                // Unbind the conflicting key first
                                keymap.unbind_key(&key_str);
                            }
                        }
                        keymap.bind(action_id, &key_str);
                        self.rebinding = None;
                        changed = true;
                    }
                }
            }
            return changed;
        }

        // Group actions by category (plugin name or "core")
        let filter_lower = self.filter_text.to_lowercase();
        for (category, actions) in registry.grouped_actions() {
            let visible: Vec<_> = actions.iter()
                .filter(|a| filter_lower.is_empty()
                    || a.label.to_lowercase().contains(&filter_lower)
                    || a.id.to_lowercase().contains(&filter_lower))
                .collect();
            if visible.is_empty() { continue; }

            egui::CollapsingHeader::new(&category)
                .default_open(true)
                .show(ui, |ui| {
                    egui::Grid::new(format!("keymap_{}", category))
                        .striped(true)
                        .show(ui, |ui| {
                            ui.label(RichText::new("Action").strong());
                            ui.label(RichText::new("Key").strong());
                            ui.label("");
                            ui.end_row();

                            for action in visible {
                                ui.label(&action.label);
                                let current_key = keymap.key_for_action(&action.id)
                                    .unwrap_or_else(|| "unbound".into());
                                ui.label(&current_key);
                                if ui.button("Rebind").clicked() {
                                    self.rebinding = Some(action.id.clone());
                                }
                                ui.end_row();
                            }
                        });
                });
        }
        changed
    }
}

fn format_key_combo(mods: egui::Modifiers, key: egui::Key) -> String {
    let mut parts: Vec<String> = Vec::new();
    if mods.ctrl { parts.push("Ctrl".into()); }
    if mods.alt { parts.push("Alt".into()); }
    if mods.shift { parts.push("Shift".into()); }
    parts.push(key_name(key));
    parts.join("+")
}

fn key_name(key: egui::Key) -> String {
    match key {
        egui::Key::ArrowLeft => "Left".into(),
        egui::Key::ArrowRight => "Right".into(),
        egui::Key::ArrowUp => "Up".into(),
        egui::Key::ArrowDown => "Down".into(),
        egui::Key::Escape => "Escape".into(),
        egui::Key::Space => "Space".into(),
        egui::Key::Enter => "Enter".into(),
        egui::Key::Delete => "Delete".into(),
        other => format!("{:?}", other),
    }
}
