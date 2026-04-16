//! EV Prototype Control Center - Windows GUI
//! Texas A&M FLiNT - Team Autopilot
//!
//! Full-featured GUI with camera feed, real map display, vehicle controls, and FSD.

// Hide console window on Windows - must use both approaches for reliability
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;
use egui::{Color32, RichText, Vec2, Rect, Pos2, Stroke, FontId};
use image::{DynamicImage, RgbImage};
use serde::Serialize;
use serialport::SerialPort;
use std::collections::HashMap;
use std::io::{Read, Write};
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

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum CameraPosition {
    Front,
    Back,
    Left,
    Right,
}

impl CameraPosition {
    fn label(&self) -> &'static str {
        match self {
            Self::Front => "Front",
            Self::Back => "Back",
            Self::Left => "Left",
            Self::Right => "Right",
        }
    }
    
    fn arrow(&self) -> &'static str {
        match self {
            Self::Front => "▲",
            Self::Back => "▼",
            Self::Left => "◀",
            Self::Right => "▶",
        }
    }
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
    active_camera: usize,
    // Real GPS position (saved when entering SIM mode)
    real_lat: f64,
    real_lon: f64,
    // Camera assignments for FSD
    camera_assignments: HashMap<CameraPosition, usize>,
    // Navigation target
    nav_target: Option<(f64, f64)>,
    nav_active: bool,
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
            sim_mode: false, // Default to GPS mode
            camera_count: 0,
            active_camera: 0,
            real_lat: 30.6187,
            real_lon: -96.3365,
            camera_assignments: HashMap::new(),
            nav_target: None,
            nav_active: false,
        }
    }
}

#[derive(Clone)]
struct CameraFrame {
    data: Vec<u8>,
    width: u32,
    height: u32,
}

// ============================================================================
// FSD Navigation System - Prioritizes sidewalks and bike lanes
// ============================================================================

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum PathType {
    Sidewalk,      // Best - primary path type
    Cycleway,      // Great - bike lanes
    SharedPath,    // Good - mixed use paths
    Crossing,      // OK - road crossings
    Road,          // Penalty - only when necessary
}

impl PathType {
    fn cost_multiplier(&self) -> f64 {
        match self {
            Self::Sidewalk => 1.0,    // Preferred
            Self::Cycleway => 1.1,    // Slightly less preferred
            Self::SharedPath => 1.2,  // Still good
            Self::Crossing => 2.0,    // Acceptable for crossing
            Self::Road => 5.0,        // Heavy penalty
        }
    }
    
    fn color(&self) -> Color32 {
        match self {
            Self::Sidewalk => Color32::from_rgb(100, 200, 100),   // Green
            Self::Cycleway => Color32::from_rgb(100, 150, 255),   // Blue
            Self::SharedPath => Color32::from_rgb(200, 200, 100), // Yellow
            Self::Crossing => Color32::from_rgb(255, 150, 50),    // Orange
            Self::Road => Color32::from_rgb(200, 80, 80),         // Red
        }
    }
}

#[derive(Clone, Debug)]
struct NavNode {
    id: u64,
    lat: f64,
    lon: f64,
}

#[derive(Clone, Debug)]
struct NavEdge {
    from: u64,
    to: u64,
    path_type: PathType,
    distance: f64,
}

#[derive(Clone, Debug)]
struct NavRoute {
    waypoints: Vec<(f64, f64)>,
    path_types: Vec<PathType>,
    total_distance: f64,
    current_index: usize,
}

struct NavigationSystem {
    nodes: HashMap<u64, NavNode>,
    edges: Vec<NavEdge>,
    adjacency: HashMap<u64, Vec<usize>>, // node_id -> edge indices
    route: Option<NavRoute>,
    destination_name: String,
    search_query: String,
    search_results: Vec<(String, f64, f64)>, // name, lat, lon
    last_fetch: Option<Instant>,
    fetch_radius: f64, // in degrees
    fetch_center: Option<(f64, f64)>,
}

