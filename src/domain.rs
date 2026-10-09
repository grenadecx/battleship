//! Core domain: coordinates, ships, and a player's own board.

use crate::rng::Rng;
use std::collections::HashSet;
use std::fmt;

pub const BOARD_SIZE: u8 = 10;

/// A square on the 10x10 grid. `x` is the column (A-J), `y` the row (1-10).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Coord {
    pub x: u8,
    pub y: u8,
}

impl Coord {
    /// Panics if the coordinate is off the board; use [`Coord::try_new`] for untrusted input.
    pub fn new(x: u8, y: u8) -> Self {
        assert!(
            x < BOARD_SIZE && y < BOARD_SIZE,
            "({x}, {y}) is off the board"
        );
        Coord { x, y }
    }

    pub fn try_new(x: i32, y: i32) -> Option<Self> {
        let size = BOARD_SIZE as i32;
        ((0..size).contains(&x) && (0..size).contains(&y)).then(|| Coord::new(x as u8, y as u8))
    }

    /// Every square, row by row.
    pub fn all() -> impl Iterator<Item = Coord> {
        (0..BOARD_SIZE).flat_map(|y| (0..BOARD_SIZE).map(move |x| Coord { x, y }))
    }

    fn offset(self, dx: i32, dy: i32) -> Option<Coord> {
        Coord::try_new(self.x as i32 + dx, self.y as i32 + dy)
    }

    /// Up, down, left and right neighbours that are on the board.
    pub fn orthogonal_neighbors(self) -> Vec<Coord> {
        [(0, -1), (-1, 0), (1, 0), (0, 1)]
            .into_iter()
            .filter_map(|(dx, dy)| self.offset(dx, dy))
            .collect()
    }

    /// All eight surrounding squares that are on the board.
    pub fn surrounding(self) -> Vec<Coord> {
        (-1..=1)
            .flat_map(|dy| (-1..=1).map(move |dx| (dx, dy)))
            .filter(|&(dx, dy)| (dx, dy) != (0, 0))
            .filter_map(|(dx, dy)| self.offset(dx, dy))
            .collect()
    }

