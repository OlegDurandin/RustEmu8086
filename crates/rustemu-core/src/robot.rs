//! Robot device, behaving like Emu8086's `robot.exe`.
//!
//! Port 9 (out) - command: 0 nothing, 1 move forward, 2 turn left, 3 turn right,
//!                4 examine, 5 switch lamp on, 6 switch lamp off.
//! Port 10 (in)  - data: result of "examine": 0 empty, 255 wall, 7 lamp on, 8 lamp off.
//! Port 11 (in)  - status: bit 0 new data available (cleared by reading port 10),
//!                 bit 1 busy, bit 2 last command failed.
//!
//! The room is 9 x 6 cells. Moving and turning take time (`MOVE_MS`); examining and switching
//! lamps complete immediately. A command sent while the robot is busy is rejected (error bit).

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
    busy_ms: f64,
    cell_journal: Option<Vec<(usize, Cell)>>,
}

pub(crate) struct RobotSnapshot {
    width: usize,
    height: usize,
    x: usize,
    y: usize,
    dir: u8,
    data: u8,
    status: u8,
    pending: Option<u8>,
    busy_ms: f64,
    cell_undo: Vec<(usize, Cell)>,
}

impl RobotSnapshot {
    pub(crate) fn is_unchanged(&self, robot: &Robot) -> bool {
        self.cell_undo.is_empty()
            && self.width == robot.width
            && self.height == robot.height
            && self.x == robot.x
            && self.y == robot.y
            && self.dir == robot.dir
            && self.data == robot.data
            && self.status == robot.status
            && self.pending == robot.pending
            && self.busy_ms == robot.busy_ms
    }
}

