#!/usr/bin/env bash
# install-desktop.sh — Install neoimv Linux desktop integration files
# Copies the .desktop entry and icon into the appropriate XDG directories,
# then registers neoimv as a file handler for common image MIME types.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

DESKTOP_FILE="$REPO_ROOT/assets/neoimv.desktop"
ICON_SVG="$REPO_ROOT/assets/neoimv.svg"

APPLICATIONS_DIR="$HOME/.local/share/applications"
ICONS_DIR="$HOME/.local/share/icons/hicolor/scalable/apps"

# ---------------------------------------------------------------------------
# Helper
# ---------------------------------------------------------------------------
info()  { printf '\033[1;34m[info]\033[0m  %s\n' "$*"; }
ok()    { printf '\033[1;32m[ ok ]\033[0m  %s\n' "$*"; }
warn()  { printf '\033[1;33m[warn]\033[0m  %s\n' "$*"; }
die()   { printf '\033[1;31m[err ]\033[0m  %s\n' "$*" >&2; exit 1; }

# ---------------------------------------------------------------------------
# Preflight checks
# ---------------------------------------------------------------------------
[[ -f "$DESKTOP_FILE" ]] || die "Desktop file not found: $DESKTOP_FILE"
[[ -f "$ICON_SVG" ]]     || die "Icon file not found: $ICON_SVG"

# ---------------------------------------------------------------------------
# Install desktop entry
# ---------------------------------------------------------------------------
info "Installing desktop entry..."
mkdir -p "$APPLICATIONS_DIR"
cp "$DESKTOP_FILE" "$APPLICATIONS_DIR/neoimv.desktop"
chmod 644 "$APPLICATIONS_DIR/neoimv.desktop"
ok "Desktop entry installed to $APPLICATIONS_DIR/neoimv.desktop"

# ---------------------------------------------------------------------------
# Install icon
# ---------------------------------------------------------------------------
info "Installing icon..."
mkdir -p "$ICONS_DIR"
cp "$ICON_SVG" "$ICONS_DIR/neoimv.svg"
chmod 644 "$ICONS_DIR/neoimv.svg"
ok "Icon installed to $ICONS_DIR/neoimv.svg"

# ---------------------------------------------------------------------------
# Optional: install binary to ~/bin if it exists in target/release
# ---------------------------------------------------------------------------
BINARY="$REPO_ROOT/target/release/neoimv"
if [[ -f "$BINARY" ]]; then
    BIN_DIR="$HOME/.local/bin"
    info "Installing binary to $BIN_DIR..."
    mkdir -p "$BIN_DIR"
    cp "$BINARY" "$BIN_DIR/neoimv"
    chmod 755 "$BIN_DIR/neoimv"
    ok "Binary installed to $BIN_DIR/neoimv"

    if [[ ":$PATH:" != *":$BIN_DIR:"* ]]; then
        warn "$BIN_DIR is not in PATH. Add the following to your shell profile:"
        warn "  export PATH=\"\$HOME/.local/bin:\$PATH\""
    fi
else
    warn "Release binary not found at $BINARY — skipping binary installation."
    warn "Build with: cargo build --release"
fi

# ---------------------------------------------------------------------------
# Update desktop and MIME databases
# ---------------------------------------------------------------------------
if command -v update-desktop-database &>/dev/null; then
    info "Updating desktop database..."
    update-desktop-database "$APPLICATIONS_DIR"
    ok "Desktop database updated."
else
    warn "update-desktop-database not found; skipping."
fi

if command -v xdg-mime &>/dev/null; then
    info "Registering neoimv as default handler for image MIME types..."
    MIME_TYPES=(
        image/png
        image/jpeg
        image/gif
        image/webp
        image/bmp
        image/tiff
    )
    for mime in "${MIME_TYPES[@]}"; do
        xdg-mime default neoimv.desktop "$mime"
    done
    ok "neoimv registered as default image viewer."
else
    warn "xdg-mime not found; skipping MIME type registration."
fi

if command -v gtk-update-icon-cache &>/dev/null; then
    info "Updating icon cache..."
    gtk-update-icon-cache -f -t "$HOME/.local/share/icons/hicolor" 2>/dev/null || true
    ok "Icon cache updated."
fi

echo ""
ok "neoimv desktop integration installed successfully."
