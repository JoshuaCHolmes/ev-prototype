# ESP32 Breadboard Wiring Diagram (VERIFIED)

## Your Hardware

- **ESP32:** USB-C facing LEFT, pins in columns a (bottom row) and i (top row)
- **Breadboard:** `+ - | a b c d e | f g h i j | + -`
- **Power:** ESP32 powered via USB-C from PC
- **Motor Controller:** VEVOR BY15WF01-A (48V system)
- **Steering Motor:** AndyMark AM-3637 NeveRest 20 (12V DC with encoder)
- **Steering Driver:** L298N H-Bridge module
- **Steering Control:** Manual centering (motor runs while key held, stops when released)

ESP32 body covers columns a through i. **Only column j is accessible for wiring** (adjacent to the top row pins D25, D26, D27, D32, D33 that we need).

---

## Understanding Ground (GND)

**Why do we need a common ground?**

All electronic devices need a reference point for voltage - that's "ground" (0V). 
When the ESP32 sends a signal (e.g., 3.3V on D25), the motor controller needs to 
measure that voltage *relative to something*. If they don't share a common ground,
they can't "agree" on what 0V means, and signals won't work.

**Think of it like this:** Ground is like sea level. Everyone needs to measure 
height from the same reference point, or the numbers are meaningless.

```
GROUND WIRING - STEP BY STEP
════════════════════════════════════════════════════════════════════════

Your breadboard has two "power rails" on each side - long rows marked + and -
The (-) rail is your GROUND BUS - a shared connection point.

STEP 1: Connect ESP32 GND to the (-) rail
─────────────────────────────────────────
    
    ESP32 GND pin (Row 2, column j)
           │
           │  ← Wire #1: Short jumper wire
           ▼
    ═══════════════════════  ← Breadboard (-) rail
    
    This connects the ESP32's "zero point" to the rail.


STEP 2: Connect motor controller GNDs to the same (-) rail  
──────────────────────────────────────────────────────────

    The motor controller has TWO black ground wires:
    - One in the THROTTLE connector (3-wire: red, black, brown)
    - One in the BRAKE connector (2-wire: purple, black)
    
    ═══════════════════════  ← Breadboard (-) rail
           ▲           ▲
           │           │
     Wire #6      Wire #7
           │           │
    Throttle       Brake
    BLACK wire    BLACK wire
    (from motor   (from motor
    controller)   controller)


STEP 3: Connect L298N GND
─────────────────────────

    ═══════════════════════  ← Breadboard (-) rail
                       ▲
                       │
                  Wire #8
                       │
                L298N GND pin


FINAL RESULT - All grounds connected:
─────────────────────────────────────

                    BREADBOARD (-) POWER RAIL
    ════════════════════════════════════════════════
         ▲              ▲              ▲         ▲
         │              │              │         │
      Wire #1        Wire #6        Wire #7   Wire #8
         │              │              │         │
      ESP32          Throttle       Brake     L298N
       GND           BLACK          BLACK       GND
     (row 2)         wire           wire


WHY THIS WORKS:
───────────────
• All devices now share the same 0V reference
• When ESP32 outputs 3.3V on D25, the motor controller 
  measures 3.3V between BROWN wire and its BLACK wire
• Same for brake signal on D32/PURPLE
• The L298N also shares ground so steering signals work
```

---

## Visual Breadboard Layout