/// Duration of a move or a turn, in milliseconds.
pub const MOVE_MS: f64 = 250.0;

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
            width: 9,
            height: 6,
            cells: Vec::new(),
            x: 0,
            y: 0,
            dir: 0,
            data: 0,
            status: 0,
            pending: None,
            busy_ms: 0.0,
            cell_journal: None,
        };
        r.default_map();
        r
    }

    /// The default room of Emu8086's robot device.
    pub fn default_map(&mut self) {
        self.cells = vec![Cell::Empty; self.width * self.height];
        for (x, y) in [(4, 1), (8, 1), (2, 4)] {
            self.set(x, y, Cell::Wall);
        }
        for (x, y) in [(7, 0), (1, 1), (0, 2), (7, 4)] {
            self.set(x, y, Cell::Lamp { on: false });
        }
        self.x = 3;
        self.y = 3;
        self.dir = 0;
        self.reset_state();
    }

    pub fn clear_map(&mut self) {
        self.cells = vec![Cell::Empty; self.width * self.height];
        self.reset_state();
    }

    /// Clears the registers and any command in progress (the room is kept).
    pub fn reset_state(&mut self) {
        self.data = 0;
        self.status = 0;
        self.pending = None;
        self.busy_ms = 0.0;
    }

    pub fn get(&self, x: usize, y: usize) -> Cell {
        self.cells[y * self.width + x]
    }

    pub fn set(&mut self, x: usize, y: usize, c: Cell) {
        if x < self.width && y < self.height {
            let i = y * self.width + x;
            if let Some(journal) = &mut self.cell_journal
                && !journal.iter().any(|(at, _)| *at == i)
            {
                journal.push((i, self.cells[i]));
            }
            self.cells[i] = c;
        }
    }

    pub(crate) fn begin_snapshot(&mut self) -> RobotSnapshot {
        self.cell_journal = Some(Vec::new());
        RobotSnapshot {
            width: self.width,
            height: self.height,
            x: self.x,
            y: self.y,
            dir: self.dir,
            data: self.data,
            status: self.status,
            pending: self.pending,
            busy_ms: self.busy_ms,
            cell_undo: Vec::new(),
        }
    }

    pub(crate) fn finish_snapshot(&mut self, mut snapshot: RobotSnapshot) -> RobotSnapshot {
        snapshot.cell_undo = self.cell_journal.take().unwrap_or_default();
        snapshot
    }

    pub(crate) fn cancel_snapshot(&mut self) {
        self.cell_journal = None;
    }

    pub(crate) fn restore(&mut self, snapshot: RobotSnapshot) {
        for (i, cell) in snapshot.cell_undo.into_iter().rev() {
            self.cells[i] = cell;
        }
        self.width = snapshot.width;
        self.height = snapshot.height;
        self.x = snapshot.x;
        self.y = snapshot.y;
        self.dir = snapshot.dir;
        self.data = snapshot.data;
        self.status = snapshot.status;
        self.pending = snapshot.pending;
        self.busy_ms = snapshot.busy_ms;
        self.cell_journal = None;
    }

    /// Map editing: empty -> wall -> lamp (on) -> lamp (off) -> empty.
    /// Clicking the robot's own cell turns it counter-clockwise.
    pub fn cycle_cell(&mut self, x: usize, y: usize) {
        if x >= self.width || y >= self.height {
            return;
        }
        if (x, y) == (self.x, self.y) {
            self.dir = (self.dir + 3) & 3;
            return;
        }
        let next = match self.get(x, y) {
            Cell::Empty => Cell::Wall,
            Cell::Wall => Cell::Lamp { on: true },
            Cell::Lamp { on: true } => Cell::Lamp { on: false },
            Cell::Lamp { on: false } => Cell::Empty,
        };
        self.set(x, y, next);
    }

    /// Moves the robot to an empty cell (map editing).
    pub fn place(&mut self, x: usize, y: usize) {
        if x < self.width && y < self.height && self.get(x, y) == Cell::Empty {
            self.x = x;
            self.y = y;
        }
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

    pub fn busy(&self) -> bool {
        self.status & STATUS_BUSY != 0
    }

    pub fn read_data(&mut self) -> u8 {
        self.status &= !STATUS_DATA;
        self.data
    }

    pub fn command(&mut self, cmd: u8) {
        if cmd == 0 {
            return;
        }
        if self.busy() {
            self.status |= STATUS_ERROR;
            return;
        }
        self.status &= !STATUS_ERROR;
        match cmd {
            1..=3 => {
                self.status |= STATUS_BUSY;
                self.pending = Some(cmd);
                self.busy_ms = MOVE_MS;
            }
            4 => {
                self.data = match self.front().map(|(x, y)| self.get(x, y)) {
                    None | Some(Cell::Wall) => 255,
                    Some(Cell::Empty) => 0,
                    Some(Cell::Lamp { on: true }) => 7,
                    Some(Cell::Lamp { on: false }) => 8,
                };
                self.status |= STATUS_DATA;
            }
            5 | 6 => {
                let on = cmd == 5;
                match self.front() {
                    Some((x, y)) if self.get(x, y) == Cell::Lamp { on: !on } => self.set(x, y, Cell::Lamp { on }),
                    // Already in that state, or not a lamp.
                    _ => self.status |= STATUS_ERROR,
                }
            }
            _ => self.status |= STATUS_ERROR,
        }
    }

    /// Advances the robot by `ms` milliseconds.
    pub fn tick(&mut self, ms: f64) {
        let Some(cmd) = self.pending else { return };
        self.busy_ms -= ms;
        if self.busy_ms > 0.0 {
            return;
        }
        self.pending = None;
        self.status &= !STATUS_BUSY;
        match cmd {
            1 => match self.front() {
                Some((x, y)) if self.get(x, y) == Cell::Empty => {
                    self.x = x;
                    self.y = y;
                }
                _ => self.status |= STATUS_ERROR,
            },
            2 => self.dir = (self.dir + 3) & 3,
            _ => self.dir = (self.dir + 1) & 3,
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
        r.x = 0;
        r.y = 0;
        r.dir = 1; // east
        r.set(2, 0, Cell::Lamp { on: false });
        r.command(1);
        assert!(r.busy());
        r.command(3); // rejected while busy
        assert_eq!(r.status() & STATUS_ERROR, STATUS_ERROR);
        r.tick(MOVE_MS);
        assert_eq!((r.x, r.y), (1, 0));
        r.command(4);
        assert_eq!(r.status() & STATUS_DATA, STATUS_DATA);
        assert_eq!(r.read_data(), 8);
        assert_eq!(r.status() & STATUS_DATA, 0);
        r.command(5);
        assert_eq!(r.get(2, 0), Cell::Lamp { on: true });
        assert_eq!(r.status() & STATUS_ERROR, 0);
        r.command(5); // already on
        assert_eq!(r.status() & STATUS_ERROR, STATUS_ERROR);
        r.command(1); // blocked by the lamp
        r.tick(MOVE_MS);
        assert_eq!(r.status() & STATUS_ERROR, STATUS_ERROR);
        r.command(2); // turn left -> north; examine sees the edge as a wall
        r.tick(MOVE_MS);
        r.command(4);
        assert_eq!(r.read_data(), 255);
    }

    #[test]
    fn default_room_matches_emu8086() {
        let r = Robot::new();
        assert_eq!((r.width, r.height, r.x, r.y, r.dir), (9, 6, 3, 3, 0));
        assert_eq!(r.cells.iter().filter(|c| **c == Cell::Wall).count(), 3);
        assert_eq!(r.cells.iter().filter(|c| matches!(c, Cell::Lamp { .. })).count(), 4);
    }
}
