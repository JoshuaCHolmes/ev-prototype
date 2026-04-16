//! EV Prototype Control Center
//! Standalone Rust executable for Windows/Linux
//! Texas A&M FLiNT - Team Autopilot

use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute, queue,
    style::Print,
    terminal::{self, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use serde::Serialize;
use serialport::SerialPort;
use std::io::{stdout, Write};
use std::time::{Duration, Instant};
use std::collections::HashMap;

#[derive(Serialize)]
struct Command {
    t: i32,  // throttle
    s: i32,  // steering
    b: bool, // brake
}

struct Controller {
    port: Option<Box<dyn SerialPort>>,
    throttle: f32,
    steering: f32,
    brake: bool,
    keys_held: HashMap<char, Instant>,
    key_timeout: Duration,
    last_update: Instant,
    running: bool,
    connected: bool,
    port_name: String,
}

impl Controller {
    fn new(port_name: &str) -> Self {
        Self {
            port: None,
            throttle: 0.0,
            steering: 0.0,
            brake: false,
            keys_held: HashMap::new(),
            key_timeout: Duration::from_millis(150),
            last_update: Instant::now(),
            running: true,
            connected: false,
            port_name: port_name.to_string(),
        }
    }

    fn connect(&mut self) -> bool {
        match serialport::new(&self.port_name, 115200)
            .timeout(Duration::from_millis(100))
            .open()
        {
            Ok(port) => {
                self.port = Some(port);
                self.connected = true;
                // Wait for ESP32 to reset
                std::thread::sleep(Duration::from_secs(2));
                true
            }
            Err(e) => {
                eprintln!("Could not open {}: {}", self.port_name, e);
                false
            }
        }
    }

    fn send_command(&mut self) {
        if let Some(ref mut port) = self.port {
            let cmd = Command {
                t: self.throttle as i32,
                s: self.steering as i32,
                b: self.brake,
            };
            if let Ok(json) = serde_json::to_string(&cmd) {
                let _ = port.write_all(format!("{}\n", json).as_bytes());
            }
        }
    }

    fn is_key_held(&self, key: char) -> bool {
        if let Some(time) = self.keys_held.get(&key) {
            time.elapsed() < self.key_timeout
        } else {
            false
        }
    }

    fn update_controls(&mut self) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_update).as_secs_f32();
        self.last_update = now;

        let accel = 150.0 * dt;
        let decel = 200.0 * dt;

        // Throttle (W = forward, S = brake/reverse)
        if self.is_key_held('w') {
            self.throttle = (self.throttle + accel).min(100.0);
            self.brake = false;
        } else if self.is_key_held('s') {
            if self.throttle > 0.0 {
                self.throttle = (self.throttle - accel * 2.0).max(0.0);
                self.brake = true;
            } else {
                self.throttle = (self.throttle - accel).max(-50.0);
                self.brake = false;
            }
        } else {
            if self.throttle.abs() < decel {
                self.throttle = 0.0;
            } else if self.throttle > 0.0 {
                self.throttle -= decel;
            } else {
                self.throttle += decel;
            }
            self.brake = false;
        }

        // Steering (A = left, D = right) - does NOT auto-center
        if self.is_key_held('a') {
            self.steering = (self.steering - accel).max(-100.0);
        } else if self.is_key_held('d') {
            self.steering = (self.steering + accel).min(100.0);
        }
        // Note: steering does NOT return to center (manual motor)

        // Emergency brake (Space)
        if self.is_key_held(' ') {
            self.brake = true;
            self.throttle = 0.0;
        }
    }

    fn make_bar(&self, value: f32, max_val: f32, width: usize, center: bool) -> String {
        if center {
            let mid = width / 2;
            let filled = ((value.abs() / max_val) * mid as f32) as usize;
            if value < 0.0 {
                format!(
                    "[{}{}|{}]",
                    " ".repeat(mid - filled),
                    "=".repeat(filled),
                    " ".repeat(mid)
                )
            } else if value > 0.0 {
                format!(
                    "[{}|{}{}]",
                    " ".repeat(mid),
                    "=".repeat(filled),
                    " ".repeat(mid - filled)
                )
            } else {
                format!("[{}|{}]", " ".repeat(mid), " ".repeat(mid))
            }
        } else {
            let filled = ((value.abs() / max_val) * width as f32) as usize;
            format!("[{}{}]", "=".repeat(filled), " ".repeat(width - filled))
        }
    }

    fn draw_ui(&self) -> std::io::Result<()> {
        let mut stdout = stdout();
        
        // Use queue! instead of execute! for buffered writes, then move cursor to top
        queue!(stdout, cursor::MoveTo(0, 0))?;

        let status = if self.connected { "CONNECTED" } else { "DISCONNECTED" };
        let direction = if self.throttle > 0.0 {
            "FWD "
        } else if self.throttle < 0.0 {
            "REV "
        } else {
            "----"
        };
        let steer_dir = if self.steering < -5.0 {
            "LEFT "
        } else if self.steering > 5.0 {
            "RIGHT"
        } else {
            "CTR  "
        };

        let tbar = self.make_bar(self.throttle, 100.0, 20, false);
        let sbar = self.make_bar(self.steering, 100.0, 20, true);

        let w = if self.is_key_held('w') { "[W]" } else { " W " };
        let a = if self.is_key_held('a') { "[A]" } else { " A " };
        let s = if self.is_key_held('s') { "[S]" } else { " S " };
        let d = if self.is_key_held('d') { "[D]" } else { " D " };
        let space = if self.is_key_held(' ') { "[SPACE]" } else { " SPACE " };

        // Build entire frame as a string, then write once
        let brake_line = if self.brake { "            >> BRAKE <<" } else { "                       " };
        
        let frame = format!(
r#"================================================================
           EV PROTOTYPE CONTROL CENTER (Rust)
              Texas A&M FLiNT - Team Autopilot
================================================================

  ESP32: {} [{}]

  Throttle: {} {:5.1}% {}
  Steering: {} {:+6.1} {}
{}

  Keys:        {}
            {} {} {}
            {} = E-STOP

----------------------------------------------------------------
  W=Accel  S=Brake/Rev  A/D=Steer  SPACE=E-Stop  Q=Quit
----------------------------------------------------------------
"#,
            self.port_name, status,
            tbar, self.throttle.abs(), direction,
            sbar, self.steering, steer_dir,
            brake_line,
            w,
            a, s, d,
            space
        );

        // Write entire frame at once
        queue!(stdout, Print(frame))?;
        stdout.flush()?;
        Ok(())
    }

    fn run(&mut self) -> std::io::Result<()> {
        let mut stdout = stdout();
        
        terminal::enable_raw_mode()?;
        // Enter alternate screen buffer to prevent flicker and preserve original content
        execute!(stdout, EnterAlternateScreen, cursor::Hide)?;

        // Initial clear
        execute!(stdout, terminal::Clear(ClearType::All))?;

        println!("\n[*] Connecting to ESP32 on {}...", self.port_name);
        if !self.connect() {
            println!("[!] Running in DEMO MODE (no ESP32)");
        } else {
            println!("[+] Connected!");
        }

        println!("[*] Starting in 2 seconds...");
        println!("[*] Hold W/A/S/D to drive, SPACE for e-brake, Q to quit");
        stdout.flush()?;
        std::thread::sleep(Duration::from_secs(2));

        let mut last_send = Instant::now();
        let mut last_draw = Instant::now();

        while self.running {
            // Handle input
            if event::poll(Duration::from_millis(10))? {
                if let Event::Key(KeyEvent { code, modifiers, .. }) = event::read()? {
                    let now = Instant::now();
                    match code {
                        KeyCode::Char('q') => self.running = false,
                        KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
                            self.running = false
                        }
                        KeyCode::Char('w') => { self.keys_held.insert('w', now); }
                        KeyCode::Char('a') => { self.keys_held.insert('a', now); }
                        KeyCode::Char('s') => { self.keys_held.insert('s', now); }
                        KeyCode::Char('d') => { self.keys_held.insert('d', now); }
                        KeyCode::Char(' ') => { self.keys_held.insert(' ', now); }
                        _ => {}
                    }
                }
            }

            // Update controls
            self.update_controls();

            // Send commands at 20Hz
            if last_send.elapsed() > Duration::from_millis(50) {
                self.send_command();
                last_send = Instant::now();
            }

            // Update UI at 15Hz (faster for smoother display)
            if last_draw.elapsed() > Duration::from_millis(66) {
                self.draw_ui()?;
                last_draw = Instant::now();
            }
        }

        // Emergency stop
        self.throttle = 0.0;
        self.brake = true;
        self.send_command();

        // Leave alternate screen and restore terminal
        execute!(stdout, LeaveAlternateScreen, cursor::Show)?;
        terminal::disable_raw_mode()?;

        println!("\n[+] Controller stopped. Vehicle safed.");
        Ok(())
    }
}

