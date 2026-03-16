use image::AnimationDecoder;
use std::io::BufReader;
use std::path::Path;
use std::time::Duration;
use thiserror::Error;

use crate::core::animation::AnimationFrame;

#[derive(Error, Debug)]
pub enum LoadError {
    #[error("failed to read file: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to decode image: {0}")]
    Decode(#[from] image::ImageError),
    #[error("unsupported format")]
    Unsupported,
}

pub struct DecodedImage {
    pub pixels: Vec<u8>,   // RGBA8
    pub width: u32,
    pub height: u32,
}

/// Result of loading an image — either a single static frame or multiple animation frames.
pub enum LoadedImage {
    Static(DecodedImage),
    Animated(Vec<AnimationFrame>),
}

pub fn load_image(path: &Path) -> Result<DecodedImage, LoadError> {
    let img = image::open(path)?;
    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();
    Ok(DecodedImage {
        pixels: rgba.into_raw(),
        width,
        height,
    })
}

/// Load an image, detecting animated GIF or WebP and extracting all frames.
/// Falls back to a single static frame for non-animated formats.
pub fn load_image_animated(path: &Path) -> Result<LoadedImage, LoadError> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "gif" => {
            let file = std::fs::File::open(path)?;
            let reader = BufReader::new(file);
            let decoder = image::codecs::gif::GifDecoder::new(reader)?;
            let frames = decoder.into_frames().collect_frames()?;
            if frames.len() > 1 {
                let animation_frames = frames
                    .into_iter()
                    .map(|f| {
                        let (num, denom) = f.delay().numer_denom_ms();
                        let delay_ms = if denom == 0 {
                            100
                        } else {
                            (num / denom).max(10)
                        };
                        let rgba = f.into_buffer();
                        let (width, height) = rgba.dimensions();
                        AnimationFrame {
                            pixels: rgba.into_raw(),
                            width,
                            height,
                            delay: Duration::from_millis(delay_ms as u64),
                        }
                    })
                    .collect();
                return Ok(LoadedImage::Animated(animation_frames));
            }
            // Single-frame GIF — treat as static
            let img = image::open(path)?;
            let rgba = img.to_rgba8();
            let (width, height) = rgba.dimensions();
            Ok(LoadedImage::Static(DecodedImage {
                pixels: rgba.into_raw(),
                width,
                height,
            }))
        }
        "webp" => {
            let file = std::fs::File::open(path)?;
            let reader = BufReader::new(file);
            let decoder = image::codecs::webp::WebPDecoder::new(reader)?;
            if decoder.has_animation() {
                let frames = decoder.into_frames().collect_frames()?;
                let animation_frames = frames
                    .into_iter()
                    .map(|f| {
                        let (num, denom) = f.delay().numer_denom_ms();
                        let delay_ms = if denom == 0 {
                            100
                        } else {
                            (num / denom).max(10)
                        };
                        let rgba = f.into_buffer();
                        let (width, height) = rgba.dimensions();
                        AnimationFrame {
                            pixels: rgba.into_raw(),
                            width,
                            height,
                            delay: Duration::from_millis(delay_ms as u64),
                        }
                    })
                    .collect();
                return Ok(LoadedImage::Animated(animation_frames));
            }
            let img = image::open(path)?;
            let rgba = img.to_rgba8();
            let (width, height) = rgba.dimensions();
            Ok(LoadedImage::Static(DecodedImage {
                pixels: rgba.into_raw(),
                width,
                height,
            }))
        }
        _ => {
            let img = image::open(path)?;
            let rgba = img.to_rgba8();
            let (width, height) = rgba.dimensions();
            Ok(LoadedImage::Static(DecodedImage {
                pixels: rgba.into_raw(),
                width,
                height,
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_load_nonexistent_file() {
        let result = load_image(Path::new("/tmp/nonexistent_image.png"));
        assert!(result.is_err());
    }

    #[test]
    fn test_load_valid_png() {
        // Create a minimal 1x1 PNG in memory and write to temp file
        let img = image::RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 255]));
        let path = std::env::temp_dir().join("neoimv_test_image.png");
        img.save(&path).unwrap();

        let result = load_image(&path);
        assert!(result.is_ok());
        let decoded = result.unwrap();
        assert_eq!(decoded.width, 2);
        assert_eq!(decoded.height, 2);
        assert_eq!(decoded.pixels.len(), 2 * 2 * 4); // RGBA

        std::fs::remove_file(&path).ok();
    }
}
