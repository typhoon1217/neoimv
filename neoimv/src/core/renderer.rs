use egui::{ColorImage, Pos2, Rect, Sense, TextureHandle, TextureOptions, Ui, Vec2};

pub struct ImageRenderer {
    texture: Option<TextureHandle>,
    image_size: Vec2,
    zoom: f32,
    pan_offset: Vec2,
    fit_mode: FitMode,
}

#[derive(Clone, Copy, PartialEq)]
pub enum FitMode {
    FitWindow,
    ActualSize,
    FitWidth,
}

impl ImageRenderer {
    pub fn new() -> Self {
        Self {
            texture: None,
            image_size: Vec2::ZERO,
            zoom: 1.0,
            pan_offset: Vec2::ZERO,
            fit_mode: FitMode::FitWindow,
        }
    }

    pub fn set_image(&mut self, ctx: &egui::Context, pixels: &[u8], width: u32, height: u32) {
        let color_image =
            ColorImage::from_rgba_unmultiplied([width as usize, height as usize], pixels);
        self.texture = Some(ctx.load_texture("current_image", color_image, TextureOptions::LINEAR));
        self.image_size = Vec2::new(width as f32, height as f32);
        self.zoom = 1.0;
        self.pan_offset = Vec2::ZERO;
    }

    pub fn clear(&mut self) {
        self.texture = None;
        self.image_size = Vec2::ZERO;
    }

    pub fn zoom_in(&mut self) {
        self.zoom = (self.zoom * 1.1).min(50.0);
    }

    pub fn zoom_out(&mut self) {
        self.zoom = (self.zoom / 1.1).max(0.01);
    }

    pub fn set_fit_mode(&mut self, mode: FitMode) {
        self.fit_mode = mode;
    }

    pub fn zoom_level(&self) -> f32 {
        self.zoom
    }

    pub fn render(&mut self, ui: &mut Ui) {
        if self.texture.is_none() {
            ui.centered_and_justified(|ui| {
                ui.label("No image loaded");
            });
            return;
        }

        let available = ui.available_size();

        let (response, painter) = ui.allocate_painter(available, Sense::click_and_drag());

        // Handle pan
        if response.dragged() {
            self.pan_offset += response.drag_delta();
        }

        // Handle scroll zoom
        let scroll = ui.input(|i| i.raw_scroll_delta.y);
        if scroll > 0.0 {
            self.zoom_in();
        } else if scroll < 0.0 {
            self.zoom_out();
        }

        // Calculate display size based on fit mode
        let base_size = match self.fit_mode {
            FitMode::FitWindow => {
                let scale = (available.x / self.image_size.x)
                    .min(available.y / self.image_size.y)
                    .min(1.0);
                self.image_size * scale
            }
            FitMode::ActualSize => self.image_size,
            FitMode::FitWidth => {
                let scale = available.x / self.image_size.x;
                self.image_size * scale
            }
        };

        let display_size = base_size * self.zoom;

        // Center the image
        let offset = (available - display_size) * 0.5 + self.pan_offset;

        let rect = Rect::from_min_size(response.rect.min + offset, display_size);

        // Safe to unwrap: checked is_none() above
        let texture_id = self.texture.as_ref().unwrap().id();
        painter.image(
            texture_id,
            rect,
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            egui::Color32::WHITE,
        );
    }
}
