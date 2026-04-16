# ESP32 Breadboard Wiring Diagram (VERIFIED)

## Your Hardware

- **ESP32:** USB-C facing LEFT, pins in columns a (bottom row) and i (top row)
- **Breadboard:** `+ - | a b c d e | f g h i j | + -`
- **Power:** ESP32 powered via USB-C from PC
- **Motor Controller:** VEVOR BY15WF01-A
- **Steering Motor:** Small DC motor (~1-1.5" diameter) with RED and BLACK wires
- **Steering Control:** Manual centering (motor runs while key held, stops when released)

ESP32 body covers columns a through i. **Only column j is accessible for wiring** (adjacent to the top row pins D25, D26, D27, D32 that we need).

⚠️ **NEED:** Motor driver board (L298N, TB6612, or similar) for steering motor

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
     Wire #2      Wire #3
           │           │
    Throttle       Brake
    BLACK wire    BLACK wire
    (from motor   (from motor
    controller)   controller)


STEP 3: Connect motor driver GND (when you get one)
───────────────────────────────────────────────────

    ═══════════════════════  ← Breadboard (-) rail
                       ▲
                       │
                  Wire #4
                       │
                Motor driver
                  GND pin


FINAL RESULT - All grounds connected:
─────────────────────────────────────

                    BREADBOARD (-) POWER RAIL
    ════════════════════════════════════════════════
         ▲              ▲              ▲         ▲
         │              │              │         │
      Wire #1        Wire #2        Wire #3   Wire #4
         │              │              │         │
      ESP32          Throttle       Brake     Motor
       GND           BLACK          BLACK     Driver
     (row 2)         wire           wire       GND


WHY THIS WORKS:
───────────────
• All devices now share the same 0V reference
• When ESP32 outputs 3.3V on D25, the motor controller 
  measures 3.3V between BROWN wire and its BLACK wire
• Same for brake signal on D32/PURPLE
• The motor driver also shares ground so steering signals work
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
    6  │ │RX2 │▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒║▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒│D27 │ │  ●5   │ ← STEER B (to driver IN2)
    7  │ │TX2 │▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒║▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒│D26 │ │  ●4   │ ← STEER A (to driver IN1)
    8  │ │D5  │▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒║▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒│D25 │ │  ●2   │ ← THROTTLE (to BROWN)
    9  │ │D18 │▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒║▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒│D33 │ │  ○    │
   10  │ │D19 │▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒║▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒│D32 │ │  ●3   │ ← BRAKE (to PURPLE)
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
●3 = Wire 3: D32 → Motor controller PURPLE wire
●4 = Wire 4: D26 → Motor driver IN1
●5 = Wire 5: D27 → Motor driver IN2
○  = Unused  
▒  = BLOCKED by ESP32 body
```

---

## Complete Wire List

| Wire # | From (ESP32) | Row | Col | To (Destination) | Wire Color Suggestion |
|--------|--------------|-----|-----|------------------|----------------------|
| 1 | **GND** | 2 | a | Breadboard (-) rail | Black |
| 2 | **D25** | 8 | a | Motor controller **BROWN** | Brown or Orange |
| 3 | **D32** | 10 | a | Motor controller **PURPLE** | Purple or Blue |
| 4 | **D26** | 7 | a | Motor driver **IN1** | Yellow |
| 5 | **D27** | 6 | a | Motor driver **IN2** | Green |

**From (-) rail to devices (ground wires):**

| Wire # | From | To | Notes |
|--------|------|----|-------|
| 6 | (-) rail | Throttle connector BLACK wire | Share ground with motor controller |
| 7 | (-) rail | Brake connector BLACK wire | Share ground with motor controller |
| 8 | (-) rail | Motor driver GND | When you get a driver board |

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
│  BLACK ── GND ◄── Wire #6 from (-) rail
│  BROWN ── Signal ◄── Wire #2 from D25 (row 8)
└─────────────────────────────────┘

BRAKE CONNECTOR (2 thin wires):  
┌─────────────────────────────────┐
│  PURPLE ── Signal ◄── Wire #3 from D32 (row 10)
│  BLACK ─── GND ◄── Wire #7 from (-) rail
└─────────────────────────────────┘
```

---

## Steering Motor Driver (NEED TO FIND)

You need an H-bridge motor driver. Look for these at the makerspace:

```
COMMON MOTOR DRIVERS:
─────────────────────

L298N (most common)          TB6612 (smaller)         L293D (chip or board)
┌──────────────────┐        ┌──────────────┐         ┌──────────────┐
│  [HEATSINK]      │        │  Small board │         │  DIP chip or │
│                  │        │  ~1" x 1"    │         │  small board │
│  Red PCB         │        │              │         │              │
│  ~2" x 2"        │        │              │         │              │
└──────────────────┘        └──────────────┘         └──────────────┘

Any of these will work!

WIRING THE MOTOR DRIVER:
────────────────────────

┌─────────────────────────────────────────────────────┐
│                   MOTOR DRIVER                      │
│                                                     │
│   CONTROL SIDE:              MOTOR SIDE:            │
│   ─────────────              ───────────            │
│   IN1 ◄── D26 (row 7)        OUT1 ──► Steering motor RED
│   IN2 ◄── D27 (row 6)        OUT2 ──► Steering motor BLACK
│   GND ◄── (-) rail                                  │
│                                                     │
│   POWER SIDE:                                       │
│   ────────────                                      │
│   VCC/12V ◄── 12V power supply (or 5V-24V depending │
│   GND ◄────── (-) rail           on your motor)     │
│                                                     │
└─────────────────────────────────────────────────────┘

NOTE: The steering motor needs its own power supply!
      ESP32 can't power it. The motor driver just
      switches that power on/off based on ESP32 signals.
```

---

## Steering Behavior

**Current implementation:** Manual centering
- Hold A → Motor spins left
- Hold D → Motor spins right  
- Release → Motor stops, wheel stays where it is

The steering doesn't auto-center because we have no position sensor.
This is fine for the prototype - just manually center before stopping.

---

## Summary Checklist

**Before powering on:**

- [ ] ESP32 GND (row 2, col j) → (-) rail
- [ ] D25 (row 8, col j) → Motor controller BROWN wire
- [ ] D32 (row 10, col j) → Motor controller PURPLE wire
- [ ] (-) rail → Throttle connector BLACK wire
- [ ] (-) rail → Brake connector BLACK wire
- [ ] Motor driver found and connected (IN1, IN2, GND, power)
- [ ] Steering motor connected to motor driver outputs
- [ ] ESP32 connected to PC via USB-C
- [ ] 48V battery connected to motor controller (RED=B+, BLACK=B-)

**Testing order:**

1. Connect ESP32 to PC only (no motor controller power)
2. Run TUI, verify ESP32 responds
3. Connect motor controller power (48V)
4. Test throttle carefully (low values first!)
5. Test brake
6. Test steering (once motor driver is connected)