    /// Human readable label such as `A1` or `J10`.
    pub fn label(self) -> String {
        format!("{}{}", (b'A' + self.x) as char, self.y + 1)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Orientation {
    Horizontal,
    Vertical,
}

impl Orientation {
    pub fn toggled(self) -> Self {
        match self {
            Orientation::Horizontal => Orientation::Vertical,
            Orientation::Vertical => Orientation::Horizontal,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ShipKind {
    Carrier,
    Battleship,
    Cruiser,
    Submarine,
    Destroyer,
}

/// The Milton Bradley fleet, largest first.
pub const FLEET: [ShipKind; 5] = [
    ShipKind::Carrier,
    ShipKind::Battleship,
    ShipKind::Cruiser,
    ShipKind::Submarine,
    ShipKind::Destroyer,
];

impl ShipKind {
    pub fn size(self) -> u8 {
        match self {
            ShipKind::Carrier => 5,
            ShipKind::Battleship => 4,
            ShipKind::Cruiser => 3,
            ShipKind::Submarine => 3,
            ShipKind::Destroyer => 2,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            ShipKind::Carrier => "Carrier",
            ShipKind::Battleship => "Battleship",
            ShipKind::Cruiser => "Cruiser",
            ShipKind::Submarine => "Submarine",
            ShipKind::Destroyer => "Destroyer",
        }
    }

    pub fn from_name(name: &str) -> Option<ShipKind> {
        FLEET.into_iter().find(|kind| kind.name() == name)
    }
}

impl fmt::Display for ShipKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Where a ship sits: its top/left-most square and direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Placement {
    pub kind: ShipKind,
    pub origin: Coord,
    pub orientation: Orientation,
}

impl Placement {
    pub fn new(kind: ShipKind, origin: Coord, orientation: Orientation) -> Self {
        Placement {
            kind,
            origin,
            orientation,
        }
    }

    /// The squares covered, or `None` if the ship would stick out of the board.
    pub fn cells(&self) -> Option<Vec<Coord>> {
        let (dx, dy) = match self.orientation {
            Orientation::Horizontal => (1, 0),
            Orientation::Vertical => (0, 1),
        };
        (0..self.kind.size() as i32)
            .map(|i| self.origin.offset(dx * i, dy * i))
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShotResult {
    Miss,
    Hit,
    Sunk { kind: ShipKind, cells: Vec<Coord> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaceError {
    OutOfBounds,
    Overlaps,
    TouchesAnotherShip,
    AlreadyPlaced,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FireError {
    AlreadyFiredAt,
}

#[derive(Debug, Clone)]
struct Ship {
    placement: Placement,
    cells: Vec<Coord>,
    hits: HashSet<Coord>,
}

impl Ship {
    fn is_sunk(&self) -> bool {
        self.hits.len() == self.cells.len()
    }
}

/// A player's own ocean: their ships and the shots received.
#[derive(Debug, Clone, Default)]
pub struct Board {
    ships: Vec<Ship>,
    shots: HashSet<Coord>,
}

impl Board {
    pub fn new() -> Self {
        Self::default()
    }

    /// A board with the full fleet placed at random, obeying the placement rules.
    pub fn random(rng: &mut Rng) -> Self {
        'attempt: loop {
            let mut board = Board::new();
            for kind in FLEET {
                let options: Vec<Placement> = Coord::all()
                    .flat_map(|origin| {
                        [Orientation::Horizontal, Orientation::Vertical]
                            .map(|o| Placement::new(kind, origin, o))
                    })
                    .filter(|p| board.check_placement(p).is_ok())
                    .collect();
                match rng.pick(&options) {
                    Some(placement) => board.place(*placement).expect("checked placement"),
                    None => continue 'attempt,
                }
            }
            return board;
        }
    }

    pub fn check_placement(&self, placement: &Placement) -> Result<(), PlaceError> {
        if self.has_ship(placement.kind) {
            return Err(PlaceError::AlreadyPlaced);
        }
        let cells = placement.cells().ok_or(PlaceError::OutOfBounds)?;
        if cells.iter().any(|c| self.ship_at(*c).is_some()) {
            return Err(PlaceError::Overlaps);
        }
        if cells
            .iter()
            .flat_map(|c| c.surrounding())
            .any(|c| self.ship_at(c).is_some())
        {
            return Err(PlaceError::TouchesAnotherShip);
        }
        Ok(())
    }

    pub fn place(&mut self, placement: Placement) -> Result<(), PlaceError> {
        self.check_placement(&placement)?;
        self.ships.push(Ship {
            placement,
            cells: placement.cells().expect("checked placement"),
            hits: HashSet::new(),
        });
        Ok(())
    }

    /// Takes a ship back off the board (only while setting up).
    pub fn remove(&mut self, kind: ShipKind) -> Option<Placement> {
        let index = self.ships.iter().position(|s| s.placement.kind == kind)?;
        Some(self.ships.remove(index).placement)
    }

    pub fn placements(&self) -> Vec<Placement> {
        self.ships.iter().map(|s| s.placement).collect()
    }

    pub fn ship_at(&self, coord: Coord) -> Option<Placement> {
        self.ships
            .iter()
            .find(|s| s.cells.contains(&coord))
            .map(|s| s.placement)
    }

    pub fn has_ship(&self, kind: ShipKind) -> bool {
        self.ships.iter().any(|s| s.placement.kind == kind)
    }

    /// All ships of the fleet are on the board.
    pub fn is_fleet_complete(&self) -> bool {
        FLEET.iter().all(|kind| self.has_ship(*kind))
    }

    pub fn fire(&mut self, coord: Coord) -> Result<ShotResult, FireError> {
        if !self.shots.insert(coord) {
            return Err(FireError::AlreadyFiredAt);
        }
        let Some(ship) = self.ships.iter_mut().find(|s| s.cells.contains(&coord)) else {
            return Ok(ShotResult::Miss);
        };
        ship.hits.insert(coord);
        if ship.is_sunk() {
            Ok(ShotResult::Sunk {
                kind: ship.placement.kind,
                cells: ship.cells.clone(),
            })
        } else {
            Ok(ShotResult::Hit)
        }
    }

    pub fn was_shot(&self, coord: Coord) -> bool {
        self.shots.contains(&coord)
    }

    pub fn is_sunk(&self, kind: ShipKind) -> bool {
        self.ships
            .iter()
            .any(|s| s.placement.kind == kind && s.is_sunk())
    }

    pub fn all_sunk(&self) -> bool {
        !self.ships.is_empty() && self.ships.iter().all(Ship::is_sunk)
    }
}

#[cfg(test)]
#[path = "tests/domain.rs"]
mod tests;
