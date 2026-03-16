use neoimv_plugin_sdk::ffi_types::{UiNode, UiNodeType};
use std::ffi::CStr;

/// Recursively walks a `UiNode` tree and renders egui widgets.
///
/// Returns the `action_id` string of the first widget the user interacted
/// with during this frame, or `None` if there was no interaction.
///
/// # Safety
/// The caller must ensure that the `UiNode` pointer fields (children, props
/// string pointers) are valid for the duration of this call.
pub fn render_ui_tree(ui: &mut egui::Ui, node: &UiNode) -> Option<String> {
    // Skip invisible nodes
    if node.props.visible == 0 {
        return None;
    }

    let label = props_label(&node.props.label);
    let action_id = props_str(&node.props.action_id);

    match node.node_type {
        UiNodeType::Panel => {
            // Render as a group container with an optional heading
            let mut result: Option<String> = None;
            ui.group(|ui| {
                if !label.is_empty() {
                    ui.heading(&label);
                }
                for child in children_slice(node) {
                    if let Some(act) = render_ui_tree(ui, child) {
                        result = Some(act);
                    }
                }
            });
            result
        }

        UiNodeType::Button => {
            let resp = ui.button(&label);
            if resp.clicked() {
                Some(action_id.unwrap_or_else(|| label.clone()))
            } else {
                None
            }
        }

        UiNodeType::Label => {
            ui.label(&label);
            None
        }

        UiNodeType::Slider => {
            let mut value = node.props.value_float;
            let min = node.props.min as f64;
            let max = node.props.max as f64;
            let changed = ui
                .add(egui::Slider::new(&mut value, (min as f32)..=(max as f32)).text(&label))
                .changed();
            if changed {
                Some(action_id.unwrap_or_else(|| label.clone()))
            } else {
                None
            }
        }

        UiNodeType::ColorPicker => {
            let [r, g, b, a] = node.props.color;
            let mut color = egui::Color32::from_rgba_unmultiplied(r, g, b, a);
            let changed = egui::color_picker::color_edit_button_srgba(
                ui,
                &mut color,
                egui::color_picker::Alpha::OnlyBlend,
            )
            .changed();
            if changed {
                Some(action_id.unwrap_or_else(|| label.clone()))
            } else {
                None
            }
        }

        UiNodeType::Separator => {
            ui.separator();
            None
        }

        UiNodeType::Toggle => {
            let mut checked = node.props.value_bool != 0;
            let changed = ui.checkbox(&mut checked, &label).changed();
            if changed {
                Some(action_id.unwrap_or_else(|| label.clone()))
            } else {
                None
            }
        }

        UiNodeType::Image => {
            // Render image from path stored in the label field (best-effort)
            if !label.is_empty() {
                ui.image(egui::ImageSource::Uri(label.clone().into()));
            }
            None
        }
    }
}

// ---------- helpers ----------

/// Return a slice over the node's children array.
///
/// # Safety
/// Relies on the plugin having set a valid pointer + count pair.
fn children_slice(node: &UiNode) -> &[UiNode] {
    if node.children.is_null() || node.children_count == 0 {
        return &[];
    }
    // SAFETY: plugin is responsible for the validity of this pointer.
    unsafe { std::slice::from_raw_parts(node.children, node.children_count as usize) }
}

/// Convert a nullable C string pointer to a Rust `String`.
fn props_str(ptr: &*const std::os::raw::c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    // SAFETY: ptr is non-null; plugin must provide a valid NUL-terminated string.
    let s = unsafe { CStr::from_ptr(*ptr) }
        .to_string_lossy()
        .into_owned();
    if s.is_empty() { None } else { Some(s) }
}

/// Convert a label pointer to a `String`, returning an empty string on null.
fn props_label(ptr: &*const std::os::raw::c_char) -> String {
    props_str(ptr).unwrap_or_default()
}
