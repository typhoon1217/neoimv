/// Supported image file extensions for drag-and-drop.
pub const IMAGE_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "bmp", "tiff", "tif",
];

/// Check whether the given extension (lowercase) is a supported image format.
pub fn is_image_extension(ext: &str) -> bool {
    IMAGE_EXTENSIONS.contains(&ext)
}

/// Extract the first valid image path from a list of dropped egui files.
/// Returns None if no valid image was found.
pub fn first_image_from_dropped(
    dropped: &[egui::DroppedFile],
) -> Option<std::path::PathBuf> {
    for file in dropped {
        if let Some(path) = &file.path {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            if is_image_extension(&ext) {
                return Some(path.clone());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_image_extension() {
        assert!(is_image_extension("png"));
        assert!(is_image_extension("jpg"));
        assert!(is_image_extension("gif"));
        assert!(is_image_extension("webp"));
        assert!(!is_image_extension("txt"));
        assert!(!is_image_extension("pdf"));
        assert!(!is_image_extension(""));
    }
}
