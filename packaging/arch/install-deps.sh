#!/usr/bin/env bash
# Einmalig: nötige Bibliotheken per pacman installieren (fragt nach dem sudo-Passwort).
# Grafiktreiber (einer davon, falls noch keiner da ist): vulkan-radeon, vulkan-intel oder nvidia-utils.
set -e
sudo pacman -S --needed vulkan-icd-loader alsa-lib libxkbcommon wayland libx11 libxcursor libxi libxrandr systemd-libs libcap libffi
echo
echo "Fertig. Falls Vulkan noch fehlt:  sudo pacman -S vulkan-radeon   (AMD)"
echo "                                   sudo pacman -S vulkan-intel    (Intel)"
echo "                                   sudo pacman -S nvidia-utils    (Nvidia)"
