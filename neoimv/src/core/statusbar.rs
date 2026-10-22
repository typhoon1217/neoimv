use egui::Ui;

pub struct StatusInfo<'a> {
    pub filename: &'a str,
    pub width: u32,
    pub height: u32,
    pub zoom_percent: f32,
    pub image_index: usize, // 0-based
    pub image_count: usize,
    pub scope: Option<&'a str>,
}

pub fn render_statusbar(ui: &mut Ui, info: &StatusInfo) {
    ui.horizontal(|ui| {
        ui.label(info.filename);
        ui.separator();
        ui.label(format!("{}x{}", info.width, info.height));
        ui.separator();
        ui.label(format!("{:.0}%", info.zoom_percent * 100.0));
        ui.separator();
        let position = if info.image_count == 0 {
            0
        } else {
            info.image_index + 1
        };
        ui.label(format!("{} / {}", position, info.image_count));
        if let Some(scope) = info.scope {
            ui.separator();
            ui.label(format!("filter: {}", scope));
        }
    });
}
