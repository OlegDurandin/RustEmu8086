//! Emu8086-compatible robot device.
//!
//! Port 9 (out) - command: 0 nothing, 1 move forward, 2 turn left, 3 turn right,
//!                4 examine, 5 switch lamp on, 6 switch lamp off.
//! Port 10 (in)  - data: result of "examine": 0 empty, 255 wall, 7 lamp on, 8 lamp off.
//! Port 11 (in)  - status: bit 0 new data available (cleared by reading port 10),
//!                 bit 1 busy, bit 2 last command failed.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cell {
    Empty,
    Wall,
    Lamp { on: bool },
}

#[derive(Debug, Clone)]
pub struct Robot {
    pub width: usize,
    pub height: usize,
    pub cells: Vec<Cell>,
    pub x: usize,
    pub y: usize,
    /// 0 = north (up), 1 = east, 2 = south, 3 = west.
    pub dir: u8,
    data: u8,
    status: u8,
    pending: Option<u8>,
    busy_left: u64,
}

/// Instructions a command takes to complete.
const COMMAND_TIME: u64 = 2_000;

pub const STATUS_DATA: u8 = 1;
pub const STATUS_BUSY: u8 = 2;
pub const STATUS_ERROR: u8 = 4;

impl Default for Robot {
    fn default() -> Self {
        Self::new()
    }
}

impl Robot {
    pub fn new() -> Self {
        let mut r = Robot {
            width: 14,
            height: 9,
            cells: Vec::new(),
            x: 0,
            y: 0,
            dir: 1,
            data: 0,
            status: 0,
            pending: None,
            busy_left: 0,
        };
        r.default_map();
        r
    }

    /// A small room with a few walls and lamps.
    pub fn default_map(&mut self) {
        self.cells = vec![Cell::Empty; self.width * self.height];
        for (x, y) in [(5, 0), (5, 1), (5, 2), (9, 6), (9, 7), (9, 8), (2, 5), (3, 5), (12, 3), (11, 3)] {
            self.set(x, y, Cell::Wall);
        }
        for (x, y) in [(8, 1), (1, 7), (13, 6), (4, 3), (11, 8)] {
            self.set(x, y, Cell::Lamp { on: false });
        }
        self.x = 0;
        self.y = 0;
        self.dir = 1;
        self.reset_state();
    }

    pub fn clear_map(&mut self) {
        self.cells = vec![Cell::Empty; self.width * self.height];
        self.reset_state();
    }

    fn reset_state(&mut self) {
        self.data = 0;
        self.status = 0;
        self.pending = None;
        self.busy_left = 0;
    }

    pub fn get(&self, x: usize, y: usize) -> Cell {
        self.cells[y * self.width + x]
    }

    pub fn set(&mut self, x: usize, y: usize, c: Cell) {
        if x < self.width && y < self.height {
            self.cells[y * self.width + x] = c;
        }
    }

    /// Cycles a cell through empty -> wall -> lamp -> empty (map editing).
    pub fn cycle_cell(&mut self, x: usize, y: usize) {
        if (x, y) == (self.x, self.y) {
            return;
        }
        let next = match self.get(x, y) {
            Cell::Empty => Cell::Wall,
            Cell::Wall => Cell::Lamp { on: false },
            Cell::Lamp { .. } => Cell::Empty,
        };
        self.set(x, y, next);
    }

    fn front(&self) -> Option<(usize, usize)> {
        let (dx, dy): (isize, isize) = match self.dir & 3 {
            0 => (0, -1),
            1 => (1, 0),
            2 => (0, 1),
            _ => (-1, 0),
        };
        let nx = self.x as isize + dx;
        let ny = self.y as isize + dy;
        (nx >= 0 && ny >= 0 && (nx as usize) < self.width && (ny as usize) < self.height)
            .then_some((nx as usize, ny as usize))
    }

    pub fn status(&self) -> u8 {
        self.status
    }

    pub fn read_data(&mut self) -> u8 {
        self.status &= !STATUS_DATA;
        self.data
    }

    pub fn command(&mut self, cmd: u8) {
        if cmd == 0 {
            return;
        }
        if self.status & STATUS_BUSY != 0 {
            self.status |= STATUS_ERROR;
            return;
        }
        self.status = (self.status | STATUS_BUSY) & !STATUS_ERROR;
        self.pending = Some(cmd);
        self.busy_left = COMMAND_TIME;
    }

    pub fn tick(&mut self, instructions: u64) {
        if self.pending.is_none() {
            return;
        }
        if self.busy_left > instructions {
            self.busy_left -= instructions;
            return;
        }
        let cmd = self.pending.take().unwrap();
        self.status &= !STATUS_BUSY;
        let front = self.front();
        let ok = match cmd {
            1 => match front {
                Some((x, y)) if self.get(x, y) == Cell::Empty => {
                    self.x = x;
                    self.y = y;
                    true
                }
                _ => false,
            },
            2 => {
                self.dir = (self.dir + 3) & 3;
                true
            }
            3 => {
                self.dir = (self.dir + 1) & 3;
                true
            }
            4 => {
                self.data = match front.map(|(x, y)| self.get(x, y)) {
                    None | Some(Cell::Wall) => 255,
                    Some(Cell::Empty) => 0,
                    Some(Cell::Lamp { on: true }) => 7,
                    Some(Cell::Lamp { on: false }) => 8,
                };
                self.status |= STATUS_DATA;
                true
            }
            5 | 6 => match front {
                Some((x, y)) if matches!(self.get(x, y), Cell::Lamp { .. }) => {
                    self.set(x, y, Cell::Lamp { on: cmd == 5 });
                    true
                }
                _ => false,
            },
            _ => false,
        };
        if !ok {
            self.status |= STATUS_ERROR;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moves_examines_and_switches_lamps() {
        let mut r = Robot::new();
        r.clear_map();
        r.set(2, 0, Cell::Lamp { on: false });
        r.command(1);
        assert_eq!(r.status() & STATUS_BUSY, STATUS_BUSY);
        r.tick(COMMAND_TIME);
        assert_eq!((r.x, r.y), (1, 0));
        r.command(4);
        r.tick(COMMAND_TIME);
        assert_eq!(r.status() & STATUS_DATA, STATUS_DATA);
        assert_eq!(r.read_data(), 8);
        assert_eq!(r.status() & STATUS_DATA, 0);
        r.command(5);
        r.tick(COMMAND_TIME);
        assert_eq!(r.get(2, 0), Cell::Lamp { on: true });
        r.command(1); // blocked by the lamp
        r.tick(COMMAND_TIME);
        assert_eq!(r.status() & STATUS_ERROR, STATUS_ERROR);
        r.command(2); // turn left -> north; examine sees the edge as a wall
        r.tick(COMMAND_TIME);
        r.command(4);
        r.tick(COMMAND_TIME);
        assert_eq!(r.read_data(), 255);
    }
}
