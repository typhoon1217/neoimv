mod app;
mod cli;
mod config;
mod core;
mod keymap;
mod plugin;

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

use config::settings::{config_dir, AppConfig};
use core::window::WindowState;

#[derive(Subcommand)]
enum Command {
    /// Manage plugins
    Plugin {
        #[command(subcommand)]
        cmd: crate::cli::commands::PluginCommand,
    },
}

#[derive(Parser)]
#[command(name = "neoimv", about = "A clean, plugin-based image viewer")]
struct Cli {
    /// Image file or directory to open
    path: Option<PathBuf>,
    #[command(subcommand)]
    command: Option<Command>,
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_env("NEOIMV_LOG").unwrap_or_else(|_| EnvFilter::new("warn")),
        )
        .init();

    let cli = Cli::parse();

    // Load config dir
    let cfg_dir = config_dir();

    // Handle CLI subcommands before starting the GUI
    if let Some(command) = &cli.command {
        match command {
            Command::Plugin { cmd } => {
                if let Err(e) = crate::cli::commands::handle_plugin_command(cmd, &cfg_dir) {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        return;
    }

    // Load config from config dir
    let config = AppConfig::load(&cfg_dir.join("config.toml"));

    // Load window state
    let window_state_path = cfg_dir.join("window_state.toml");
    let window_state = WindowState::load(&window_state_path);

    // Build viewport from saved window state
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([window_state.width, window_state.height])
        .with_title("neoimv");

    if let (Some(x), Some(y)) = (window_state.x, window_state.y) {
        viewport = viewport.with_position(egui::pos2(x, y));
    }

    if window_state.fullscreen {
        viewport = viewport.with_fullscreen(true);
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    let initial_path = cli.path.map(|p| {
        if p.is_relative() {
            std::env::current_dir().unwrap_or_default().join(p)
        } else {
            p
        }
    });

    eframe::run_native(
        "neoimv",
        options,
        Box::new(move |cc| Ok(Box::new(app::NeoimvApp::new(cc, initial_path, config)))),
    )
    .expect("Failed to start neoimv");
}
