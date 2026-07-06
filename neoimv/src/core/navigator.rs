use natord::compare as natural_compare;
use std::path::{Path, PathBuf};

const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "bmp", "tiff", "tif"];

pub struct Navigator {
    images: Vec<PathBuf>,
    index: usize,
}

impl Navigator {
    pub fn new() -> Self {
        Self {
            images: Vec::new(),
            index: 0,
        }
    }

    pub fn scan_directory(&mut self, path: &Path) {
        let dir = if path.is_file() {
            path.parent().unwrap_or(path)
        } else {
            path
        };

        let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.extension()
                    .and_then(|ext| ext.to_str())
                    .map(|ext| IMAGE_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
                    .unwrap_or(false)
            })
            .collect();

        files.sort_by(|a, b| {
            natural_compare(
                a.file_name().unwrap_or_default().to_str().unwrap_or(""),
                b.file_name().unwrap_or_default().to_str().unwrap_or(""),
            )
        });

        self.images = files;

        // If opened with a specific file, set index to that file
        if path.is_file() {
            self.index = self.images.iter().position(|p| p == path).unwrap_or(0);
        } else {
            self.index = 0;
        }
    }

    pub fn current(&self) -> Option<&PathBuf> {
        self.images.get(self.index)
    }

    pub fn next(&mut self) -> Option<&PathBuf> {
        if self.images.is_empty() {
            return None;
        }
        self.index = (self.index + 1) % self.images.len();
        self.current()
    }

    pub fn prev(&mut self) -> Option<&PathBuf> {
        if self.images.is_empty() {
            return None;
        }
        self.index = if self.index == 0 {
            self.images.len() - 1
        } else {
            self.index - 1
        };
        self.current()
    }

    pub fn count(&self) -> usize {
        self.images.len()
    }

    pub fn current_index(&self) -> usize {
        self.index
    }

    /// Return the full list of image paths.
    pub fn images(&self) -> &[PathBuf] {
        &self.images
    }

    /// Jump to a specific index. Clamps to valid range.
    pub fn go_to(&mut self, index: usize) -> Option<&PathBuf> {
        if self.images.is_empty() {
            return None;
        }
        self.index = index.min(self.images.len() - 1);
        self.current()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn setup_test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("neoimv_nav_{}", name));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        // Create test image files (empty files with image extensions)
        for name in &[
            "img_1.png",
            "img_2.jpg",
            "img_10.png",
            "img_3.gif",
            "readme.txt",
        ] {
            fs::write(dir.join(name), b"").unwrap();
        }
        dir
    }

    #[test]
    fn test_scan_filters_images_only() {
        let dir = setup_test_dir("filter");
        let mut nav = Navigator::new();
        nav.scan_directory(&dir);
        assert_eq!(nav.count(), 4); // excludes readme.txt
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_natural_sort_order() {
        let dir = setup_test_dir("sort");
        let mut nav = Navigator::new();
        nav.scan_directory(&dir);

        let names: Vec<&str> = nav
            .images
            .iter()
            .filter_map(|p| p.file_name()?.to_str())
            .collect();
        assert_eq!(
            names,
            vec!["img_1.png", "img_2.jpg", "img_3.gif", "img_10.png"]
        );
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_next_prev_wraps() {
        let dir = setup_test_dir("wraps");
        let mut nav = Navigator::new();
        nav.scan_directory(&dir);

        assert_eq!(nav.current_index(), 0);
        nav.next();
        assert_eq!(nav.current_index(), 1);
        nav.prev();
        assert_eq!(nav.current_index(), 0);
        nav.prev(); // wraps to end
        assert_eq!(nav.current_index(), 3);
        nav.next(); // wraps to start
        assert_eq!(nav.current_index(), 0);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_scan_with_file_path_sets_index() {
        let dir = setup_test_dir("fileindex");
        let mut nav = Navigator::new();
        nav.scan_directory(&dir.join("img_3.gif"));
        assert_eq!(nav.current_index(), 2); // img_3.gif is 3rd in natural sort
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_empty_directory() {
        let dir = std::env::temp_dir().join("neoimv_nav_empty");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let mut nav = Navigator::new();
        nav.scan_directory(&dir);
        assert_eq!(nav.count(), 0);
        assert!(nav.current().is_none());
        assert!(nav.next().is_none());
        fs::remove_dir_all(&dir).ok();
    }
}
