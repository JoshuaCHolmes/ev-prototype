#!/usr/bin/env python3
"""
EV Prototype - PC Controller (runs on WSL)
Direct control without Coral - PC handles everything
"""
import serial
import json
import sys
import time

def send_command(ser, throttle=0, steering=0, brake=False):
    cmd = json.dumps({'t': throttle, 's': steering, 'b': brake})
    ser.write((cmd + '\n').encode())
    time.sleep(0.05)
    if ser.in_waiting:
        return ser.readline().decode().strip()
    return None

def keyboard_mode(ser):
    """WASD control - requires terminal input"""
    print("=== KEYBOARD MODE ===")
    print("W/S = throttle, A/D = steering, SPACE = brake, Q = quit")
    print("Press keys + Enter (or use 'stty -icanon' for instant input)")
    
    throttle, steering = 0, 0
    while True:
        try:
            key = input("> ").lower().strip()
            if key == 'q':
                send_command(ser, 0, 0, True)
                break
            elif key == 'w':
                throttle = min(100, throttle + 20)
            elif key == 's':
                throttle = max(0, throttle - 20)
            elif key == 'a':
                steering = max(-100, steering - 25)
            elif key == 'd':
                steering = min(100, steering + 25)
            elif key == ' ' or key == 'b':
                throttle = 0
                send_command(ser, 0, 0, True)
                print("BRAKE!")
                continue
            elif key == 'x':
                throttle, steering = 0, 0
            
            resp = send_command(ser, throttle, steering, False)
            print(f"Throttle: {throttle:3d}% | Steering: {steering:+4d} | {resp}")
        except KeyboardInterrupt:
            send_command(ser, 0, 0, True)
            break

def demo_mode(ser):
    """Simple demo sequence"""
    print("=== DEMO MODE ===")
    print("Running test sequence...")
    
    steps = [
        (0, 0, False, "Idle"),
        (20, 0, False, "Forward slow"),
        (40, 0, False, "Forward medium"),
        (40, 50, False, "Turn right"),
        (40, -50, False, "Turn left"),
        (40, 0, False, "Straight"),
        (0, 0, True, "BRAKE"),
        (0, 0, False, "Done"),
    ]
    
    for throttle, steering, brake, desc in steps:
        print(f"{desc}: T={throttle}, S={steering}, B={brake}")
        resp = send_command(ser, throttle, steering, brake)
        print(f"  Response: {resp}")
        time.sleep(2)
    
    print("Demo complete!")

def main():
    port = sys.argv[1] if len(sys.argv) > 1 else '/dev/ttyUSB0'
    mode = sys.argv[2] if len(sys.argv) > 2 else 'keyboard'
    
    print(f"Connecting to ESP32 on {port}...")
    try:
        ser = serial.Serial(port, 115200, timeout=1)
        ser.reset_input_buffer()
        time.sleep(0.5)
        
        # Test connection
        resp = send_command(ser, 0, 0, False)
        print(f"Connected! ESP32 says: {resp}")
        
        if mode == 'demo':
            demo_mode(ser)
        else:
            keyboard_mode(ser)
        
        ser.close()
    except serial.SerialException as e:
        print(f"Error: {e}")
        print("Try: sudo chmod 666 /dev/ttyUSB0")
        sys.exit(1)

if __name__ == '__main__':
    print("Usage: python3 pc_controller.py [port] [mode]")
    print("  port: /dev/ttyUSB0 (default)")
    print("  mode: keyboard (default), demo")
    print()
    main()
