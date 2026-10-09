use super::*;
use crate::domain::{Board, ShotResult};
use crate::test_support::c;

const SEEDS: u64 = 20;
const GAMES: u64 = 60;
/// Shooting at random squares needs about 95 shots to sink a whole fleet.
const MAX_AVERAGE_SHOTS: u64 = 60;

/// The AI only ever picks one of `allowed`, whatever its random seed.
fn assert_targets_only(grid: &TargetGrid, allowed: &[Coord]) {
    for seed in 0..SEEDS {
        let target = choose_target(grid, &mut Rng::new(seed));
        assert!(allowed.contains(&target), "seed {seed} chose {target:?}");
    }
}

fn grid_with(shots: &[(Coord, ShotResult)]) -> TargetGrid {
    let mut grid = TargetGrid::new();
    for (square, result) in shots {
        grid.record(*square, result).unwrap();
    }
    grid
}

/// Plays the computer against a random board until the fleet is sunk; returns the number of shots.
fn shots_to_win(board_seed: u64, ai_seed: u64) -> u64 {
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
    }
    shots
}

#[test]
fn opens_in_the_middle_of_an_empty_board() {
    assert_targets_only(&TargetGrid::new(), &[c(4, 4), c(4, 5), c(5, 4), c(5, 5)]);
}

#[test]
fn targets_the_only_square_left() {
    let last = c(7, 3);
    let mut grid = TargetGrid::new();
    for square in Coord::all().filter(|square| *square != last) {
        grid.record(square, &ShotResult::Miss).unwrap();
    }
    assert_targets_only(&grid, &[last]);
}

#[test]
fn after_a_single_hit_shoots_next_to_it() {
    let grid = grid_with(&[(c(5, 5), ShotResult::Hit)]);
    assert_targets_only(&grid, &[c(5, 4), c(4, 5), c(6, 5), c(5, 6)]);
}

#[test]
fn follows_a_horizontal_line_of_hits() {
    let grid = grid_with(&[(c(4, 5), ShotResult::Hit), (c(5, 5), ShotResult::Hit)]);
    assert_targets_only(&grid, &[c(3, 5), c(6, 5)]);
}

#[test]
fn follows_a_vertical_line_away_from_a_blocked_end() {
    let grid = grid_with(&[
        (c(2, 0), ShotResult::Miss),
        (c(2, 1), ShotResult::Hit),
        (c(2, 2), ShotResult::Hit),
    ]);
    assert_targets_only(&grid, &[c(2, 3)]);
}

#[test]
fn follows_a_line_that_starts_at_the_board_edge() {
    let grid = grid_with(&[(c(0, 9), ShotResult::Hit), (c(1, 9), ShotResult::Hit)]);
    assert_targets_only(&grid, &[c(2, 9)]);
}

#[test]
fn never_fires_twice_at_a_square_while_sinking_a_fleet() {
    for seed in 0..GAMES {
        shots_to_win(seed, seed + 1000);
    }
}

#[test]
fn plays_much_better_than_random_shooting() {
    let total: u64 = (0..GAMES).map(|seed| shots_to_win(seed + 500, seed)).sum();
    assert!(
        total < GAMES * MAX_AVERAGE_SHOTS,
        "average shots {}",
        total / GAMES
    );
}

#[test]
fn a_blocked_line_of_hits_falls_back_to_the_squares_around_it() {
    let grid = grid_with(&[
        (c(0, 0), ShotResult::Hit),
        (c(1, 0), ShotResult::Hit),
        (c(2, 0), ShotResult::Miss),
    ]);
    assert_targets_only(&grid, &[c(0, 1), c(1, 1)]);
}

#[test]
fn extends_a_line_of_hits_at_both_ends() {
    let grid = grid_with(&[(c(4, 5), ShotResult::Hit), (c(5, 5), ShotResult::Hit)]);
    let targets: std::collections::HashSet<Coord> = (0..SEEDS)
        .map(|seed| choose_target(&grid, &mut Rng::new(seed)))
        .collect();
    assert_eq!(targets, [c(3, 5), c(6, 5)].into());
}

#[test]
fn follows_a_vertical_line_back_past_a_blocked_far_end() {
    let grid = grid_with(&[
        (c(2, 3), ShotResult::Miss),
        (c(2, 1), ShotResult::Hit),
        (c(2, 2), ShotResult::Hit),
    ]);
    assert_targets_only(&grid, &[c(2, 0)]);
}
