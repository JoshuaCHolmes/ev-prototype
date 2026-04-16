# DEPLOYMENT GUIDE - Crunch Time

## Step 1: Flash ESP32 (5 min)

### Option A: Arduino IDE
1. Open Arduino IDE
2. Install ESP32 board: File → Preferences → Board Manager URLs, add:
   ```
   https://raw.githubusercontent.com/espressif/arduino-esp32/gh-pages/package_esp32_index.json
   ```
3. Tools → Board → ESP32 Dev Module
4. Install library: Sketch → Include Library → Manage Libraries → "ArduinoJson"
5. Open `esp32/main.cpp`, copy contents into new sketch
6. Connect ESP32 via USB-C, select port
7. Upload

### Option B: PlatformIO (if installed)
```bash
cd /home/joshua/personal/ev-prototype/esp32
# Create platformio.ini if needed:
cat > platformio.ini << 'PIOEOF'
[env:esp32]
platform = espressif32
board = esp32dev
framework = arduino
lib_deps = bblanchon/ArduinoJson@^6.21.0
monitor_speed = 115200
PIOEOF

pio run -t upload
pio device monitor
```

### Test ESP32 (before connecting to vehicle!)
1. Open Serial Monitor (115200 baud)
2. Send these test commands:
```
{"t":10}
{"s":50}
{"s":-50}
{"b":true}
{"t":0,"s":0,"b":false}
```
You should see responses like `T:10`, `S:50`, `B:ON`

---

## Step 2: Setup Coral (10 min)

### Copy files to Coral
```bash
# From your computer, SCP to Coral (adjust IP/hostname)
scp /home/joshua/personal/ev-prototype/coral/controller.py mendel@coral-ip:~/

# Or if Coral has internet:
# SSH into Coral and wget/curl the file
```

### On Coral, install dependencies
```bash
ssh mendel@coral-ip

# Install Python serial library
pip3 install pyserial

# OpenCV might already be there, if not:
pip3 install opencv-python-headless
```

### Connect ESP32 to Coral
1. Plug USB-C cable from ESP32 into Coral **OTG port**
2. Find the device:
```bash
ls /dev/ttyACM* /dev/ttyUSB*
# Usually /dev/ttyACM0
```

### Test communication
```bash
# Quick test - send a command manually
echo '{"t":0,"b":true}' > /dev/ttyACM0

# Or use screen/minicom
screen /dev/ttyACM0 115200
# Type: {"t":10}  then Enter
# Ctrl-A k to exit screen
```

---

## Step 3: Run Controller (2 min)

### Keyboard mode (manual control for testing)
```bash
python3 controller.py keyboard /dev/ttyACM0
```
Controls:
- `w/s` = throttle up/down
- `a/d` = steer left/right  
- `space` = emergency brake
- `c` = center steering
- `q` = quit

### Simple mode (drives straight, no camera)
```bash
python3 controller.py simple /dev/ttyACM0
```

### Detection mode (uses camera, stops for obstacles)
```bash
python3 controller.py detect /dev/ttyACM0
```

---

## Quick Troubleshooting

| Problem | Fix |
|---------|-----|
| `/dev/ttyACM0` not found | Try `/dev/ttyUSB0`, or replug USB cable |
| Permission denied | `sudo chmod 666 /dev/ttyACM0` or add user to dialout group |
| No response from ESP32 | Check Serial Monitor on computer first, verify upload worked |
| ESP32 keeps resetting | Power issue - try powered USB hub |
| Camera not found | `ls /dev/video*`, try `/dev/video0` or `/dev/video1` |

---

## Wiring Checklist

Before powering on:
- [ ] ESP32 GND (row 2) → (-) rail → all other GNDs
- [ ] D25 (row 8) → motor controller BROWN (throttle)
- [ ] D32 (row 10) → motor controller PURPLE (brake)
- [ ] D26 (row 7) → motor driver IN1
- [ ] D27 (row 6) → motor driver IN2
- [ ] 48V battery → controller thick RED/BLACK wires
- [ ] USB-C: ESP32 ↔ Coral OTG port

---

## Demo Day Sequence

1. **Power on Coral** (5V supply to data port)
2. **Wait 30 sec** for Coral to boot
3. **Verify ESP32 detected:** `ls /dev/ttyACM*`
4. **Start keyboard mode:** `python3 controller.py keyboard`
5. **Test controls** before enabling motor power:
   - Press `w` - should see "T:10" (throttle won't move without 48V)
   - Press `a/d` - steering motor should move (if driver powered)
6. **Connect 48V battery** (motor controller)
7. **Slowly test throttle** with `w` key
8. **E-STOP:** Press `space` or `Ctrl+C`

Good luck! 🚗
