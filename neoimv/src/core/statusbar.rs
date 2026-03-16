use egui::Ui;

pub struct StatusInfo<'a> {
    pub filename: &'a str,
    pub width: u32,
    pub height: u32,
    pub zoom_percent: f32,
    pub image_index: usize,    // 0-based
    pub image_count: usize,
}

pub fn render_statusbar(ui: &mut Ui, info: &StatusInfo) {
    ui.horizontal(|ui| {
        ui.label(info.filename);
        ui.separator();
        ui.label(format!("{}x{}", info.width, info.height));
        ui.separator();
        ui.label(format!("{:.0}%", info.zoom_percent * 100.0));
        ui.separator();
        ui.label(format!("{} / {}", info.image_index + 1, info.image_count));
    });
}
