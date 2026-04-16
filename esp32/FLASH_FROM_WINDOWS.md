# Flash ESP32 from Windows (Fastest for NixOS users)

## Step 1: Open Arduino IDE on Windows

## Step 2: Install ESP32 Board
1. File → Preferences
2. Add to "Additional Board Manager URLs":
   ```
   https://raw.githubusercontent.com/espressif/arduino-esp32/gh-pages/package_esp32_index.json
   ```
3. Tools → Board → Boards Manager → Search "esp32" → Install

## Step 3: Install ArduinoJson Library
1. Sketch → Include Library → Manage Libraries
2. Search "ArduinoJson" by Benoit Blanchon → Install

## Step 4: Copy this code into a new sketch
(See main.cpp contents below, or copy from WSL)

## Step 5: Configure & Upload
1. Tools → Board → ESP32 Dev Module
2. Tools → Port → COM port (check Device Manager)
3. Click Upload button

## The Code (copy this):
/**
 * EV Prototype - ESP32 Controller
 * Texas A&M Senior Design - Crunch Time Edition
 * 
 * Receives JSON commands via USB Serial, controls:
 * - Throttle (DAC → motor controller BROWN wire)
 * - Steering (PWM → motor driver)
 * - Brake (GPIO → motor controller PURPLE wire)
 */

#include <ArduinoJson.h>

// Pins - ALL TOP ROW (column j on breadboard)
#define THROTTLE_DAC 25      // DAC output (row 8) → controller BROWN
#define BRAKE_PIN 32         // Active LOW (row 10) → controller PURPLE
#define STEER_A 26           // Steering motor (row 7) → driver IN1
#define STEER_B 27           // Steering motor (row 6) → driver IN2

String buffer = "";

void setup() {
    Serial.begin(115200);
    while (!Serial) delay(10);  // Wait for USB connection
    
    pinMode(BRAKE_PIN, OUTPUT);
    digitalWrite(BRAKE_PIN, HIGH);  // HIGH = brake OFF
    
    // Steering PWM setup
    ledcSetup(0, 5000, 8);
    ledcSetup(1, 5000, 8);
    ledcAttachPin(STEER_A, 0);
    ledcAttachPin(STEER_B, 1);
    
    // Start safe
    dacWrite(THROTTLE_DAC, 0);
    ledcWrite(0, 0);
    ledcWrite(1, 0);
    
    Serial.println("ESP32 EV Ready");
    Serial.println("Commands: {\"t\":0-100, \"s\":-100 to 100, \"b\":true/false}");
}

void setThrottle(int pct) {
    pct = constrain(pct, 0, 100);
    dacWrite(THROTTLE_DAC, map(pct, 0, 100, 0, 255));
    Serial.printf("T:%d\n", pct);
}

void setSteering(int val) {
    val = constrain(val, -100, 100);
    int spd = map(abs(val), 0, 100, 0, 255);
    
    if (val > 10) {
        ledcWrite(0, spd); ledcWrite(1, 0);
    } else if (val < -10) {
        ledcWrite(0, 0); ledcWrite(1, spd);
    } else {
        ledcWrite(0, 0); ledcWrite(1, 0);
    }
    Serial.printf("S:%d\n", val);
}

void setBrake(bool on) {
    digitalWrite(BRAKE_PIN, on ? LOW : HIGH);
    Serial.printf("B:%s\n", on ? "ON" : "OFF");
}

void processCommand(String& json) {
    StaticJsonDocument<128> doc;
    if (deserializeJson(doc, json)) return;
    
    if (doc.containsKey("t")) setThrottle(doc["t"].as<int>());
    if (doc.containsKey("s")) setSteering(doc["s"].as<int>());
    if (doc.containsKey("b")) setBrake(doc["b"].as<bool>());
}

void loop() {
    while (Serial.available()) {
        char c = Serial.read();
        if (c == '\n') {
            processCommand(buffer);
            buffer = "";
        } else {
            buffer += c;
        }
    }
}
