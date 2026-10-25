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

/// Time base of the virtual devices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceClock {
    /// Host time, like the original Emu8086 devices.
    RealTime,
    /// Deterministic: one millisecond per this many instructions (tests).
    Instructions(u64),
}

#[derive(Debug, Clone)]
pub struct Devices {
    /// Last value written to every port (generic "simple I/O" behaviour).
    pub latch: HashMap<u16, u16>,
    pub traffic_lights: u16,
    /// Last byte written to the stepper motor port.
    pub stepper_pattern: u8,
    /// Rotor position 0..3 (Emu8086's stepper state machine).
    pub stepper_rotor: u8,
    /// Position of the marker on the rim, 0..31 (one half-step = 1, full step = 2).
    pub stepper_dot: u8,
    /// Direction of the last move: +1 clockwise, -1 counter-clockwise, 0 none.
    pub stepper_last_move: i8,
    pub led: u16,
    pub printer: Vec<u8>,
    pub heater_on: bool,
    /// Thermometer reading in degrees Celsius (-65..120).
    pub temperature: i32,
    /// Air temperature the room cools down (or warms up) to without the heater.
    pub air_temperature: i32,
    /// Time base of device simulations.
    pub clock: DeviceClock,
    last_tick: Option<std::time::Instant>,
    thermo_ms: f64,
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
            stepper_rotor: 0,
            stepper_dot: 0,
            stepper_last_move: 0,
            led: 0,
            printer: Vec::new(),
            heater_on: false,
            temperature: 0,
            air_temperature: 0,
            clock: DeviceClock::RealTime,
            last_tick: None,
            thermo_ms: 0.0,
            palette: crate::video::default_palette(),
            dac_write_index: 0,
            dac_read_index: 0,
            dac_component: 0,
            retrace: 0,
            last_scan: 0,
            events: Vec::new(),
            robot: crate::robot::Robot::new(),
        }
    }

    /// Advances time-based device simulations after `instructions` executed instructions.
    pub fn tick(&mut self, instructions: u64) {
        let ms = match self.clock {
            DeviceClock::RealTime => {
                let now = std::time::Instant::now();
                let last = *self.last_tick.get_or_insert(now);
                self.last_tick = Some(now);
                // A paused debugger does not make time jump forward.
                (now - last).as_secs_f64().min(0.25) * 1000.0
            }
            DeviceClock::Instructions(per_ms) => instructions as f64 / per_ms.max(1) as f64,
        };
        if ms <= 0.0 {
            return;
        }
        self.robot.tick(ms);
        self.thermometer(ms);
    }

    /// Like Emu8086's thermometer: +1 degree every 200 ms with the heater on, otherwise
    /// one degree per second towards the air temperature; range -65..120.
    fn thermometer(&mut self, ms: f64) {
        self.thermo_ms += ms;
        let period = if self.heater_on { 200.0 } else { 1000.0 };
        while self.thermo_ms >= period {
            self.thermo_ms -= period;
            let t = if self.heater_on {
                self.temperature + 1
            } else {
                self.temperature + (self.air_temperature - self.temperature).signum()
            };
            if (-65..=120).contains(&t) {
                self.temperature = t;
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
            // Bit 7 = ready (the motor completes a step immediately).
            ports::STEPPER_MOTOR => 0x80 | self.stepper_pattern,
            ports::THERMOMETER => self.temperature as i8 as u8,
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
            ports::STEPPER_MOTOR => self.step_motor(v),
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

    /// Emu8086's 3-phase stepper motor: the rotor has 4 positions; the new coil pattern decides
    /// where it moves and by how much (+1/-1 half step, +2/-2 full step). The device reacts only
    /// when the value written changes.
    fn step_motor(&mut self, value: u8) {
        if value == self.stepper_pattern {
            return;
        }
        self.stepper_pattern = value;
        // (new rotor position, move) for rotor positions 0..3; None = stays.
        type T = [Option<(u8, i8)>; 4];
        const TABLE: [T; 8] = [
            [None, None, None, None],
            [Some((1, 2)), None, Some((1, 1)), Some((1, -1))],
            [None, None, Some((0, -1)), Some((0, 1))],
            [Some((3, -1)), Some((3, 1)), Some((3, -2)), None],
            [Some((1, -2)), None, Some((1, 1)), Some((1, -1))],
            [None, None, Some((1, 1)), Some((1, -1))],
            [Some((2, 1)), Some((2, -1)), None, Some((2, 2))],
            [None, None, Some((0, -1)), Some((0, 1))],
        ];
        match TABLE[(value & 7) as usize][self.stepper_rotor as usize & 3] {
            Some((pos, mv)) => {
                self.stepper_rotor = pos;
                self.stepper_dot = (self.stepper_dot as i32 + mv as i32).rem_euclid(32) as u8;
                self.stepper_last_move = mv.signum();
            }
            None => self.stepper_last_move = 0,
        }
    }
}