impl NavigationSystem {
    fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            edges: Vec::new(),
            adjacency: HashMap::new(),
            route: None,
            destination_name: String::new(),
            search_query: String::new(),
            search_results: Vec::new(),
            last_fetch: None,
            fetch_radius: 0.01, // ~1km
            fetch_center: None,
        }
    }
    
    fn geocode_search(&mut self, query: &str, current_lat: f64, current_lon: f64) -> Vec<(String, f64, f64)> {
        // Use Nominatim for geocoding with location bias
        let encoded = query.replace(' ', "+");
        // Use viewbox to bias results toward current location (~50km radius)
        let bias = 0.5; // degrees, roughly 50km
        let url = format!(
            "https://nominatim.openstreetmap.org/search?q={}&format=json&limit=10&viewbox={},{},{},{}&bounded=0",
            encoded,
            current_lon - bias, current_lat + bias,
            current_lon + bias, current_lat - bias
        );
        
        let client = reqwest::blocking::Client::new();
        if let Ok(response) = client
            .get(&url)
            .header("User-Agent", "EV-Prototype-FSD/1.5")
            .timeout(Duration::from_secs(5))
            .send()
        {
            if let Ok(json) = response.json::<serde_json::Value>() {
                if let Some(results) = json.as_array() {
                    let mut parsed: Vec<(String, f64, f64, f64)> = results.iter().filter_map(|r| {
                        let name = r["display_name"].as_str()?.to_string();
                        let lat: f64 = r["lat"].as_str()?.parse().ok()?;
                        let lon: f64 = r["lon"].as_str()?.parse().ok()?;
                        let dist = haversine_distance(current_lat, current_lon, lat, lon);
                        Some((name, lat, lon, dist))
                    }).collect();
                    
                    // Sort by distance (closest first)
                    parsed.sort_by(|a, b| a.3.partial_cmp(&b.3).unwrap_or(std::cmp::Ordering::Equal));
                    
                    // Return top 5, without distance field
                    return parsed.into_iter()
                        .take(5)
                        .map(|(name, lat, lon, _)| (name, lat, lon))
                        .collect();
                }
            }
        }
        Vec::new()
    }
    
    fn fetch_paths_around(&mut self, lat: f64, lon: f64) {
        // Check if we need to refetch
        if let Some(center) = self.fetch_center {
            let dist = ((lat - center.0).powi(2) + (lon - center.1).powi(2)).sqrt();
            if dist < self.fetch_radius * 0.5 {
                return; // Still within cached area
            }
        }
        
        // Rate limit
        if let Some(last) = self.last_fetch {
            if last.elapsed() < Duration::from_secs(10) {
                return;
            }
        }
        
        // Overpass query for sidewalks, bike lanes, paths, and crossings
        let bbox = format!("{},{},{},{}", 
            lat - self.fetch_radius, 
            lon - self.fetch_radius,
            lat + self.fetch_radius,
            lon + self.fetch_radius
        );
        
        let query = format!(r#"
            [out:json][timeout:25];
            (
              way["highway"="footway"]({bbox});
              way["highway"="sidewalk"]({bbox});
              way["highway"="cycleway"]({bbox});
              way["highway"="path"]({bbox});
              way["highway"="pedestrian"]({bbox});
              way["highway"="crossing"]({bbox});
              way["footway"="crossing"]({bbox});
              way["sidewalk"]["sidewalk"!="no"]({bbox});
              way["highway"="residential"]({bbox});
              way["highway"="tertiary"]({bbox});
            );
            out body;
            >;
            out skel qt;
        "#, bbox = bbox);
        
        let url = "https://overpass-api.de/api/interpreter";
        let client = reqwest::blocking::Client::new();
        
        if let Ok(response) = client
            .post(url)
            .header("User-Agent", "EV-Prototype-FSD/1.5")
            .timeout(Duration::from_secs(30))
            .body(query)
            .send()
        {
            if let Ok(json) = response.json::<serde_json::Value>() {
                self.parse_overpass_response(&json);
                self.fetch_center = Some((lat, lon));
                self.last_fetch = Some(Instant::now());
            }
        }
    }
    
    fn parse_overpass_response(&mut self, json: &serde_json::Value) {
        self.nodes.clear();
        self.edges.clear();
        self.adjacency.clear();
        
        if let Some(elements) = json["elements"].as_array() {
            // First pass: collect all nodes
            for elem in elements {
                if elem["type"].as_str() == Some("node") {
                    if let (Some(id), Some(lat), Some(lon)) = (
                        elem["id"].as_u64(),
                        elem["lat"].as_f64(),
                        elem["lon"].as_f64(),
                    ) {
                        self.nodes.insert(id, NavNode { id, lat, lon });
                    }
                }
            }
            
            // Second pass: create edges from ways
            for elem in elements {
                if elem["type"].as_str() == Some("way") {
                    let tags = &elem["tags"];
                    let path_type = self.classify_way(tags);
                    
                    if let Some(nodes) = elem["nodes"].as_array() {
                        let node_ids: Vec<u64> = nodes.iter()
                            .filter_map(|n| n.as_u64())
                            .collect();
                        
                        for window in node_ids.windows(2) {
                            if let [from, to] = window {
                                if let (Some(n1), Some(n2)) = (self.nodes.get(from), self.nodes.get(to)) {
                                    let distance = haversine_distance(n1.lat, n1.lon, n2.lat, n2.lon);
                                    let edge_idx = self.edges.len();
                                    
                                    self.edges.push(NavEdge {
                                        from: *from,
                                        to: *to,
                                        path_type,
                                        distance,
                                    });
                                    
                                    self.adjacency.entry(*from).or_default().push(edge_idx);
                                    
                                    // Add reverse edge (paths are bidirectional)
                                    let rev_idx = self.edges.len();
                                    self.edges.push(NavEdge {
                                        from: *to,
                                        to: *from,
                                        path_type,
                                        distance,
                                    });
                                    self.adjacency.entry(*to).or_default().push(rev_idx);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    
    fn classify_way(&self, tags: &serde_json::Value) -> PathType {
        let highway = tags["highway"].as_str().unwrap_or("");
        let footway = tags["footway"].as_str().unwrap_or("");
        let sidewalk = tags["sidewalk"].as_str().unwrap_or("");
        
        // Priority order for microtransport
        if highway == "footway" || highway == "sidewalk" || highway == "pedestrian" {
            if footway == "crossing" {
                PathType::Crossing
            } else {
                PathType::Sidewalk
            }
        } else if highway == "cycleway" {
            PathType::Cycleway
        } else if highway == "path" {
            PathType::SharedPath
        } else if highway == "crossing" || footway == "crossing" {
            PathType::Crossing
        } else if sidewalk == "both" || sidewalk == "left" || sidewalk == "right" {
            PathType::Sidewalk // Road with sidewalk - use sidewalk
        } else {
            PathType::Road // Fallback to road
        }
    }
    
    fn find_nearest_node(&self, lat: f64, lon: f64) -> Option<u64> {
        self.nodes.values()
            .min_by(|a, b| {
                let da = (a.lat - lat).powi(2) + (a.lon - lon).powi(2);
                let db = (b.lat - lat).powi(2) + (b.lon - lon).powi(2);
                da.partial_cmp(&db).unwrap()
            })
            .map(|n| n.id)
    }
    
    fn calculate_route(&mut self, from_lat: f64, from_lon: f64, to_lat: f64, to_lon: f64) -> bool {
        // Ensure we have path data
        self.fetch_paths_around((from_lat + to_lat) / 2.0, (from_lon + to_lon) / 2.0);
        
        let start = match self.find_nearest_node(from_lat, from_lon) {
            Some(id) => id,
            None => return false,
        };
        
        let goal = match self.find_nearest_node(to_lat, to_lon) {
            Some(id) => id,
            None => return false,
        };
        
        // A* pathfinding with path type cost weighting
        let result = pathfinding::directed::astar::astar(
            &start,
            |&node| {
                self.adjacency.get(&node)
                    .map(|edges| {
                        edges.iter().map(|&idx| {
                            let edge = &self.edges[idx];
                            let cost = (edge.distance * edge.path_type.cost_multiplier() * 1000.0) as u32;
                            (edge.to, cost)
                        }).collect::<Vec<_>>()
                    })
                    .unwrap_or_default()
            },
            |&node| {
                self.nodes.get(&node)
                    .map(|n| (haversine_distance(n.lat, n.lon, to_lat, to_lon) * 1000.0) as u32)
                    .unwrap_or(0)
            },
            |&node| node == goal,
        );
        
        if let Some((path, _cost)) = result {
            let mut waypoints = Vec::new();
            let mut path_types = Vec::new();
            let mut total_distance = 0.0;
            
            for window in path.windows(2) {
                if let [from, to] = window {
                    if let Some(node) = self.nodes.get(from) {
                        waypoints.push((node.lat, node.lon));
                    }
                    
                    // Find edge type
                    if let Some(edges) = self.adjacency.get(from) {
                        for &idx in edges {
                            let edge = &self.edges[idx];
                            if edge.to == *to {
                                path_types.push(edge.path_type);
                                total_distance += edge.distance;
                                break;
                            }
                        }
                    }
                }
            }
            
            // Add final waypoint
            if let Some(&last) = path.last() {
                if let Some(node) = self.nodes.get(&last) {
                    waypoints.push((node.lat, node.lon));
                }
            }
            
            self.route = Some(NavRoute {
                waypoints,
                path_types,
                total_distance,
                current_index: 0,
            });
            
            true
        } else {
            false
        }
    }
    
    fn get_steering_to_next_waypoint(&self, current_lat: f64, current_lon: f64, current_heading: f32) -> Option<f32> {
        let route = self.route.as_ref()?;
        
        if route.current_index >= route.waypoints.len() {
            return None; // Arrived
        }
        
        let (target_lat, target_lon) = route.waypoints[route.current_index];
        
        // Calculate bearing to target
        let target_bearing = calculate_bearing(current_lat, current_lon, target_lat, target_lon);
        
        // Calculate steering needed
        let mut angle_diff = target_bearing - current_heading;
        
        // Normalize to -180 to 180
        while angle_diff > 180.0 { angle_diff -= 360.0; }
        while angle_diff < -180.0 { angle_diff += 360.0; }
        
        // Convert to steering value (-100 to 100)
        let steering = (angle_diff / 45.0 * 100.0).clamp(-100.0, 100.0);
        
        Some(steering)
    }
    
    fn update_progress(&mut self, current_lat: f64, current_lon: f64) -> bool {
        if let Some(ref mut route) = self.route {
            if route.current_index >= route.waypoints.len() {
                return true; // Already arrived
            }
            
            let (wp_lat, wp_lon) = route.waypoints[route.current_index];
            let dist = haversine_distance(current_lat, current_lon, wp_lat, wp_lon);
            
            // If within 15 meters, advance to next waypoint (more forgiving)
            if dist < 0.015 { // ~15m in km
                route.current_index += 1;
                if route.current_index >= route.waypoints.len() {
                    return true; // Arrived!
                }
            }
        }
        false
    }
}

fn haversine_distance(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let r = 6371.0; // Earth radius in km
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let a = (dlat / 2.0).sin().powi(2) 
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().asin();
    r * c
}

fn calculate_bearing(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f32 {
    let lat1_rad = lat1.to_radians();
    let lat2_rad = lat2.to_radians();
    let dlon = (lon2 - lon1).to_radians();
    
    // Standard bearing formula
    let y = dlon.sin() * lat2_rad.cos();
    let x = lat1_rad.cos() * lat2_rad.sin() - lat1_rad.sin() * lat2_rad.cos() * dlon.cos();
    
    let bearing = y.atan2(x).to_degrees();
    ((bearing + 360.0) % 360.0) as f32
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
        
        // Calculate how many tiles we need in each direction
        // Add extra buffer to prevent edge loading issues
        let tiles_x = ((width as i32 / 256) / 2 + 2) as i32;
        let tiles_y = ((height as i32 / 256) / 2 + 2) as i32;

        // Render grid of tiles centered on current position
        for dy in -tiles_y..=tiles_y {
            for dx in -tiles_x..=tiles_x {
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
        
        // Pre-fetch additional outer ring for smooth scrolling
        let prefetch_range = tiles_x.max(tiles_y) + 1;
        for dy in -prefetch_range..=prefetch_range {
            for dx in -prefetch_range..=prefetch_range {
                if dx.abs() <= tiles_x && dy.abs() <= tiles_y {
                    continue; // Skip already-fetched tiles
                }
                let ttx = (tile_ix as i32 + dx) as u32;
                let tty = (tile_iy as i32 + dy) as u32;
                let _ = self.fetch_tile(ttx, tty); // Pre-cache
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
    firmware_version: Option<String>,
}

// Expected firmware version - update when ESP32 code changes
const EXPECTED_FIRMWARE_VERSION: &str = "1.5.8";

impl SerialController {
    fn new() -> Self {
        Self {
            port: None,
            port_name: String::new(),
            firmware_version: None,
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
                
                // Check firmware version
                self.check_firmware_version(logs);
                
                logs.push(format!("[{}] ESP32 ready", timestamp()));
                true
            }
            Err(e) => {
                logs.push(format!("[{}] Connection failed: {}", timestamp(), e));
                false
            }
        }
    }
    
    fn check_firmware_version(&mut self, logs: &mut Vec<String>) {
        if let Some(ref mut port) = self.port {
            // Request version
            let _ = port.write_all(b"{\"v\":true}\n");
            std::thread::sleep(Duration::from_millis(200));
            
            // Read response
            let mut buf = [0u8; 256];
            if let Ok(n) = port.read(&mut buf) {
                let response = String::from_utf8_lossy(&buf[..n]);
                for line in response.lines() {
                    if let Some(version) = line.strip_prefix("VERSION:") {
                        let ver = version.trim().to_string();
                        logs.push(format!("[{}] ESP32 firmware: v{}", timestamp(), ver));
                        
                        if ver != EXPECTED_FIRMWARE_VERSION {
                            logs.push(format!("[{}] ⚠️ FIRMWARE MISMATCH! Expected v{}, got v{}", 
                                timestamp(), EXPECTED_FIRMWARE_VERSION, ver));
                            logs.push(format!("[{}] Please update ESP32 firmware", timestamp()));
                        } else {
                            logs.push(format!("[{}] ✓ Firmware version OK", timestamp()));
                        }
                        
                        self.firmware_version = Some(ver);
                        return;
                    }
                }
            }
            logs.push(format!("[{}] Could not read firmware version (may be old firmware)", timestamp()));
        }
    }
    
    fn firmware_ok(&self) -> bool {
        self.firmware_version.as_ref().map(|v| v == EXPECTED_FIRMWARE_VERSION).unwrap_or(false)
    }
    
    fn disconnect(&mut self) {
        self.port = None;
        self.firmware_version = None;
    }
    
    fn flash_firmware(&mut self, logs: &mut Vec<String>) -> bool {
        // Only flash if we have a port name (even if disconnected for flashing)
        if self.port_name.is_empty() {
            logs.push(format!("[{}] No ESP32 port known - connect first", timestamp()));
            return false;
        }
        
        // Close port before flashing
        self.port = None;
        logs.push(format!("[{}] Downloading firmware v{}...", timestamp(), EXPECTED_FIRMWARE_VERSION));
        
        // Download firmware from GitHub releases
        let firmware_url = format!(
            "https://github.com/JoshuaCHolmes/ev-prototype/releases/download/v{}/firmware-esp32.bin",
            EXPECTED_FIRMWARE_VERSION
        );
        
        let cache_dir = directories::ProjectDirs::from("edu", "tamu", "ev-prototype")
            .map(|d| d.cache_dir().to_path_buf())
            .unwrap_or_else(|| std::env::temp_dir());
        let _ = std::fs::create_dir_all(&cache_dir);
        let firmware_path = cache_dir.join("firmware-esp32.bin");
        
        // Download firmware
        let client = reqwest::blocking::Client::new();
        match client.get(&firmware_url)
            .header("User-Agent", "EV-Prototype-GUI")
            .send()
        {
            Ok(response) if response.status().is_success() => {
                match response.bytes() {
                    Ok(bytes) => {
                        if let Err(e) = std::fs::write(&firmware_path, &bytes) {
                            logs.push(format!("[{}] Failed to save firmware: {}", timestamp(), e));
                            return false;
                        }
                        logs.push(format!("[{}] Downloaded {} bytes", timestamp(), bytes.len()));
                    }
                    Err(e) => {
                        logs.push(format!("[{}] Download failed: {}", timestamp(), e));
                        return false;
                    }
                }
            }
            Ok(response) => {
                logs.push(format!("[{}] Download failed: HTTP {}", timestamp(), response.status()));
                return false;
            }
            Err(e) => {
                logs.push(format!("[{}] Download failed: {}", timestamp(), e));
                return false;
            }
        }
        
        logs.push(format!("[{}] Flashing to {}...", timestamp(), self.port_name));
        
        // Try esptool via Python
        let flash_result = std::process::Command::new("python")
            .args([
                "-m", "esptool",
                "--chip", "esp32",
                "--port", &self.port_name,
                "--baud", "921600",
                "write_flash", "0x10000",
                firmware_path.to_str().unwrap_or("firmware.bin")
            ])
            .output();
        
        match flash_result {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                
                if output.status.success() || stdout.contains("Hash of data verified") {
                    logs.push(format!("[{}] ✓ Flash complete!", timestamp()));
                    logs.push(format!("[{}] Reconnecting...", timestamp()));
                    std::thread::sleep(Duration::from_secs(2));
                    return self.connect(logs);
                } else {
                    logs.push(format!("[{}] Flash failed: {}", timestamp(), stderr.lines().next().unwrap_or("unknown error")));
                    // Try with python3
                    let retry = std::process::Command::new("python3")
                        .args(["-m", "esptool", "--chip", "esp32", "--port", &self.port_name, 
                               "--baud", "921600", "write_flash", "0x10000", 
                               firmware_path.to_str().unwrap_or("firmware.bin")])
                        .output();
                    if let Ok(out) = retry {
                        if out.status.success() {
                            logs.push(format!("[{}] ✓ Flash complete!", timestamp()));
                            std::thread::sleep(Duration::from_secs(2));
                            return self.connect(logs);
                        }
                    }
                }
            }
            Err(e) => {
                logs.push(format!("[{}] esptool not found: {}", timestamp(), e));
                logs.push(format!("[{}] Install with: pip install esptool", timestamp()));
            }
        }
        
        false
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
// Camera Handler using escapi (Windows DirectShow)
// ============================================================================

struct CameraHandler {
    frame: Arc<Mutex<Option<CameraFrame>>>,
    running: Arc<Mutex<bool>>,
    camera_count: Arc<Mutex<usize>>,
    active_index: Arc<Mutex<usize>>,
}

impl CameraHandler {
    fn new() -> Self {
        Self {
            frame: Arc::new(Mutex::new(None)),
            running: Arc::new(Mutex::new(false)),
            camera_count: Arc::new(Mutex::new(0)),
            active_index: Arc::new(Mutex::new(0)),
        }
    }

    #[cfg(windows)]
    fn start(&self, logs: Arc<Mutex<Vec<String>>>, camera_index: usize) {
        // Stop any existing capture
        self.stop();
        std::thread::sleep(Duration::from_millis(100));
        
        let frame = self.frame.clone();
        let running = self.running.clone();
        let camera_count = self.camera_count.clone();
        let active_index = self.active_index.clone();
        *running.lock().unwrap() = true;
        *active_index.lock().unwrap() = camera_index;

        std::thread::spawn(move || {
            // Count available devices
            let device_count = escapi::num_devices();
            *camera_count.lock().unwrap() = device_count;
            
            if let Ok(mut l) = logs.lock() {
                l.push(format!("[{}] ESCAPI: {} camera(s) detected", timestamp(), device_count));
            }
            
            if device_count == 0 {
                if let Ok(mut l) = logs.lock() {
                    l.push(format!("[{}] No cameras available", timestamp()));
                }
                return;
            }
            
            // Clamp camera index to valid range
            let idx = camera_index.min(device_count.saturating_sub(1));
            
            let width: u32 = 320;
            let height: u32 = 240;
            let fps: u64 = 30;
            
            match escapi::init(idx, width, height, fps) {
                Ok(camera) => {
                    let name = camera.name();
                    if let Ok(mut l) = logs.lock() {
                        l.push(format!("[{}] Camera {}: {}", timestamp(), idx, name));
                    }
                    
                    while *running.lock().unwrap() {
                        if let Ok(pixels) = camera.capture() {
                            // escapi returns BGRA, convert to RGB
                            let mut rgb_data = Vec::with_capacity((width * height * 3) as usize);
                            for chunk in pixels.chunks(4) {
                                if chunk.len() >= 4 {
                                    rgb_data.push(chunk[2]); // R
                                    rgb_data.push(chunk[1]); // G
                                    rgb_data.push(chunk[0]); // B
                                }
                            }
                            
                            if let Ok(mut f) = frame.lock() {
                                *f = Some(CameraFrame {
                                    data: rgb_data,
                                    width,
                                    height,
                                });
                            }
                        }
                        std::thread::sleep(Duration::from_millis(33));
                    }
                }
                Err(e) => {
                    if let Ok(mut l) = logs.lock() {
                        l.push(format!("[{}] Failed to open camera {}: {}", timestamp(), idx, e));
                    }
                }
            }
        });
    }

    #[cfg(not(windows))]
    fn start(&self, logs: Arc<Mutex<Vec<String>>>, _camera_index: usize) {
        if let Ok(mut l) = logs.lock() {
            l.push(format!("[{}] Camera support is Windows-only", timestamp()));
        }
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
    reconnect_all: bool,
    switch_camera: bool,
    toggle_sim: bool,
    flash_firmware: bool,
    // FSD Navigation
    nav: NavigationSystem,
    nav_search_open: bool,
}

impl EVControlApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let logs = Arc::new(Mutex::new(Vec::new()));
        
        {
            let mut l = logs.lock().unwrap();
            l.push(format!("[{}] ═══════════════════════════════════════", timestamp()));
            l.push(format!("[{}] EV Prototype Control Center v1.5.8", timestamp()));
            l.push(format!("[{}] Texas A&M FLiNT - Team Autopilot", timestamp()));
            l.push(format!("[{}] Microtransport FSD - Sidewalk Priority", timestamp()));
            l.push(format!("[{}] ═══════════════════════════════════════", timestamp()));
        }
        
        let mut serial = SerialController::new();
        let connected = {
            let mut l = logs.lock().unwrap();
            serial.find_and_connect(&mut l)
        };
        
        let camera = CameraHandler::new();
        camera.start(logs.clone(), 0);

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
            reconnect_all: false,
            switch_camera: false,
            toggle_sim: false,
            flash_firmware: false,
            nav: NavigationSystem::new(),
            nav_search_open: false,
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

        // Speed estimate
        self.state.speed_estimate = self.state.throttle.abs() * 0.3;
        
        // Only update position in manual mode (FSD handles its own position updates)
        if self.state.sim_mode && !self.state.auto_mode && self.state.throttle.abs() > 0.0 && !self.state.brake {
            self.update_sim_position();
        }
        
        self.state.camera_count = self.camera.get_camera_count();
    }
    
    fn update_sim_position(&mut self) {
        // ~0.000005 degrees/frame at full throttle ≈ ~0.5m/frame at 60fps ≈ 30m/s max
        let speed_deg = (self.state.throttle.abs() / 100.0) as f64 * 0.000005;
        
        // Move in heading direction (heading 0 = North = +lat, 90 = East = +lon)
        let heading_rad = (self.state.heading as f64).to_radians();
        self.state.lat += speed_deg * heading_rad.cos();
        // Adjust longitude for latitude (degrees get smaller near poles)
        self.state.lon += speed_deg * heading_rad.sin() / self.state.lat.to_radians().cos().abs().max(0.1);
        
        // Turn rate: at full steering (100), turn ~0.5 degrees per frame = 30°/sec at 60fps
        let turn_rate = self.state.steering * 0.005;
        self.state.heading = (self.state.heading + turn_rate) % 360.0;
        if self.state.heading < 0.0 {
            self.state.heading += 360.0;
        }
    }
    
    fn toggle_sim_mode(&mut self) {
        if self.state.sim_mode {
            // Leaving SIM mode - snap back to real position
            self.state.lat = self.state.real_lat;
            self.state.lon = self.state.real_lon;
            self.state.sim_mode = false;
            self.log("GPS mode - snapped to real position");
        } else {
            // Entering SIM mode - save current real position
            self.state.real_lat = self.state.lat;
            self.state.real_lon = self.state.lon;
            self.state.sim_mode = true;
            self.log("SIM mode - position can move freely");
        }
    }
    
    fn update_fsd(&mut self) {
        // FSD needs both auto_mode and nav_active with a route
        if !self.state.auto_mode {
            return;
        }
        
        // If no active navigation, just idle (don't move randomly)
        if !self.state.nav_active || self.nav.route.is_none() {
            self.state.speed_estimate = 0.0;
            return;
        }
        
        // Check if we've arrived
        if self.nav.update_progress(self.state.lat, self.state.lon) {
            self.state.throttle = 0.0;
            self.state.steering = 0.0;
            self.state.brake = true;
            self.state.auto_mode = false;
            self.state.nav_active = false;
            self.log("🎉 Arrived at destination!");
            return;
        }
        
        // Get steering direction to next waypoint
        if let Some(target_steering) = self.nav.get_steering_to_next_waypoint(
            self.state.lat, 
            self.state.lon, 
            self.state.heading
        ) {
            // Proportional steering - don't overshoot
            // If we need to turn 45°, steer at 100%. Less angle = less steering.
            let steering_needed = target_steering.clamp(-100.0, 100.0);
            
            // Gradually adjust steering (prevents jerky movement)
            let max_steer_change = 3.0; // Max 3% change per frame
            let steering_diff = steering_needed - self.state.steering;
            self.state.steering += steering_diff.clamp(-max_steer_change, max_steer_change);
            self.state.steering = self.state.steering.clamp(-100.0, 100.0);
            
            // Speed based on how much we need to turn (slow down for sharp turns)
            let turn_severity = (self.state.steering.abs() / 100.0); // 0.0 to 1.0
            let target_speed = 40.0 * (1.0 - turn_severity * 0.7); // 40% max, down to 12% for sharp turns
            
            // Gradually adjust throttle
            if self.state.throttle < target_speed {
                self.state.throttle = (self.state.throttle + 1.5).min(target_speed);
            } else {
                self.state.throttle = (self.state.throttle - 0.5).max(target_speed);
            }
            
            self.state.brake = false;
            
            // In GPS mode with cameras, we would add obstacle detection here
            if !self.state.sim_mode {
                self.check_camera_obstacles();
            }
        } else {
            // No waypoint to steer to - might need to recalculate route
            self.state.throttle = 0.0;
            self.log("FSD: No path - recalculating...");
            if let Some((dest_lat, dest_lon)) = self.state.nav_target {
                if !self.nav.calculate_route(self.state.lat, self.state.lon, dest_lat, dest_lon) {
                    self.log("Could not find route");
                    self.state.nav_active = false;
                }
            }
            return;
        }
        
        // Update position in SIM mode
        if self.state.sim_mode && self.state.throttle > 0.0 && !self.state.brake {
            self.update_sim_position();
        }
        
        self.state.speed_estimate = self.state.throttle.abs() * 0.3;
    }
    
    fn check_camera_obstacles(&mut self) {
        // Basic obstacle detection using camera brightness analysis
        // In a real implementation, this would use ML/CV for object detection
        
        if let Some(ref frame) = self.camera.get_frame() {
            // Check front camera if assigned
            if let Some(&front_idx) = self.state.camera_assignments.get(&CameraPosition::Front) {
                if front_idx == self.state.active_camera {
                    // Simple brightness check in center of frame (crude obstacle detection)
                    let center_y = frame.height / 2;
                    let center_x = frame.width / 2;
                    let sample_size = 20;
                    
                    let mut dark_pixels = 0;
                    for dy in 0..sample_size {
                        for dx in 0..sample_size {
                            let x = (center_x as i32 - sample_size as i32 / 2 + dx as i32) as u32;
                            let y = (center_y as i32 + dy as i32) as u32; // Look ahead/down
                            if x < frame.width && y < frame.height {
                                let idx = ((y * frame.width + x) * 3) as usize;
                                if idx + 2 < frame.data.len() {
                                    let brightness = (frame.data[idx] as u32 + 
                                                     frame.data[idx + 1] as u32 + 
                                                     frame.data[idx + 2] as u32) / 3;
                                    if brightness < 50 {
                                        dark_pixels += 1;
                                    }
                                }
                            }
                        }
                    }
                    
                    // If too many dark pixels (potential obstacle), slow down
                    let dark_ratio = dark_pixels as f32 / (sample_size * sample_size) as f32;
                    if dark_ratio > 0.5 {
                        self.state.throttle *= 0.5;
                        // self.log("Obstacle detected - slowing");
                    }
                }
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
        ui.horizontal(|ui| {
            ui.heading("📷 Camera");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let count = self.state.camera_count;
                if count > 0 {
                    ui.label(RichText::new(format!("{} found", count)).color(Color32::GREEN).small());
                } else {
                    ui.label(RichText::new("None").color(Color32::RED).small());
                }
            });
        });
        ui.separator();

        let available = ui.available_size();
        let (response, painter) = ui.allocate_painter(available, egui::Sense::hover());
        let rect = response.rect;

        if let Some(frame) = self.camera.get_frame() {
            // ASCII-style rendering with color
            let chars = [' ', '░', '▒', '▓', '█'];
            let char_w = 6.0;
            let char_h = 10.0;
            let cols = (rect.width() / char_w) as usize;
            let rows = (rect.height() / char_h) as usize;

            for row in 0..rows {
                for col in 0..cols {
                    let src_x = col * frame.width as usize / cols.max(1);
                    let src_y = row * frame.height as usize / rows.max(1);
                    let idx = (src_y * frame.width as usize + src_x) * 3;
                    
                    if idx + 2 < frame.data.len() {
                        let r = frame.data[idx];
                        let g = frame.data[idx + 1];
                        let b = frame.data[idx + 2];
                        let brightness = ((r as u32 + g as u32 + b as u32) / 3) as usize;
                        let char_idx = (brightness * chars.len() / 256).min(chars.len() - 1);
                        
                        painter.text(
                            Pos2::new(rect.min.x + col as f32 * char_w, rect.min.y + row as f32 * char_h),
                            egui::Align2::LEFT_TOP,
                            chars[char_idx],
                            FontId::monospace(10.0),
                            Color32::from_rgb(r, g, b),
                        );
                    }
                }
            }
        } else {
            painter.rect_filled(rect, 4.0, Color32::from_gray(30));
            let msg = if self.state.camera_count > 0 {
                format!("{} camera(s) detected\nInitializing...", self.state.camera_count)
            } else {
                "No Camera Feed\n\nConnect USB camera".to_string()
            };
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                msg,
                FontId::proportional(14.0),
                Color32::GRAY,
            );
        }
    }

    fn draw_map_panel(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        // Header with search toggle
        ui.horizontal(|ui| {
            ui.heading("🗺️ Map");
            if ui.button("🔍 Navigate").clicked() {
                self.nav_search_open = !self.nav_search_open;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new(format!("{:.5}°, {:.5}°", self.state.lat, self.state.lon)).small().weak());
            });
        });
        
        // Navigation search bar
        if self.nav_search_open {
            ui.horizontal(|ui| {
                ui.label("To:");
                let response = ui.text_edit_singleline(&mut self.nav.search_query);
                if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    // Perform search - clone query to avoid borrow conflict
                    let query = self.nav.search_query.clone();
                    let (lat, lon) = (self.state.lat, self.state.lon);
                    self.nav.search_results = self.nav.geocode_search(&query, lat, lon);
                    if !self.nav.search_results.is_empty() {
                        self.log(&format!("Found {} results (nearest first)", self.nav.search_results.len()));
                    } else {
                        self.log("No results found");
                    }
                }
                if ui.button("Search").clicked() {
                    let query = self.nav.search_query.clone();
                    let (lat, lon) = (self.state.lat, self.state.lon);
                    self.nav.search_results = self.nav.geocode_search(&query, lat, lon);
                }
            });
            
            // Show search results
            if !self.nav.search_results.is_empty() {
                ui.group(|ui| {
                    for (name, lat, lon) in self.nav.search_results.clone() {
                        let short_name: String = name.chars().take(40).collect();
                        if ui.button(&short_name).clicked() {
                            self.state.nav_target = Some((lat, lon));
                            self.nav.destination_name = name.clone();
                            self.nav.search_results.clear();
                            self.nav_search_open = false;
                            
                            // Calculate route
                            if self.nav.calculate_route(self.state.lat, self.state.lon, lat, lon) {
                                self.state.nav_active = true;
                                if let Some(ref route) = self.nav.route {
                                    self.log(&format!("Route: {:.1}km via sidewalks/paths", route.total_distance));
                                }
                            } else {
                                self.log("Could not calculate route - try closer destination");
                            }
                        }
                    }
                });
            }
            
            // Show active navigation info
            if self.state.nav_active {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("🧭").color(Color32::GREEN));
                    let dest: String = self.nav.destination_name.chars().take(25).collect();
                    ui.label(RichText::new(&dest).small());
                    if ui.small_button("✕ Cancel").clicked() {
                        self.state.nav_active = false;
                        self.state.nav_target = None;
                        self.nav.route = None;
                        self.log("Navigation cancelled");
                    }
                });
            }
        }
        
        ui.separator();

        let available = ui.available_size();
        let map_size = available;

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
        
        let center = rect.center();
        let zoom = self.map_cache.zoom as f64;
        let scale = 256.0 * 2_f64.powi(zoom as i32) / 360.0; // pixels per degree (approximate)
        
        // Draw route if active
        if let Some(ref route) = self.nav.route {
            // Draw route line segments with path type coloring
            for i in 0..route.waypoints.len().saturating_sub(1) {
                let (lat1, lon1) = route.waypoints[i];
                let (lat2, lon2) = route.waypoints[i + 1];
                
                // Convert lat/lon offset from current position to screen pixels
                let dx1 = ((lon1 - self.state.lon) * scale * (self.state.lat.to_radians().cos())) as f32;
                let dy1 = ((self.state.lat - lat1) * scale) as f32;
                let dx2 = ((lon2 - self.state.lon) * scale * (self.state.lat.to_radians().cos())) as f32;
                let dy2 = ((self.state.lat - lat2) * scale) as f32;
                
                let p1 = Pos2::new(center.x + dx1, center.y + dy1);
                let p2 = Pos2::new(center.x + dx2, center.y + dy2);
                
                let color = if i < route.path_types.len() {
                    route.path_types[i].color()
                } else {
                    Color32::YELLOW
                };
                
                // Highlight current segment
                let width = if i == route.current_index { 4.0 } else { 2.0 };
                painter.line_segment([p1, p2], Stroke::new(width, color));
            }
        }
        
        // Always draw destination marker if we have a nav target (even without route)
        if let Some((dest_lat, dest_lon)) = self.state.nav_target {
            let dx = ((dest_lon - self.state.lon) * scale * (self.state.lat.to_radians().cos())) as f32;
            let dy = ((self.state.lat - dest_lat) * scale) as f32;
            let dest_pos = Pos2::new(center.x + dx, center.y + dy);
            
            // Destination marker
            painter.circle_filled(dest_pos, 10.0, Color32::from_rgb(255, 80, 80));
            painter.circle_stroke(dest_pos, 10.0, Stroke::new(2.0, Color32::WHITE));
            painter.text(dest_pos + Vec2::new(0.0, -18.0), egui::Align2::CENTER_CENTER, "🎯", FontId::proportional(14.0), Color32::WHITE);
            
            // Distance to destination
            let dist = haversine_distance(self.state.lat, self.state.lon, dest_lat, dest_lon);
            let dist_text = if dist < 1.0 {
                format!("{:.0}m", dist * 1000.0)
            } else {
                format!("{:.1}km", dist)
            };
            painter.text(
                dest_pos + Vec2::new(0.0, 18.0), 
                egui::Align2::CENTER_CENTER, 
                &dist_text, 
                FontId::proportional(10.0), 
                Color32::WHITE
            );
        }

        // Vehicle marker (always on top)
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
        
        // Path type legend if navigating
        if self.state.nav_active {
            let legend_y = rect.max.y - 60.0;
            let legend_x = rect.min.x + 10.0;
            painter.rect_filled(
                Rect::from_min_size(Pos2::new(legend_x - 5.0, legend_y - 5.0), Vec2::new(95.0, 55.0)),
                4.0,
                Color32::from_rgba_unmultiplied(0, 0, 0, 180),
            );
            for (i, (pt, label)) in [
                (PathType::Sidewalk, "Sidewalk"),
                (PathType::Cycleway, "Bike Lane"),
                (PathType::SharedPath, "Path"),
                (PathType::Road, "Road"),
            ].iter().enumerate() {
                let y = legend_y + i as f32 * 12.0;
                painter.circle_filled(Pos2::new(legend_x + 5.0, y + 4.0), 4.0, pt.color());
                painter.text(Pos2::new(legend_x + 15.0, y), egui::Align2::LEFT_TOP, *label, FontId::proportional(10.0), Color32::WHITE);
            }
        }
    }

    fn draw_controls_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("🎮 Controls");
        ui.separator();

        // Mode toggles
        ui.horizontal(|ui| {
            let sim_text = if self.state.sim_mode { "🎮 SIM" } else { "📍 GPS" };
            let sim_color = if self.state.sim_mode { Color32::from_rgb(200, 100, 255) } else { Color32::GREEN };
            if ui.add(egui::Button::new(RichText::new(sim_text).color(sim_color))).clicked() {
                self.toggle_sim = true;
            }
            
            let auto_text = if self.state.auto_mode { "🤖 AUTO" } else { "👤 MANUAL" };
            let auto_color = if self.state.auto_mode { Color32::from_rgb(0, 200, 255) } else { Color32::YELLOW };
            if ui.add(egui::Button::new(RichText::new(auto_text).color(auto_color))).clicked() {
                self.state.auto_mode = !self.state.auto_mode;
                self.log(if self.state.auto_mode { "AUTO mode" } else { "MANUAL mode" });
            }
        });
        
        // Firmware status
        ui.horizontal(|ui| {
            if self.serial.port.is_some() {
                if let Some(ref ver) = self.serial.firmware_version {
                    if self.serial.firmware_ok() {
                        ui.label(RichText::new(format!("✓ ESP32 v{}", ver)).color(Color32::GREEN).small());
                    } else {
                        ui.label(RichText::new(format!("⚠ v{}", ver)).color(Color32::YELLOW).small());
                        if ui.small_button("⚡ Flash").clicked() {
                            self.flash_firmware = true;
                        }
                    }
                } else {
                    ui.label(RichText::new("? old firmware").color(Color32::YELLOW).small());
                    if ui.small_button("⚡ Flash").clicked() {
                        self.flash_firmware = true;
                    }
                }
            } else {
                ui.label(RichText::new("⊘ ESP32 offline").color(Color32::RED).small());
            }
        });
        
        ui.add_space(8.0);

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
            for (key, label, color) in [
                (egui::Key::A, "A", Color32::DARK_GREEN),
                (egui::Key::S, "S", Color32::DARK_RED),
                (egui::Key::D, "D", Color32::DARK_GREEN),
            ] {
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

        ui.add_space(8.0);

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
            
            let cardinals = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"];
            let idx = ((self.state.heading + 22.5) / 45.0) as usize % 8;
            ui.label(RichText::new(format!("  {} ({:.0}°)", cardinals[idx], self.state.heading)).weak());
        });

        ui.add_space(8.0);
        ui.separator();
        
        // Vehicle diagram with camera assignments
        ui.collapsing("🚗 Vehicle Cameras", |ui| {
            self.draw_vehicle_diagram(ui);
        });
        
        ui.separator();
        
        ui.horizontal(|ui| {
            if ui.add(egui::Button::new(RichText::new("🛑 E-STOP").color(Color32::WHITE)).fill(Color32::DARK_RED)).clicked() {
                self.estop_pressed = true;
            }
            if ui.button("↺ Reset").clicked() {
                self.reset_pressed = true;
            }
            if ui.button("🔌 Reconnect").clicked() {
                self.reconnect_all = true;
            }
        });
        
        ui.horizontal(|ui| {
            let cam_count = self.state.camera_count;
            if cam_count > 0 {
                if ui.button(format!("📷 Cam {} →", self.state.active_camera)).clicked() {
                    self.switch_camera = true;
                }
            }
            ui.label(RichText::new(format!("{} cam(s)", cam_count)).small().weak());
        });

        ui.add_space(4.0);
        ui.label(RichText::new("W/S=Throttle A/D=Steer Space=Stop M=Mode").small().weak());
    }
    
    fn draw_vehicle_diagram(&mut self, ui: &mut egui::Ui) {
        let cam_count = self.state.camera_count;
        
        // Draw a simple top-down vehicle view
        let size = Vec2::new(120.0, 80.0);
        let (response, painter) = ui.allocate_painter(size, egui::Sense::hover());
        let rect = response.rect;
        let center = rect.center();
        
        // Vehicle body
        let body_rect = Rect::from_center_size(center, Vec2::new(40.0, 60.0));
        painter.rect_filled(body_rect, 4.0, Color32::from_rgb(60, 60, 80));
        painter.rect_stroke(body_rect, 4.0, Stroke::new(1.0, Color32::WHITE));
        
        // Direction indicator (front)
        painter.line_segment(
            [Pos2::new(center.x, body_rect.min.y), Pos2::new(center.x, body_rect.min.y - 8.0)],
            Stroke::new(2.0, Color32::GREEN),
        );
        
        // Camera position indicators
        let positions = [
            (CameraPosition::Front, Pos2::new(center.x, rect.min.y + 10.0)),
            (CameraPosition::Back, Pos2::new(center.x, rect.max.y - 10.0)),
            (CameraPosition::Left, Pos2::new(rect.min.x + 15.0, center.y)),
            (CameraPosition::Right, Pos2::new(rect.max.x - 15.0, center.y)),
        ];
        
        for (pos, point) in &positions {
            let assigned = self.state.camera_assignments.get(pos);
            let color = if assigned.is_some() { Color32::GREEN } else { Color32::GRAY };
            painter.circle_filled(*point, 6.0, color);
            painter.text(
                *point,
                egui::Align2::CENTER_CENTER,
                pos.arrow(),
                FontId::proportional(8.0),
                Color32::WHITE,
            );
        }
        
        // Camera assignment buttons
        if cam_count > 0 {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Assign:").small());
                for pos in [CameraPosition::Front, CameraPosition::Back, CameraPosition::Left, CameraPosition::Right] {
                    let current = self.state.camera_assignments.get(&pos).copied();
                    let label = match current {
                        Some(idx) => format!("{}{}", pos.arrow(), idx),
                        None => format!("{}?", pos.arrow()),
                    };
                    let color = if current.is_some() { Color32::GREEN } else { Color32::GRAY };
                    if ui.add(egui::Button::new(RichText::new(&label).small().color(color)).small()).clicked() {
                        // Cycle through cameras or unassign
                        let next = match current {
                            None => Some(0),
                            Some(idx) if idx + 1 < cam_count => Some(idx + 1),
                            Some(_) => None,
                        };
                        if let Some(idx) = next {
                            self.state.camera_assignments.insert(pos, idx);
                            self.log(&format!("Assigned camera {} to {}", idx, pos.label()));
                        } else {
                            self.state.camera_assignments.remove(&pos);
                            self.log(&format!("Unassigned {} camera", pos.label()));
                        }
                    }
                }
            });
        }
    }

    fn draw_logs_panel(&self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("📊 Logs");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if self.state.connected {
                    ui.label(RichText::new("● ESP32").color(Color32::GREEN).small());
                } else {
                    ui.label(RichText::new("○ ESP32").color(Color32::RED).small());
                }
                let cam_color = if self.state.camera_count > 0 { Color32::GREEN } else { Color32::RED };
                ui.label(RichText::new(format!("● {}cam", self.state.camera_count)).color(cam_color).small());
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
                self.toggle_sim = true;
            }
            
            if i.key_pressed(egui::Key::P) {
                self.state.auto_mode = !self.state.auto_mode;
                self.log(if self.state.auto_mode { "AUTO mode" } else { "MANUAL mode" });
            }
        });
        
        // Handle sim mode toggle (with position snap)
        if self.toggle_sim {
            self.toggle_sim_mode();
            self.toggle_sim = false;
        }
        
        if self.estop_pressed {
            self.state.throttle = 0.0;
            self.state.steering = 0.0;
            self.state.brake = true;
            self.state.auto_mode = false;
            self.log("EMERGENCY STOP");
            self.estop_pressed = false;
        }
        
        if self.reset_pressed {
            let old_assignments = self.state.camera_assignments.clone();
            self.state = VehicleState {
                connected: self.state.connected,
                camera_count: self.state.camera_count,
                active_camera: self.state.active_camera,
                real_lat: self.state.real_lat,
                real_lon: self.state.real_lon,
                camera_assignments: old_assignments,
                ..Default::default()
            };
            self.log("Reset");
            self.reset_pressed = false;
        }
        
        if self.reconnect_all {
            self.log("Reconnecting all devices...");
            {
                let mut logs_vec = self.logs.lock().unwrap();
                self.state.connected = self.serial.find_and_connect(&mut logs_vec);
            }
            // Re-count cameras before starting
            self.state.camera_count = self.camera.get_camera_count();
            let cam_idx = self.state.active_camera.min(self.state.camera_count.saturating_sub(1));
            self.state.active_camera = cam_idx;
            self.camera.start(self.logs.clone(), cam_idx);
            self.reconnect_all = false;
        }
        
        if self.flash_firmware {
            self.log("Starting firmware flash...");
            let success = {
                let mut logs_vec = self.logs.lock().unwrap();
                self.serial.flash_firmware(&mut logs_vec)
            };
            self.state.connected = success;
            self.flash_firmware = false;
        }
        
        if self.switch_camera {
            // Get fresh camera count from handler
            let count = self.camera.get_camera_count();
            self.state.camera_count = count;
            
            if count > 0 {
                let next = (self.state.active_camera + 1) % count;
                self.state.active_camera = next;
                self.log(&format!("Switching to camera {}", next));
                self.camera.start(self.logs.clone(), next);
            } else {
                self.log("No cameras available to switch");
            }
            self.switch_camera = false;
        }

        // Update vehicle - FSD with manual override, or pure manual
        // Manual input always takes precedence
        let manual_input = self.is_key_held(egui::Key::W) || 
                          self.is_key_held(egui::Key::A) || 
                          self.is_key_held(egui::Key::S) || 
                          self.is_key_held(egui::Key::D) ||
                          self.is_key_held(egui::Key::Space);
        
        if manual_input {
            // Manual override - user is driving
            self.update_controls();
        } else if self.state.auto_mode && self.state.nav_active {
            // FSD active with route
            self.update_fsd();
        } else {
            // No auto, no input - just decay controls
            self.update_controls();
        }
        self.send_command();

        // Header
        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading(RichText::new("🚗 EV Prototype Control Center").strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new("Texas A&M FLiNT").weak());
                    ui.separator();
                    let mode = if self.state.sim_mode { "SIM" } else { "GPS" };
                    let color = if self.state.sim_mode { Color32::from_rgb(200, 100, 255) } else { Color32::GREEN };
                    ui.label(RichText::new(mode).color(color));
                });
            });
        });

        // 2x2 grid
        egui::CentralPanel::default().show(ctx, |ui| {
            let available = ui.available_size();
            let half_w = available.x / 2.0 - 5.0;
            let half_h = available.y / 2.0 - 5.0;
            
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(half_w);
                    ui.set_height(half_h);
                    egui::Frame::dark_canvas(ui.style()).inner_margin(8.0).show(ui, |ui| {
                        self.draw_camera_panel(ui);
                    });
                });
                
                ui.vertical(|ui| {
                    ui.set_width(half_w);
                    ui.set_height(half_h);
                    egui::Frame::dark_canvas(ui.style()).inner_margin(8.0).show(ui, |ui| {
                        self.draw_map_panel(ui, ctx);
                    });
                });
            });
            
            ui.add_space(5.0);
            
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(half_w);
                    ui.set_height(half_h);
                    egui::Frame::dark_canvas(ui.style()).inner_margin(8.0).show(ui, |ui| {
                        self.draw_controls_panel(ui);
                    });
                });
                
                ui.vertical(|ui| {
                    ui.set_width(half_w);
                    ui.set_height(half_h);
                    egui::Frame::dark_canvas(ui.style()).inner_margin(8.0).show(ui, |ui| {
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
// Main - Windows entry point to hide console
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
