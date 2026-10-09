use super::*;
use Orientation::*;
use ShipKind::*;

fn c(x: u8, y: u8) -> Coord {
    Coord::new(x, y)
}

fn p(kind: ShipKind, x: u8, y: u8, o: Orientation) -> Placement {
    Placement::new(kind, c(x, y), o)
}

// --- Coord ---

#[test]
fn try_new_rejects_squares_off_the_board() {
    assert_eq!(Coord::try_new(0, 0), Some(c(0, 0)));
    assert_eq!(Coord::try_new(9, 9), Some(c(9, 9)));
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
fn corner_has_two_orthogonal_and_three_surrounding_neighbours() {
    let mut n = c(0, 0).orthogonal_neighbors();
    n.sort();
    assert_eq!(n, vec![c(0, 1), c(1, 0)]);
    assert_eq!(c(0, 0).surrounding().len(), 3);
}

#[test]
fn middle_square_has_four_and_eight_neighbours() {
    assert_eq!(c(5, 5).orthogonal_neighbors().len(), 4);
    let s = c(5, 5).surrounding();
    assert_eq!(s.len(), 8);
    assert!(!s.contains(&c(5, 5)));
    assert!(s.contains(&c(4, 4)));
    assert!(s.contains(&c(6, 6)));
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
    assert_eq!(FLEET.iter().map(|k| k.size() as u32).sum::<u32>(), 17);
}

#[test]
fn ship_names_round_trip() {
    for kind in FLEET {
        assert_eq!(ShipKind::from_name(kind.name()), Some(kind));
    }
    assert_eq!(ShipKind::from_name("Dinghy"), None);
}

#[test]
fn orientation_toggles() {
    assert_eq!(Horizontal.toggled(), Vertical);
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
fn placement_sticking_out_of_the_board_has_no_cells() {
    assert_eq!(p(Carrier, 6, 0, Horizontal).cells(), None);
    assert_eq!(p(Carrier, 0, 6, Vertical).cells(), None);
    assert!(p(Carrier, 5, 0, Horizontal).cells().is_some());
}

// --- Placement rules ---

#[test]
fn ship_can_be_placed_on_empty_board() {
    let mut board = Board::new();
    assert_eq!(board.place(p(Carrier, 0, 0, Horizontal)), Ok(()));
    assert!(board.has_ship(Carrier));
    assert_eq!(board.ship_at(c(4, 0)).map(|pl| pl.kind), Some(Carrier));
    assert_eq!(board.ship_at(c(5, 0)), None);
}

#[test]
fn ship_cannot_stick_out_of_the_board() {
    let mut board = Board::new();
    assert_eq!(
        board.place(p(Carrier, 8, 0, Horizontal)),
        Err(PlaceError::OutOfBounds)
    );
}

#[test]
fn ships_cannot_overlap() {
    let mut board = Board::new();
    board.place(p(Carrier, 0, 0, Horizontal)).unwrap();
    assert_eq!(
        board.place(p(Battleship, 2, 0, Vertical)),
        Err(PlaceError::Overlaps)
    );
}

#[test]
fn ships_cannot_touch_side_by_side() {
    let mut board = Board::new();
    board.place(p(Carrier, 0, 0, Horizontal)).unwrap();
    assert_eq!(
        board.place(p(Battleship, 0, 1, Horizontal)),
        Err(PlaceError::TouchesAnotherShip)
    );
}

#[test]
fn ships_cannot_touch_end_to_end() {
    let mut board = Board::new();
    board.place(p(Destroyer, 0, 0, Horizontal)).unwrap();
    assert_eq!(
        board.place(p(Cruiser, 2, 0, Horizontal)),
        Err(PlaceError::TouchesAnotherShip)
    );
}

#[test]
fn ships_cannot_touch_diagonally() {
    let mut board = Board::new();
    board.place(p(Destroyer, 0, 0, Horizontal)).unwrap();
    assert_eq!(
        board.place(p(Cruiser, 2, 1, Vertical)),
        Err(PlaceError::TouchesAnotherShip)
    );
}

#[test]
fn ships_one_square_apart_are_fine() {
    let mut board = Board::new();
    board.place(p(Destroyer, 0, 0, Horizontal)).unwrap();
    assert_eq!(board.place(p(Cruiser, 0, 2, Horizontal)), Ok(()));
    assert_eq!(board.place(p(Submarine, 3, 0, Horizontal)), Ok(()));
}

#[test]
fn each_kind_of_ship_can_only_be_placed_once() {
    let mut board = Board::new();
    board.place(p(Destroyer, 0, 0, Horizontal)).unwrap();
    assert_eq!(
        board.place(p(Destroyer, 5, 5, Horizontal)),
        Err(PlaceError::AlreadyPlaced)
    );
}

#[test]
fn check_placement_does_not_modify_the_board() {
    let board = Board::new();
    assert_eq!(board.check_placement(&p(Carrier, 0, 0, Vertical)), Ok(()));
    assert!(!board.has_ship(Carrier));
}

#[test]
fn removed_ship_frees_its_squares() {
    let mut board = Board::new();
    let placement = p(Carrier, 0, 0, Horizontal);
    board.place(placement).unwrap();
    assert_eq!(board.remove(Carrier), Some(placement));
    assert_eq!(board.remove(Carrier), None);
    assert_eq!(board.place(p(Battleship, 0, 1, Horizontal)), Ok(()));
}

#[test]
fn fleet_is_complete_once_all_five_ships_are_placed() {
    let mut board = Board::new();
    let placements = [
        p(Carrier, 0, 0, Horizontal),
        p(Battleship, 0, 2, Horizontal),
        p(Cruiser, 0, 4, Horizontal),
        p(Submarine, 0, 6, Horizontal),
        p(Destroyer, 0, 8, Horizontal),
    ];
    for (i, placement) in placements.iter().enumerate() {
        assert!(!board.is_fleet_complete(), "complete after {i} ships");
        board.place(*placement).unwrap();
    }
    assert!(board.is_fleet_complete());
    assert_eq!(board.placements().len(), 5);
}

// --- Firing ---

fn board_with_destroyer() -> Board {
    let mut board = Board::new();
    board.place(p(Destroyer, 2, 2, Horizontal)).unwrap();
    board
}

#[test]
fn shot_into_open_water_misses() {
    let mut board = board_with_destroyer();
    assert_eq!(board.fire(c(0, 0)), Ok(ShotResult::Miss));
    assert!(board.was_shot(c(0, 0)));
}

#[test]
fn shot_on_a_ship_hits() {
    let mut board = board_with_destroyer();
    assert_eq!(board.fire(c(2, 2)), Ok(ShotResult::Hit));
    assert!(!board.is_sunk(Destroyer));
}

#[test]
fn hitting_every_square_of_a_ship_sinks_it() {
    let mut board = board_with_destroyer();
    board.fire(c(2, 2)).unwrap();
    assert_eq!(
        board.fire(c(3, 2)),
        Ok(ShotResult::Sunk {
            kind: Destroyer,
            cells: vec![c(2, 2), c(3, 2)]
        })
    );
    assert!(board.is_sunk(Destroyer));
}

#[test]
fn same_square_cannot_be_fired_at_twice() {
    let mut board = board_with_destroyer();
    board.fire(c(0, 0)).unwrap();
    assert_eq!(board.fire(c(0, 0)), Err(FireError::AlreadyFiredAt));
    board.fire(c(2, 2)).unwrap();
    assert_eq!(board.fire(c(2, 2)), Err(FireError::AlreadyFiredAt));
}

#[test]
fn all_sunk_only_when_every_ship_is_sunk() {
    let mut board = board_with_destroyer();
    board.place(p(Submarine, 6, 6, Vertical)).unwrap();
    board.fire(c(2, 2)).unwrap();
    board.fire(c(3, 2)).unwrap();
    assert!(!board.all_sunk());
    board.fire(c(6, 6)).unwrap();
    board.fire(c(6, 7)).unwrap();
    board.fire(c(6, 8)).unwrap();
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
    for seed in 0..300 {
        let board = Board::random(&mut Rng::new(seed));
        assert_valid_full_fleet(&board);
    }
}

#[test]
fn random_board_is_reproducible_from_a_seed() {
    let a = Board::random(&mut Rng::new(5)).placements();
    let b = Board::random(&mut Rng::new(5)).placements();
    assert_eq!(a, b);
}

#[test]
fn random_boards_vary() {
    let a = Board::random(&mut Rng::new(1)).placements();
    let b = Board::random(&mut Rng::new(2)).placements();
    assert_ne!(a, b);
}

#[test]
fn random_boards_use_both_orientations() {
    let mut rng = Rng::new(11);
    let mut seen = HashSet::new();
    for _ in 0..20 {
        for placement in Board::random(&mut rng).placements() {
            seen.insert(placement.orientation);
        }
    }
    assert_eq!(seen.len(), 2);
}
