#!/usr/bin/env python3
"""
EV Prototype - TUI Control Center v5b
Hold-to-drive with timeout-based key tracking (terminal compatible)
"""
import serial
import json
import time
import threading
import sys
import math
from datetime import datetime
from pathlib import Path

try:
    from textual.app import App, ComposeResult
    from textual.containers import Container, Horizontal
    from textual.widgets import Header, Footer, Static, Button, Log, ProgressBar
    from textual.reactive import reactive
    from textual import events
    from rich.text import Text
    from rich.style import Style
    HAS_TEXTUAL = True
except ImportError:
    HAS_TEXTUAL = False

try:
    import cv2
    import numpy as np
    HAS_CV2 = True
except ImportError:
    HAS_CV2 = False

try:
    import requests
    from PIL import Image
    import io
    HAS_MAP = True
except ImportError:
    HAS_MAP = False


class VehicleState:
    def __init__(self):
        self.throttle = 0
        self.steering = 0
        self.brake = False
        self.speed_estimate = 0.0
        self.lat = 30.6187
        self.lon = -96.3365
        self.heading = 0
        self.auto_mode = False
        self.connected = False
        self.cameras_ok = [False, False]
        self.obstacle_detected = False
        self.last_frame = [None, None]
        self.sim_mode = True
        
        # Key timing - track when each key was last pressed
        self.key_last_press = {}
        self.key_timeout = 0.15  # seconds - key considered "released" after this


class SerialController:
    def __init__(self, port='/dev/ttyUSB0'):
        self.port = port
        self.ser = None
        self.state = VehicleState()
        self.lock = threading.Lock()
        
    def connect(self):
        try:
            self.ser = serial.Serial(self.port, 115200, timeout=0.5)
            self.ser.reset_input_buffer()
            time.sleep(0.2)
            self.state.connected = True
            return True
        except:
            self.state.connected = False
            return False
    
    def send(self, throttle=None, steering=None, brake=None):
        with self.lock:
            if throttle is not None:
                self.state.throttle = max(0, min(100, throttle))
            if steering is not None:
                self.state.steering = max(-100, min(100, steering))
            if brake is not None:
                self.state.brake = brake
            
            if self.ser and self.ser.is_open:
                cmd = json.dumps({
                    't': self.state.throttle,
                    's': self.state.steering,
                    'b': self.state.brake
                })
                try:
                    self.ser.write((cmd + '\n').encode())
                    self.state.speed_estimate = self.state.throttle * 0.3
                    return True
                except:
                    return False
        return False
    
    def emergency_stop(self):
        self.state.throttle = 0
        self.state.steering = 0
        self.state.brake = True
        self.state.auto_mode = False
        self.state.key_last_press.clear()
        self.send()
    
    def close(self):
        if self.ser:
            self.emergency_stop()
            self.ser.close()


