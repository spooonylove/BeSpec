#!/bin/bash

# Define target directories (Standard XDG locations)
INSTALL_DIR="$HOME/.local/bin"
DESKTOP_DIR="$HOME/.local/share/applications"
ICON_DIR="$HOME/.local/share/icons"

# 1. Ensure directories exist
mkdir -p "$INSTALL_DIR"
mkdir -p "$DESKTOP_DIR"
mkdir -p "$ICON_DIR"

echo "Installing BeSpec to $INSTALL_DIR..."

# 2. Copy the Binary
cp bespec "$INSTALL_DIR/bespec"
chmod +x "$INSTALL_DIR/bespec"

# 3. Copy the Icon
# (Renaming it to bespec.png ensures it matches the app name if needed later)
cp icon.png "$ICON_DIR/bespec.png"

# 4. Generate the Desktop Entry
# We generate this dynamically to ensure the paths are absolute and correct
echo "[Desktop Entry]
Type=Application
Name=BeSpec
Comment=Audio Spectrum Visualizer
Exec=$INSTALL_DIR/bespec
Icon=bespec
Terminal=false
Categories=AudioVideo;Audio;
StartupWMClass=bespec" > "$DESKTOP_DIR/bespec.desktop"

# 5. Refresh Database (so the icon appears immediately)
update-desktop-database "$DESKTOP_DIR" 2>/dev/null

echo "✅ Installation complete!"
echo "You can now launch BeSpec from your application menu."