fn find_serial_port() -> Option<String> {
    let ports = serialport::available_ports().ok()?;
    
    for port in &ports {
        let name = port.port_name.to_lowercase();
        // Look for CP2102 or common USB serial names
        if let serialport::SerialPortType::UsbPort(info) = &port.port_type {
            // CP2102 VID:PID = 10C4:EA60
            if info.vid == 0x10C4 && info.pid == 0xEA60 {
                return Some(port.port_name.clone());
            }
            // CH340
            if info.vid == 0x1A86 {
                return Some(port.port_name.clone());
            }
        }
        // Fallback: ttyUSB on Linux
        if name.contains("ttyusb") || name.contains("ttyacm") {
            return Some(port.port_name.clone());
        }
    }
    
    // Just return first available port if nothing matched
    ports.first().map(|p| p.port_name.clone())
}

fn list_ports() {
    println!("\nAvailable serial ports:");
    match serialport::available_ports() {
        Ok(ports) => {
            if ports.is_empty() {
                println!("  (none found)");
            }
            for port in ports {
                let info = match &port.port_type {
                    serialport::SerialPortType::UsbPort(usb) => {
                        format!("USB {:04X}:{:04X}", usb.vid, usb.pid)
                    }
                    _ => "Unknown".to_string(),
                };
                println!("  {} - {}", port.port_name, info);
            }
        }
        Err(e) => println!("  Error listing ports: {}", e),
    }
    println!();
}

fn main() {
    println!("================================================================");
    println!("         EV PROTOTYPE CONTROL CENTER (Rust)");
    println!("           Texas A&M FLiNT - Team Autopilot");
    println!("================================================================\n");

    let args: Vec<String> = std::env::args().collect();
    
    let port_name = if args.len() > 1 {
        if args[1] == "--list" || args[1] == "-l" {
            list_ports();
            wait_for_enter();
            return;
        }
        args[1].clone()
    } else {
        println!("[*] Auto-detecting ESP32...");
        match find_serial_port() {
            Some(port) => {
                println!("[+] Found: {}", port);
                port
            }
            None => {
                println!("[!] No serial port found.");
                list_ports();
                println!("Usage: ev-control [COM_PORT]");
                println!("       ev-control --list");
                wait_for_enter();
                return;
            }
        }
    };

    let mut controller = Controller::new(&port_name);
    if let Err(e) = controller.run() {
        eprintln!("Error: {}", e);
        wait_for_enter();
    }
}

fn wait_for_enter() {
    println!("\nPress ENTER to exit...");
    let mut input = String::new();
    let _ = std::io::stdin().read_line(&mut input);
}
