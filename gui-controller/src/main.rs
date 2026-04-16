//! EV Prototype Control Center - Windows GUI
//! Texas A&M FLiNT - Team Autopilot
//!
//! Full-featured GUI with camera feed, real map display, and vehicle controls.

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
        }
    }
}

struct CameraFrame {
    data: Vec<u8>,
    width: u32,
    height: u32,
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

        // Try cache file
        let cache_path = self.cache_dir.join(format!("{}_{}_{}_.png", self.zoom, tx, ty));
        if cache_path.exists() {
            if let Ok(img) = image::open(&cache_path) {
                self.tiles.insert(key, img);
                return self.tiles.get(&key);
            }
        }

        // Fetch from OSM
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
        
        // Fill with gray background
        for pixel in composite.pixels_mut() {
            *pixel = image::Rgb([200, 200, 200]);
        }

        let half_w = (width / 2) as i32;
        let half_h = (height / 2) as i32;

        // Composite tiles around center
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

    fn find_and_connect(&mut self) -> bool {
        if let Ok(ports) = serialport::available_ports() {
            for port in ports {
                if let serialport::SerialPortType::UsbPort(info) = &port.port_type {
                    // CP2102 or CH340
                    if (info.vid == 0x10C4 && info.pid == 0xEA60) || info.vid == 0x1A86 {
                        self.port_name = port.port_name.clone();
                        return self.connect();
                    }
                }
            }
        }
        false
    }

