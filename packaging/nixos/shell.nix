# Stellt die Bibliotheken bereit, die DriftCrew zum Laufen braucht (Grafik, Ton, Eingabe).
{ pkgs ? import <nixpkgs> { } }:
pkgs.mkShell {
  LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath (with pkgs; [
    vulkan-loader
    alsa-lib
    udev
    libxkbcommon
    wayland
    # Neue Namen (ab 2026), mit Rückfall auf die alten für ältere Nixpkgs.
    (pkgs.libx11 or pkgs.xorg.libX11)
    (pkgs.libxcursor or pkgs.xorg.libXcursor)
    (pkgs.libxi or pkgs.xorg.libXi)
    (pkgs.libxrandr or pkgs.xorg.libXrandr)
    stdenv.cc.cc.lib
  ]);
  # Normale Linux-Programme suchen ihren Lader unter /lib64 – den gibt es auf NixOS nicht.
  DRIFTCREW_LOADER = "${pkgs.glibc}/lib/ld-linux-x86-64.so.2";
}
