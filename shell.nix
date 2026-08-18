{ pkgs ? import <nixpkgs> { } }:

with pkgs;

mkShell rec {
  nativeBuildInputs = [
    chromium
    lld
    pkg-config
    trunk
  ];
  buildInputs = [
    udev alsa-lib-with-plugins vulkan-loader
    libx11 libxcursor libxi libxrandr # To use the x11 feature
    libxkbcommon wayland # To use the wayland feature
  ];
  LD_LIBRARY_PATH = lib.makeLibraryPath buildInputs;
  shellHook = ''
    export PLAYWRIGHT_CHROMIUM_PATH=${chromium}/bin/chromium
  '';
}
