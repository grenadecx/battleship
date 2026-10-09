//! Screen geometry in virtual pixels: where the boards sit, which square a
//! point falls on, and the arc a missile flies along.

use crate::domain::{BOARD_SIZE, Coord};
use crate::game::Shot;
use macroquad::math::{Vec2, vec2};

pub const CELL: f32 = 38.0;
pub const BOARD_PX: f32 = CELL * BOARD_SIZE as f32;
/// Top left corner of our own board.
pub const LEFT_BOARD: Vec2 = Vec2::new(110.0, 165.0);
/// Top left corner of the enemy board.
pub const RIGHT_BOARD: Vec2 = Vec2::new(710.0, 165.0);
const _: () = assert!(
    LEFT_BOARD.x + BOARD_PX < RIGHT_BOARD.x,
    "boards must not overlap"
);

pub fn cell_origin(board: Vec2, c: Coord) -> Vec2 {
    board + vec2(c.x as f32 * CELL, c.y as f32 * CELL)
}

pub fn cell_center(board: Vec2, c: Coord) -> Vec2 {
    cell_origin(board, c) + vec2(CELL / 2.0, CELL / 2.0)
}

/// The square of `board` under `point`, if any.
pub fn board_cell_at(board: Vec2, point: Vec2) -> Option<Coord> {
    let local = (point - board) / CELL;
    if local.x < 0.0 || local.y < 0.0 {
        return None;
    }
    Coord::try_new(local.x as i32, local.y as i32)
}

/// Launch and landing points: our shells rise from below our board, theirs
/// drop in from above the enemy board.
pub fn missile_path(shot: Shot) -> (Vec2, Vec2) {
    match shot {
        Shot::Outgoing(target) => (
            LEFT_BOARD + vec2(BOARD_PX / 2.0, BOARD_PX + 40.0),
            cell_center(RIGHT_BOARD, target),
        ),
        Shot::Incoming(target) => (
            RIGHT_BOARD + vec2(BOARD_PX / 2.0, -120.0),
            cell_center(LEFT_BOARD, target),
        ),
    }
}

/// Position along a ballistic arc at progress `p` in `0.0..=1.0`.
pub fn missile_position(from: Vec2, to: Vec2, p: f32) -> Vec2 {
    let arc = from.distance(to) * 0.35;
    from.lerp(to, p) - vec2(0.0, arc * 4.0 * p * (1.0 - p))
}

#[cfg(test)]
#[path = "tests/layout.rs"]
mod tests;
