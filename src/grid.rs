//! The shooter's view of the enemy ocean: what we have learned from our shots.

use crate::domain::{Coord, FLEET, ShipKind, ShotResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Knowledge {
    Unknown,
    Miss,
    /// Part of a ship that is still afloat.
    Hit,
    /// Part of a ship that has been sunk.
    Sunk,
    /// Never shot at, but cannot hold a ship because it touches a sunk ship.
    Water,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordError {
    AlreadyKnown,
    /// A sunk report whose squares do not include the square that was shot.
    InconsistentReport,
}

#[derive(Debug, Clone)]
pub struct TargetGrid {
    cells: [[Knowledge; 10]; 10],
    sunk: Vec<ShipKind>,
}

impl Default for TargetGrid {
    fn default() -> Self {
        Self::new()
    }
}

impl TargetGrid {
    pub fn new() -> Self {
        TargetGrid {
            cells: [[Knowledge::Unknown; 10]; 10],
            sunk: Vec::new(),
        }
    }

    pub fn get(&self, coord: Coord) -> Knowledge {
        self.cells[coord.y as usize][coord.x as usize]
    }

    fn set(&mut self, coord: Coord, knowledge: Knowledge) {
        self.cells[coord.y as usize][coord.x as usize] = knowledge;
    }

    /// Squares that are still worth shooting at.
    pub fn can_target(&self, coord: Coord) -> bool {
        self.get(coord) == Knowledge::Unknown
    }

    pub fn record(&mut self, coord: Coord, result: &ShotResult) -> Result<(), RecordError> {
        if !self.can_target(coord) {
            return Err(RecordError::AlreadyKnown);
        }
        match result {
            ShotResult::Miss => self.set(coord, Knowledge::Miss),
            ShotResult::Hit => self.set(coord, Knowledge::Hit),
            ShotResult::Sunk { kind, cells } => {
                if !cells.contains(&coord) || self.sunk.contains(kind) {
                    return Err(RecordError::InconsistentReport);
                }
                for cell in cells {
                    self.set(*cell, Knowledge::Sunk);
                }
                for neighbour in cells.iter().flat_map(|c| c.surrounding()) {
                    if self.get(neighbour) == Knowledge::Unknown {
                        self.set(neighbour, Knowledge::Water);
                    }
                }
                self.sunk.push(*kind);
            }
        }
        Ok(())
    }

    pub fn sunk_ships(&self) -> &[ShipKind] {
        &self.sunk
    }

    /// Ships of the fleet not yet sunk, largest first.
    pub fn remaining_ships(&self) -> Vec<ShipKind> {
        FLEET
            .into_iter()
            .filter(|kind| !self.sunk.contains(kind))
            .collect()
    }

    pub fn fleet_destroyed(&self) -> bool {
        self.remaining_ships().is_empty()
    }

    /// Hits on ships that are still afloat.
    pub fn open_hits(&self) -> Vec<Coord> {
        Coord::all()
            .filter(|c| self.get(*c) == Knowledge::Hit)
            .collect()
    }
}

#[cfg(test)]
#[path = "tests/grid.rs"]
mod tests;
