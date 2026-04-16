//! EV Prototype Control Center - Windows GUI
//! Texas A&M FLiNT - Team Autopilot
//!
//! Full-featured GUI with camera feed, real map display, and vehicle controls.

#![windows_subsystem = "windows"]  // Hide console window on Windows

use eframe::egui;
use egui::{Color32, RichText, Vec2, Rect, Pos2, Stroke, FontId};
use image::{DynamicImage, RgbImage};
use serde::Serialize;
use serialport::SerialPort;
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

// ============================================================================
// Data Structures
// ============================================================================

#[derive(Serialize)]
struct Command {
    t: i32,
    s: i32,
    b: bool,
}

#[derive(Clone)]
struct VehicleState {
    throttle: f32,
    steering: f32,
    brake: bool,
    speed_estimate: f32,
    lat: f64,
    lon: f64,
    heading: f32,
    auto_mode: bool,
    connected: bool,
    sim_mode: bool,
    camera_count: usize,
}

impl Default for VehicleState {
    fn default() -> Self {
        Self {
            throttle: 0.0,
            steering: 0.0,
            brake: false,
            speed_estimate: 0.0,
            lat: 30.6187,
            lon: -96.3365,
            heading: 0.0,
            auto_mode: false,
            connected: false,
            sim_mode: true,
            camera_count: 0,
        }
    }
}

struct CameraFrame {
    data: Vec<u8>,
    width: u32,
    height: u32,
}

impl Clone for CameraFrame {
    fn clone(&self) -> Self {
        Self {
            data: self.data.clone(),
            width: self.width,
            height: self.height,
        }
    }
}

// ============================================================================
// Map Tile Cache
// ============================================================================

struct MapTileCache {
    cache_dir: PathBuf,
    tiles: HashMap<(u32, u32, u32), DynamicImage>,
    zoom: u32,
}

impl MapTileCache {
    fn new() -> Self {
        let cache_dir = directories::ProjectDirs::from("edu", "tamu", "ev-prototype")
            .map(|d| d.cache_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from(".cache"));
        let _ = std::fs::create_dir_all(&cache_dir);
        
        Self {
            cache_dir,
            tiles: HashMap::new(),
            zoom: 17,
        }
    }

    fn lat_lon_to_tile(&self, lat: f64, lon: f64) -> (f64, f64) {
        let n = 2_f64.powi(self.zoom as i32);
        let x = (lon + 180.0) / 360.0 * n;
        let y = (1.0 - (lat.to_radians().tan() + 1.0 / lat.to_radians().cos()).ln() / std::f64::consts::PI) / 2.0 * n;
        (x, y)
    }

    fn fetch_tile(&mut self, tx: u32, ty: u32) -> Option<&DynamicImage> {
        let key = (self.zoom, tx, ty);
        
        if self.tiles.contains_key(&key) {
            return self.tiles.get(&key);
        }

        let cache_path = self.cache_dir.join(format!("{}_{}_{}_.png", self.zoom, tx, ty));
        if cache_path.exists() {
            if let Ok(img) = image::open(&cache_path) {
                self.tiles.insert(key, img);
                return self.tiles.get(&key);
            }
        }

        let url = format!(
            "https://tile.openstreetmap.org/{}/{}/{}.png",
            self.zoom, tx, ty
        );
        
        if let Ok(response) = reqwest::blocking::Client::new()
            .get(&url)
            .header("User-Agent", "EV-Prototype/1.0")
            .timeout(Duration::from_secs(5))
            .send()
        {
            if let Ok(bytes) = response.bytes() {
                if let Ok(img) = image::load_from_memory(&bytes) {
                    let _ = img.save(&cache_path);
                    self.tiles.insert(key, img);
                    return self.tiles.get(&key);
                }
            }
        }

        None
    }

