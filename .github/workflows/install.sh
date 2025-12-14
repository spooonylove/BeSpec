#!/bin/bash

# Define target directories (Standard XDG locations)
INSTALL_DIR="$HOME/.local/bin"
DESKTOP_DIR="$HOME/.local/share/applications"
ICON_DIR="$HOME/.local/share/icons"

# 1. Ensure directories exist
mkdir -p "$INSTALL_DIR"
mkdir -p "$DESKTOP_DIR"
mkdir -p "$ICON_DIR"

echo "Installing BeAnal to $INSTALL_DIR..."

# 2. Copy the Binary
cp beanal "$INSTALL_DIR/beanal"
chmod +x "$INSTALL_DIR/beanal"

# 3. Copy the Icon
# (Renaming it to beanal.png ensures it matches the app name if needed later)
cp icon.png "$ICON_DIR/beanal.png"

# 4. Generate the Desktop Entry
# We generate this dynamically to ensure the paths are absolute and correct
echo "[Desktop Entry]
Type=Application
Name=BeAnal
Comment=Audio Spectrum Visualizer
Exec=$INSTALL_DIR/beanal
Icon=beanal
Terminal=false
Categories=AudioVideo;Audio;
StartupWMClass=beanal" > "$DESKTOP_DIR/beanal.desktop"

# 5. Refresh Database (so the icon appears immediately)
update-desktop-database "$DESKTOP_DIR" 2>/dev/null

echo "✅ Installation complete!"
echo "You can now launch BeAnal from your application menu."