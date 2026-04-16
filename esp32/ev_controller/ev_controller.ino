/**
 * EV Prototype - ESP32 Controller
 * Texas A&M FLiNT - Team Autopilot
 * 
 * Receives JSON commands via USB Serial, controls:
 * - Throttle (DAC → motor controller BROWN wire)
 * - Steering (L298N H-Bridge → AndyMark AM-3637 NeveRest 20)
 * - Brake (GPIO → motor controller PURPLE wire)
 * 
 * Steering motor: 12V DC, controlled via L298N
 * - IN1/IN2 control direction, ENA controls speed via PWM
 */

#include <ArduinoJson.h>

// Firmware version - must match GUI expected version
#define FIRMWARE_VERSION "1.5.8"

// Pins - ALL TOP ROW (column j on breadboard)
#define THROTTLE_DAC 25      // DAC output (row 8) → controller BROWN
#define BRAKE_PIN 32         // Active LOW (row 10) → controller PURPLE

// L298N Steering Control
#define STEER_IN1 26         // L298N IN1 (row 7) - direction A
#define STEER_IN2 27         // L298N IN2 (row 6) - direction B  
#define STEER_ENA 33         // L298N ENA (row 9) - PWM speed control

// Steering state tracking
int current_steering = 0;    // -100 to +100
unsigned long last_steer_cmd = 0;
const unsigned long STEER_TIMEOUT_MS = 200;  // Stop steering if no commands

String buffer = "";

void setup() {
    Serial.begin(115200);
    while (!Serial) delay(10);  // Wait for USB connection
    
    // Brake setup
    pinMode(BRAKE_PIN, OUTPUT);
    digitalWrite(BRAKE_PIN, HIGH);  // HIGH = brake OFF
    
    // L298N steering setup
    pinMode(STEER_IN1, OUTPUT);
    pinMode(STEER_IN2, OUTPUT);
    digitalWrite(STEER_IN1, LOW);
    digitalWrite(STEER_IN2, LOW);
    
    // PWM for steering speed (ENA)
    ledcSetup(0, 5000, 8);  // Channel 0, 5kHz, 8-bit
    ledcAttachPin(STEER_ENA, 0);
    
    // Start safe
    dacWrite(THROTTLE_DAC, 0);
    ledcWrite(0, 0);
    
    Serial.println("ESP32 EV Ready (L298N Steering)");
    Serial.print("VERSION:");
    Serial.println(FIRMWARE_VERSION);
    Serial.println("Commands: {\"t\":0-100, \"s\":-100 to 100, \"b\":true/false, \"v\":true}");
}

void setThrottle(int pct) {
    pct = constrain(pct, 0, 100);
    dacWrite(THROTTLE_DAC, map(pct, 0, 100, 0, 255));
    Serial.printf("T:%d\n", pct);
}

void setSteering(int val) {
    val = constrain(val, -100, 100);
    current_steering = val;
    last_steer_cmd = millis();
    
    int speed = map(abs(val), 0, 100, 0, 255);
    
    // Dead zone to prevent jitter
    if (abs(val) < 5) {
        // Stop motor
        digitalWrite(STEER_IN1, LOW);
        digitalWrite(STEER_IN2, LOW);
        ledcWrite(0, 0);
    } else if (val > 0) {
        // Turn right
        digitalWrite(STEER_IN1, HIGH);
        digitalWrite(STEER_IN2, LOW);
        ledcWrite(0, speed);
    } else {
        // Turn left
        digitalWrite(STEER_IN1, LOW);
        digitalWrite(STEER_IN2, HIGH);
        ledcWrite(0, speed);
    }
    Serial.printf("S:%d\n", val);
}

void stopSteering() {
    digitalWrite(STEER_IN1, LOW);
    digitalWrite(STEER_IN2, LOW);
    ledcWrite(0, 0);
    current_steering = 0;
}

void setBrake(bool on) {
    digitalWrite(BRAKE_PIN, on ? LOW : HIGH);
    Serial.printf("B:%s\n", on ? "ON" : "OFF");
}

void processCommand(String& json) {
    StaticJsonDocument<128> doc;
    if (deserializeJson(doc, json)) return;
    
    // Version query
    if (doc.containsKey("v") && doc["v"].as<bool>()) {
        Serial.print("VERSION:");
        Serial.println(FIRMWARE_VERSION);
        return;
    }
    
    if (doc.containsKey("t")) setThrottle(doc["t"].as<int>());
    if (doc.containsKey("s")) setSteering(doc["s"].as<int>());
    if (doc.containsKey("b")) setBrake(doc["b"].as<bool>());
}

void loop() {
    // Read serial commands
    while (Serial.available()) {
        char c = Serial.read();
        if (c == '\n') {
            processCommand(buffer);
            buffer = "";
        } else {
            buffer += c;
        }
    }
    
    // Safety: stop steering if no commands received recently
    // This prevents runaway steering if connection lost
    if (current_steering != 0 && (millis() - last_steer_cmd > STEER_TIMEOUT_MS)) {
        stopSteering();
        Serial.println("STEER_TIMEOUT");
    }
}
