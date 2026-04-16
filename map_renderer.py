#!/usr/bin/env python3
"""
Map Renderer - Fetches OSM tiles and renders as colored ASCII
"""
import math
import os
import hashlib
from pathlib import Path

try:
    import requests
    HAS_REQUESTS = True
except ImportError:
    HAS_REQUESTS = False

try:
    from PIL import Image
    import io
    HAS_PIL = True
except ImportError:
    HAS_PIL = False

try:
    from rich.text import Text
    from rich.style import Style
    HAS_RICH = True
except ImportError:
    HAS_RICH = False


class MapRenderer:
    """Renders OpenStreetMap tiles as colored ASCII"""
    
    # OSM tile server (using a public one)
    TILE_URL = "https://tile.openstreetmap.org/{z}/{x}/{y}.png"
    
    # Block characters for rendering
    BLOCKS = " ░▒▓█"
    
    # Cache directory
    CACHE_DIR = Path.home() / ".cache" / "ev-prototype" / "tiles"
    
    def __init__(self):
        self.CACHE_DIR.mkdir(parents=True, exist_ok=True)
        self.current_tile = None
        self.tile_x = 0
        self.tile_y = 0
        self.zoom = 17  # Street level zoom
    
    def lat_lon_to_tile(self, lat, lon, zoom):
        """Convert lat/lon to tile coordinates"""
        n = 2 ** zoom
        x = int((lon + 180) / 360 * n)
        y = int((1 - math.asinh(math.tan(math.radians(lat))) / math.pi) / 2 * n)
        return x, y
    
    def lat_lon_to_pixel_in_tile(self, lat, lon, zoom, tile_x, tile_y):
        """Get pixel position within a tile (0-255)"""
        n = 2 ** zoom
        x_tile = (lon + 180) / 360 * n
        y_tile = (1 - math.asinh(math.tan(math.radians(lat))) / math.pi) / 2 * n
        
        pixel_x = int((x_tile - tile_x) * 256)
        pixel_y = int((y_tile - tile_y) * 256)
        
        return pixel_x, pixel_y
    
    def get_tile_path(self, z, x, y):
        """Get cached tile path"""
        return self.CACHE_DIR / f"{z}_{x}_{y}.png"
    
    def fetch_tile(self, z, x, y):
        """Fetch a tile from OSM, with caching"""
        if not HAS_REQUESTS or not HAS_PIL:
            return None
        
        cache_path = self.get_tile_path(z, x, y)
        
        # Check cache
        if cache_path.exists():
            try:
                return Image.open(cache_path)
            except:
                pass
        
        # Fetch from server
        try:
            url = self.TILE_URL.format(z=z, x=x, y=y)
            headers = {'User-Agent': 'EV-Prototype/1.0'}
            resp = requests.get(url, headers=headers, timeout=5)
            if resp.status_code == 200:
                img = Image.open(io.BytesIO(resp.content))
                # Cache it
                img.save(cache_path)
                return img
        except Exception as e:
            pass
        
        return None
    
    def render_map_ascii(self, lat, lon, heading=0, width=35, height=16):
        """Render map centered on lat/lon as colored ASCII"""
        if not HAS_RICH:
            return Text("Map unavailable", style="dim")
        
        text = Text()
        
        # Get tile coordinates
        tile_x, tile_y = self.lat_lon_to_tile(lat, lon, self.zoom)
        
        # Fetch the tile
        tile_img = self.fetch_tile(self.zoom, tile_x, tile_y)
        
        if tile_img is None:
            return self._render_fallback_map(lat, lon, heading, width, height)
        
        # Convert to RGB if needed
        if tile_img.mode != 'RGB':
            tile_img = tile_img.convert('RGB')
        
        # Get our position in the tile
        px, py = self.lat_lon_to_pixel_in_tile(lat, lon, self.zoom, tile_x, tile_y)
        
        # Calculate crop region centered on our position
        half_w = (width * 256) // (2 * width)  # pixels per char
        half_h = (height * 256) // (2 * height)
        
        # We want to show area around our position
        # Each character represents ~4x8 pixels for aspect ratio
        char_w = 4
        char_h = 8
        
        crop_w = width * char_w
        crop_h = height * char_h
        
        left = max(0, px - crop_w // 2)
        top = max(0, py - crop_h // 2)
        right = min(256, left + crop_w)
        bottom = min(256, top + crop_h)
        
        # Adjust if we hit edges
        if right - left < crop_w:
            left = max(0, right - crop_w)
        if bottom - top < crop_h:
            top = max(0, bottom - crop_h)
        
        # Crop and resize
        cropped = tile_img.crop((left, top, right, bottom))
        resized = cropped.resize((width, height), Image.Resampling.LANCZOS)
        
        # Our position in the resized image
        our_x = (px - left) * width // crop_w
        our_y = (py - top) * height // crop_h
        our_x = max(0, min(width - 1, our_x))
        our_y = max(0, min(height - 1, our_y))
        
        # Direction arrow based on heading
        arrows = {
            0: "▲", 45: "◥", 90: "▶", 135: "◢",
            180: "▼", 225: "◣", 270: "◀", 315: "◤"
        }
        heading_key = round(heading / 45) * 45 % 360
        arrow = arrows.get(heading_key, "▲")
        
        # Render as colored ASCII
        for y in range(height):
            for x in range(width):
                r, g, b = resized.getpixel((x, y))
                
                # Check if this is our position
                if x == our_x and y == our_y:
                    text.append(arrow, style=Style(color="red", bold=True))
                else:
                    # Calculate brightness for character
                    brightness = (r + g + b) // 3
                    char_idx = min(brightness * len(self.BLOCKS) // 256, len(self.BLOCKS) - 1)
                    char = self.BLOCKS[char_idx]
                    
                    # Use actual color
                    text.append(char, style=Style(color=f"rgb({r},{g},{b})"))
            
            if y < height - 1:
                text.append("\n")
        
        return text
    
    def _render_fallback_map(self, lat, lon, heading, width, height):
        """Fallback ASCII map when tiles unavailable"""
        text = Text()
        
        arrows = {
            0: "▲", 45: "◥", 90: "▶", 135: "◢",
            180: "▼", 225: "◣", 270: "◀", 315: "◤"
        }
        heading_key = round(heading / 45) * 45 % 360
        arrow = arrows.get(heading_key, "▲")
        
        center_x = width // 2
        center_y = height // 2
        
        for y in range(height):
            for x in range(width):
                if x == center_x and y == center_y:
                    text.append(arrow, style="bold red")
                elif abs(x - center_x) < 3 and abs(y - center_y) < 2:
                    text.append("░", style="yellow")
                elif (x + y) % 8 == 0:
                    text.append("·", style="dim")
                else:
                    text.append(" ")
            if y < height - 1:
                text.append("\n")
        
        return text
    
    def render_map_panel(self, lat, lon, heading, speed, auto_mode, width=35, height=20):
        """Render complete map panel with info overlay"""
        if not HAS_RICH:
            return Text("Map unavailable")
        
        text = Text()
        
        # Header
        mode_text = "🤖 AUTO" if auto_mode else "👤 MANUAL"
        mode_style = "bold cyan" if auto_mode else "bold yellow"
        
        cardinals = {0: "N", 45: "NE", 90: "E", 135: "SE", 180: "S", 225: "SW", 270: "W", 315: "NW"}
        heading_key = round(heading / 45) * 45 % 360
        cardinal = cardinals.get(heading_key, "N")
        
        # Top info bar
        text.append("┌" + "─" * (width - 2) + "┐\n", style="blue")
        
        info_line = f" {cardinal:2} │ {speed:4.1f}mph │ "
        remaining = width - 2 - len(info_line) - len(mode_text) + 2  # emoji adjustment
        text.append("│", style="blue")
        text.append(info_line, style="white")
        text.append(mode_text, style=mode_style)
        text.append(" " * max(0, remaining - 2), style="dim")
        text.append("│\n", style="blue")
        
        text.append("├" + "─" * (width - 2) + "┤\n", style="blue")
        
        # Map area
        map_text = self.render_map_ascii(lat, lon, heading, width - 2, height - 8)
        
        # Add borders to each line of map
        map_lines = str(map_text).split('\n')
        for i, line in enumerate(map_lines):
            text.append("│", style="blue")
            # Re-render with colors (the str() loses them)
            pass
        
        # Actually, let's integrate the map directly
        # We need to wrap each line with borders
        
        # Render map and wrap with borders
        map_content = self.render_map_ascii(lat, lon, heading, width - 2, height - 8)
        
        for line in str(map_content).split('\n'):
            text.append("│", style="blue")
            text.append(line)
            # Pad if needed
            pad = width - 2 - len(line)
            if pad > 0:
                text.append(" " * pad)
            text.append("│\n", style="blue")
        
        # Bottom info
        text.append("├" + "─" * (width - 2) + "┤\n", style="blue")
        
        lat_str = f"Lat: {lat:.5f}°"
        lon_str = f"Lon: {lon:.5f}°"
        text.append("│ ", style="blue")
        text.append(lat_str, style="dim white")
        text.append(" " * (width - 4 - len(lat_str) - len(lon_str)), style="dim")
        text.append(lon_str, style="dim white")
        text.append(" │\n", style="blue")
        
        text.append("│ ", style="blue")
        text.append("@ College Station, TX", style="white")
        text.append(" " * (width - 4 - 21), style="dim")
        text.append(" │\n", style="blue")
        
        text.append("└" + "─" * (width - 2) + "┘", style="blue")
        
        return text


if __name__ == '__main__':
    # Test
    renderer = MapRenderer()
    result = renderer.render_map_ascii(30.6187, -96.3365, 45, 40, 20)
    print(result)
