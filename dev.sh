#!/usr/bin/env bash
# Rebuilds and restarts NixOS Update Checker on every change, via devloop
# (~/src/home-services/devloop): the same optimized, incremental build the
# launcher and the login autostart run. Shows the log here; Ctrl+C stops it.
exec "$HOME/src/home-services/devloop/devloop" dev nixos-update-checker "$@"