    fn render_map(&mut self, lat: f64, lon: f64, width: u32, height: u32) -> Option<RgbImage> {
        let (tile_x, tile_y) = self.lat_lon_to_tile(lat, lon);
        let tile_ix = tile_x as u32;
        let tile_iy = tile_y as u32;
        
        let px_in_tile = ((tile_x - tile_ix as f64) * 256.0) as i32;
        let py_in_tile = ((tile_y - tile_iy as f64) * 256.0) as i32;

        let mut composite = RgbImage::new(width, height);
        
        for pixel in composite.pixels_mut() {
            *pixel = image::Rgb([200, 200, 200]);
        }

        let half_w = (width / 2) as i32;
        let half_h = (height / 2) as i32;

        for dy in -1i32..=1 {
            for dx in -1i32..=1 {
                let ttx = (tile_ix as i32 + dx) as u32;
                let tty = (tile_iy as i32 + dy) as u32;
                
                if let Some(tile) = self.fetch_tile(ttx, tty) {
                    let tile_rgb = tile.to_rgb8();
                    let paste_x = dx * 256 - px_in_tile + half_w;
                    let paste_y = dy * 256 - py_in_tile + half_h;

                    for (tx, ty, pixel) in tile_rgb.enumerate_pixels() {
                        let dest_x = paste_x + tx as i32;
                        let dest_y = paste_y + ty as i32;
                        if dest_x >= 0 && dest_x < width as i32 && dest_y >= 0 && dest_y < height as i32 {
                            composite.put_pixel(dest_x as u32, dest_y as u32, *pixel);
                        }
                    }
                }
            }
        }

        Some(composite)
    }
}

// ============================================================================
// Serial Controller
// ============================================================================

struct SerialController {
    port: Option<Box<dyn SerialPort>>,
    port_name: String,
}

impl SerialController {
    fn new() -> Self {
        Self {
            port: None,
            port_name: String::new(),
        }
    }

    fn find_and_connect(&mut self, logs: &mut Vec<String>) -> bool {
        logs.push(format!("[{}] Scanning for ESP32...", timestamp()));
        
        match serialport::available_ports() {
            Ok(ports) => {
                logs.push(format!("[{}] Found {} serial ports", timestamp(), ports.len()));
                for port in ports {
                    if let serialport::SerialPortType::UsbPort(info) = &port.port_type {
                        let name = info.product.as_deref().unwrap_or("Unknown");
                        logs.push(format!("[{}] Port: {} ({})", timestamp(), port.port_name, name));
                        
                        // CP2102 or CH340
                        if (info.vid == 0x10C4 && info.pid == 0xEA60) || info.vid == 0x1A86 {
                            self.port_name = port.port_name.clone();
                            logs.push(format!("[{}] Found ESP32 on {}", timestamp(), self.port_name));
                            return self.connect(logs);
                        }
                    }
                }
                logs.push(format!("[{}] No ESP32 found", timestamp()));
            }
            Err(e) => {
                logs.push(format!("[{}] Error scanning ports: {}", timestamp(), e));
            }
        }
        false
    }

    fn connect(&mut self, logs: &mut Vec<String>) -> bool {
        logs.push(format!("[{}] Connecting to {}...", timestamp(), self.port_name));
        
        match serialport::new(&self.port_name, 115200)
            .timeout(Duration::from_millis(100))
            .open()
        {
            Ok(port) => {
                self.port = Some(port);
                logs.push(format!("[{}] Connected! Waiting for ESP32 reset...", timestamp()));
                std::thread::sleep(Duration::from_secs(2));
                logs.push(format!("[{}] ESP32 ready", timestamp()));
                true
            }
            Err(e) => {
                logs.push(format!("[{}] Connection failed: {}", timestamp(), e));
                false
            }
        }
    }

    fn send(&mut self, throttle: i32, steering: i32, brake: bool) {
        if let Some(ref mut port) = self.port {
            let cmd = Command {
                t: throttle,
                s: steering,
                b: brake,
            };
            if let Ok(json) = serde_json::to_string(&cmd) {
                let _ = port.write_all(format!("{}\n", json).as_bytes());
            }
        }
    }
}

// ============================================================================
// Camera Handler (using OpenCV via system call for Windows compatibility)
// ============================================================================

struct CameraHandler {
    frame: Arc<Mutex<Option<CameraFrame>>>,
    running: Arc<Mutex<bool>>,
    camera_count: Arc<Mutex<usize>>,
}