    fn connect(&mut self) -> bool {
        match serialport::new(&self.port_name, 115200)
            .timeout(Duration::from_millis(100))
            .open()
        {
            Ok(port) => {
                self.port = Some(port);
                std::thread::sleep(Duration::from_secs(2)); // Wait for ESP32 reset
                true
            }
            Err(_) => false,
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

    fn is_connected(&self) -> bool {
        self.port.is_some()
    }
}

// ============================================================================
// Camera Handler  
// ============================================================================

struct CameraHandler {
    frame: Arc<Mutex<Option<CameraFrame>>>,
    #[allow(dead_code)]
    running: Arc<Mutex<bool>>,
}

impl CameraHandler {
    fn new() -> Self {
        Self {
            frame: Arc::new(Mutex::new(None)),
            running: Arc::new(Mutex::new(false)),
        }
    }

    #[cfg(feature = "camera")]
    fn start(&self) {
        let frame = self.frame.clone();
        let running = self.running.clone();
        *running.lock().unwrap() = true;

        std::thread::spawn(move || {
            use nokhwa::pixel_format::RgbFormat;
            use nokhwa::utils::{CameraIndex, RequestedFormat, RequestedFormatType};
            use nokhwa::Camera;

            let index = CameraIndex::Index(0);
            let requested = RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestFrameRate);
            
            if let Ok(mut camera) = Camera::new(index, requested) {
                let _ = camera.open_stream();
                
                while *running.lock().unwrap() {
                    if let Ok(buffer) = camera.frame() {
                        let decoded = buffer.decode_image::<RgbFormat>().unwrap();
                        let mut frame_lock = frame.lock().unwrap();
                        *frame_lock = Some(CameraFrame {
                            data: decoded.to_vec(),
                            width: decoded.width(),
                            height: decoded.height(),
                        });
                    }
                    std::thread::sleep(Duration::from_millis(33));
                }
            }
        });
    }

    #[cfg(not(feature = "camera"))]
    fn start(&self) {
        // Camera support requires building with --features camera
    }

    fn stop(&self) {
        *self.running.lock().unwrap() = false;
    }

    fn get_frame(&self) -> Option<CameraFrame> {
        self.frame.lock().unwrap().clone()
    }
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
// Main Application
// ============================================================================

struct EVControlApp {
    state: VehicleState,
    serial: SerialController,
    camera: CameraHandler,
    map_cache: MapTileCache,
    map_texture: Option<egui::TextureHandle>,
    logs: Vec<String>,
    keys_held: HashMap<egui::Key, Instant>,
    key_timeout: Duration,
    last_send: Instant,
    last_map_update: Instant,
}

impl EVControlApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let mut serial = SerialController::new();
        let connected = serial.find_and_connect();
        
        let camera = CameraHandler::new();
        camera.start();

        let mut logs = Vec::new();
        logs.push(format!("[{}] EV Control GUI started", Self::timestamp()));
        if connected {
            logs.push(format!("[{}] ESP32 connected: {}", Self::timestamp(), serial.port_name));
        } else {
            logs.push(format!("[{}] ESP32 not found - Demo mode", Self::timestamp()));
        }

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
        }
    }

    fn timestamp() -> String {
        chrono::Local::now().format("%H:%M:%S").to_string()
    }

    fn log(&mut self, msg: &str) {
        self.logs.push(format!("[{}] {}", Self::timestamp(), msg));
        if self.logs.len() > 50 {
            self.logs.remove(0);
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
        // Steering doesn't auto-center (manual motor)

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
        ui.heading("📷 Camera");
        ui.separator();

        let available = ui.available_size();
        let (response, painter) = ui.allocate_painter(
            Vec2::new(available.x, available.y.min(200.0)),
            egui::Sense::hover(),
        );
        let rect = response.rect;

        // ASCII-style camera rendering
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
                        
                        let pos = Pos2::new(
                            rect.min.x + col as f32 * char_w,
                            rect.min.y + row as f32 * char_h,
                        );
                        
                        painter.text(
                            pos,
                            egui::Align2::LEFT_TOP,
                            chars[char_idx],
                            FontId::monospace(12.0),
                            Color32::from_rgb(r as u8, g as u8, b as u8),
                        );
                    }
                }
            }
        } else {
            // No camera - show placeholder
            painter.rect_filled(rect, 4.0, Color32::from_gray(40));
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "No Camera Feed\n(Connect USB camera)",
                FontId::proportional(16.0),
                Color32::GRAY,
            );
        }
    }

    fn draw_map_panel(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.heading("🗺️ Map");
        ui.horizontal(|ui| {
            let mode = if self.state.sim_mode { "SIM" } else { "GPS" };
            ui.label(format!("{:.5}°, {:.5}° | {}", self.state.lat, self.state.lon, mode));
            if ui.small_button(if self.state.sim_mode { "📍 GPS" } else { "🎮 SIM" }).clicked() {
                self.state.sim_mode = !self.state.sim_mode;
            }
        });
        ui.separator();

        let available = ui.available_size();
        let map_size = Vec2::new(available.x, available.y.min(250.0));

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

        // Draw map texture
        if let Some(tex) = &self.map_texture {
            painter.image(tex.id(), rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
        } else {
            painter.rect_filled(rect, 4.0, Color32::from_gray(60));
        }

        // Draw vehicle marker at center
        let center = rect.center();
        let heading_rad = (self.state.heading - 90.0).to_radians();
        let arrow_len = 15.0;
        
        // Vehicle triangle
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

        // Heading indicator circle
        painter.circle_stroke(center, 20.0, Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 128)));
    }

    fn draw_controls_panel(&self, ui: &mut egui::Ui) {
        ui.heading("🎮 Controls");
        ui.separator();

        // Key indicators
        ui.horizontal(|ui| {
            ui.add_space(40.0);
            let w_style = if self.is_key_held(egui::Key::W) {
                egui::Button::new(RichText::new("[W]").strong()).fill(Color32::DARK_GREEN)
            } else {
                egui::Button::new(" W ")
            };
            ui.add(w_style);
        });

        ui.horizontal(|ui| {
            let a_style = if self.is_key_held(egui::Key::A) {
                egui::Button::new(RichText::new("[A]").strong()).fill(Color32::DARK_GREEN)
            } else {
                egui::Button::new(" A ")
            };
            let s_style = if self.is_key_held(egui::Key::S) {
                egui::Button::new(RichText::new("[S]").strong()).fill(Color32::DARK_RED)
            } else {
                egui::Button::new(" S ")
            };
            let d_style = if self.is_key_held(egui::Key::D) {
                egui::Button::new(RichText::new("[D]").strong()).fill(Color32::DARK_GREEN)
            } else {
                egui::Button::new(" D ")
            };
            
            ui.add(a_style);
            ui.add(s_style);
            ui.add(d_style);
            
            ui.add_space(20.0);
            
            if self.state.brake {
                ui.label(RichText::new("🛑 BRAKE").color(Color32::RED).strong());
            }
        });

        ui.add_space(10.0);

        // Throttle gauge
        ui.horizontal(|ui| {
            ui.label("Throttle:");
            let progress = (self.state.throttle.abs() / 100.0).min(1.0);
            let color = if self.state.throttle >= 0.0 { Color32::GREEN } else { Color32::YELLOW };
            ui.add(egui::ProgressBar::new(progress).fill(color).text(format!("{:+.0}%", self.state.throttle)));
        });

        // Steering gauge
        ui.horizontal(|ui| {
            ui.label("Steering:");
            // Center-based gauge
            let normalized = (self.state.steering + 100.0) / 200.0;
            ui.add(egui::ProgressBar::new(normalized).text(format!("{:+.0}", self.state.steering)));
        });

        // Speed
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
        });

        ui.add_space(10.0);
        ui.separator();
        
        ui.horizontal(|ui| {
            if ui.button("🛑 E-STOP").clicked() {
                // Handle in update
            }
            if ui.button("↺ Reset").clicked() {
                // Handle in update
            }
        });

        ui.label(RichText::new("W=Accel S=Brake A/D=Steer Space=Stop Q=Quit").small().weak());
    }

    fn draw_status_panel(&self, ui: &mut egui::Ui) {
        ui.heading("📊 Status");
        ui.separator();

        // Connection status
        ui.horizontal(|ui| {
            ui.label("ESP32:");
            if self.state.connected {
                ui.label(RichText::new("✓ Connected").color(Color32::GREEN));
                ui.label(RichText::new(&self.serial.port_name).weak());
            } else {
                ui.label(RichText::new("✗ Disconnected").color(Color32::RED));
            }
        });

        ui.horizontal(|ui| {
            ui.label("Mode:");
            if self.state.auto_mode {
                ui.label(RichText::new("🤖 AUTO").color(Color32::from_rgb(0, 200, 255)));
            } else {
                ui.label(RichText::new("👤 MANUAL").color(Color32::YELLOW));
            }
        });

        ui.add_space(5.0);
        ui.separator();
        ui.label("Log:");

        egui::ScrollArea::vertical()
            .max_height(100.0)
            .stick_to_bottom(true)
            .show(ui, |ui| {
                for log in &self.logs {
                    ui.label(RichText::new(log).small().monospace());
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
                std::process::exit(0);
            }
            
            if i.key_pressed(egui::Key::Escape) {
                self.state.throttle = 0.0;
                self.state.steering = 0.0;
                self.state.brake = true;
                self.log("EMERGENCY STOP");
            }
            
            if i.key_pressed(egui::Key::R) {
                self.state = VehicleState {
                    connected: self.state.connected,
                    ..Default::default()
                };
                self.log("Reset");
            }
        });

        // Update vehicle
        if !self.state.auto_mode {
            self.update_controls();
        }
        self.send_command();

        // Main UI
        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading(RichText::new("🚗 EV Prototype Control Center").strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label("Texas A&M FLiNT - Team Autopilot");
                });
            });
        });

        egui::SidePanel::right("right_panel")
            .min_width(250.0)
            .show(ctx, |ui| {
                self.draw_controls_panel(ui);
                ui.add_space(10.0);
                self.draw_status_panel(ui);
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical(|ui| {
                // Camera takes top portion
                egui::Frame::dark_canvas(ui.style())
                    .show(ui, |ui| {
                        self.draw_camera_panel(ui);
                    });
                
                ui.add_space(5.0);
                
                // Map takes bottom portion
                egui::Frame::dark_canvas(ui.style())
                    .show(ui, |ui| {
                        self.draw_map_panel(ui, ctx);
                    });
            });
        });

        // Request continuous repaints for smooth animation
        ctx.request_repaint_after(Duration::from_millis(33));
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.camera.stop();
        self.serial.send(0, 0, true); // Emergency stop
    }
}

// ============================================================================
// Main
// ============================================================================

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([900.0, 650.0])
            .with_min_inner_size([700.0, 500.0])
            .with_title("EV Prototype Control Center"),
        ..Default::default()
    };

    eframe::run_native(
        "EV Control",
        options,
        Box::new(|cc| Ok(Box::new(EVControlApp::new(cc)))),
    )
}
