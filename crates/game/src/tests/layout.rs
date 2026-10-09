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
