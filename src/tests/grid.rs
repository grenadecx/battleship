use super::*;
use ShipKind::*;

fn c(x: u8, y: u8) -> Coord {
    Coord::new(x, y)
}

#[test]
fn everything_starts_unknown_and_targetable() {
    let grid = TargetGrid::new();
    assert!(Coord::all().all(|co| grid.get(co) == Knowledge::Unknown));
    assert!(Coord::all().all(|co| grid.can_target(co)));
    assert_eq!(grid.remaining_ships(), FLEET.to_vec());
}

#[test]
fn records_a_miss() {
    let mut grid = TargetGrid::new();
    grid.record(c(1, 1), &ShotResult::Miss).unwrap();
    assert_eq!(grid.get(c(1, 1)), Knowledge::Miss);
    assert!(!grid.can_target(c(1, 1)));
}

#[test]
fn records_a_hit_as_open() {
    let mut grid = TargetGrid::new();
    grid.record(c(1, 1), &ShotResult::Hit).unwrap();
    assert_eq!(grid.get(c(1, 1)), Knowledge::Hit);
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
fn sinking_marks_the_whole_ship_and_its_surroundings() {
    let mut grid = TargetGrid::new();
    grid.record(c(4, 4), &ShotResult::Hit).unwrap();
    grid.record(
        c(5, 4),
        &ShotResult::Sunk {
            kind: Destroyer,
            cells: vec![c(4, 4), c(5, 4)],
        },
    )
    .unwrap();

    assert_eq!(grid.get(c(4, 4)), Knowledge::Sunk);
    assert_eq!(grid.get(c(5, 4)), Knowledge::Sunk);
    assert!(grid.open_hits().is_empty());
    for water in [
        c(3, 3),
        c(3, 4),
        c(3, 5),
        c(6, 3),
        c(6, 4),
        c(6, 5),
        c(4, 3),
        c(5, 5),
    ] {
        assert_eq!(grid.get(water), Knowledge::Water, "{water:?}");
        assert!(!grid.can_target(water));
    }
    assert_eq!(grid.get(c(7, 4)), Knowledge::Unknown);
    assert_eq!(grid.sunk_ships(), &[Destroyer]);
    assert!(!grid.remaining_ships().contains(&Destroyer));
}

#[test]
fn deduced_water_does_not_overwrite_misses() {
    let mut grid = TargetGrid::new();
    grid.record(c(3, 4), &ShotResult::Miss).unwrap();
    grid.record(
        c(4, 4),
        &ShotResult::Sunk {
            kind: Destroyer,
            cells: vec![c(4, 4), c(5, 4)],
        },
    )
    .unwrap();
    assert_eq!(grid.get(c(3, 4)), Knowledge::Miss);
}

#[test]
fn sunk_report_must_contain_the_shot_square() {
    let mut grid = TargetGrid::new();
    assert_eq!(
        grid.record(
            c(0, 0),
            &ShotResult::Sunk {
                kind: Destroyer,
                cells: vec![c(4, 4), c(5, 4)],
            },
        ),
        Err(RecordError::InconsistentReport)
    );
}

#[test]
fn fleet_destroyed_after_five_sinkings() {
    let mut grid = TargetGrid::new();
    for (row, kind) in FLEET.iter().enumerate() {
        assert!(!grid.fleet_destroyed());
        let y = row as u8 * 2;
        let cells: Vec<Coord> = (0..kind.size()).map(|x| c(x, y)).collect();
        grid.record(
            cells[0],
            &ShotResult::Sunk {
                kind: *kind,
                cells: cells.clone(),
            },
        )
        .unwrap();
    }
    assert!(grid.fleet_destroyed());
    assert!(grid.remaining_ships().is_empty());
}

#[test]
fn default_grid_is_all_unknown() {
    let grid = TargetGrid::default();
    assert!(Coord::all().all(|c| grid.get(c) == Knowledge::Unknown));
    assert!(grid.open_hits().is_empty());
}
