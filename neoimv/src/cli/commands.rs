use clap::Subcommand;
use std::path::PathBuf;

use crate::plugin::{installer, registry::PluginRegistry};

#[derive(Subcommand)]
pub enum PluginCommand {
    /// Install a plugin from a GitHub repository URL
    Install { url: String },
    /// Remove an installed plugin
    Remove { name: String },
    /// List all installed plugins
    List,
    /// Update a plugin to the latest version
    Update { name: String },
    /// Enable a disabled plugin
    Enable { name: String },
    /// Disable a plugin without removing it
    Disable { name: String },
}

pub fn handle_plugin_command(
    cmd: &PluginCommand,
    config_dir: &PathBuf,
) -> Result<(), Box<dyn std::error::Error>> {
    let registry_path = config_dir.join("plugins").join("registry.toml");
    let plugins_dir = config_dir.join("plugins");
    let mut registry = PluginRegistry::load(&registry_path);

    match cmd {
        PluginCommand::Install { url } => {
            let entry = installer::install_from_github(url, &plugins_dir, 1)?;
            if registry.find(&entry.name).is_some() {
                registry.remove(&entry.name); // replace existing
            }
            println!("Installed: {} v{}", entry.name, entry.version);
            registry.plugins.push(entry);
        }
        PluginCommand::Remove { name } => {
            if let Some(entry) = registry.find(name) {
                let lib_path = plugins_dir.join(&entry.lib_filename);
                let _ = std::fs::remove_file(&lib_path);
            }
            if registry.remove(name) {
                println!("Removed: {}", name);
            } else {
                println!("Plugin not found: {}", name);
            }
        }
        PluginCommand::List => {
            if registry.plugins.is_empty() {
                println!("No plugins installed.");
            } else {
                for p in &registry.plugins {
                    let status = if p.enabled { "enabled" } else { "disabled" };
                    println!("  {} v{} [{}] ({})", p.name, p.version, status, p.source);
                }
            }
        }
        PluginCommand::Update { name } => {
            let source = registry
                .find(name)
                .map(|e| e.source.clone())
                .ok_or(format!("Plugin not found: {}", name))?;
            let entry = installer::install_from_github(&source, &plugins_dir, 1)?;
            registry.remove(name);
            println!("Updated: {} v{}", entry.name, entry.version);
            registry.plugins.push(entry);
        }
        PluginCommand::Enable { name } => {
            if let Some(entry) = registry.find_mut(name) {
                entry.enabled = true;
                println!("Enabled: {}", name);
            } else {
                println!("Plugin not found: {}", name);
            }
        }
        PluginCommand::Disable { name } => {
            if let Some(entry) = registry.find_mut(name) {
                entry.enabled = false;
                println!("Disabled: {}", name);
            } else {
                println!("Plugin not found: {}", name);
            }
        }
    }

    registry.save(&registry_path)?;
    Ok(())
}
