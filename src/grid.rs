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
mod tests {
    use super::*;
    use crate::test_support::{c, row_fleet_placements, sunk};
    use ShipKind::*;

    fn assert_knowledge(grid: &TargetGrid, squares: &[Coord], expected: Knowledge) {
        for square in squares {
            assert_eq!(grid.get(*square), expected, "{square:?}");
        }
    }

    /// A destroyer on (4,4)-(5,4), hit once and then sunk.
    fn grid_with_sunk_destroyer() -> TargetGrid {
        let mut grid = TargetGrid::new();
        grid.record(c(4, 4), &ShotResult::Hit).unwrap();
        grid.record(c(5, 4), &sunk(Destroyer, &[c(4, 4), c(5, 4)]))
            .unwrap();
        grid
    }

    fn sink_ships(grid: &mut TargetGrid, count: usize) {
        for placement in &row_fleet_placements()[..count] {
            let cells = placement.cells().unwrap();
            grid.record(cells[0], &sunk(placement.kind, &cells))
                .unwrap();
        }
    }

    #[test]
    fn every_square_starts_unknown() {
        let grid = TargetGrid::new();
        assert!(Coord::all().all(|co| grid.get(co) == Knowledge::Unknown));
    }

    #[test]
    fn every_square_starts_targetable() {
        let grid = TargetGrid::new();
        assert!(Coord::all().all(|co| grid.can_target(co)));
    }

    #[test]
    fn whole_fleet_starts_afloat() {
        assert_eq!(TargetGrid::new().remaining_ships(), FLEET.to_vec());
    }

    #[test]
    fn records_a_miss() {
        let mut grid = TargetGrid::new();
        grid.record(c(1, 1), &ShotResult::Miss).unwrap();
        assert_eq!(grid.get(c(1, 1)), Knowledge::Miss);
    }

    #[test]
    fn recorded_square_cannot_be_targeted() {
        let mut grid = TargetGrid::new();
        grid.record(c(1, 1), &ShotResult::Miss).unwrap();
        assert!(!grid.can_target(c(1, 1)));
    }

    #[test]
    fn records_a_hit() {
        let mut grid = TargetGrid::new();
        grid.record(c(1, 1), &ShotResult::Hit).unwrap();
        assert_eq!(grid.get(c(1, 1)), Knowledge::Hit);
    }

    #[test]
    fn hit_on_a_ship_afloat_is_open() {
        let mut grid = TargetGrid::new();
        grid.record(c(1, 1), &ShotResult::Hit).unwrap();
        assert_eq!(grid.open_hits(), vec![c(1, 1)]);
    }

    #[test]
    fn cannot_record_the_same_square_twice() {
        let mut grid = TargetGrid::new();
        grid.record(c(1, 1), &ShotResult::Miss).unwrap();
        assert_eq!(
            grid.record(c(1, 1), &ShotResult::Hit),
            Err(RecordError::AlreadyKnown)
        );
    }

    #[test]
    fn sinking_marks_every_square_of_the_ship_as_sunk() {
        assert_knowledge(
            &grid_with_sunk_destroyer(),
            &[c(4, 4), c(5, 4)],
            Knowledge::Sunk,
        );
    }

    #[test]
    fn sinking_closes_the_hits_on_that_ship() {
        assert!(grid_with_sunk_destroyer().open_hits().is_empty());
    }

    #[test]
    fn sinking_marks_the_surrounding_squares_as_water() {
        assert_knowledge(
            &grid_with_sunk_destroyer(),
            &[
                c(3, 3),
                c(4, 3),
                c(5, 3),
                c(6, 3),
                c(3, 4),
                c(6, 4),
                c(3, 5),
                c(4, 5),
                c(5, 5),
                c(6, 5),
            ],
            Knowledge::Water,
        );
    }

    #[test]
    fn deduced_water_stops_one_square_from_the_ship() {
        assert_eq!(grid_with_sunk_destroyer().get(c(7, 4)), Knowledge::Unknown);
    }

    #[test]
    fn sunk_ship_is_listed_as_sunk() {
        assert_eq!(grid_with_sunk_destroyer().sunk_ships(), &[Destroyer]);
    }

    #[test]
    fn sunk_ship_is_no_longer_remaining() {
        assert!(
            !grid_with_sunk_destroyer()
                .remaining_ships()
                .contains(&Destroyer)
        );
    }

    #[test]
    fn deduced_water_does_not_overwrite_misses() {
        let mut grid = TargetGrid::new();
        grid.record(c(3, 4), &ShotResult::Miss).unwrap();
        grid.record(c(4, 4), &sunk(Destroyer, &[c(4, 4), c(5, 4)]))
            .unwrap();
        assert_eq!(grid.get(c(3, 4)), Knowledge::Miss);
    }

    #[test]
    fn sunk_report_must_contain_the_shot_square() {
        assert_eq!(
            TargetGrid::new().record(c(0, 0), &sunk(Destroyer, &[c(4, 4), c(5, 4)])),
            Err(RecordError::InconsistentReport)
        );
    }

    #[test]
    fn fleet_is_not_destroyed_while_a_ship_remains() {
        let mut grid = TargetGrid::new();
        sink_ships(&mut grid, FLEET.len() - 1);
        assert!(!grid.fleet_destroyed());
    }

    #[test]
    fn fleet_is_destroyed_once_every_ship_is_sunk() {
        let mut grid = TargetGrid::new();
        sink_ships(&mut grid, FLEET.len());
        assert!(grid.fleet_destroyed());
    }
}
