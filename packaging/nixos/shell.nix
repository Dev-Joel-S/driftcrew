# Stellt die Bibliotheken bereit, die DriftCrew zum Laufen braucht (Grafik, Ton, Eingabe).
{ pkgs ? import <nixpkgs> { } }:
pkgs.mkShell {
  LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath (with pkgs; [
    vulkan-loader
    alsa-lib
    udev
    libxkbcommon
    wayland
    xorg.libX11
    xorg.libXcursor
    xorg.libXi
    xorg.libXrandr
    stdenv.cc.cc.lib
  ]);
  # Normale Linux-Programme suchen ihren Lader unter /lib64 – den gibt es auf NixOS nicht.
  DRIFTCREW_LOADER = "${pkgs.glibc}/lib/ld-linux-x86-64.so.2";
}
