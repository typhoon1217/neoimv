use egui::{TextureHandle, ColorImage, TextureOptions, Ui, Vec2};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use sha2::{Sha256, Digest};

const THUMB_SIZE: u32 = 128;

pub struct Filmstrip {
    thumbnails: HashMap<PathBuf, TextureHandle>,
    pending: HashMap<PathBuf, ()>,
    result_rx: mpsc::Receiver<(PathBuf, Vec<u8>, u32, u32)>,
    request_tx: mpsc::Sender<(PathBuf, PathBuf)>, // (image_path, cache_path)
    cache_dir: PathBuf,
}

impl Filmstrip {
    pub fn new(cache_dir: PathBuf) -> Self {
        let _ = std::fs::create_dir_all(&cache_dir);
        let (req_tx, req_rx) = mpsc::channel::<(PathBuf, PathBuf)>();
        let (res_tx, res_rx) = mpsc::channel::<(PathBuf, Vec<u8>, u32, u32)>();

        // Single background thread for thumbnail generation
        std::thread::spawn(move || {
            while let Ok((path, cache_path)) = req_rx.recv() {
                if let Ok(thumb) = generate_thumbnail(&path, THUMB_SIZE) {
                    // Save to disk cache (best effort)
                    let _ = save_thumbnail_cache(&thumb.0, thumb.1, thumb.2, &cache_path);
                    let _ = res_tx.send((path, thumb.0, thumb.1, thumb.2));
                }
            }
        });

        Self {
            thumbnails: HashMap::new(),
            pending: HashMap::new(),
            result_rx: res_rx,
            request_tx: req_tx,
            cache_dir,
        }
    }

    /// Render the filmstrip bar. `images` is the full list, `current_index` is highlighted.
    /// Returns the index the user clicked, if any.
    pub fn render(
        &mut self,
        ui: &mut Ui,
        ctx: &egui::Context,
        images: &[PathBuf],
        current_index: usize,
    ) -> Option<usize> {
        // Collect completed thumbnails from background thread
        while let Ok((path, pixels, w, h)) = self.result_rx.try_recv() {
            let color_img =
                ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &pixels);
            let tex = ctx.load_texture(
                path.to_string_lossy().into_owned(),
                color_img,
                TextureOptions::LINEAR,
            );
            self.thumbnails.insert(path.clone(), tex);
            self.pending.remove(&path);
        }

        let mut clicked_index: Option<usize> = None;

        ui.horizontal(|ui| {
            let thumb_slot_width = 90.0_f32;
            let visible_count = ((ui.available_width() / thumb_slot_width) as usize).max(1);
            let start = current_index.saturating_sub(visible_count / 2);
            let end = (start + visible_count + 4).min(images.len());

            for i in start..end {
                let path = &images[i];
                let is_current = i == current_index;

                if let Some(tex) = self.thumbnails.get(path) {
                    let size = Vec2::new(80.0, 80.0);
                    let resp = ui.add(
                        egui::ImageButton::new(
                            egui::load::SizedTexture::new(tex.id(), size),
                        ),
                    );
                    if is_current {
                        ui.painter().rect_stroke(
                            resp.rect,
                            2.0,
                            egui::Stroke::new(2.0, egui::Color32::WHITE),
                            egui::StrokeKind::Outside,
                        );
                    }
                    if resp.clicked() {
                        clicked_index = Some(i);
                    }
                } else {
                    // Try to load from disk cache or schedule background generation
                    if !self.pending.contains_key(path) {
                        let cache_path = self.cache_path(path);
                        if let Ok(cached) = load_thumbnail_cache(&cache_path) {
                            let color_img = ColorImage::from_rgba_unmultiplied(
                                [cached.1 as usize, cached.2 as usize],
                                &cached.0,
                            );
                            let tex = ctx.load_texture(
                                path.to_string_lossy().into_owned(),
                                color_img,
                                TextureOptions::LINEAR,
                            );
                            self.thumbnails.insert(path.clone(), tex);
                        } else {
                            self.pending.insert(path.clone(), ());
                            let _ = self.request_tx.send((path.clone(), cache_path));
                        }
                    }
                    // Show placeholder while loading
                    let placeholder_resp = ui.add_sized([80.0, 80.0], egui::Label::new("..."));
                    if placeholder_resp.interact(egui::Sense::click()).clicked() {
                        clicked_index = Some(i);
                    }
                }
            }
        });

        clicked_index
    }

    fn cache_path(&self, image_path: &Path) -> PathBuf {
        let mut hasher = Sha256::new();
        hasher.update(image_path.to_string_lossy().as_bytes());
        let hash = format!("{:x}", hasher.finalize());
        self.cache_dir.join(format!("{}.png", &hash[..16]))
    }
}

fn generate_thumbnail(
    path: &Path,
    size: u32,
) -> Result<(Vec<u8>, u32, u32), image::ImageError> {
    let img = image::open(path)?;
    let thumb = img.thumbnail(size, size).to_rgba8();
    let (w, h) = thumb.dimensions();
    Ok((thumb.into_raw(), w, h))
}

fn save_thumbnail_cache(
    pixels: &[u8],
    w: u32,
    h: u32,
    path: &Path,
) -> Result<(), image::ImageError> {
    let img = image::RgbaImage::from_raw(w, h, pixels.to_vec())
        .expect("pixel buffer size mismatch");
    img.save(path)?;
    Ok(())
}

fn load_thumbnail_cache(
    path: &Path,
) -> Result<(Vec<u8>, u32, u32), image::ImageError> {
    let img = image::open(path)?.to_rgba8();
    let (w, h) = img.dimensions();
    Ok((img.into_raw(), w, h))
}