impl CameraHandler {
    fn new() -> Self {
        Self {
            frame: Arc::new(Mutex::new(None)),
            running: Arc::new(Mutex::new(false)),
            camera_count: Arc::new(Mutex::new(0)),
        }
    }

    fn start(&self, logs: Arc<Mutex<Vec<String>>>) {
        let frame = self.frame.clone();
        let running = self.running.clone();
        let camera_count = self.camera_count.clone();
        *running.lock().unwrap() = true;

        std::thread::spawn(move || {
            // Try to use OpenCV if available, otherwise use platform-specific capture
            #[cfg(windows)]
            {
                use std::process::Command;
                
                // Check if we can find cameras via WMI or DirectShow
                if let Ok(output) = Command::new("powershell")
                    .args(["-Command", "Get-PnpDevice -Class Camera | Select-Object -ExpandProperty FriendlyName"])
                    .output()
                {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let count = stdout.lines().filter(|l| !l.is_empty()).count();
                    *camera_count.lock().unwrap() = count;
                    
                    if let Ok(mut logs) = logs.lock() {
                        logs.push(format!("[{}] Found {} camera(s)", timestamp(), count));
                        for (i, name) in stdout.lines().filter(|l| !l.is_empty()).enumerate() {
                            logs.push(format!("[{}]   Camera {}: {}", timestamp(), i, name.trim()));
                        }
                    }
                }
                
                // For actual capture, we'd need opencv or nokhwa with proper features
                // For now, indicate cameras found but capture not available without opencv
                if let Ok(mut logs) = logs.lock() {
                    logs.push(format!("[{}] Camera capture requires OpenCV (build with --features camera)", timestamp()));
                }
            }
            
            #[cfg(not(windows))]
            {
                // Linux: try v4l2
                use std::fs;
                if let Ok(entries) = fs::read_dir("/dev") {
                    let videos: Vec<_> = entries
                        .filter_map(|e| e.ok())
                        .filter(|e| e.file_name().to_string_lossy().starts_with("video"))
                        .collect();
                    *camera_count.lock().unwrap() = videos.len();
                    
                    if let Ok(mut logs) = logs.lock() {
                        logs.push(format!("[{}] Found {} video device(s)", timestamp(), videos.len()));
                    }
                }
            }
        });
    }

    fn stop(&self) {
        *self.running.lock().unwrap() = false;
    }

    fn get_frame(&self) -> Option<CameraFrame> {
        self.frame.lock().unwrap().clone()
    }
    
    fn get_camera_count(&self) -> usize {
        *self.camera_count.lock().unwrap()
    }
}

fn timestamp() -> String {
    chrono::Local::now().format("%H:%M:%S").to_string()
}

// ============================================================================
// Main Application
// ============================================================================

struct EVControlApp {
    state: VehicleState,
    serial: SerialController,
    camera: CameraHandler,
    map_cache: MapTileCache,
    map_texture: Option<egui::TextureHandle>,
    logs: Arc<Mutex<Vec<String>>>,
    keys_held: HashMap<egui::Key, Instant>,
    key_timeout: Duration,
    last_send: Instant,
    last_map_update: Instant,
    estop_pressed: bool,
    reset_pressed: bool,
}

