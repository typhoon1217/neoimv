use sha2::{Digest, Sha256};
use std::path::Path;
use thiserror::Error;
use tracing::info;

#[derive(Error, Debug)]
pub enum InstallError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("No compatible release found for API version {0}")]
    NoCompatibleRelease(u32),
    #[error("No binary for platform {os}/{arch}")]
    NoBinaryForPlatform { os: String, arch: String },
    #[error("SHA256 mismatch: expected {expected}, got {actual}")]
    ChecksumMismatch { expected: String, actual: String },
    #[error("Failed to parse manifest: {0}")]
    ManifestError(String),
}

/// Expected manifest file in GitHub release assets: neoimv-plugin.toml
#[derive(Debug, serde::Deserialize)]
struct PluginManifest {
    name: String,
    version: String,
    api_version: u32,
    #[allow(dead_code)]
    description: String,
}

/// Binary naming convention: <name>-<os>-<arch>.<ext>
/// e.g. neoimv-rotate-linux-x86_64.so, neoimv-rotate-macos-aarch64.dylib
fn platform_binary_name(plugin_name: &str) -> String {
    let os = if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "windows"
    };
    let arch = if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "unknown"
    };
    let ext = if cfg!(target_os = "linux") {
        "so"
    } else if cfg!(target_os = "macos") {
        "dylib"
    } else {
        "dll"
    };
    format!("{}-{}-{}.{}", plugin_name, os, arch, ext)
}

pub fn install_from_github(
    repo_url: &str,
    plugins_dir: &Path,
    supported_api_version: u32,
) -> Result<super::registry::PluginEntry, InstallError> {
    let client = reqwest::blocking::Client::new();

    // Parse owner/repo from URL: https://github.com/owner/repo
    let parts: Vec<&str> = repo_url.trim_end_matches('/').rsplitn(3, '/').collect();
    let (repo, owner) = (parts[0], parts[1]);

    // 1. Fetch latest release
    let release_url = format!(
        "https://api.github.com/repos/{}/{}/releases/latest",
        owner, repo
    );
    let release: serde_json::Value = client
        .get(&release_url)
        .header("User-Agent", "neoimv")
        .send()?
        .json()?;

    let assets = release["assets"]
        .as_array()
        .ok_or(InstallError::ManifestError("no assets in release".into()))?;

    // 2. Download and parse manifest
    let manifest_asset = assets
        .iter()
        .find(|a| a["name"].as_str() == Some("neoimv-plugin.toml"))
        .ok_or(InstallError::ManifestError(
            "no neoimv-plugin.toml in release".into(),
        ))?;
    let manifest_url = manifest_asset["browser_download_url"]
        .as_str()
        .ok_or(InstallError::ManifestError("no download URL".into()))?;
    let manifest_text = client
        .get(manifest_url)
        .header("User-Agent", "neoimv")
        .send()?
        .text()?;
    let manifest: PluginManifest =
        toml::from_str(&manifest_text).map_err(|e| InstallError::ManifestError(e.to_string()))?;

    // 3. Check API version
    if manifest.api_version != supported_api_version {
        return Err(InstallError::NoCompatibleRelease(supported_api_version));
    }

    // 4. Find platform binary
    let binary_name = platform_binary_name(&manifest.name);
    let binary_asset = assets
        .iter()
        .find(|a| a["name"].as_str() == Some(binary_name.as_str()))
        .ok_or(InstallError::NoBinaryForPlatform {
            os: std::env::consts::OS.into(),
            arch: std::env::consts::ARCH.into(),
        })?;
    let binary_url = binary_asset["browser_download_url"]
        .as_str()
        .ok_or(InstallError::ManifestError("no binary download URL".into()))?;

    // 5. Download binary
    info!("Downloading {} ...", binary_name);
    let binary_data = client
        .get(binary_url)
        .header("User-Agent", "neoimv")
        .send()?
        .bytes()?;

    // 6. Verify SHA256 (checksum file: <binary_name>.sha256)
    let sha_name = format!("{}.sha256", binary_name);
    if let Some(sha_asset) = assets
        .iter()
        .find(|a| a["name"].as_str() == Some(sha_name.as_str()))
    {
        if let Some(sha_url) = sha_asset["browser_download_url"].as_str() {
            let expected = client
                .get(sha_url)
                .header("User-Agent", "neoimv")
                .send()?
                .text()?
                .trim()
                .to_lowercase();
            let mut hasher = Sha256::new();
            hasher.update(&binary_data);
            let actual = format!("{:x}", hasher.finalize());
            if actual != expected {
                return Err(InstallError::ChecksumMismatch { expected, actual });
            }
            info!("SHA256 verified.");
        }
    }

    // 7. Write to plugins dir
    std::fs::create_dir_all(plugins_dir)?;
    let lib_filename = format!(
        "lib{}{}",
        manifest.name.replace('-', "_"),
        if cfg!(target_os = "linux") {
            ".so"
        } else if cfg!(target_os = "macos") {
            ".dylib"
        } else {
            ".dll"
        },
    );
    let dest = plugins_dir.join(&lib_filename);
    std::fs::write(&dest, &binary_data)?;

    // Set executable permission on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755))?;
    }

    info!("Installed {} to {}", manifest.name, dest.display());

    Ok(super::registry::PluginEntry {
        name: manifest.name,
        version: manifest.version,
        source: repo_url.to_string(),
        enabled: true,
        api_version: manifest.api_version,
        lib_filename,
    })
}
