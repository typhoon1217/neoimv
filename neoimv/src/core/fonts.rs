use std::path::PathBuf;
use tracing::{info, warn};

/// Detect system font paths for UI rendering.
/// Returns a list of font file paths to load as fallbacks.
pub fn detect_system_fonts() -> Vec<PathBuf> {
    let mut fonts = Vec::new();

    #[cfg(target_os = "linux")]
    {
        // Query fontconfig for the default sans-serif font
        if let Ok(output) = std::process::Command::new("fc-match")
            .args(["--format=%{file}", "sans-serif"])
            .output()
        {
            if output.status.success() {
                let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !path.is_empty() {
                    info!("Detected system font: {}", path);
                    fonts.push(PathBuf::from(path));
                }
            }
        }

        // Also try to get CJK fallback
        if let Ok(output) = std::process::Command::new("fc-match")
            .args(["--format=%{file}", ":lang=ko"])
            .output()
        {
            if output.status.success() {
                let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !path.is_empty() && !fonts.iter().any(|f| f.to_str() == Some(&path)) {
                    info!("Detected CJK fallback font: {}", path);
                    fonts.push(PathBuf::from(path));
                }
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        let candidates = [
            "/System/Library/Fonts/SFNS.ttf",
            "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
            "/System/Library/Fonts/AppleSDGothicNeo.ttc",
        ];
        for path in &candidates {
            let p = PathBuf::from(path);
            if p.exists() {
                fonts.push(p);
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
        let candidates = [
            format!("{windir}\\Fonts\\segoeui.ttf"),
            format!("{windir}\\Fonts\\malgun.ttf"),  // Korean
            format!("{windir}\\Fonts\\msyh.ttc"),    // Chinese
            format!("{windir}\\Fonts\\yugothm.ttc"), // Japanese
        ];
        for path in &candidates {
            let p = PathBuf::from(path);
            if p.exists() {
                fonts.push(p);
            }
        }
    }

    if fonts.is_empty() {
        warn!("No system fonts detected — UI will use egui default font");
    }

    fonts
}

/// Load fonts into egui context.
/// If `font_override` is a valid path, use that font instead of system detection.
/// Pass "system" or "" to use auto-detection.
pub fn configure_fonts(ctx: &egui::Context, font_override: &str) {
    let font_paths = if !font_override.is_empty() && font_override != "system" {
        let p = std::path::PathBuf::from(font_override);
        if p.exists() {
            vec![p]
        } else {
            warn!(
                "Font override path not found: {}, falling back to system",
                font_override
            );
            detect_system_fonts()
        }
    } else {
        detect_system_fonts()
    };

    if font_paths.is_empty() {
        return;
    }

    let mut fonts = egui::FontDefinitions::default();

    for (i, path) in font_paths.iter().enumerate() {
        match std::fs::read(path) {
            Ok(data) => {
                let name = format!("system_font_{}", i);
                fonts
                    .font_data
                    .insert(name.clone(), egui::FontData::from_owned(data).into());
                fonts
                    .families
                    .get_mut(&egui::FontFamily::Proportional)
                    .unwrap()
                    .push(name);
            }
            Err(e) => {
                warn!("Failed to read font {}: {}", path.display(), e);
            }
        }
    }

    ctx.set_fonts(fonts);
}