```
BREADBOARD TOP VIEW
═══════════════════════════════════════════════════════════════════════

                              USB-C ◄── to PC (power + data)
                             ◄──────
                                                              ACTIVE
                         ESP32 BODY (covers a through i)       SIDE
                                                                │
         │ a     b  c  d  e     ║     f  g  h     i │          ▼
       ┌─┼────┬─────────────────╫─────────────────┬─┼────┬────────┐
    1  │ │3V3 │▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒║▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒│VN  │ │  ○    │
    2  │ │GND │▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒║▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒│GND │ │  ●1   │ ← GROUND to (-) rail
    3  │ │D15 │▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒║▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒│D13 │ │  ○    │
    4  │ │D2  │▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒║▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒│D12 │ │  ○    │
    5  │ │D4  │▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒║▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒│D14 │ │  ○    │
    6  │ │RX2 │▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒║▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒│D27 │ │  ●4   │ ← L298N IN2
    7  │ │TX2 │▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒║▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒│D26 │ │  ●3   │ ← L298N IN1
    8  │ │D5  │▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒║▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒│D25 │ │  ●2   │ ← THROTTLE (to BROWN)
    9  │ │D18 │▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒║▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒│D33 │ │  ●5   │ ← L298N ENA (PWM speed)
   10  │ │D19 │▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒║▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒│D32 │ │  ●6   │ ← BRAKE (to PURPLE)
   11  │ │RX0 │▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒║▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒│D35 │ │  ○    │
   12  │ │TX0 │▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒║▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒│D34 │ │  ○    │
   13  │ │D22 │▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒║▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒│VN  │ │  ○    │
   14  │ │D23 │▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒║▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒│VP  │ │  ○    │
   15  │ │ -  │▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒║▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒│EN  │ │  ○    │
       └─┼────┴─────────────────╫─────────────────┴─┼────┴────────┘
                                ║                      j
         └──────────────────────╨──────────────────┘
                         BLOCKED (a-i)

●1 = Wire 1: GND → (-) rail
●2 = Wire 2: D25 → Motor controller BROWN wire  
●3 = Wire 3: D26 → L298N IN1
●4 = Wire 4: D27 → L298N IN2
●5 = Wire 5: D33 → L298N ENA (PWM for speed control)
●6 = Wire 6: D32 → Motor controller PURPLE wire
○  = Unused  
▒  = BLOCKED by ESP32 body
```

---

## Complete Wire List

### ESP32 to Devices

| Wire # | From (ESP32) | Row | Col | To (Destination) | Wire Color Suggestion |
|--------|--------------|-----|-----|------------------|----------------------|
| 1 | **GND** | 2 | j | Breadboard (-) rail | Black |
| 2 | **D25** | 8 | j | Motor controller **BROWN** | Brown or Orange |
| 3 | **D26** | 7 | j | L298N **IN1** | Yellow |
| 4 | **D27** | 6 | j | L298N **IN2** | Green |
| 5 | **D33** | 9 | j | L298N **ENA** | Blue |
| 6 | **D32** | 10 | j | Motor controller **PURPLE** | Purple |

### Ground Connections (all to (-) rail)

| Wire # | From | To | Notes |
|--------|------|----|-------|
| 7 | (-) rail | Throttle connector BLACK wire | Motor controller GND |
| 8 | (-) rail | Brake connector BLACK wire | Motor controller GND |
| 9 | (-) rail | L298N **GND** | Steering driver GND |

---

## L298N Wiring Diagram (AndyMark AM-3637 NeveRest 20)

```
L298N MODULE PINOUT
═══════════════════════════════════════════════════════════════════

      ┌─────────────────────────────────────────────────┐
      │                                                 │
      │    ┌─────────────────────────────────────┐      │
      │    │         [HEAT SINK]                 │      │
      │    └─────────────────────────────────────┘      │
      │                                                 │
      │   MOTOR A           POWER           MOTOR B     │
      │  OUT1  OUT2      +12V  GND        OUT3  OUT4    │
      │   │     │         │     │          │     │      │
      └───┼─────┼─────────┼─────┼──────────┼─────┼──────┘
          │     │         │     │          │     │
          │     │         │     │          (unused)
          ▼     ▼         ▼     ▼
        NeveRest       12V    (-) rail
        RED  BLACK    supply  (Wire #9)
        wire wire


        ┌─────────────────────────────────────────────────┐
        │                                                 │
        │    ENA   IN1   IN2   IN3   IN4   ENB   +5V     │
        │     │     │     │     │     │     │     │      │
        └─────┼─────┼─────┼─────┼─────┼─────┼─────┼──────┘
              │     │     │     │     │     │     │
              │     │     │   (unused - Motor B)  │
              │     │     │                       │
              ▼     ▼     ▼                       ▼
            D33   D26   D27                   (leave open or
          Wire#5 Wire#3 Wire#4                 jumper to ENA)


SIGNAL TRUTH TABLE:
───────────────────
  ENA   IN1   IN2   │  MOTOR ACTION
══════════════════════════════════════
  LOW   X     X     │  Motor OFF (coasts)
  PWM   HIGH  LOW   │  Turn RIGHT at PWM speed
  PWM   LOW   HIGH  │  Turn LEFT at PWM speed
  PWM   HIGH  HIGH  │  Brake (motor stops fast)
  PWM   LOW   LOW   │  Motor OFF (coasts)
```

