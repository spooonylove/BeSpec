{ pkgs ? import <nixpkgs> {} }:

pkgs.mkShell {
  nativeBuildInputs = with pkgs; [
    cargo
    rustc
    pkg-config
    gcc
    clang
  ];

  buildInputs = with pkgs; [
    alsa-lib
    dbus
    pipewire

    # GPU / Wayland / X11 runtime libs (needed by egui/glow)
    libglvnd
    libxkbcommon
    wayland
    vulkan-loader
    xorg.libX11
    xorg.libXcursor
    xorg.libXi
    xorg.libXrandr
    pipewire.jack
  ];

  # libspa-sys / pipewire-sys use bindgen → need libclang at compile time
  LIBCLANG_PATH = "${pkgs.libclang.lib}/lib";

  LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [
    pkgs.libglvnd
    pkgs.libxkbcommon
    pkgs.wayland
    pkgs.vulkan-loader
    pkgs.xorg.libX11
    pkgs.xorg.libXcursor
    pkgs.xorg.libXi
    pkgs.xorg.libXrandr
    pkgs.pipewire
    pkgs.pipewire.jack
  ];
}
