#!/usr/bin/env python3
"""
EV Prototype - Full Controller with Camera Support
Runs on PC (WSL), controls ESP32, processes camera feeds
"""
import serial
import json
import sys
import time
import threading
import signal

# Optional imports
try:
    import cv2
    HAS_CV2 = True
except ImportError:
    HAS_CV2 = False
    print("Warning: OpenCV not available. Install with: nix-shell -p python3Packages.opencv4")

class EVController:
    def __init__(self, serial_port='/dev/ttyUSB0', cameras=[0, 2]):
        self.serial_port = serial_port
        self.camera_ids = cameras
        self.ser = None
        self.caps = []
        self.running = False
        self.throttle = 0
        self.steering = 0
        self.brake = False
        
    def connect_serial(self):
        """Connect to ESP32"""
        try:
            self.ser = serial.Serial(self.serial_port, 115200, timeout=1)
            self.ser.reset_input_buffer()
            time.sleep(0.3)
            print(f"✓ ESP32 connected on {self.serial_port}")
            return True
        except Exception as e:
            print(f"✗ ESP32 connection failed: {e}")
            return False
    
    def connect_cameras(self):
        """Connect to cameras"""
        if not HAS_CV2:
            print("✗ Cameras disabled (no OpenCV)")
            return False
        
        for cam_id in self.camera_ids:
            cap = cv2.VideoCapture(cam_id, cv2.CAP_V4L2)
            if cap.isOpened():
                cap.set(cv2.CAP_PROP_FOURCC, cv2.VideoWriter_fourcc('M','J','P','G'))
                cap.set(cv2.CAP_PROP_FRAME_WIDTH, 640)
                cap.set(cv2.CAP_PROP_FRAME_HEIGHT, 480)
                cap.set(cv2.CAP_PROP_FPS, 30)
                self.caps.append((cam_id, cap))
                print(f"✓ Camera {cam_id} connected")
            else:
                print(f"✗ Camera {cam_id} failed")
        return len(self.caps) > 0
    
    def send_command(self, throttle=None, steering=None, brake=None):
        """Send command to ESP32"""
        if throttle is not None:
            self.throttle = max(0, min(100, throttle))
        if steering is not None:
            self.steering = max(-100, min(100, steering))
        if brake is not None:
            self.brake = brake
        
        if self.ser and self.ser.is_open:
            cmd = json.dumps({'t': self.throttle, 's': self.steering, 'b': self.brake})
            self.ser.write((cmd + '\n').encode())
            time.sleep(0.02)
            if self.ser.in_waiting:
                return self.ser.readline().decode().strip()
        return None
    
    def emergency_stop(self):
        """Immediate stop"""
        self.throttle = 0
        self.steering = 0
        self.brake = True
        self.send_command()
        print("\n!!! EMERGENCY STOP !!!")
    
    def get_frame(self, camera_idx=0):
        """Get frame from camera"""
        if camera_idx < len(self.caps):
            ret, frame = self.caps[camera_idx][1].read()
            if ret:
                return frame
        return None
    
    def process_frame(self, frame):
        """Simple obstacle detection - returns steering suggestion"""
        if frame is None:
            return 0
        
        # Convert to grayscale
        gray = cv2.cvtColor(frame, cv2.COLOR_BGR2GRAY)
        h, w = gray.shape
        
        # Split into left/center/right regions
        left = gray[:, :w//3].mean()
        center = gray[:, w//3:2*w//3].mean()
        right = gray[:, 2*w//3:].mean()
        
        # Simple logic: darker = obstacle, steer away
        threshold = 60
        if center < threshold:
            # Obstacle ahead - steer toward brighter side
            if left > right:
                return -30  # Steer left
            else:
                return 30   # Steer right
        return 0
    
    def run_keyboard(self):
        """Keyboard control mode"""
        print("\n=== KEYBOARD CONTROL ===")
        print("W/S = throttle up/down")
        print("A/D = steer left/right")
        print("SPACE/B = brake")
        print("X = reset to zero")
        print("Q = quit")
        print("Type key + Enter\n")
        
        while self.running:
            try:
                key = input(f"[T:{self.throttle:3d} S:{self.steering:+4d}]> ").lower().strip()
                
                if key == 'q':
                    self.emergency_stop()
                    break
                elif key == 'w':
                    self.send_command(throttle=self.throttle + 10)
                elif key == 's':
                    self.send_command(throttle=self.throttle - 10)
                elif key == 'a':
                    self.send_command(steering=self.steering - 15)
                elif key == 'd':
                    self.send_command(steering=self.steering + 15)
                elif key in (' ', 'b'):
                    self.emergency_stop()
                elif key == 'x':
                    self.send_command(throttle=0, steering=0, brake=False)
                    
            except (KeyboardInterrupt, EOFError):
                self.emergency_stop()
                break
    
    def run_auto(self, speed=20):
        """Autonomous mode with camera obstacle avoidance"""
        if not self.caps:
            print("No cameras - falling back to keyboard mode")
            return self.run_keyboard()
        
        print("\n=== AUTONOMOUS MODE ===")
        print(f"Speed: {speed}%, Press Ctrl+C to stop")
        
        self.send_command(throttle=speed, steering=0, brake=False)
        
        while self.running:
            try:
                frame = self.get_frame(0)
                if frame is not None:
                    steer_adjust = self.process_frame(frame)
                    new_steer = max(-100, min(100, self.steering + steer_adjust))
                    self.send_command(steering=new_steer)
                    
                    # Display if possible
                    cv2.putText(frame, f"T:{self.throttle} S:{self.steering}", 
                               (10, 30), cv2.FONT_HERSHEY_SIMPLEX, 1, (0, 255, 0), 2)
                    cv2.imshow('EV Camera', frame)
                    
                    if cv2.waitKey(1) & 0xFF == ord('q'):
                        break
                
                time.sleep(0.05)
                
            except KeyboardInterrupt:
                break
        
        self.emergency_stop()
        cv2.destroyAllWindows()
    
    def run_demo(self):
        """Demo sequence"""
        print("\n=== DEMO MODE ===")
        steps = [
            (0, 0, False, 2, "Ready"),
            (20, 0, False, 2, "Forward slow"),
            (30, 0, False, 2, "Forward medium"),
            (30, 40, False, 2, "Turn right"),
            (30, -40, False, 2, "Turn left"),
            (30, 0, False, 2, "Straight"),
            (0, 0, True, 1, "BRAKE"),
        ]
        
        for t, s, b, wait, desc in steps:
            if not self.running:
                break
            print(f"  {desc}: T={t}, S={s}, B={b}")
            self.send_command(throttle=t, steering=s, brake=b)
            time.sleep(wait)
        
        self.emergency_stop()
        print("Demo complete!")
    
    def run_camera_test(self):
        """Test cameras only"""
        if not self.caps:
            print("No cameras available")
            return
        
        print("\n=== CAMERA TEST ===")
        print("Press Q to quit")
        
        while self.running:
            for i, (cam_id, cap) in enumerate(self.caps):
                ret, frame = cap.read()
                if ret:
                    cv2.imshow(f'Camera {cam_id}', frame)
            
            if cv2.waitKey(1) & 0xFF == ord('q'):
                break
        
        cv2.destroyAllWindows()
    
    def cleanup(self):
        """Clean shutdown"""
        self.running = False
        if self.ser and self.ser.is_open:
            self.send_command(throttle=0, steering=0, brake=True)
            self.ser.close()
        for _, cap in self.caps:
            cap.release()
        try:
            if HAS_CV2:
                cv2.destroyAllWindows()
        except cv2.error:
            pass  # No GUI available
        print("Shutdown complete")
    
    def start(self, mode='keyboard'):
        """Main entry point"""
        print("=" * 40)
        print("   EV PROTOTYPE CONTROLLER")
        print("=" * 40)
        
        # Connect devices
        if not self.connect_serial():
            print("Cannot continue without ESP32")
            return
        
        self.connect_cameras()
        
        self.running = True
        
        # Handle Ctrl+C
        def handler(sig, frame):
            self.emergency_stop()
            self.cleanup()
            sys.exit(0)
        signal.signal(signal.SIGINT, handler)
        
        # Run selected mode
        if mode == 'demo':
            self.run_demo()
        elif mode == 'auto':
            self.run_auto()
        elif mode == 'camera':
            self.run_camera_test()
        else:
            self.run_keyboard()
        
        self.cleanup()


def main():
    import argparse
    parser = argparse.ArgumentParser(description='EV Prototype Controller')
    parser.add_argument('mode', nargs='?', default='keyboard',
                       choices=['keyboard', 'demo', 'auto', 'camera'],
                       help='Control mode')
    parser.add_argument('--port', '-p', default='/dev/ttyUSB0',
                       help='ESP32 serial port')
    parser.add_argument('--cameras', '-c', default='0,2',
                       help='Camera device IDs (comma-separated)')
    args = parser.parse_args()
    
    cameras = [int(x) for x in args.cameras.split(',')]
    
    controller = EVController(serial_port=args.port, cameras=cameras)
    controller.start(args.mode)


if __name__ == '__main__':
    main()
