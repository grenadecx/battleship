use super::*;
use crate::test_support::{board_with, c, row_fleet_placements};
use Orientation::*;
use ShipKind::*;

const RANDOM_BOARDS: u64 = 300;

fn p(kind: ShipKind, x: u8, y: u8, o: Orientation) -> Placement {
    Placement::new(kind, c(x, y), o)
}

fn sorted(mut coords: Vec<Coord>) -> Vec<Coord> {
    coords.sort();
    coords
}

// --- Coord ---

#[test]
fn try_new_accepts_the_corners_of_the_board() {
    assert_eq!(Coord::try_new(0, 0), Some(c(0, 0)));
    assert_eq!(Coord::try_new(9, 9), Some(c(9, 9)));
}

#[test]
fn try_new_rejects_squares_off_the_board() {
    assert_eq!(Coord::try_new(-1, 0), None);
    assert_eq!(Coord::try_new(0, 10), None);
    assert_eq!(Coord::try_new(10, 3), None);
}

#[test]
#[should_panic]
fn new_panics_off_the_board() {
    Coord::new(10, 0);
}

#[test]
fn all_enumerates_one_hundred_distinct_squares() {
    let all: HashSet<Coord> = Coord::all().collect();
    assert_eq!(all.len(), 100);
}

#[test]
fn corner_has_two_orthogonal_neighbours() {
    assert_eq!(
        sorted(c(0, 0).orthogonal_neighbors()),
        vec![c(0, 1), c(1, 0)]
    );
}

#[test]
fn corner_is_surrounded_by_three_squares() {
    assert_eq!(
        sorted(c(0, 0).surrounding()),
        vec![c(0, 1), c(1, 0), c(1, 1)]
    );
}

#[test]
fn middle_square_has_four_orthogonal_neighbours() {
    assert_eq!(
        sorted(c(5, 5).orthogonal_neighbors()),
        vec![c(4, 5), c(5, 4), c(5, 6), c(6, 5)]
    );
}

#[test]
fn middle_square_is_surrounded_by_eight_squares() {
    assert_eq!(
        sorted(c(5, 5).surrounding()),
        vec![
            c(4, 4),
            c(4, 5),
            c(4, 6),
            c(5, 4),
            c(5, 6),
            c(6, 4),
            c(6, 5),
            c(6, 6)
        ]
    );
}

#[test]
fn labels_use_letter_columns_and_one_based_rows() {
    assert_eq!(c(0, 0).label(), "A1");
    assert_eq!(c(9, 9).label(), "J10");
    assert_eq!(c(2, 4).label(), "C5");
}

// --- Ships ---

#[test]
fn fleet_matches_milton_bradley_sizes() {
    let sizes: Vec<u8> = FLEET.iter().map(|k| k.size()).collect();
    assert_eq!(sizes, vec![5, 4, 3, 3, 2]);
}

#[test]
fn ship_names_round_trip() {
    for kind in FLEET {
        assert_eq!(ShipKind::from_name(kind.name()), Some(kind));
    }
}

#[test]
fn unknown_ship_name_is_rejected() {
    assert_eq!(ShipKind::from_name("Dinghy"), None);
}

#[test]
fn horizontal_toggles_to_vertical() {
    assert_eq!(Horizontal.toggled(), Vertical);
}

#[test]
fn vertical_toggles_to_horizontal() {
    assert_eq!(Vertical.toggled(), Horizontal);
}

#[test]
fn horizontal_placement_extends_right() {
    assert_eq!(
        p(Destroyer, 3, 4, Horizontal).cells(),
        Some(vec![c(3, 4), c(4, 4)])
    );
}

#[test]
fn vertical_placement_extends_down() {
    assert_eq!(
        p(Cruiser, 3, 4, Vertical).cells(),
        Some(vec![c(3, 4), c(3, 5), c(3, 6)])
    );
}

#[test]
fn placement_sticking_out_to_the_right_has_no_cells() {
    assert_eq!(p(Carrier, 6, 0, Horizontal).cells(), None);
}

#[test]
fn placement_sticking_out_of_the_bottom_has_no_cells() {
    assert_eq!(p(Carrier, 0, 6, Vertical).cells(), None);
}

#[test]
fn placement_ending_on_the_edge_fits() {
    assert!(p(Carrier, 5, 0, Horizontal).cells().is_some());
}

