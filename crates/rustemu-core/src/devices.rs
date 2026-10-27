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
    latch_journal: Option<Vec<(u16, Option<u16>)>>,
    palette_journal: Option<Vec<(usize, [u8; 3])>>,
}

/// Compact reversible state. Append-only observer buffers keep only their
/// lengths; mutable collections record only entries touched by the step.
pub(crate) struct DevicesSnapshot {
    latch_undo: Vec<(u16, Option<u16>)>,
    traffic_lights: u16,
    stepper_pattern: u8,
    stepper_rotor: u8,
    stepper_dot: u8,
    stepper_last_move: i8,
    led: u16,
    printer_len: usize,
    heater_on: bool,
    temperature: i32,
    air_temperature: i32,
    clock: DeviceClock,
    last_tick: Option<std::time::Instant>,
    thermo_ms: f64,
    palette_undo: Vec<(usize, [u8; 3])>,
    dac_write_index: u8,
    dac_read_index: u8,
    dac_component: u8,
    retrace: u8,
    last_scan: u8,
    events_len: usize,
    robot: crate::robot::RobotSnapshot,
}

impl DevicesSnapshot {
    pub(crate) fn is_unchanged(&self, devices: &Devices) -> bool {
        self.latch_undo.is_empty()
            && self.traffic_lights == devices.traffic_lights
            && self.stepper_pattern == devices.stepper_pattern
            && self.stepper_rotor == devices.stepper_rotor
            && self.stepper_dot == devices.stepper_dot
            && self.stepper_last_move == devices.stepper_last_move
            && self.led == devices.led
            && self.printer_len == devices.printer.len()
            && self.heater_on == devices.heater_on
            && self.temperature == devices.temperature
            && self.air_temperature == devices.air_temperature
            && self.clock == devices.clock
            && self.last_tick == devices.last_tick
            && self.thermo_ms == devices.thermo_ms
            && self.palette_undo.is_empty()
            && self.dac_write_index == devices.dac_write_index
            && self.dac_read_index == devices.dac_read_index
            && self.dac_component == devices.dac_component
            && self.retrace == devices.retrace
            && self.last_scan == devices.last_scan
            && self.events_len == devices.events.len()
            && self.robot.is_unchanged(&devices.robot)
    }
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
            latch_journal: None,
            palette_journal: None,
        }
    }

    /// Current value of the vertical retrace bit read from port 3DAh (toggles on every read).
    pub fn retrace(&self) -> u8 {
        self.retrace
    }

    pub(crate) fn begin_snapshot(&mut self) -> DevicesSnapshot {
        self.latch_journal = Some(Vec::new());
        self.palette_journal = Some(Vec::new());
        DevicesSnapshot {
            latch_undo: Vec::new(),
            traffic_lights: self.traffic_lights,
            stepper_pattern: self.stepper_pattern,
            stepper_rotor: self.stepper_rotor,
            stepper_dot: self.stepper_dot,
            stepper_last_move: self.stepper_last_move,
            led: self.led,
            printer_len: self.printer.len(),
            heater_on: self.heater_on,
            temperature: self.temperature,
            air_temperature: self.air_temperature,
            clock: self.clock,
            last_tick: self.last_tick,
            thermo_ms: self.thermo_ms,
            palette_undo: Vec::new(),
            dac_write_index: self.dac_write_index,
            dac_read_index: self.dac_read_index,
            dac_component: self.dac_component,
            retrace: self.retrace,
            last_scan: self.last_scan,
            events_len: self.events.len(),
            robot: self.robot.begin_snapshot(),
        }
    }

    pub(crate) fn finish_snapshot(&mut self, mut snapshot: DevicesSnapshot) -> DevicesSnapshot {
        snapshot.latch_undo = self.latch_journal.take().unwrap_or_default();
        snapshot.palette_undo = self.palette_journal.take().unwrap_or_default();
        snapshot.robot = self.robot.finish_snapshot(snapshot.robot);
        snapshot
    }

    pub(crate) fn cancel_snapshot(&mut self) {
        self.latch_journal = None;
        self.palette_journal = None;
        self.robot.cancel_snapshot();
    }

    pub(crate) fn restore(&mut self, snapshot: DevicesSnapshot) {
        for (port, value) in snapshot.latch_undo.into_iter().rev() {
            match value {
                Some(value) => self.latch.insert(port, value),
                None => self.latch.remove(&port),
            };
        }
        self.traffic_lights = snapshot.traffic_lights;
        self.stepper_pattern = snapshot.stepper_pattern;
        self.stepper_rotor = snapshot.stepper_rotor;
        self.stepper_dot = snapshot.stepper_dot;
        self.stepper_last_move = snapshot.stepper_last_move;
        self.led = snapshot.led;
        self.printer.truncate(snapshot.printer_len);
        self.heater_on = snapshot.heater_on;
        self.temperature = snapshot.temperature;
        self.air_temperature = snapshot.air_temperature;
        self.clock = snapshot.clock;
        self.last_tick = snapshot.last_tick;
        self.thermo_ms = snapshot.thermo_ms;
        for (i, value) in snapshot.palette_undo.into_iter().rev() {
            self.palette[i] = value;
        }
        self.dac_write_index = snapshot.dac_write_index;
        self.dac_read_index = snapshot.dac_read_index;
        self.dac_component = snapshot.dac_component;
        self.retrace = snapshot.retrace;
        self.last_scan = snapshot.last_scan;
        self.events.truncate(snapshot.events_len);
        self.robot.restore(snapshot.robot);
        self.latch_journal = None;
        self.palette_journal = None;
    }

    fn set_latch(&mut self, port: u16, value: u16) {
        if let Some(journal) = &mut self.latch_journal
            && !journal.iter().any(|(p, _)| *p == port)
        {
            journal.push((port, self.latch.get(&port).copied()));
        }
        self.latch.insert(port, value);
    }

    pub(crate) fn set_palette(&mut self, index: usize, value: [u8; 3]) {
        if let Some(journal) = &mut self.palette_journal
            && !journal.iter().any(|(i, _)| *i == index)
        {
            journal.push((index, self.palette[index]));
        }
        self.palette[index] = value;
    }

    pub(crate) fn replace_palette(&mut self, palette: [[u8; 3]; 256]) {
        for (i, value) in palette.into_iter().enumerate() {
            self.set_palette(i, value);
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
        self.set_latch(port, v as u16);
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
                let i = self.dac_write_index as usize;
                let mut value = self.palette[i];
                value[self.dac_component as usize] = v & 0x3F;
                self.set_palette(i, value);
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
                self.set_latch(port, v);
                self.events.push((port, v));
            }
            ports::LED_DISPLAY => {
                self.led = v;
                self.set_latch(port, v);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::robot::Cell;

    #[test]
    fn snapshot_uses_deltas_for_collections_and_lengths_for_buffers() {
        let mut d = Devices::new();
        d.latch.insert(42, 7);
        d.printer = Vec::with_capacity(1_000_001);
        d.printer.resize(1_000_000, b'x');
        d.robot.clear_map();
        let printer_ptr = d.printer.as_ptr();
        let old_palette = d.palette[0];

        let snapshot = d.begin_snapshot();
        assert_eq!(d.printer.as_ptr(), printer_ptr);
        d.output8(42, 9);
        d.output8(43, 10);
        d.output8(ports::PRINTER, b'y');
        d.output16(ports::TRAFFIC_LIGHTS, 0x0249);
        d.set_palette(0, [1, 2, 3]);
        d.robot.set(1, 1, Cell::Wall);
        let snapshot = d.finish_snapshot(snapshot);

        d.restore(snapshot);
        assert_eq!(d.latch.get(&42), Some(&7));
        assert!(!d.latch.contains_key(&43));
        assert_eq!(d.printer.len(), 1_000_000);
        assert_eq!(d.printer.as_ptr(), printer_ptr);
        assert_eq!(d.traffic_lights, 0);
        assert_eq!(d.palette[0], old_palette);
        assert_eq!(d.robot.get(1, 1), Cell::Empty);
        assert!(d.events.is_empty());
    }
}
