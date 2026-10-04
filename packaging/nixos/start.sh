#!/usr/bin/env bash
# DriftCrew auf NixOS starten: einfach  ./start.sh  ausführen.
cd "$(dirname "$0")"
exec nix-shell shell.nix --run '"$DRIFTCREW_LOADER" ./driftcrew'