---

## Motor Controller Connections (BY15WF01-A)

```
MOTOR CONTROLLER - WHAT TO CONNECT
══════════════════════════════════

DON'T TOUCH (already set up):
─────────────────────────────
• Thick RED wire (B+) ← 48V battery positive
• Thick BLACK wire (B-) ← 48V battery negative  
• Yellow/Green/Blue phase wires ← Connected to motor
• 3-Speed connector (blue K1, black, yellow K2) ← Physical throttle grip

ACTIVE CONNECTIONS (from ESP32):
────────────────────────────────

THROTTLE CONNECTOR (3 thin wires):
┌─────────────────────────────────┐
│  RED ──── +4.3V (leave alone)   │
│  BLACK ── GND ◄── Wire #7 from (-) rail
│  BROWN ── Signal ◄── Wire #2 from D25 (row 8)
└─────────────────────────────────┘

BRAKE CONNECTOR (2 thin wires):  
┌─────────────────────────────────┐
│  PURPLE ── Signal ◄── Wire #6 from D32 (row 10)
│  BLACK ─── GND ◄── Wire #8 from (-) rail
└─────────────────────────────────┘
```

---

## Power Requirements

### 48V System (Main Drive Motor)
- **Source:** 48V 10000mAh battery
- **Connection:** Already connected to BY15WF01-A motor controller

### 12V System (Steering Motor - AM-3637 NeveRest 20)
- **Required:** 12V DC, ~3A capability (2.7A stall)
- **Options:**
  1. **DC-DC Buck Converter** (48V → 12V) - Most elegant
  2. **Separate 12V battery** (e.g., 3S LiPo = 11.1V)
  3. **12V power supply** if testing on bench

⚠️ **DO NOT** connect the NeveRest motor directly to ESP32 - it will damage the ESP32!

---

## Steering Behavior

**Current implementation:** Manual centering with timeout safety
- Hold A → Motor spins left
- Hold D → Motor spins right  
- Release → Motor stops, wheel stays where it is
- If no commands for 200ms → Motor automatically stops (safety feature)

The steering doesn't auto-center because we have no position sensor.
This is fine for the prototype - just manually center before stopping.

**FSD Mode:** The system calculates bearing to next waypoint and sends steering commands automatically. Manual input (A/D keys) always overrides FSD.

---

## Summary Checklist

**Before powering on:**

- [ ] ESP32 GND (row 2, col j) → (-) rail
- [ ] D25 (row 8, col j) → Motor controller BROWN wire
- [ ] D26 (row 7, col j) → L298N IN1
- [ ] D27 (row 6, col j) → L298N IN2
- [ ] D33 (row 9, col j) → L298N ENA
- [ ] D32 (row 10, col j) → Motor controller PURPLE wire
- [ ] (-) rail → Throttle connector BLACK wire
- [ ] (-) rail → Brake connector BLACK wire
- [ ] (-) rail → L298N GND
- [ ] L298N +12V → 12V power source (NOT 48V!)
- [ ] L298N OUT1 → NeveRest RED wire
- [ ] L298N OUT2 → NeveRest BLACK wire
- [ ] ESP32 connected to PC via USB-C
- [ ] 48V battery connected to motor controller (RED=B+, BLACK=B-)

**Testing order:**

1. Connect ESP32 to PC only (no motor controller power, no L298N power)
2. Run GUI, verify ESP32 responds
3. Connect 12V to L298N, test steering with A/D keys
4. Connect 48V to motor controller
5. Test throttle carefully (low values first!)
6. Test brake
7. Test FSD mode (SIM mode first, then GPS)
