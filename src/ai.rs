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
#[path = "tests/ai.rs"]
mod tests;
