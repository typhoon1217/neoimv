use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct ActionInfo {
    pub id: String,          // "core:prev_image" or "rotate:rotate_cw"
    pub label: String,       // "Previous Image"
    pub category: String,    // "Navigation" or "Transform"
    pub plugin_name: String, // "core" or "rotate"
}

pub struct ActionRegistry {
    actions: HashMap<String, ActionInfo>,
}

impl ActionRegistry {
    pub fn new() -> Self {
        let mut registry = Self { actions: HashMap::new() };
        registry.register_core_actions();
        registry
    }

    fn register_core_actions(&mut self) {
        let core_actions = [
            ("core:prev_image", "Previous Image", "Navigation"),
            ("core:next_image", "Next Image", "Navigation"),
            ("core:zoom_in", "Zoom In", "View"),
            ("core:zoom_out", "Zoom Out", "View"),
            ("core:fit_window", "Fit to Window", "View"),
            ("core:fit_width", "Fit Width", "View"),
            ("core:actual_size", "Actual Size", "View"),
            ("core:fullscreen", "Toggle Fullscreen", "View"),
            ("core:quit", "Quit", "Application"),
            ("core:undo", "Undo", "Edit"),
            ("core:redo", "Redo", "Edit"),
            ("core:copy_image", "Copy Image", "Edit"),
            ("core:copy_path", "Copy File Path", "Edit"),
            ("core:rename_file", "Rename File", "File"),
            ("core:rotate_cw", "Rotate Clockwise", "Transform"),
            ("core:rotate_ccw", "Rotate Counterclockwise", "Transform"),
            ("core:save", "Save", "File"),
            ("core:save_as", "Save As", "File"),
        ];

        for (id, label, category) in core_actions {
            self.actions.insert(id.to_string(), ActionInfo {
                id: id.to_string(),
                label: label.to_string(),
                category: category.to_string(),
                plugin_name: "core".to_string(),
            });
        }
    }

    pub fn register(&mut self, id: String, label: String, category: String, plugin_name: String) {
        self.actions.insert(id.clone(), ActionInfo { id, label, category, plugin_name });
    }

    pub fn get(&self, id: &str) -> Option<&ActionInfo> {
        self.actions.get(id)
    }

    pub fn all_actions(&self) -> Vec<&ActionInfo> {
        let mut actions: Vec<_> = self.actions.values().collect();
        actions.sort_by(|a, b| a.plugin_name.cmp(&b.plugin_name).then(a.id.cmp(&b.id)));
        actions
    }

    pub fn actions_for_plugin(&self, plugin_name: &str) -> Vec<&ActionInfo> {
        self.actions.values()
            .filter(|a| a.plugin_name == plugin_name)
            .collect()
    }

    /// Group actions by category (plugin_name), sorted. Returns (category, actions) pairs.
    pub fn grouped_actions(&self) -> Vec<(String, Vec<&ActionInfo>)> {
        let mut groups: HashMap<String, Vec<&ActionInfo>> = HashMap::new();
        for action in self.actions.values() {
            groups.entry(action.plugin_name.clone()).or_default().push(action);
        }
        let mut result: Vec<_> = groups.into_iter().collect();
        result.sort_by(|a, b| a.0.cmp(&b.0));
        for (_, actions) in &mut result {
            actions.sort_by(|a, b| a.id.cmp(&b.id));
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_core_actions_registered() {
        let registry = ActionRegistry::new();
        assert!(registry.get("core:prev_image").is_some());
        assert!(registry.get("core:quit").is_some());
        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn test_register_plugin_action() {
        let mut registry = ActionRegistry::new();
        registry.register("rotate:cw".into(), "Rotate CW".into(), "Transform".into(), "rotate".into());
        let action = registry.get("rotate:cw").unwrap();
        assert_eq!(action.plugin_name, "rotate");
    }
}
