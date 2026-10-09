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
mod tests {
    use super::*;

    #[test]
    fn every_cell_center_maps_back_to_its_cell() {
        for board in [LEFT_BOARD, RIGHT_BOARD] {
            for c in Coord::all() {
                assert_eq!(board_cell_at(board, cell_center(board, c)), Some(c));
            }
        }
    }

    #[test]
    fn board_corners_are_inclusive_at_the_top_left_only() {
        assert_eq!(
            board_cell_at(LEFT_BOARD, LEFT_BOARD),
            Some(Coord::new(0, 0))
        );
        let bottom_right = LEFT_BOARD + vec2(BOARD_PX, BOARD_PX);
        assert_eq!(board_cell_at(LEFT_BOARD, bottom_right), None);
        let last = bottom_right - vec2(0.5, 0.5);
        let edge = BOARD_SIZE - 1;
        assert_eq!(
            board_cell_at(LEFT_BOARD, last),
            Some(Coord::new(edge, edge))
        );
    }

    #[test]
    fn points_off_the_board_have_no_cell() {
        for point in [
            LEFT_BOARD - vec2(0.5, 0.0),
            LEFT_BOARD - vec2(0.0, 0.5),
            LEFT_BOARD + vec2(BOARD_PX + 1.0, 10.0),
            LEFT_BOARD + vec2(10.0, BOARD_PX + 1.0),
        ] {
            assert_eq!(board_cell_at(LEFT_BOARD, point), None, "{point:?}");
        }
    }

    #[test]
    fn a_point_on_one_board_is_off_the_other() {
        let on_left = cell_center(LEFT_BOARD, Coord::new(5, 5));
        assert_eq!(board_cell_at(RIGHT_BOARD, on_left), None);
    }

    #[test]
    fn missiles_land_on_the_targeted_board() {
        let target = Coord::new(3, 7);
        let (_, to) = missile_path(Shot::Outgoing(target));
        assert_eq!(board_cell_at(RIGHT_BOARD, to), Some(target));
        let (_, to) = missile_path(Shot::Incoming(target));
        assert_eq!(board_cell_at(LEFT_BOARD, to), Some(target));
    }

    #[test]
    fn missile_starts_and_ends_on_its_path_and_arcs_upwards() {
        let (from, to) = missile_path(Shot::Outgoing(Coord::new(0, 0)));
        assert_eq!(missile_position(from, to, 0.0), from);
        assert!(missile_position(from, to, 1.0).distance(to) < 1e-3);
        let mid = missile_position(from, to, 0.5);
        let straight = from.lerp(to, 0.5);
        assert!((mid.x - straight.x).abs() < 1e-3);
        assert!(
            mid.y < straight.y,
            "screen y grows downwards, so up is smaller"
        );
    }
}
