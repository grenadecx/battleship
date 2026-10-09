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

#[test]
fn default_grid_is_a_new_grid() {
    let grid = TargetGrid::default();
    assert_knowledge(&grid, &Coord::all().collect::<Vec<_>>(), Knowledge::Unknown);
}
