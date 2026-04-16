# EV Prototype - Complete Integration Summary

## Project Overview

Building a hybrid manual/autonomous electric vehicle prototype using:
- **Google Coral Dev Board Mini** - ML inference for perception
- **ESP32** - Real-time control hub
- **BY15WF01-A** - 48V/1800W BLDC motor controller
- **Innomaker U20CAM-1080P** - USB cameras (4 available)
- **LABWORK LP402216KS** - Manual rack-and-pinion steering
- **48V 10Ah Battery** - Main power source

---

## System Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                                                                             │
│   ┌─────────────┐         UART          ┌─────────────┐                    │
│   │             │◄────────────────────►│             │                    │
│   │    CORAL    │    (115200 baud)      │    ESP32    │                    │
│   │             │                       │             │                    │
│   │  • Camera   │                       │  • Throttle │──► Motor Controller│
│   │  • ML Model │                       │  • Steering │──► Steering Motor  │
│   │  • Decisions│                       │  • Brake    │──► Motor Controller│
│   │             │                       │             │                    │
│   └──────┬──────┘                       └─────────────┘                    │
│          │                                                                  │
│          │ USB                                                              │
│          ▼                                                                  │
│   ┌─────────────┐                                                          │
│   │   Camera    │                                                          │
│   │ U20CAM-1080P│                                                          │
│   └─────────────┘                                                          │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Hardware Details

### BY15WF01-A Motor Controller

**Key specs:**
- Voltage: 48V DC
- Max current: 33A
- Max power: 1800W
- Interface: Analog/Digital signals (NO CAN, NO UART)

**Wiring:**

| Function | Wire Color | Signal Type | ESP32 Connection |
|----------|-----------|-------------|------------------|
| Throttle +5V | Orange | Power | 5V rail (from controller) |
| Throttle GND | Black | Ground | Common GND |
| Throttle Signal | White | 0-5V analog | GPIO25 (DAC) via level shifter |
| Brake | Brown | Active LOW | GPIO32 |
| Reverse | Blue | Toggle to GND | GPIO33 (optional) |
| 3-Speed Select | Various | GND to select | Leave on preferred setting |

**Throttle behavior:**
- 0V = no throttle
- 5V = full throttle
- ESP32 DAC outputs 0-3.3V, so level shifter recommended for full range
- Without level shifter: ~66% max throttle (often acceptable for demo)

**3-Speed switch:**
- This is a *limiter*, not the speed control
- Low = caps at ~50% max speed
- Medium = caps at ~75%
- High = 100%
- The twist throttle (or ESP32 DAC) provides continuous 0-100% within that cap

### Twist Throttle (Original Manual Control)

**Type:** Hall-effect sensor, motorcycle grip style

**Wiring:**
- Red: +5V (from controller)
- Black: GND
- Green/White: Signal output (1V-4.2V proportional to twist)

**For autonomous control:** Disconnect green/white signal wire, connect ESP32 DAC output instead.

### Steering System - LABWORK LP402216KS

**Type:** Pure mechanical rack-and-pinion (no electric assist)