class MapRenderer:
    TILE_URL = "https://tile.openstreetmap.org/{z}/{x}/{y}.png"
    BLOCKS = " ░▒▓█"
    CACHE_DIR = Path.home() / ".cache" / "ev-prototype" / "tiles"
    
    def __init__(self):
        self.CACHE_DIR.mkdir(parents=True, exist_ok=True)
        self.zoom = 17
        self.tile_cache = {}
    
    def lat_lon_to_tile(self, lat, lon, zoom):
        n = 2 ** zoom
        x = (lon + 180) / 360 * n
        y = (1 - math.asinh(math.tan(math.radians(lat))) / math.pi) / 2 * n
        return x, y
    
    def fetch_tile(self, z, tx, ty):
        key = (z, tx, ty)
        if key in self.tile_cache:
            return self.tile_cache[key]
        
        if not HAS_MAP:
            return None
        
        cache_path = self.CACHE_DIR / f"{z}_{tx}_{ty}.png"
        
        if cache_path.exists():
            try:
                img = Image.open(cache_path)
                self.tile_cache[key] = img
                return img
            except:
                pass
        
        try:
            url = self.TILE_URL.format(z=z, x=tx, y=ty)
            headers = {'User-Agent': 'EV-Prototype/1.0'}
            resp = requests.get(url, headers=headers, timeout=5)
            if resp.status_code == 200:
                img = Image.open(io.BytesIO(resp.content))
                img.save(cache_path)
                self.tile_cache[key] = img
                return img
        except:
            pass
        return None
    
    def render(self, lat, lon, heading=0, width=56, height=10):
        text = Text()
        
        tile_x, tile_y = self.lat_lon_to_tile(lat, lon, self.zoom)
        tile_ix, tile_iy = int(tile_x), int(tile_y)
        
        px_in_tile = (tile_x - tile_ix) * 256
        py_in_tile = (tile_y - tile_iy) * 256
        
        char_w, char_h = 4, 7
        total_px_w = width * char_w
        total_px_h = height * char_h
        
        start_px = int(px_in_tile - total_px_w // 2)
        start_py = int(py_in_tile - total_px_h // 2)
        
        composite = Image.new('RGB', (total_px_w, total_px_h), (200, 200, 200))
        
        for dy in range(-1, 2):
            for dx in range(-1, 2):
                ttx, tty = tile_ix + dx, tile_iy + dy
                tile_img = self.fetch_tile(self.zoom, ttx, tty)
                if tile_img is None:
                    continue
                if tile_img.mode != 'RGB':
                    tile_img = tile_img.convert('RGB')
                paste_x = dx * 256 - start_px
                paste_y = dy * 256 - start_py
                composite.paste(tile_img, (paste_x, paste_y))
        
        resized = composite.resize((width, height), Image.Resampling.BILINEAR)
        
        center_x, center_y = width // 2, height // 2
        
        arrows = {0: "▲", 45: "◥", 90: "▶", 135: "◢", 180: "▼", 225: "◣", 270: "◀", 315: "◤"}
        arrow = arrows.get(round(heading / 45) * 45 % 360, "▲")
        
        for y in range(height):
            for x in range(width):
                dist = abs(x - center_x) + abs(y - center_y)
                
                if x == center_x and y == center_y:
                    text.append(arrow, style="bold red on black")
                elif dist <= 1:
                    r, g, b = resized.getpixel((x, y))
                    text.append("░", style=Style(color=f"rgb({min(255,r+60)},{g},{b})"))
                else:
                    r, g, b = resized.getpixel((x, y))
                    brightness = (r + g + b) // 3
                    char_idx = min(brightness * len(self.BLOCKS) // 256, len(self.BLOCKS) - 1)
                    text.append(self.BLOCKS[char_idx], style=Style(color=f"rgb({r},{g},{b})"))
            
            if y < height - 1:
                text.append("\n")
        
        return text
    
    def render_fallback(self, heading, width, height):
        text = Text()
        arrows = {0: "▲", 45: "◥", 90: "▶", 135: "◢", 180: "▼", 225: "◣", 270: "◀", 315: "◤"}
        arrow = arrows.get(round(heading / 45) * 45 % 360, "▲")
        cx, cy = width // 2, height // 2
        
        for y in range(height):
            for x in range(width):
                if x == cx and y == cy:
                    text.append(arrow, style="bold red on black")
                elif abs(x - cx) + abs(y - cy) <= 1:
                    text.append("░", style="yellow")
                elif x % 8 == 0:
                    text.append("│", style="dim blue")
                elif y % 4 == 0:
                    text.append("─", style="dim blue")
                else:
                    text.append(" ")
            if y < height - 1:
                text.append("\n")
        return text


class CameraProcessor:
    BLOCKS = " ░▒▓█"
    
    def __init__(self, camera_ids=[0, 2]):
        self.camera_ids = camera_ids
        self.caps = []
        self.state = None
        self.running = False
        self.thread = None
        
    def connect(self, state):
        self.state = state
        if not HAS_CV2:
            return False
        
        for i, cam_id in enumerate(self.camera_ids):
            try:
                cap = cv2.VideoCapture(cam_id, cv2.CAP_V4L2)
                cap.set(cv2.CAP_PROP_FOURCC, cv2.VideoWriter_fourcc('M','J','P','G'))
                cap.set(cv2.CAP_PROP_FRAME_WIDTH, 320)
                cap.set(cv2.CAP_PROP_FRAME_HEIGHT, 240)
                if cap.isOpened():
                    self.caps.append(cap)
                    self.state.cameras_ok[i] = True
            except:
                pass
        return len(self.caps) > 0
    
    def start_processing(self):
        self.running = True
        self.thread = threading.Thread(target=self._loop, daemon=True)
        self.thread.start()
    
    def _loop(self):
        while self.running:
            for i, cap in enumerate(self.caps):
                ret, frame = cap.read()
                if ret:
                    self.state.last_frame[i] = frame
                    if i == 0:
                        self._detect(frame)
            time.sleep(0.033)
    
    def _detect(self, frame):
        gray = cv2.cvtColor(frame, cv2.COLOR_BGR2GRAY)
        edges = cv2.Canny(gray, 50, 150)
        h, w = edges.shape
        center = edges[h//3:2*h//3, w//3:2*w//3]
        self.state.obstacle_detected = np.mean(center) / 255.0 > 0.15
    
    def render(self, idx=0, width=56, height=12, color=True):
        if idx >= len(self.state.last_frame) or self.state.last_frame[idx] is None:
            return Text("No camera feed", style="dim")
        
        frame = self.state.last_frame[idx]
        small = cv2.resize(frame, (width, height))
        rgb = cv2.cvtColor(small, cv2.COLOR_BGR2RGB)
        
        text = Text()
        for y in range(height):
            for x in range(width):
                r, g, b = rgb[y, x]
                brightness = (int(r) + int(g) + int(b)) // 3
                char_idx = min(brightness * len(self.BLOCKS) // 256, len(self.BLOCKS) - 1)
                char = self.BLOCKS[char_idx]
                if color:
                    text.append(char, style=Style(color=f"rgb({r},{g},{b})"))
                else:
                    text.append(char)
            if y < height - 1:
                text.append("\n")
        return text
    
    def stop(self):
        self.running = False
        for cap in self.caps:
            cap.release()


class AutoPilot:
    def __init__(self, controller, camera):
        self.controller = controller
        self.camera = camera
        self.running = False
        self.thread = None
        
    def start(self):
        self.running = True
        self.controller.state.auto_mode = True
        self.thread = threading.Thread(target=self._loop, daemon=True)
        self.thread.start()
    
    def stop(self):
        self.running = False
        self.controller.state.auto_mode = False
        self.controller.state.throttle = 0
        self.controller.state.steering = 0
        self.controller.send()
    
    def _loop(self):
        while self.running:
            state = self.controller.state
            if state.obstacle_detected:
                self.controller.send(throttle=0, brake=True)
                time.sleep(0.5)
                self.controller.send(throttle=15, steering=50, brake=False)
                time.sleep(1)
                self.controller.send(steering=0)
            else:
                frame = state.last_frame[0]
                steer = self._calc(frame) if frame is not None else 0
                self.controller.send(throttle=25, steering=steer, brake=False)
            time.sleep(0.1)
    
    def _calc(self, frame):
        gray = cv2.cvtColor(frame, cv2.COLOR_BGR2GRAY)
        edges = cv2.Canny(gray, 50, 150)
        h, w = edges.shape
        roi = edges[2*h//3:, :]
        left = np.sum(roi[:, :w//2])
        right = np.sum(roi[:, w//2:])
        return int((right - left) / (left + right + 1) * 30)


class EVControlApp(App):
    CSS = """
    Screen { layout: grid; grid-size: 2 2; grid-rows: 1fr 1fr; }
    #camera-panel { border: solid green; }
    #map-panel { border: solid blue; }
    #controls-panel { border: solid yellow; }
    #status-panel { border: solid cyan; }
    .panel-title { text-style: bold; background: $surface; }
    Button { margin: 0 1; }
    """
    
    BINDINGS = [
        ("p", "toggle_auto", "Auto"),
        ("c", "toggle_color", "Color"),
        ("m", "toggle_sim", "Sim"),
        ("x", "reset", "Reset"),
        ("q", "quit", "Quit"),
        ("escape", "brake", "Stop"),
    ]
    
    color_mode = reactive(True)
    
    def __init__(self, controller, camera, autopilot, map_renderer):
        super().__init__()
        self.controller = controller
        self.camera = camera
        self.autopilot = autopilot
        self.map_renderer = map_renderer
        
        # Control parameters
        self.max_throttle = 50
        self.max_steering = 60
        self.accel_rate = 6
        self.decel_rate = 4
        self.steer_rate = 8
        self.steer_return = 6
    
    def compose(self) -> ComposeResult:
        yield Header(show_clock=True)
        
        with Container(id="camera-panel"):
            yield Static("📷 CAMERA", classes="panel-title")
            yield Static("", id="camera-feed")
        
        with Container(id="map-panel"):
            yield Static("🗺️  MAP", classes="panel-title")
            yield Static("", id="map-display")
        
        with Container(id="controls-panel"):
            yield Static("🎮 HOLD KEYS TO DRIVE", classes="panel-title")
            yield Static("", id="key-status")
            yield Static("Throttle:")
            yield ProgressBar(total=100, show_eta=False, id="throttle-bar")
            yield Static("Steering:")
            yield ProgressBar(total=200, show_eta=False, id="steering-bar")
            with Horizontal():
                yield Button("AUTO [P]", id="btn-auto", variant="primary")
                yield Button("STOP [Esc]", id="btn-brake", variant="error")
        
        with Container(id="status-panel"):
            yield Static("📊 STATUS", classes="panel-title")
            yield Log(id="status-log", max_lines=6)
        
        yield Footer()
    
    def on_mount(self):
        self.set_interval(0.06, self._update)  # ~17Hz
        self._log("EV Control v5 - Hold to drive")
        self._log(f"ESP32: {'✓' if self.controller.state.connected else '✗'} | Cams: {sum(self.controller.state.cameras_ok)}/2")
    
    def _is_key_held(self, key):
        """Check if key is currently held (pressed recently)"""
        state = self.controller.state
        if key not in state.key_last_press:
            return False
        elapsed = time.time() - state.key_last_press[key]
        return elapsed < state.key_timeout
    
    def on_key(self, event: events.Key) -> None:
        """Track key press timing"""
        state = self.controller.state
        
        if state.auto_mode:
            return
        
        key = event.key.lower()
        if key in ('w', 'a', 's', 'd'):
            state.key_last_press[key] = time.time()
            state.brake = False
    
    def _update(self):
        state = self.controller.state
        
        if not state.auto_mode:
            self._update_controls()
        
        # Simulation movement
        if state.sim_mode and state.throttle > 0 and not state.brake:
            speed_deg = state.speed_estimate * 0.000006
            state.lat += speed_deg * math.cos(math.radians(state.heading))
            state.lon += speed_deg * math.sin(math.radians(state.heading))
            state.heading = (state.heading + state.steering * 0.04) % 360
        
        # Update display every 3rd frame
        if not hasattr(self, '_frame'):
            self._frame = 0
        self._frame += 1
        if self._frame % 3 == 0:
            self._update_display()
    
    def _update_controls(self):
        """Update controls based on key hold state"""
        state = self.controller.state
        
        w_held = self._is_key_held('w')
        s_held = self._is_key_held('s')
        a_held = self._is_key_held('a')
        d_held = self._is_key_held('d')
        
        # Throttle
        if w_held:
            new_throttle = min(self.max_throttle, state.throttle + self.accel_rate)
        elif s_held:
            new_throttle = max(0, state.throttle - self.decel_rate * 3)
        else:
            new_throttle = max(0, state.throttle - self.decel_rate)
        
        # Steering
        if a_held:
            new_steering = max(-self.max_steering, state.steering - self.steer_rate)
        elif d_held:
            new_steering = min(self.max_steering, state.steering + self.steer_rate)
        else:
            # Return to center
            if state.steering > 0:
                new_steering = max(0, state.steering - self.steer_return)
            elif state.steering < 0:
                new_steering = min(0, state.steering + self.steer_return)
            else:
                new_steering = 0
        
        self.controller.send(throttle=new_throttle, steering=new_steering, brake=False)
    
    def _update_display(self):
        state = self.controller.state
        
        # Camera
        if HAS_CV2 and self.camera.caps:
            cam = self.query_one("#camera-feed", Static)
            frame = self.camera.render(0, 56, 11, self.color_mode)
            if state.obstacle_detected:
                frame.append("\n⚠️ OBSTACLE", style="bold red")
            cam.update(frame)
        
        # Key status
        w = self._is_key_held('w')
        a = self._is_key_held('a')
        s = self._is_key_held('s')
        d = self._is_key_held('d')
        
        key_text = Text()
        key_text.append("      ")
        key_text.append("[W]", style="bold white on green" if w else "dim white on #333333")
        key_text.append("        ")
        key_text.append("Accel" if w else "     ", style="green" if w else "dim")
        key_text.append("\n    ")
        key_text.append("[A]", style="bold white on green" if a else "dim white on #333333")
        key_text.append("[S]", style="bold white on red" if s else "dim white on #333333")
        key_text.append("[D]", style="bold white on green" if d else "dim white on #333333")
        key_text.append("    ")
        
        status_parts = []
        if a: status_parts.append("Left")
        if s: status_parts.append("Brake")
        if d: status_parts.append("Right")
        key_text.append(" ".join(status_parts) if status_parts else "     ", style="yellow" if status_parts else "dim")
        
        self.query_one("#key-status", Static).update(key_text)
        
        # Map
        self.query_one("#map-display", Static).update(self._build_map())
        
        # Gauges
        self.query_one("#throttle-bar", ProgressBar).progress = state.throttle
        self.query_one("#steering-bar", ProgressBar).progress = state.steering + 100
    
    def _build_map(self):
        state = self.controller.state
        text = Text()
        
        mode_icon = "🤖" if state.auto_mode else "👤"
        mode_style = "bold cyan" if state.auto_mode else "bold yellow"
        sim_icon = "🎮SIM" if state.sim_mode else "📡GPS"
        
        cardinals = {0:"N", 45:"NE", 90:"E", 135:"SE", 180:"S", 225:"SW", 270:"W", 315:"NW"}
        hk = round(state.heading / 45) * 45 % 360
        
        speed_color = "red" if state.speed_estimate > 15 else ("yellow" if state.speed_estimate > 8 else "green")
        
        text.append(f" {cardinals[hk]:2} ", style="bold white")
        text.append(f"{state.speed_estimate:4.1f}", style=f"bold {speed_color}")
        text.append("mph ", style="dim")
        text.append(f"{mode_icon} ", style=mode_style)
        text.append(sim_icon, style="dim magenta")
        text.append("\n")
        text.append("─" * 56, style="dim blue")
        text.append("\n")
        
        if HAS_MAP:
            map_content = self.map_renderer.render(state.lat, state.lon, state.heading, 56, 9)
        else:
            map_content = self.map_renderer.render_fallback(state.heading, 56, 9)
        text.append_text(map_content)
        text.append("\n")
        
        text.append("─" * 56, style="dim blue")
        text.append("\n")
        text.append(f" {state.lat:.5f}°, {state.lon:.5f}°", style="dim")
        text.append(" │ College Station", style="white")
        
        return text
    
    def _log(self, msg):
        ts = datetime.now().strftime("%H:%M:%S")
        self.query_one("#status-log", Log).write_line(f"[{ts}] {msg}")
    
    def action_brake(self):
        self.controller.emergency_stop()
        self._log("EMERGENCY STOP")
    
    def action_reset(self):
        self.controller.emergency_stop()
        state = self.controller.state
        state.lat = 30.6187
        state.lon = -96.3365
        state.heading = 0
        state.brake = False
        self._log("Reset")
    
    def action_toggle_auto(self):
        if self.controller.state.auto_mode:
            self.autopilot.stop()
            self._log("Auto OFF")
        else:
            self.autopilot.start()
            self._log("Auto ON")
    
    def action_toggle_color(self):
        self.color_mode = not self.color_mode
    
    def action_toggle_sim(self):
        self.controller.state.sim_mode = not self.controller.state.sim_mode
        self._log(f"{'SIM' if self.controller.state.sim_mode else 'GPS'} mode")
    
    def action_quit(self):
        self.controller.emergency_stop()
        self.camera.stop()
        self.exit()
    
    def on_button_pressed(self, event):
        if event.button.id == "btn-brake":
            self.action_brake()
        elif event.button.id == "btn-auto":
            self.action_toggle_auto()


def main():
    if not HAS_TEXTUAL:
        print("ERROR: textual required")
        sys.exit(1)
    
    import argparse
    parser = argparse.ArgumentParser()
    parser.add_argument('--port', '-p', default='/dev/ttyUSB0')
    parser.add_argument('--cameras', '-c', default='0,2')
    args = parser.parse_args()
    
    cameras = [int(x) for x in args.cameras.split(',')]
    
    print("EV Control v5 - Hold keys to drive")
    
    controller = SerialController(args.port)
    controller.connect()
    
    camera = CameraProcessor(cameras)
    camera.connect(controller.state)
    camera.start_processing()
    
    autopilot = AutoPilot(controller, camera)
    map_renderer = MapRenderer()
    
    app = EVControlApp(controller, camera, autopilot, map_renderer)
    app.run()
    
    controller.close()
    camera.stop()


if __name__ == '__main__':
    main()