impl EVControlApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let logs = Arc::new(Mutex::new(Vec::new()));
        
        {
            let mut l = logs.lock().unwrap();
            l.push(format!("[{}] ═══════════════════════════════════════", timestamp()));
            l.push(format!("[{}] EV Prototype Control Center v1.1", timestamp()));
            l.push(format!("[{}] Texas A&M FLiNT - Team Autopilot", timestamp()));
            l.push(format!("[{}] ═══════════════════════════════════════", timestamp()));
        }
        
        let mut serial = SerialController::new();
        let connected = {
            let mut l = logs.lock().unwrap();
            serial.find_and_connect(&mut l)
        };
        
        let camera = CameraHandler::new();
        camera.start(logs.clone());

        Self {
            state: VehicleState {
                connected,
                ..Default::default()
            },
            serial,
            camera,
            map_cache: MapTileCache::new(),
            map_texture: None,
            logs,
            keys_held: HashMap::new(),
            key_timeout: Duration::from_millis(150),
            last_send: Instant::now(),
            last_map_update: Instant::now(),
            estop_pressed: false,
            reset_pressed: false,
        }
    }

    fn log(&self, msg: &str) {
        if let Ok(mut logs) = self.logs.lock() {
            logs.push(format!("[{}] {}", timestamp(), msg));
            if logs.len() > 100 {
                logs.remove(0);
            }
        }
    }

    fn is_key_held(&self, key: egui::Key) -> bool {
        self.keys_held.get(&key)
            .map(|t| t.elapsed() < self.key_timeout)
            .unwrap_or(false)
    }

    fn update_controls(&mut self) {
        let accel = 4.0;
        let decel = 6.0;

        // Throttle
        if self.is_key_held(egui::Key::W) {
            self.state.throttle = (self.state.throttle + accel).min(100.0);
            self.state.brake = false;
        } else if self.is_key_held(egui::Key::S) {
            if self.state.throttle > 0.0 {
                self.state.throttle = (self.state.throttle - accel * 2.0).max(0.0);
                self.state.brake = true;
            } else {
                self.state.throttle = (self.state.throttle - accel).max(-50.0);
                self.state.brake = false;
            }
        } else {
            if self.state.throttle.abs() < decel {
                self.state.throttle = 0.0;
            } else if self.state.throttle > 0.0 {
                self.state.throttle -= decel;
            } else {
                self.state.throttle += decel;
            }
            self.state.brake = false;
        }

        // Steering
        if self.is_key_held(egui::Key::A) {
            self.state.steering = (self.state.steering - accel).max(-100.0);
        } else if self.is_key_held(egui::Key::D) {
            self.state.steering = (self.state.steering + accel).min(100.0);
        }

        // Emergency brake
        if self.is_key_held(egui::Key::Space) {
            self.state.brake = true;
            self.state.throttle = 0.0;
        }

        // Speed estimate and simulation
        self.state.speed_estimate = self.state.throttle.abs() * 0.3;
        
        if self.state.sim_mode && self.state.throttle.abs() > 0.0 && !self.state.brake {
            let speed_deg = self.state.speed_estimate as f64 * 0.000006;
            self.state.lat += speed_deg * (self.state.heading as f64).to_radians().cos();
            self.state.lon += speed_deg * (self.state.heading as f64).to_radians().sin();
            self.state.heading = (self.state.heading + self.state.steering * 0.04) % 360.0;
            if self.state.heading < 0.0 {
                self.state.heading += 360.0;
            }
        }
        
        // Update camera count
        self.state.camera_count = self.camera.get_camera_count();
    }

    fn send_command(&mut self) {
        if self.last_send.elapsed() > Duration::from_millis(50) {
            self.serial.send(
                self.state.throttle as i32,
                self.state.steering as i32,
                self.state.brake,
            );
            self.last_send = Instant::now();
        }
    }

    fn draw_camera_panel(&self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("📷 Camera");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let count = self.state.camera_count;
                if count > 0 {
                    ui.label(RichText::new(format!("{} found", count)).color(Color32::GREEN).small());
                } else {
                    ui.label(RichText::new("None found").color(Color32::RED).small());
                }
            });
        });
        ui.separator();

        let available = ui.available_size();
        let (response, painter) = ui.allocate_painter(available, egui::Sense::hover());
        let rect = response.rect;

        if let Some(frame) = self.camera.get_frame() {
            let chars = [' ', '░', '▒', '▓', '█'];
            let char_w = 8.0;
            let char_h = 12.0;
            let cols = (rect.width() / char_w) as usize;
            let rows = (rect.height() / char_h) as usize;

            for row in 0..rows {
                for col in 0..cols {
                    let src_x = (col * frame.width as usize / cols) as u32;
                    let src_y = (row * frame.height as usize / rows) as u32;
                    let idx = ((src_y * frame.width + src_x) * 3) as usize;
                    
                    if idx + 2 < frame.data.len() {
                        let r = frame.data[idx] as u32;
                        let g = frame.data[idx + 1] as u32;
                        let b = frame.data[idx + 2] as u32;
                        let brightness = (r + g + b) / 3;
                        let char_idx = (brightness * chars.len() as u32 / 256) as usize;
                        let char_idx = char_idx.min(chars.len() - 1);
                        
                        painter.text(
                            Pos2::new(rect.min.x + col as f32 * char_w, rect.min.y + row as f32 * char_h),
                            egui::Align2::LEFT_TOP,
                            chars[char_idx],
                            FontId::monospace(12.0),
                            Color32::from_rgb(r as u8, g as u8, b as u8),
                        );
                    }
                }
            }
        } else {
            painter.rect_filled(rect, 4.0, Color32::from_gray(30));
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                format!("No Camera Feed\n\n{} camera(s) detected\nCapture requires OpenCV build", self.state.camera_count),
                FontId::proportional(14.0),
                Color32::GRAY,
            );
        }
    }

    fn draw_map_panel(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            ui.heading("🗺️ Map");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new(format!("{:.5}°, {:.5}°", self.state.lat, self.state.lon)).small().weak());
            });
        });
        ui.separator();

        let available = ui.available_size();
        let map_size = available;

        // Update map texture periodically
        if self.last_map_update.elapsed() > Duration::from_millis(500) || self.map_texture.is_none() {
            if let Some(map_img) = self.map_cache.render_map(
                self.state.lat,
                self.state.lon,
                map_size.x as u32,
                map_size.y as u32,
            ) {
                let size = [map_img.width() as usize, map_img.height() as usize];
                let pixels: Vec<egui::Color32> = map_img
                    .pixels()
                    .map(|p| egui::Color32::from_rgb(p[0], p[1], p[2]))
                    .collect();
                
                let image = egui::ColorImage { size, pixels };
                self.map_texture = Some(ctx.load_texture("map", image, egui::TextureOptions::LINEAR));
                self.last_map_update = Instant::now();
            }
        }

        let (response, painter) = ui.allocate_painter(map_size, egui::Sense::hover());
        let rect = response.rect;

        if let Some(tex) = &self.map_texture {
            painter.image(tex.id(), rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
        } else {
            painter.rect_filled(rect, 4.0, Color32::from_gray(60));
        }

        // Draw vehicle marker at center
        let center = rect.center();
        let heading_rad = (self.state.heading - 90.0).to_radians();
        let arrow_len = 15.0;
        
        let tip = Pos2::new(
            center.x + heading_rad.cos() * arrow_len,
            center.y + heading_rad.sin() * arrow_len,
        );
        let left = Pos2::new(
            center.x + (heading_rad + 2.5).cos() * arrow_len * 0.6,
            center.y + (heading_rad + 2.5).sin() * arrow_len * 0.6,
        );
        let right = Pos2::new(
            center.x + (heading_rad - 2.5).cos() * arrow_len * 0.6,
            center.y + (heading_rad - 2.5).sin() * arrow_len * 0.6,
        );
        
        painter.add(egui::Shape::convex_polygon(
            vec![tip, left, right],
            Color32::RED,
            Stroke::new(2.0, Color32::WHITE),
        ));
        painter.circle_stroke(center, 20.0, Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 128)));
    }

    fn draw_controls_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("🎮 Controls");
        ui.separator();

        // Mode toggles
        ui.horizontal(|ui| {
            let sim_text = if self.state.sim_mode { "🎮 SIM" } else { "📍 GPS" };
            let sim_color = if self.state.sim_mode { Color32::from_rgb(200, 100, 255) } else { Color32::GREEN };
            if ui.add(egui::Button::new(RichText::new(sim_text).color(sim_color))).clicked() {
                self.state.sim_mode = !self.state.sim_mode;
                self.log(if self.state.sim_mode { "Switched to SIM mode" } else { "Switched to GPS mode" });
            }
            
            let auto_text = if self.state.auto_mode { "🤖 AUTO" } else { "👤 MANUAL" };
            let auto_color = if self.state.auto_mode { Color32::from_rgb(0, 200, 255) } else { Color32::YELLOW };
            if ui.add(egui::Button::new(RichText::new(auto_text).color(auto_color))).clicked() {
                self.state.auto_mode = !self.state.auto_mode;
                self.log(if self.state.auto_mode { "AUTO mode enabled" } else { "MANUAL mode" });
            }
        });
        
        ui.add_space(10.0);

        // Key indicators
        ui.horizontal(|ui| {
            ui.add_space(30.0);
            let w_style = if self.is_key_held(egui::Key::W) {
                egui::Button::new(RichText::new("[W]").strong()).fill(Color32::DARK_GREEN)
            } else {
                egui::Button::new(" W ")
            };
            ui.add(w_style);
        });

        ui.horizontal(|ui| {
            let keys = [
                (egui::Key::A, "A", Color32::DARK_GREEN),
                (egui::Key::S, "S", Color32::DARK_RED),
                (egui::Key::D, "D", Color32::DARK_GREEN),
            ];
            for (key, label, color) in keys {
                let btn = if self.is_key_held(key) {
                    egui::Button::new(RichText::new(format!("[{}]", label)).strong()).fill(color)
                } else {
                    egui::Button::new(format!(" {} ", label))
                };
                ui.add(btn);
            }
            
            ui.add_space(10.0);
            if self.state.brake {
                ui.label(RichText::new("🛑 BRAKE").color(Color32::RED).strong());
            }
        });

        ui.add_space(10.0);

        // Gauges
        ui.horizontal(|ui| {
            ui.label("Throttle:");
            let progress = (self.state.throttle.abs() / 100.0).min(1.0);
            let color = if self.state.throttle >= 0.0 { Color32::GREEN } else { Color32::YELLOW };
            ui.add(egui::ProgressBar::new(progress).fill(color).text(format!("{:+.0}%", self.state.throttle)));
        });

        ui.horizontal(|ui| {
            ui.label("Steering:");
            let normalized = (self.state.steering + 100.0) / 200.0;
            ui.add(egui::ProgressBar::new(normalized).text(format!("{:+.0}", self.state.steering)));
        });

        ui.horizontal(|ui| {
            ui.label("Speed:");
            let color = if self.state.speed_estimate > 15.0 {
                Color32::RED
            } else if self.state.speed_estimate > 8.0 {
                Color32::YELLOW
            } else {
                Color32::GREEN
            };
            ui.label(RichText::new(format!("{:.1} mph", self.state.speed_estimate)).color(color).strong());
            
            // Heading
            let cardinals = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"];
            let idx = ((self.state.heading + 22.5) / 45.0) as usize % 8;
            ui.label(RichText::new(format!("  {} ({:.0}°)", cardinals[idx], self.state.heading)).weak());
        });

        ui.add_space(10.0);
        ui.separator();
        
        // Action buttons
        ui.horizontal(|ui| {
            if ui.add(egui::Button::new(RichText::new("🛑 E-STOP").color(Color32::WHITE)).fill(Color32::DARK_RED)).clicked() {
                self.estop_pressed = true;
            }
            if ui.button("↺ Reset").clicked() {
                self.reset_pressed = true;
            }
            if ui.button("🔌 Reconnect").clicked() {
                let mut logs_vec = self.logs.lock().unwrap();
                self.state.connected = self.serial.find_and_connect(&mut logs_vec);
            }
        });

        ui.add_space(5.0);
        ui.label(RichText::new("W/S=Throttle  A/D=Steer  Space=E-Stop  Esc=Stop  R=Reset").small().weak());
    }

    fn draw_logs_panel(&self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("📊 Logs");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Status indicators
                if self.state.connected {
                    ui.label(RichText::new("● ESP32").color(Color32::GREEN).small());
                } else {
                    ui.label(RichText::new("○ ESP32").color(Color32::RED).small());
                }
                ui.label(RichText::new(format!("● {}cam", self.state.camera_count))
                    .color(if self.state.camera_count > 0 { Color32::GREEN } else { Color32::RED }).small());
            });
        });
        ui.separator();

        egui::ScrollArea::vertical()
            .stick_to_bottom(true)
            .show(ui, |ui| {
                if let Ok(logs) = self.logs.lock() {
                    for log in logs.iter() {
                        ui.label(RichText::new(log).small().monospace());
                    }
                }
            });
    }
}

