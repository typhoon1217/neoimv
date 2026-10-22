# neoimv

A clean, fast, cross-platform plugin-based image viewer built with Rust and egui.

## Features

- **Image Display** — Zoom, pan, fit-to-window/width/actual-size modes
- **Navigation** — Directory scanning with natural sort, prev/next, filmstrip thumbnails
- **Open Path** — `Ctrl+O` path box: Tab-complete folders, fzf-style search through subfolders, zoxide folder jumps, and browsing only the matches
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
| Ctrl+O | Open path / filter folder |
| f | Fit to window |
| q | Quit |
| Space | Next image |
| F11 | Toggle fullscreen |
| Ctrl+Z | Undo |
| Ctrl+Shift+Z | Redo |
| Ctrl+S | Save |

Keybindings are fully customizable via `~/.config/neoimv/keymap.toml` or the settings UI.

### Open path

`Ctrl+O` (or **Open** in the toolbar) shows a path box prefilled with the current folder. Text up to the last `/` picks the search root (`~/` and relative paths work); the rest is a case-insensitive fzf-style query (`^prefix`, `suffix$`, `'exact`, `!negate`; spaces separate terms).

- With nothing after the last `/`, the list shows that folder's direct images and subfolders.
- With a query, the root is scanned recursively in the background (dot-folders skipped, symlinks not followed, stops at 50,000 images). Results rank by file name: exact → prefix → substring → fuzzy, then shallower first.
- A query typed without any `/` also lists up to 5 [zoxide](https://github.com/ajeetdsouza/zoxide) folders (marked `z`, frecency order, zoxide's keyword rule: `doc ws` → `…/Documents/…/ws`) when `zoxide` is installed.
- `Enter` opens the selection: a folder opens as a folder (even one without images, so the next `Ctrl+O` starts there), an image opens with its own folder as the navigation list. `Ctrl+Enter` on an image instead limits navigation, filmstrip, and counter to all matched images (shown as `filter:` in the status bar; the next `Ctrl+O` restores that query).
- `Tab` completes the selection (folders get a trailing `/`), `Up`/`Down` move, `Esc` closes.
- Example: `/tmp/biocard-` then `Ctrl+Enter` browses only the `biocard-*` images under `/tmp`, nearest first.

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
