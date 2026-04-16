#!/bin/bash
# EV Prototype Controller Launcher (WSL/Linux)
# Texas A&M FLiNT - Team Autopilot

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# Set permissions for USB devices
sudo chmod 666 /dev/ttyUSB* 2>/dev/null
sudo chmod 666 /dev/video* 2>/dev/null

MODE="${1:-tui}"

case "$MODE" in
    tui)
        echo "Starting TUI Controller..."
        nix-shell -p python3Packages.textual python3Packages.pyserial python3Packages.opencv4 python3Packages.requests python3Packages.pillow --run "python3 tui_controller.py"
        ;;
    *)
        echo "Usage: ./run.sh [tui]"
        ;;
esac
