#!/usr/bin/env python3
"""
EV Prototype - Coral Minimal Controller
RAPID PROTOTYPE - 1 week deadline

Simple: camera → detect stuff → send commands to ESP32
"""

import serial
import json
import time

try:
    import cv2
    HAS_CV2 = True
except ImportError:
    print("Warning: OpenCV not installed. Run: pip install opencv-python")
    HAS_CV2 = False

# Try to import Coral libraries
try:
    from pycoral.adapters import common, detect
    from pycoral.utils.edgetpu import make_interpreter
    HAS_CORAL = True
except ImportError:
    print("Warning: PyCoral not installed. Running without ML.")
    HAS_CORAL = False


class SimpleEVController:
    def __init__(self, serial_port='/dev/ttyS1', baud=115200):
        self.ser = serial.Serial(serial_port, baud, timeout=0.1)
        self.cap = None
        self.interpreter = None
        
        if HAS_CV2:
            self.cap = cv2.VideoCapture(0)
            self.cap.set(cv2.CAP_PROP_FRAME_WIDTH, 320)
            self.cap.set(cv2.CAP_PROP_FRAME_HEIGHT, 240)
            print(f"Camera: {'OK' if self.cap.isOpened() else 'FAILED'}")
    
    def send(self, throttle=0, steer=0, brake=False):
        """Send command to ESP32"""
        cmd = {"t": int(throttle), "s": int(steer), "b": brake}
        msg = json.dumps(cmd) + "\n"
        self.ser.write(msg.encode())
        print(f"Sent: {cmd}")
    
    def stop(self):
        """Emergency stop"""
        self.send(throttle=0, steer=0, brake=True)
    
    def run_simple(self):
        """Super simple: just go slow and straight"""
        print("Running SIMPLE mode (no detection)")
        print("Press Ctrl+C to stop")
        
        try:
            while True:
                self.send(throttle=15, steer=0)
                time.sleep(0.2)
        except KeyboardInterrupt:
            self.stop()
    
    def run_basic_detection(self):
        """Basic detection: stop if something is in the way"""
        if not HAS_CV2:
            print("No OpenCV - falling back to simple mode")
            return self.run_simple()
        
        print("Running BASIC DETECTION mode")
        print("Press Ctrl+C to stop")
        
        try:
            while True:
                ret, frame = self.cap.read()
                if not ret:
                    continue
                
                # VERY SIMPLE: check if center of image is "blocked"
                # (dark area = possible obstacle)
                h, w = frame.shape[:2]
                center = frame[h//3:2*h//3, w//3:2*w//3]
                brightness = center.mean()
                
                if brightness < 50:  # Dark = something close
                    print("Obstacle? Stopping...")
                    self.send(throttle=0, steer=0, brake=True)
                else:
                    self.send(throttle=20, steer=0)
                
                time.sleep(0.1)
                
        except KeyboardInterrupt:
            self.stop()
    
    def run_keyboard(self):
        """Manual keyboard control for testing"""
        print("KEYBOARD CONTROL MODE")
        print("  w/s = throttle up/down")
        print("  a/d = steer left/right")
        print("  space = brake")
        print("  q = quit")
        
        throttle = 0
        steer = 0
        
        import sys, tty, termios
        fd = sys.stdin.fileno()
        old = termios.tcgetattr(fd)
        
        try:
            tty.setraw(fd)
            while True:
                ch = sys.stdin.read(1)
                
                if ch == 'w':
                    throttle = min(100, throttle + 10)
                elif ch == 's':
                    throttle = max(0, throttle - 10)
                elif ch == 'a':
                    steer = max(-100, steer - 20)
                elif ch == 'd':
                    steer = min(100, steer + 20)
                elif ch == ' ':
                    throttle = 0
                    self.send(throttle=0, steer=0, brake=True)
                    continue
                elif ch == 'c':  # center steering
                    steer = 0
                elif ch == 'q':
                    break
                
                self.send(throttle=throttle, steer=steer)
                
        finally:
            termios.tcsetattr(fd, termios.TCSADRAIN, old)
            self.stop()
    
    def cleanup(self):
        self.stop()
        if self.cap:
            self.cap.release()
        self.ser.close()


if __name__ == '__main__':
    import sys
    
    # Detect serial port
    port = '/dev/ttyACM0'  # Default: ESP32 via USB-C to Coral OTG port
    if len(sys.argv) > 1:
        if sys.argv[1] == '--help':
            print("Usage: python3 controller.py [mode] [port]")
            print("Modes: simple, detect, keyboard")
            print("Default port: /dev/ttyACM0 (ESP32 via USB)")
            print("Alt ports: /dev/ttyUSB0, /dev/ttyS1 (UART)")
            sys.exit(0)
        if sys.argv[-1].startswith('/dev'):
            port = sys.argv[-1]
    
    ctrl = SimpleEVController(serial_port=port)
    
    mode = sys.argv[1] if len(sys.argv) > 1 else 'keyboard'
    
    try:
        if mode == 'simple':
            ctrl.run_simple()
        elif mode == 'detect':
            ctrl.run_basic_detection()
        else:
            ctrl.run_keyboard()
    finally:
        ctrl.cleanup()
        print("\nStopped.")