// --- Placement rules ---

fn board_with_carrier() -> Board {
    board_with(&[p(Carrier, 0, 0, Horizontal)])
}

fn board_with_destroyer_at_origin() -> Board {
    board_with(&[p(Destroyer, 0, 0, Horizontal)])
}

#[test]
fn ship_can_be_placed_on_empty_board() {
    assert_eq!(Board::new().place(p(Carrier, 0, 0, Horizontal)), Ok(()));
}

#[test]
fn placed_ship_is_on_the_board() {
    assert!(board_with_carrier().has_ship(Carrier));
}

#[test]
fn placed_ship_occupies_its_last_square() {
    let board = board_with_carrier();
    assert_eq!(board.ship_at(c(4, 0)).map(|pl| pl.kind), Some(Carrier));
}

#[test]
fn placed_ship_does_not_occupy_the_square_past_its_end() {
    assert_eq!(board_with_carrier().ship_at(c(5, 0)), None);
}

#[test]
fn ship_cannot_stick_out_of_the_board() {
    assert_eq!(
        Board::new().place(p(Carrier, 8, 0, Horizontal)),
        Err(PlaceError::OutOfBounds)
    );
}

#[test]
fn ships_cannot_overlap() {
    assert_eq!(
        board_with_carrier().place(p(Battleship, 2, 0, Vertical)),
        Err(PlaceError::Overlaps)
    );
}

#[test]
fn ships_cannot_touch_side_by_side() {
    assert_eq!(
        board_with_carrier().place(p(Battleship, 0, 1, Horizontal)),
        Err(PlaceError::TouchesAnotherShip)
    );
}

#[test]
fn ships_cannot_touch_end_to_end() {
    assert_eq!(
        board_with_destroyer_at_origin().place(p(Cruiser, 2, 0, Horizontal)),
        Err(PlaceError::TouchesAnotherShip)
    );
}

#[test]
fn ships_cannot_touch_diagonally() {
    assert_eq!(
        board_with_destroyer_at_origin().place(p(Cruiser, 2, 1, Vertical)),
        Err(PlaceError::TouchesAnotherShip)
    );
}

#[test]
fn ships_one_row_apart_are_fine() {
    assert_eq!(
        board_with_destroyer_at_origin().place(p(Cruiser, 0, 2, Horizontal)),
        Ok(())
    );
}

#[test]
fn ships_one_column_apart_are_fine() {
    assert_eq!(
        board_with_destroyer_at_origin().place(p(Submarine, 3, 0, Horizontal)),
        Ok(())
    );
}

#[test]
fn each_kind_of_ship_can_only_be_placed_once() {
    assert_eq!(
        board_with_destroyer_at_origin().place(p(Destroyer, 5, 5, Horizontal)),
        Err(PlaceError::AlreadyPlaced)
    );
}

#[test]
fn check_placement_accepts_a_legal_placement() {
    assert_eq!(
        Board::new().check_placement(&p(Carrier, 0, 0, Vertical)),
        Ok(())
    );
}

#[test]
fn check_placement_does_not_modify_the_board() {
    let board = Board::new();
    let _ = board.check_placement(&p(Carrier, 0, 0, Vertical));
    assert!(!board.has_ship(Carrier));
}

#[test]
fn removing_a_ship_returns_its_placement() {
    assert_eq!(
        board_with_carrier().remove(Carrier),
        Some(p(Carrier, 0, 0, Horizontal))
    );
}

#[test]
fn removing_a_ship_that_is_not_on_the_board_returns_nothing() {
    assert_eq!(Board::new().remove(Carrier), None);
}

#[test]
fn removed_ship_frees_its_surroundings() {
    let mut board = board_with_carrier();
    board.remove(Carrier);
    assert_eq!(board.place(p(Battleship, 0, 1, Horizontal)), Ok(()));
}

#[test]
fn fleet_is_incomplete_while_a_ship_is_missing() {
    let four_ships = &row_fleet_placements()[..4];
    assert!(!board_with(four_ships).is_fleet_complete());
}

#[test]
fn fleet_is_complete_once_all_five_ships_are_placed() {
    assert!(board_with(&row_fleet_placements()).is_fleet_complete());
}

#[test]
fn placements_lists_every_placed_ship() {
    let placements = row_fleet_placements();
    assert_eq!(board_with(&placements).placements(), placements);
}

