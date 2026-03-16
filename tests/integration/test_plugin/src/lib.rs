use neoimv_plugin_sdk::prelude::*;

pub struct TestPlugin;

impl Default for TestPlugin {
    fn default() -> Self {
        TestPlugin
    }
}

impl NeoimvPlugin for TestPlugin {
    fn info(&self) -> PluginInfoDef {
        PluginInfoDef {
            name: "test-plugin",
            version: "0.1.0",
            description: "Minimal plugin for integration tests",
        }
    }

    fn capabilities(&self) -> u32 {
        CAPABILITY_ACTIONS | CAPABILITY_UI | CAPABILITY_CONFIG
    }

    fn actions(&self) -> Vec<Action> {
        vec![Action {
            id: "test:hello",
            label: "Hello",
            default_key: None,
            category: "test",
        }]
    }

    fn on_action(&mut self, action: &str, _ctx: &Context) -> Option<Image> {
        if action == "test:hello" {
            // Return a tiny 1x1 red pixel image
            Some(Image {
                pixels: vec![255, 0, 0, 255],
                width: 1,
                height: 1,
            })
        } else {
            None
        }
    }

    fn render_ui(&mut self, _ctx: &Context) -> Option<Vec<UiNode>> {
        Some(vec![UiNode {
            node_type: UiNodeType::Label,
            children: std::ptr::null(),
            children_count: 0,
            props: UiProps {
                id: std::ptr::null(),
                label: std::ptr::null(),
                action_id: std::ptr::null(),
                value_float: 0.0,
                value_bool: 0,
                min: 0.0,
                max: 0.0,
                color: [0, 0, 0, 255],
                visible: 1,
            },
        }])
    }

    fn on_config_changed(&mut self, _config: &str) {
        // Pass through — no-op for tests
    }

    fn init(&mut self, _ctx: &Context) -> Result<(), String> {
        Ok(())
    }

    fn shutdown(&mut self) {
        // No-op
    }
}

declare_plugin!(TestPlugin);
