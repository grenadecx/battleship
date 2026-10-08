//! The computer's targeting strategy.
//!
//! * Target mode: once a ship is hit, finish it off by shooting along the line of hits.
//! * Hunt mode: shoot where the remaining ships could most likely be, by counting
//!   how many legal positions of the remaining ships cover each square.

use crate::domain::{BOARD_SIZE, Coord, Orientation, Placement};
use crate::grid::{Knowledge, TargetGrid};
use crate::rng::Rng;

/// Picks the next square to fire at. Panics if no square is left to target.
pub fn choose_target(grid: &TargetGrid, rng: &mut Rng) -> Coord {
    let candidates = finishing_shots(grid);
    let candidates = if candidates.is_empty() {
        best_hunting_squares(grid)
    } else {
        candidates
    };
    if let Some(target) = rng.pick(&candidates) {
        return *target;
    }
    let unknown: Vec<Coord> = Coord::all().filter(|c| grid.can_target(*c)).collect();
    *rng.pick(&unknown).expect("no square left to target")
}

/// Squares that continue a damaged ship: along the line of hits if there are
/// several in a row, otherwise around a lone hit.
fn finishing_shots(grid: &TargetGrid) -> Vec<Coord> {
    let hits = grid.open_hits();
    for &hit in &hits {
        for (dx, dy) in [(1, 0), (0, 1)] {
            let next = Coord::try_new(hit.x as i32 + dx, hit.y as i32 + dy);
            if next.is_some_and(|n| grid.get(n) == Knowledge::Hit) {
                let ends = line_ends(grid, hit, dx, dy);
                if !ends.is_empty() {
                    return ends;
                }
            }
        }
    }
    hits.iter()
        .flat_map(|h| h.orthogonal_neighbors())
        .filter(|n| grid.can_target(*n))
        .collect()
}

/// The open squares just beyond both ends of the run of hits through `start`.
fn line_ends(grid: &TargetGrid, start: Coord, dx: i32, dy: i32) -> Vec<Coord> {
    let mut ends = Vec::new();
    for direction in [1, -1] {
        let mut step = 1;
        loop {
            let at = Coord::try_new(
                start.x as i32 + dx * direction * step,
                start.y as i32 + dy * direction * step,
            );
            match at.map(|c| (c, grid.get(c))) {
                Some((_, Knowledge::Hit)) => step += 1,
                Some((c, Knowledge::Unknown)) => {
                    ends.push(c);
                    break;
                }
                _ => break,
            }
        }
    }
    ends
}

/// The squares covered by the most legal positions of the ships still afloat.
fn best_hunting_squares(grid: &TargetGrid) -> Vec<Coord> {
    let mut density = [[0u32; BOARD_SIZE as usize]; BOARD_SIZE as usize];
    for kind in grid.remaining_ships() {
        for origin in Coord::all() {
            for orientation in [Orientation::Horizontal, Orientation::Vertical] {
                let Some(cells) = Placement::new(kind, origin, orientation).cells() else {
                    continue;
                };
                if cells.iter().all(|c| grid.can_target(*c)) {
                    for c in cells {
                        density[c.y as usize][c.x as usize] += 1;
                    }
                }
            }
        }
    }
    let best = Coord::all()
        .map(|c| density[c.y as usize][c.x as usize])
        .max()
        .unwrap_or(0);
    if best == 0 {
        return Vec::new();
    }
    Coord::all()
        .filter(|c| density[c.y as usize][c.x as usize] == best)
        .collect()
}

#[cfg(test)]
mod tests {
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
}
