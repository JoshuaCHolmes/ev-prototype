# EV Prototype Control Center

Self-driving EV prototype control system - Texas A&M FLiNT - Team Autopilot.

## Quick Start

### Option 1: Windows (Standalone Executable)
1. Download `ev-control.exe` from Releases
2. Connect ESP32 via USB
3. Run `ev-control.exe`

### Option 2: WSL/Linux
1. Run `scripts/attach-usb-wsl.bat` (as Admin) to attach USB devices to WSL
2. In WSL:
   ```bash
   cd wsl
   ./run.sh tui
   ```

## Controls

| Key | Action |
|-----|--------|
| **W** | Accelerate (hold) |
| **S** | Brake / Reverse (hold) |
| **A** | Steer Left (hold) |
| **D** | Steer Right (hold) |
| **SPACE** | Emergency Brake |
| **Q** | Quit |

**Note:** Steering does NOT auto-center (manual steering motor).

## Hardware

- **ESP32** (30-pin, CP2102 USB-UART)
- **BY15WF01-A** Motor Controller
- **Small DC Motor** for steering (requires H-bridge driver)
- **Innomaker U20CAM-1080P** cameras (optional)

See [BREADBOARD.md](BREADBOARD.md) for wiring instructions.

## Project Structure

```
ev-prototype/
├── rust-controller/     # Standalone Rust executable (Windows/Linux)
│   ├── src/main.rs
│   └── Cargo.toml
├── wsl/                 # WSL/Linux TUI controller (Python)
│   ├── tui_controller.py
│   └── run.sh
├── esp32/               # ESP32 firmware (Arduino)
│   └── ev_controller/
├── scripts/             # Utility scripts
│   └── attach-usb-wsl.bat
├── BREADBOARD.md        # Wiring diagram
└── README.md
```

## Building from Source

### Rust Controller (Windows/Linux)
```bash
cd rust-controller
cargo build --release
# Binary at target/release/ev-control(.exe)
```

### ESP32 Firmware
```bash
# Using arduino-cli
cd esp32/ev_controller
arduino-cli compile --fqbn esp32:esp32:esp32
arduino-cli upload --fqbn esp32:esp32:esp32 -p /dev/ttyUSB0
```

## Releases

Each release includes:
- `ev-control.exe` - Windows executable
- `ev-control` - Linux executable  
- `attach-usb-wsl.bat` - USB attach script for WSL users
- `wsl/` - Python TUI controller for WSL/Linux

## License

MIT License - Texas A&M University FLiNT - Team Autopilot
