use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use neoimv_plugin_sdk::*;

pub enum LoadRequest {
    Load(PathBuf),
    Shutdown,
}

pub enum LoadResult {
    Loaded {
        path: PathBuf,
        pixels: Vec<u8>,
        width: u32,
        height: u32,
    },
    Error {
        path: PathBuf,
        error: String,
    },
}

/// Extracted decoder info from a loaded plugin — all fields are Copy+Send.
/// Bare function pointers are Copy+Send, so they can be sent to the background thread.
#[derive(Clone, Copy)]
pub struct PluginDecoder {
    pub decode_fn: unsafe extern "C" fn(*const u8, usize) -> MutatedImage,
    pub free_fn: Option<unsafe extern "C" fn(*mut u8)>,
}

// Safety: bare fn pointers are Send
unsafe impl Send for PluginDecoder {}

pub struct BackgroundLoader {
    sender: mpsc::Sender<LoadRequest>,
    pub receiver: mpsc::Receiver<LoadResult>,
}

impl BackgroundLoader {
    /// Create a new background loader with plugin decoder function pointers.
    /// `decoders` is ordered by decoder_priority config — first match wins.
    /// `ext_map` maps file extension (lowercase, no dot) → index into `decoders`.
    pub fn new(
        decoders: Vec<PluginDecoder>,
        ext_map: std::collections::HashMap<String, Vec<usize>>,
    ) -> Self {
        let (req_tx, req_rx) = mpsc::channel::<LoadRequest>();
        let (res_tx, res_rx) = mpsc::channel::<LoadResult>();

        thread::spawn(move || {
            while let Ok(request) = req_rx.recv() {
                match request {
                    LoadRequest::Load(path) => {
                        // Try plugin decoders first based on file extension
                        let ext = path
                            .extension()
                            .and_then(|e| e.to_str())
                            .map(|e| e.to_lowercase())
                            .unwrap_or_default();
                        let mut decoded_by_plugin = false;

                        if let Some(indices) = ext_map.get(&ext) {
                            if let Ok(file_data) = std::fs::read(&path) {
                                for &idx in indices {
                                    let dec = &decoders[idx];
                                    let result = std::panic::catch_unwind(
                                        std::panic::AssertUnwindSafe(|| unsafe {
                                            (dec.decode_fn)(file_data.as_ptr(), file_data.len())
                                        }),
                                    );
                                    if let Ok(mi) = result {
                                        if mi.result == PluginResultCode::Ok && !mi.pixels.is_null() {
                                            let size = (mi.width * mi.height * 4) as usize;
                                            let pixels = unsafe {
                                                std::slice::from_raw_parts(mi.pixels, size)
                                            }
                                            .to_vec();
                                            // Free plugin memory immediately after copy
                                            if let Some(free) = dec.free_fn {
                                                unsafe { free(mi.pixels) };
                                            }
                                            let _ = res_tx.send(LoadResult::Loaded {
                                                path: path.clone(),
                                                pixels,
                                                width: mi.width,
                                                height: mi.height,
                                            });
                                            decoded_by_plugin = true;
                                            break;
                                        }
                                    }
                                }
                            }
                        }

                        // Fall back to built-in decoder
                        if !decoded_by_plugin {
                            match crate::core::image_loader::load_image(&path) {
                                Ok(decoded) => {
                                    let _ = res_tx.send(LoadResult::Loaded {
                                        path,
                                        pixels: decoded.pixels,
                                        width: decoded.width,
                                        height: decoded.height,
                                    });
                                }
                                Err(e) => {
                                    let _ = res_tx.send(LoadResult::Error {
                                        path,
                                        error: e.to_string(),
                                    });
                                }
                            }
                        }
                    }
                    LoadRequest::Shutdown => break,
                }
            }
        });

        Self {
            sender: req_tx,
            receiver: res_rx,
        }
    }

    pub fn request_load(&self, path: PathBuf) {
        let _ = self.sender.send(LoadRequest::Load(path));
    }
}

impl Drop for BackgroundLoader {
    fn drop(&mut self) {
        let _ = self.sender.send(LoadRequest::Shutdown);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_background_load() {
        let loader = BackgroundLoader::new(vec![], std::collections::HashMap::new());

        // Create a test image
        let path = std::env::temp_dir().join("neoimv_bg_test.png");
        let img = image::RgbaImage::from_pixel(4, 4, image::Rgba([255, 0, 0, 255]));
        img.save(&path).unwrap();

        loader.request_load(path.clone());

        match loader
            .receiver
            .recv_timeout(std::time::Duration::from_secs(5))
        {
            Ok(LoadResult::Loaded { width, height, .. }) => {
                assert_eq!(width, 4);
                assert_eq!(height, 4);
            }
            Ok(LoadResult::Error { error, .. }) => panic!("Load failed: {}", error),
            Err(_) => panic!("Timeout waiting for background load"),
        }

        std::fs::remove_file(&path).ok();
    }
}
