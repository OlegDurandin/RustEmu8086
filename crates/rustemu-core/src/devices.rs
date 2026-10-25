//! I/O ports: standard PC ports and Emu8086-style virtual devices.

use std::collections::HashMap;

/// Emu8086 virtual device ports.
pub mod ports {
    /// Traffic lights (word: 4 lights x 3 bits red/yellow/green).
    pub const TRAFFIC_LIGHTS: u16 = 4;
    /// Stepper motor: output = coil pattern; input bit 7 = ready.
    pub const STEPPER_MOTOR: u16 = 7;
    /// Robot: 9 = command (out), 10 = data (in), 11 = status (in).
    pub const ROBOT_COMMAND: u16 = 9;
    pub const ROBOT_DATA: u16 = 10;
    pub const ROBOT_STATUS: u16 = 11;
    /// Thermometer (in) and heater on/off (out).
    pub const THERMOMETER: u16 = 125;
    pub const HEATER: u16 = 127;
    /// Printer: each byte written is printed.
    pub const PRINTER: u16 = 130;
    /// LED display: shows the word written.
    pub const LED_DISPLAY: u16 = 199;
}

#[derive(Debug, Clone)]
pub struct Devices {
    /// Last value written to every port (generic "simple I/O" behaviour).
    pub latch: HashMap<u16, u16>,
    pub traffic_lights: u16,
    pub stepper_pattern: u8,
    pub stepper_angle: i32,
    pub led: u16,
    pub printer: Vec<u8>,
    pub heater_on: bool,
    /// Temperature in 1/100 degrees Celsius.
    pub temperature: i32,
    /// VGA DAC palette (6-bit components).
    pub palette: [[u8; 3]; 256],
    dac_write_index: u8,
    dac_read_index: u8,
    dac_component: u8,
    /// Toggles to emulate the vertical retrace bit of port 3DAh.
    retrace: u8,
    /// Last keyboard scan code for port 60h.
    pub last_scan: u8,
    /// Ports written (port, value) since the last [`Devices::take_events`].
    events: Vec<(u16, u16)>,
    pub robot: crate::robot::Robot,
    thermo_acc: u64,
}

impl Default for Devices {
    fn default() -> Self {
        Self::new()
    }
}

impl Devices {
    pub fn new() -> Self {
        Devices {
            latch: HashMap::new(),
            traffic_lights: 0,
            stepper_pattern: 0,
            stepper_angle: 0,
            led: 0,
            printer: Vec::new(),
            heater_on: false,
            temperature: 2000,
            palette: crate::video::default_palette(),
            dac_write_index: 0,
            dac_read_index: 0,
            dac_component: 0,
            retrace: 0,
            last_scan: 0,
            events: Vec::new(),
            robot: crate::robot::Robot::new(),
            thermo_acc: 0,
        }
    }

    /// Advances time-based device simulations by `instructions` executed instructions.
    pub fn tick(&mut self, instructions: u64) {
        self.robot.tick(instructions);
        // Thermometer: 0.01 degree per 1000 instructions toward heating/cooling.
        self.thermo_acc += instructions;
        let delta = (self.thermo_acc / 1000) as i32;
        self.thermo_acc %= 1000;
        if delta > 0 {
            if self.heater_on {
                self.temperature = (self.temperature + delta).min(10000);
            } else {
                self.temperature = (self.temperature - delta).max(-2000);
            }
        }
    }

    pub fn take_events(&mut self) -> Vec<(u16, u16)> {
        std::mem::take(&mut self.events)
    }

    pub fn input8(&mut self, port: u16) -> u8 {
        match port {
            0x60 => self.last_scan,
            0x64 => 0x14,
            0x3DA => {
                self.retrace = self.retrace.wrapping_add(1);
                if self.retrace & 4 != 0 { 0x09 } else { 0x00 }
            }
            0x3C9 => {
                let c = self.palette[self.dac_read_index as usize][self.dac_component as usize];
                self.advance_dac(false);
                c
            }
            ports::STEPPER_MOTOR => 0x80 | self.stepper_pattern,
            ports::THERMOMETER => (self.temperature / 100).clamp(-128, 127) as i8 as u8,
            ports::ROBOT_DATA => self.robot.read_data(),
            ports::ROBOT_STATUS => self.robot.status(),
            ports::LED_DISPLAY => self.led as u8,
            p => self.latch.get(&p).map(|v| *v as u8).unwrap_or(0xFF),
        }
    }

    pub fn input16(&mut self, port: u16) -> u16 {
        match port {
            ports::TRAFFIC_LIGHTS => self.traffic_lights,
            ports::LED_DISPLAY => self.led,
            _ => {
                let lo = self.input8(port) as u16;
                let hi = self.input8(port.wrapping_add(1)) as u16;
                lo | (hi << 8)
            }
        }
    }

    fn advance_dac(&mut self, write: bool) {
        self.dac_component += 1;
        if self.dac_component == 3 {
            self.dac_component = 0;
            if write {
                self.dac_write_index = self.dac_write_index.wrapping_add(1);
            } else {
                self.dac_read_index = self.dac_read_index.wrapping_add(1);
            }
        }
    }

    pub fn output8(&mut self, port: u16, v: u8) {
        self.latch.insert(port, v as u16);
        self.events.push((port, v as u16));
        match port {
            0x3C7 => {
                self.dac_read_index = v;
                self.dac_component = 0;
            }
            0x3C8 => {
                self.dac_write_index = v;
                self.dac_component = 0;
            }
            0x3C9 => {
                self.palette[self.dac_write_index as usize][self.dac_component as usize] = v & 0x3F;
                self.advance_dac(true);
            }
            ports::TRAFFIC_LIGHTS => self.traffic_lights = (self.traffic_lights & 0xFF00) | v as u16,
            ports::STEPPER_MOTOR => self.step_motor(v & 7),
            ports::HEATER => self.heater_on = v & 1 != 0,
            ports::PRINTER => self.printer.push(v),
            ports::ROBOT_COMMAND => self.robot.command(v),
            ports::LED_DISPLAY => self.led = v as u16,
            _ => {}
        }
    }

    pub fn output16(&mut self, port: u16, v: u16) {
        match port {
            ports::TRAFFIC_LIGHTS => {
                self.traffic_lights = v;
                self.latch.insert(port, v);
                self.events.push((port, v));
            }
            ports::LED_DISPLAY => {
                self.led = v;
                self.latch.insert(port, v);
                self.events.push((port, v));
            }
            _ => {
                self.output8(port, v as u8);
                self.output8(port.wrapping_add(1), (v >> 8) as u8);
            }
        }
    }

    /// Half-step sequence of a 3-coil stepper motor; moving to the neighbouring pattern
    /// rotates by one step in that direction.
    fn step_motor(&mut self, pattern: u8) {
        const SEQ: [u8; 6] = [0b001, 0b011, 0b010, 0b110, 0b100, 0b101];
        let old = SEQ.iter().position(|&p| p == self.stepper_pattern);
        let new = SEQ.iter().position(|&p| p == pattern);
        if let (Some(o), Some(n)) = (old, new) {
            let d = (n as i32 - o as i32).rem_euclid(6);
            match d {
                1 => self.stepper_angle += 1,
                5 => self.stepper_angle -= 1,
                _ => {}
            }
        }
        self.stepper_pattern = pattern;
    }
}