// --- Firing ---

fn board_with_destroyer() -> Board {
    board_with(&[p(Destroyer, 2, 2, Horizontal)])
}

fn sink(board: &mut Board, placement: Placement) {
    for cell in placement.cells().unwrap() {
        board.fire(cell).unwrap();
    }
}

#[test]
fn shot_into_open_water_misses() {
    assert_eq!(board_with_destroyer().fire(c(0, 0)), Ok(ShotResult::Miss));
}

#[test]
fn board_remembers_where_it_was_shot() {
    let mut board = board_with_destroyer();
    board.fire(c(0, 0)).unwrap();
    assert!(board.was_shot(c(0, 0)));
}

#[test]
fn shot_on_a_ship_hits() {
    assert_eq!(board_with_destroyer().fire(c(2, 2)), Ok(ShotResult::Hit));
}

#[test]
fn ship_with_squares_left_is_not_sunk() {
    let mut board = board_with_destroyer();
    board.fire(c(2, 2)).unwrap();
    assert!(!board.is_sunk(Destroyer));
}

#[test]
fn hitting_the_last_square_reports_the_sunk_ship_and_its_squares() {
    let mut board = board_with_destroyer();
    board.fire(c(2, 2)).unwrap();
    assert_eq!(
        board.fire(c(3, 2)),
        Ok(ShotResult::Sunk {
            kind: Destroyer,
            cells: vec![c(2, 2), c(3, 2)]
        })
    );
}

#[test]
fn hitting_every_square_of_a_ship_sinks_it() {
    let mut board = board_with_destroyer();
    sink(&mut board, p(Destroyer, 2, 2, Horizontal));
    assert!(board.is_sunk(Destroyer));
}

#[test]
fn open_water_cannot_be_fired_at_twice() {
    let mut board = board_with_destroyer();
    board.fire(c(0, 0)).unwrap();
    assert_eq!(board.fire(c(0, 0)), Err(FireError::AlreadyFiredAt));
}

#[test]
fn a_ship_square_cannot_be_fired_at_twice() {
    let mut board = board_with_destroyer();
    board.fire(c(2, 2)).unwrap();
    assert_eq!(board.fire(c(2, 2)), Err(FireError::AlreadyFiredAt));
}

#[test]
fn not_all_sunk_while_a_ship_is_afloat() {
    let destroyer = p(Destroyer, 2, 2, Horizontal);
    let mut board = board_with(&[destroyer, p(Submarine, 6, 6, Vertical)]);
    sink(&mut board, destroyer);
    assert!(!board.all_sunk());
}

#[test]
fn all_sunk_once_every_ship_is_sunk() {
    let destroyer = p(Destroyer, 2, 2, Horizontal);
    let submarine = p(Submarine, 6, 6, Vertical);
    let mut board = board_with(&[destroyer, submarine]);
    sink(&mut board, destroyer);
    sink(&mut board, submarine);
    assert!(board.all_sunk());
}

#[test]
fn empty_board_is_not_all_sunk() {
    assert!(!Board::new().all_sunk());
}

// --- Random placement ---

fn assert_valid_full_fleet(board: &Board) {
    assert!(board.is_fleet_complete());
    let mut rebuilt = Board::new();
    for placement in board.placements() {
        assert_eq!(rebuilt.place(placement), Ok(()), "{placement:?}");
    }
}

#[test]
fn random_board_always_has_a_legal_full_fleet() {
    for seed in 0..RANDOM_BOARDS {
        assert_valid_full_fleet(&Board::random(&mut Rng::new(seed)));
    }
}

#[test]
fn random_board_is_reproducible_from_a_seed() {
    let first = Board::random(&mut Rng::new(5)).placements();
    let second = Board::random(&mut Rng::new(5)).placements();
    assert_eq!(first, second);
}

#[test]
fn random_boards_vary() {
    let seed_one = Board::random(&mut Rng::new(1)).placements();
    let seed_two = Board::random(&mut Rng::new(2)).placements();
    assert_ne!(seed_one, seed_two);
}

#[test]
fn random_boards_use_both_orientations() {
    let mut rng = Rng::new(11);
    let orientations: HashSet<Orientation> = (0..20)
        .flat_map(|_| Board::random(&mut rng).placements())
        .map(|placement| placement.orientation)
        .collect();
    assert_eq!(orientations.len(), 2);
}