impl eframe::App for EVControlApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Handle keyboard input
        ctx.input(|i| {
            let now = Instant::now();
            for key in [egui::Key::W, egui::Key::A, egui::Key::S, egui::Key::D, egui::Key::Space] {
                if i.key_down(key) {
                    self.keys_held.insert(key, now);
                }
            }
            
            if i.key_pressed(egui::Key::Q) || (i.modifiers.ctrl && i.key_pressed(egui::Key::C)) {
                self.log("Shutting down...");
                std::process::exit(0);
            }
            
            if i.key_pressed(egui::Key::Escape) {
                self.estop_pressed = true;
            }
            
            if i.key_pressed(egui::Key::R) {
                self.reset_pressed = true;
            }
            
            if i.key_pressed(egui::Key::M) {
                self.state.sim_mode = !self.state.sim_mode;
                self.log(if self.state.sim_mode { "SIM mode" } else { "GPS mode" });
            }
            
            if i.key_pressed(egui::Key::P) {
                self.state.auto_mode = !self.state.auto_mode;
                self.log(if self.state.auto_mode { "AUTO mode" } else { "MANUAL mode" });
            }
        });
        
        // Handle button presses
        if self.estop_pressed {
            self.state.throttle = 0.0;
            self.state.steering = 0.0;
            self.state.brake = true;
            self.state.auto_mode = false;
            self.log("EMERGENCY STOP");
            self.estop_pressed = false;
        }
        
        if self.reset_pressed {
            self.state = VehicleState {
                connected: self.state.connected,
                camera_count: self.state.camera_count,
                ..Default::default()
            };
            self.log("Reset to defaults");
            self.reset_pressed = false;
        }

        // Update vehicle
        if !self.state.auto_mode {
            self.update_controls();
        }
        self.send_command();

        // Header
        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading(RichText::new("🚗 EV Prototype Control Center").strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new("Texas A&M FLiNT - Team Autopilot").weak());
                    ui.separator();
                    let mode = if self.state.sim_mode { "SIM" } else { "GPS" };
                    let mode_color = if self.state.sim_mode { Color32::from_rgb(200, 100, 255) } else { Color32::GREEN };
                    ui.label(RichText::new(mode).color(mode_color));
                });
            });
        });

        // Fixed 2x2 grid layout
        egui::CentralPanel::default().show(ctx, |ui| {
            let available = ui.available_size();
            let half_w = available.x / 2.0 - 5.0;
            let half_h = available.y / 2.0 - 5.0;
            
            ui.horizontal(|ui| {
                // Top row
                ui.vertical(|ui| {
                    ui.set_width(half_w);
                    ui.set_height(half_h);
                    egui::Frame::dark_canvas(ui.style())
                        .inner_margin(8.0)
                        .show(ui, |ui| {
                            self.draw_camera_panel(ui);
                        });
                });
                
                ui.vertical(|ui| {
                    ui.set_width(half_w);
                    ui.set_height(half_h);
                    egui::Frame::dark_canvas(ui.style())
                        .inner_margin(8.0)
                        .show(ui, |ui| {
                            self.draw_map_panel(ui, ctx);
                        });
                });
            });
            
            ui.add_space(5.0);
            
            ui.horizontal(|ui| {
                // Bottom row
                ui.vertical(|ui| {
                    ui.set_width(half_w);
                    ui.set_height(half_h);
                    egui::Frame::dark_canvas(ui.style())
                        .inner_margin(8.0)
                        .show(ui, |ui| {
                            self.draw_controls_panel(ui);
                        });
                });
                
                ui.vertical(|ui| {
                    ui.set_width(half_w);
                    ui.set_height(half_h);
                    egui::Frame::dark_canvas(ui.style())
                        .inner_margin(8.0)
                        .show(ui, |ui| {
                            self.draw_logs_panel(ui);
                        });
                });
            });
        });

        ctx.request_repaint_after(Duration::from_millis(33));
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.camera.stop();
        self.serial.send(0, 0, true);
    }
}

// ============================================================================
// Main
// ============================================================================

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1000.0, 700.0])
            .with_min_inner_size([800.0, 600.0])
            .with_title("EV Prototype Control Center"),
        ..Default::default()
    };

    eframe::run_native(
        "EV Control",
        options,
        Box::new(|cc| Ok(Box::new(EVControlApp::new(cc)))),
    )
}
