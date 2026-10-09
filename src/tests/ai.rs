use super::*;
use crate::domain::{Board, ShipKind, ShotResult};

fn c(x: u8, y: u8) -> Coord {
    Coord::new(x, y)
}

/// Plays the computer against a board until the fleet is sunk; returns the number of shots.
fn shots_to_win(board_seed: u64, ai_seed: u64) -> usize {
    let mut board = Board::random(&mut Rng::new(board_seed));
    let mut grid = TargetGrid::new();
    let mut rng = Rng::new(ai_seed);
    let mut shots = 0;
    while !board.all_sunk() {
        let target = choose_target(&grid, &mut rng);
        let result = board
            .fire(target)
            .expect("AI must never fire at the same square twice");
        grid.record(target, &result).unwrap();
        shots += 1;
        assert!(shots <= 100, "AI took more than 100 shots");
    }
    shots
}

#[test]
fn opens_on_an_untouched_board() {
    let grid = TargetGrid::new();
    let target = choose_target(&grid, &mut Rng::new(1));
    assert!(grid.can_target(target));
}

#[test]
fn never_targets_known_squares() {
    let mut grid = TargetGrid::new();
    let mut rng = Rng::new(3);
    for co in Coord::all().filter(|co| (co.x + co.y) % 3 != 0) {
        grid.record(co, &ShotResult::Miss).unwrap();
    }
    for _ in 0..50 {
        assert!(grid.can_target(choose_target(&grid, &mut rng)));
    }
}

#[test]
fn after_a_single_hit_shoots_next_to_it() {
    let mut grid = TargetGrid::new();
    grid.record(c(5, 5), &ShotResult::Hit).unwrap();
    for seed in 0..20 {
        let target = choose_target(&grid, &mut Rng::new(seed));
        assert!(
            c(5, 5).orthogonal_neighbors().contains(&target),
            "{target:?}"
        );
    }
}

#[test]
fn follows_a_horizontal_line_of_hits() {
    let mut grid = TargetGrid::new();
    grid.record(c(4, 5), &ShotResult::Hit).unwrap();
    grid.record(c(5, 5), &ShotResult::Hit).unwrap();
    for seed in 0..20 {
        let target = choose_target(&grid, &mut Rng::new(seed));
        assert!([c(3, 5), c(6, 5)].contains(&target), "{target:?}");
    }
}

#[test]
fn follows_a_vertical_line_away_from_a_blocked_end() {
    let mut grid = TargetGrid::new();
    grid.record(c(2, 0), &ShotResult::Miss).unwrap();
    grid.record(c(2, 1), &ShotResult::Hit).unwrap();
    grid.record(c(2, 2), &ShotResult::Hit).unwrap();
    for seed in 0..10 {
        assert_eq!(choose_target(&grid, &mut Rng::new(seed)), c(2, 3));
    }
}

#[test]
fn follows_a_line_that_starts_at_the_board_edge() {
    let mut grid = TargetGrid::new();
    grid.record(c(0, 9), &ShotResult::Hit).unwrap();
    grid.record(c(1, 9), &ShotResult::Hit).unwrap();
    assert_eq!(choose_target(&grid, &mut Rng::new(0)), c(2, 9));
}

#[test]
fn hunts_away_from_sunk_ships() {
    let mut grid = TargetGrid::new();
    grid.record(
        c(0, 0),
        &ShotResult::Sunk {
            kind: ShipKind::Destroyer,
            cells: vec![c(0, 0), c(1, 0)],
        },
    )
    .unwrap();
    for seed in 0..20 {
        let target = choose_target(&grid, &mut Rng::new(seed));
        assert!(grid.can_target(target));
    }
}

#[test]
fn hunting_prefers_open_ocean_over_cramped_corners() {
    let grid = TargetGrid::new();
    for seed in 0..20 {
        let target = choose_target(&grid, &mut Rng::new(seed));
        assert!(
            (2..=7).contains(&target.x) && (2..=7).contains(&target.y),
            "{target:?}"
        );
    }
}

#[test]
fn always_sinks_the_whole_fleet() {
    for seed in 0..60 {
        shots_to_win(seed, seed + 1000);
    }
}

#[test]
fn plays_much_better_than_random_shooting() {
    let games = 60;
    let total: usize = (0..games).map(|s| shots_to_win(s + 500, s)).sum();
    let average = total as f64 / games as f64;
    // Random shooting needs ~95 shots on average.
    assert!(average < 60.0, "average shots {average}");
}

#[test]
fn a_blocked_line_of_hits_falls_back_to_the_squares_around_them() {
    let mut grid = TargetGrid::new();
    grid.record(c(0, 0), &ShotResult::Hit).unwrap();
    grid.record(c(1, 0), &ShotResult::Hit).unwrap();
    grid.record(c(2, 0), &ShotResult::Miss).unwrap();
    for seed in 0..20 {
        let target = choose_target(&grid, &mut Rng::new(seed));
        assert!([c(0, 1), c(1, 1)].contains(&target), "{target:?}");
    }
}
