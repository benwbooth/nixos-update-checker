#!/usr/bin/env bash
# Development script for NixOS Update Checker
#
# Usage:
#   ./dev.sh          # Start with cargo-watch auto-restart
#   ./dev.sh --no-watch  # Start without auto-restart

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$SCRIPT_DIR"
UNIT_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"
UNIT_NAME="nixos-update-checker-dev"

# Install systemd user unit
install_unit() {
    mkdir -p "$UNIT_DIR"

    if [ "$1" = "--no-watch" ]; then
        # Simple mode without cargo-watch
        cat > "$UNIT_DIR/$UNIT_NAME.service" << EOF
[Unit]
Description=NixOS Update Checker (dev)

[Service]
Type=simple
WorkingDirectory=$PROJECT_DIR
ExecStart=/nix/var/nix/profiles/system/sw/bin/nix develop --command cargo run
Restart=on-failure
RestartSec=2
Environment=RUST_BACKTRACE=1
EOF
    else
        # Watch mode with auto-restart on file changes
        cat > "$UNIT_DIR/$UNIT_NAME.service" << EOF
[Unit]
Description=NixOS Update Checker (dev with watch)

[Service]
Type=simple
WorkingDirectory=$PROJECT_DIR
ExecStart=/nix/var/nix/profiles/system/sw/bin/nix develop --command cargo watch -x run -w src -w qml -w resources -w build.rs -w Cargo.toml
Restart=on-failure
RestartSec=2
Environment=RUST_BACKTRACE=1
EOF
    fi

    systemctl --user daemon-reload
}

start_unit() {
    systemctl --user start "$UNIT_NAME.service"
}

stop_unit() {
    echo ""
    echo "Shutting down..."
    systemctl --user stop "$UNIT_NAME.service" 2>/dev/null || true
}

cleanup() {
    stop_unit
    exit 0
}

trap cleanup SIGINT SIGTERM SIGPIPE EXIT

MODE="${1:-watch}"

echo "Starting NixOS Update Checker development mode..."
echo ""

# Install/update systemd unit
install_unit "$1"

# Stop any existing instance
systemctl --user stop "$UNIT_NAME.service" 2>/dev/null || true

# Start the service
if [ "$1" = "--no-watch" ]; then
    echo "Starting without auto-restart (use --no-watch to disable)"
else
    echo "Starting with cargo-watch (auto-restarts on file changes)"
    echo "Watching: src/, qml/, resources/, build.rs, Cargo.toml"
fi
echo ""
start_unit

# Wait a moment for service to start
sleep 2

echo "═══════════════════════════════════════════════════════"
echo "  NixOS Update Checker is running"
echo ""
echo "  Press Ctrl+C to stop"
echo "═══════════════════════════════════════════════════════"
echo ""

# Follow the logs until interrupted
journalctl --user -f -u "$UNIT_NAME.service"
