# neoimv

A clean, fast, cross-platform plugin-based image viewer built with Rust and egui.

## Features

- **Image Display** — Zoom, pan, fit-to-window/width/actual-size modes
- **Navigation** — Directory scanning with natural sort, prev/next, filmstrip thumbnails
- **Plugin System** — C ABI plugin architecture with `declare_plugin!` macro
- **Animation** — GIF/WebP frame playback with play/pause/stop
- **Keymap** — Centralized, fully rebindable keybindings with TOML config
- **Settings UI** — Auto-generated settings panel with per-plugin config tabs
- **Undo/Redo** — Memory-capped undo stack for plugin mutations
- **Background Loading** — Non-blocking image loading with plugin decoder priority
- **Drag & Drop** — Open images by dropping files onto the window
- **Save** — Save/Save-As with format detection
- **Plugin Manager** — Install/remove/update plugins from GitHub Releases via CLI
- **Desktop Integration** — `.desktop` file and install script for Linux file managers

## Building

```bash
cargo build --release
```

## Usage

```bash
# Open an image
neoimv path/to/image.png

# Open a directory
neoimv path/to/photos/

# Plugin management
neoimv plugin install owner/repo
neoimv plugin list
neoimv plugin remove plugin-name
```

## Keybindings

| Key | Action |
|-----|--------|
| Left/Right | Previous/Next image |
| Up/Down | Zoom in/out |
| f | Fit to window |
| q | Quit |
| Space | Next image |
| F11 | Toggle fullscreen |
| Ctrl+Z | Undo |
| Ctrl+Shift+Z | Redo |
| Ctrl+S | Save |

Keybindings are fully customizable via `~/.config/neoimv/keymap.toml` or the settings UI.

## Plugin Development

Create a new plugin using the SDK:

```rust
use neoimv_plugin_sdk::prelude::*;

struct MyPlugin;

impl NeoimvPlugin for MyPlugin {
    fn info() -> PluginInfoDef {
        PluginInfoDef {
            name: "my-plugin",
            version: "0.1.0",
            description: "My custom plugin",
        }
    }
}

declare_plugin!(MyPlugin);
```

See `plugins/rotate/` and `plugins/drawer/` for complete examples.

## License

MIT
