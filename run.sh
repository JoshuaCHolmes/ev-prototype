#!/usr/bin/env bash
set -e
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MODE="${1:-tui}"

echo "╔═══════════════════════════════════════╗"
echo "║     EV PROTOTYPE CONTROL SYSTEM       ║"
echo "╚═══════════════════════════════════════╝"
echo ""

# Fix permissions
for dev in /dev/ttyUSB* /dev/video*; do
    [ -e "$dev" ] && sudo chmod 666 "$dev" 2>/dev/null
done

# Check devices
echo "[Check] Devices:"
[ -e /dev/ttyUSB0 ] && echo "  ✓ ESP32" || echo "  ✗ ESP32"
[ -e /dev/video0 ] && echo "  ✓ Camera 0" || echo "  ✗ Camera 0"
[ -e /dev/video2 ] && echo "  ✓ Camera 2" || echo "  ✗ Camera 2"
echo ""

DEPS="python3Packages.pyserial python3Packages.opencv4"

case "$MODE" in
    tui)
        echo "[Mode] TUI Control Center v3"
        DEPS="$DEPS python3Packages.textual python3Packages.requests python3Packages.pillow"
        SCRIPT="tui_controller.py"
        ;;
    *)
        echo "[Mode] $MODE"
        SCRIPT="ev_controller.py $MODE"
        ;;
esac

echo ""
exec nix-shell -p $DEPS --run "python3 '$SCRIPT_DIR/$SCRIPT'"
