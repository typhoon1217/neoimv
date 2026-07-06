use image::{ImageFormat, RgbaImage};
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SaveError {
    #[error("no image to save")]
    NoImage,
    #[error("failed to create image buffer")]
    BufferError,
    #[error("failed to save: {0}")]
    Io(#[from] std::io::Error),
    #[error("unsupported format: {0}")]
    UnsupportedFormat(String),
    #[error("image encoding error: {0}")]
    Encode(#[from] image::ImageError),
}

/// Save RGBA pixel buffer to a file. Format is inferred from extension.
pub fn save_image(pixels: &[u8], width: u32, height: u32, path: &Path) -> Result<(), SaveError> {
    let img = RgbaImage::from_raw(width, height, pixels.to_vec()).ok_or(SaveError::BufferError)?;

    let format = ImageFormat::from_path(path).map_err(|_| {
        SaveError::UnsupportedFormat(
            path.extension()
                .unwrap_or_default()
                .to_string_lossy()
                .into(),
        )
    })?;

    img.save_with_format(path, format)?;
    Ok(())
}

/// Show a native file dialog for "Save As". Returns chosen path or None if cancelled.
pub fn save_as_dialog(current_path: Option<&Path>) -> Option<std::path::PathBuf> {
    let mut dialog = rfd::FileDialog::new()
        .add_filter("PNG", &["png"])
        .add_filter("JPEG", &["jpg", "jpeg"])
        .add_filter("WebP", &["webp"])
        .add_filter("BMP", &["bmp"])
        .add_filter("All Images", &["png", "jpg", "jpeg", "webp", "bmp", "tiff"]);

    if let Some(p) = current_path.and_then(|p| p.parent()) {
        dialog = dialog.set_directory(p);
    }

    dialog.save_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_save_png() {
        let pixels = vec![
            255u8, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
        ];
        let path = std::env::temp_dir().join("neoimv_save_test.png");
        save_image(&pixels, 2, 2, &path).unwrap();
        assert!(path.exists());
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_save_unsupported_format() {
        let pixels = vec![0u8; 16];
        let path = std::env::temp_dir().join("neoimv_save_test.xyz");
        assert!(save_image(&pixels, 2, 2, &path).is_err());
    }
}