**Specs:**
- Steering wheel: 300mm (11.8") diameter
- Rack length: 320mm
- Tie rod range: 33-35" (840-890mm)
- Designed for: 110cc go-karts, ATVs

**Autonomous control approach:**
Motor attached to steering wheel shaft rotates the wheel programmatically.

```
┌─────────────────┐
│  Steering Motor │◄── ESP32 PWM (GPIO18, GPIO19)
│  (on wheel shaft)│
└────────┬────────┘
         │ rotates
         ▼
┌─────────────────┐
│ Steering Wheel  │
└────────┬────────┘
         │ mechanical linkage (column → rack → tie rods)
         ▼
    Front Wheels
```

**Control method:** Open-loop (timed pulses)
- Steer left: energize motor direction A
- Steer right: energize motor direction B
- Stop: de-energize both
- Calibrate duration experimentally

### Cameras - Innomaker U20CAM-1080P

**Specs:**
- Resolution: 1080p @ 30fps (or 720p, 480p)
- FOV: 130° diagonal, ~103° horizontal
- Interface: USB 2.0 UVC (standard, no drivers needed)
- Power: USB bus-powered

**Linux detection:** Shows up as `/dev/video0`, `/dev/video1`, etc.

**For prototype:** Use 1 front-facing camera. Additional cameras can be added later.

### Power System

```
48V 10Ah Battery (~480Wh)
         │
         ├────────────────────► BY15WF01-A (48V input)
         │                              │
         │                              ▼
         │                        BLDC Motor
         │
         ▼
┌─────────────────┐
│  AC-DC / Buck   │
│   Regulator     │
└────────┬────────┘
         │
         ▼ 5V output
         │
         ├──► Coral Dev Board Mini (USB-C power)
         ├──► ESP32 (VIN or USB)
         └──► USB Hub (for cameras)
```

**Check your regulator output voltage.** If it's 12V, you'll need an additional 5V buck converter.

---

## Communication Protocol

### Physical Connection (UART)

```
Coral Dev Board Mini          ESP32
─────────────────────         ─────
GPIO4 (TX)          ────────► GPIO16 (RX2)
GPIO5 (RX)          ◄──────── GPIO17 (TX2)
GND                 ────────► GND
```

**Settings:** 115200 baud, 8N1

### Message Format

Simple JSON, newline-terminated:

**Coral → ESP32 (commands):**
```json
{"t": 25, "s": -30, "b": false}
```
- `t` = throttle (0-100 percent)
- `s` = steering (-100 to +100, negative=left, positive=right)
- `b` = brake (true/false)

**Examples:**
```json
{"t": 0, "b": true}           // Emergency stop
{"t": 30, "s": 0}             // Forward, straight
{"t": 20, "s": 50}            // Forward, turning right
{"t": 0, "s": -100}           // Stationary, hard left
```

---

## Software

### ESP32 Firmware (Arduino)

**Location:** `esp32/main.cpp`

**Key functions:**
- `setThrottle(int pct)` - Sets motor speed 0-100%
- `setSteering(int value)` - Steers left (-100) to right (+100)
- `setBrake(bool on)` - Activates/releases brake

**Pin assignments:**
```cpp
#define THROTTLE_DAC 25    // DAC output for throttle
#define BRAKE_PIN 32       // Active LOW brake
#define STEER_A 18         // Steering motor PWM direction A
#define STEER_B 19         // Steering motor PWM direction B
#define CORAL_RX 16        // UART RX from Coral
#define CORAL_TX 17        // UART TX to Coral
```

**Testing without Coral:**
Connect ESP32 via USB, open Serial Monitor at 115200 baud, send:
```
{"t":20}
{"s":50}
{"t":0,"b":true}
```

### Coral Controller (Python)

**Location:** `coral/controller.py`

**Three operating modes:**

1. **Keyboard mode** (default) - Manual control for testing
   ```bash
   python3 controller.py keyboard
   ```
   - W/S = throttle up/down
   - A/D = steer left/right
   - Space = brake
   - C = center steering
   - Q = quit

2. **Simple mode** - Just drives straight slowly
   ```bash
   python3 controller.py simple
   ```

3. **Detect mode** - Basic obstacle detection
   ```bash
   python3 controller.py detect
   ```

**Serial port:** Default `/dev/ttyS1` on Coral. Override with:
```bash
python3 controller.py keyboard /dev/ttyUSB0
```

---

## Wiring Checklist

### Power
- [ ] 48V battery → BY15WF01-A power input
- [ ] 48V battery → Regulator input
- [ ] Regulator 5V output → Coral USB-C
- [ ] Regulator 5V output → ESP32 (VIN pin or USB)
- [ ] Common GND between all devices

### Motor Controller (BY15WF01-A)
- [ ] Motor phase wires (Yellow, Green, Blue) → BLDC motor
- [ ] Hall sensor wires → BLDC motor hall sensors
- [ ] ESP32 GPIO25 → [Level shifter] → White throttle wire
- [ ] ESP32 GPIO32 → Brown brake wire
- [ ] ESP32 GND → Black throttle GND

### Steering Motor
- [ ] ESP32 GPIO18 → Motor driver IN1
- [ ] ESP32 GPIO19 → Motor driver IN2
- [ ] Motor driver OUT → Steering motor
- [ ] Motor driver power → 12V (or appropriate voltage for your motor)
- [ ] Motor driver GND → ESP32 GND

### Coral ↔ ESP32
- [ ] Coral GPIO4 (TX) → ESP32 GPIO16 (RX)
- [ ] Coral GPIO5 (RX) ← ESP32 GPIO17 (TX)
- [ ] Coral GND → ESP32 GND

### Camera
- [ ] Camera USB → Coral USB port (directly or via hub)

---

## Testing Procedure

### Step 1: Power Verification
1. Measure regulator output - confirm 5V
2. Power Coral, verify boot (LED activity)
3. Power ESP32, verify boot (Serial output "Ready")

### Step 2: ESP32 Standalone Test
1. Connect ESP32 via USB to computer
2. Open Arduino Serial Monitor (115200 baud)
3. Send: `{"t":10}` - motor should spin slowly
4. Send: `{"t":0}` - motor should stop
5. Send: `{"s":50}` - steering motor should turn one direction
6. Send: `{"s":-50}` - steering motor should turn other direction
7. Send: `{"b":true}` - brake should engage

### Step 3: Coral Camera Test
```bash
# On Coral, test camera
fswebcam -r 640x480 test.jpg
# or
python3 -c "import cv2; cap=cv2.VideoCapture(0); print('OK' if cap.isOpened() else 'FAIL')"
```

### Step 4: UART Test
```bash
# On Coral, test serial
python3 -c "
import serial
ser = serial.Serial('/dev/ttyS1', 115200, timeout=1)
ser.write(b'{\"t\":10}\n')
print('Sent throttle command')
ser.close()
"
```

### Step 5: Integrated Test
```bash
# On Coral
python3 controller.py keyboard
# Use WASD to control vehicle
```

---

## Rapid Prototype Schedule (1 Week)

### Days 1-2: Hardware Assembly
- [ ] Mount all components on vehicle frame
- [ ] Complete power wiring
- [ ] Complete signal wiring (throttle, brake, steering)
- [ ] Verify ESP32 can control motors via USB serial

### Days 3-4: Communication
- [ ] Wire Coral ↔ ESP32 UART
- [ ] Test camera on Coral
- [ ] Test keyboard control mode end-to-end

### Days 5-6: Autonomy (if time permits)
- [ ] Implement basic obstacle detection
- [ ] Test autonomous driving in controlled area
- [ ] Tune parameters

### Day 7: Demo Prep
- [ ] Final testing
- [ ] Document known issues
- [ ] Prepare demo script/route

---

## Fallback Options

If full autonomy isn't ready:

1. **Keyboard/Remote Control Demo**
   - Shows full integration
   - Human controls via laptop/phone
   - Still impressive as proof of concept

2. **Scripted Demo**
   - Hardcoded sequence: forward 3s → turn → forward 2s → stop
   - Repeatable for demo

3. **Component Demo**
   - Show each subsystem working independently
   - Camera detecting objects (display only)
   - Manual throttle/steering test

---

## Troubleshooting

### Motor won't spin
- Check 48V battery charge
- Verify hall sensor connections
- Test throttle signal with multimeter (should vary 0-3.3V from ESP32)
- Try direct 5V to throttle signal wire (should go full speed)

### Steering motor wrong direction
- Swap GPIO18 and GPIO19 assignments, or
- Swap motor wires at driver

### No UART communication
- Verify TX→RX and RX→TX (crossed, not straight)
- Check baud rate matches (115200)
- Test with simple echo first

### Camera not detected
- Check `ls /dev/video*`
- Try different USB port
- Verify USB hub has power (if using hub)

### ESP32 not responding
- Check power (3.3V on 3V3 pin)
- Try different USB cable
- Re-flash firmware

---

## File Structure

```
/home/joshua/personal/ev-prototype/
├── README.md              # Quick start guide
├── SUMMARY.md             # This file - complete reference
├── esp32/
│   └── main.cpp           # ESP32 Arduino firmware
└── coral/
    └── controller.py      # Coral Python controller
```

---

## Parts You Have

| Item | Quantity | Status |
|------|----------|--------|
| Coral Dev Board Mini | 1 | Ready |
| ESP32 | 4 | Ready (using 1) |
| BY15WF01-A Controller | 1 | Ready |
| U20CAM-1080P Camera | 4 | Ready (using 1-2) |
| 48V 10Ah Battery | 1 | Ready |
| AC-DC Regulator | 1 | Check output voltage |
| Steering System | 1 | Ready |
| Twist Throttle | 1 | Ready (bypassing for auto) |
| Steering Motor | 1 | Ready |

## Parts You May Need

| Item | Purpose | Critical? |
|------|---------|-----------|
| 3.3V→5V Level Shifter | Full throttle range | Nice to have |
| Motor Driver (L298N/similar) | Steering motor control | Yes, unless motor is small enough for direct GPIO |
| USB Hub | Multiple cameras | Only if using >1 camera |
| 5V Buck Converter | If regulator outputs 12V | Depends on regulator |

---

## Quick Reference

### ESP32 Pinout (Used)
| GPIO | Function |
|------|----------|
| 25 | Throttle DAC output |
| 32 | Brake (active LOW) |
| 18 | Steering motor A |
| 19 | Steering motor B |
| 16 | UART RX (from Coral) |
| 17 | UART TX (to Coral) |

### Command Cheat Sheet
```json
{"t":0,"b":true}        // Stop + brake
{"t":20,"s":0}          // Slow forward, straight
{"t":30,"s":30}         // Forward, gentle right
{"t":0,"s":-80}         // Stationary, sharp left
{"t":50}                // 50% throttle, no steering change
```

### Coral Serial Port
- Built-in UART: `/dev/ttyS1`
- USB serial (if ESP32 connected via USB): `/dev/ttyUSB0` or `/dev/ttyACM0`
