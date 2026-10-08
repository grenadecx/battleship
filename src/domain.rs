//! Core domain: coordinates, ships, and a player's own board.

use crate::rng::Rng;
use std::collections::HashSet;
use std::fmt;

pub const BOARD_SIZE: u8 = 10;

/// A square on the 10x10 grid. `x` is the column (A-J), `y` the row (1-10).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Coord {
    pub x: u8,
    pub y: u8,
}

impl Coord {
    /// Panics if the coordinate is off the board; use [`Coord::try_new`] for untrusted input.
    pub fn new(x: u8, y: u8) -> Self {
        assert!(
            x < BOARD_SIZE && y < BOARD_SIZE,
            "({x}, {y}) is off the board"
        );
        Coord { x, y }
    }

    pub fn try_new(x: i32, y: i32) -> Option<Self> {
        let size = BOARD_SIZE as i32;
        ((0..size).contains(&x) && (0..size).contains(&y)).then(|| Coord::new(x as u8, y as u8))
    }

    /// Every square, row by row.
    pub fn all() -> impl Iterator<Item = Coord> {
        (0..BOARD_SIZE).flat_map(|y| (0..BOARD_SIZE).map(move |x| Coord { x, y }))
    }

    fn offset(self, dx: i32, dy: i32) -> Option<Coord> {
        Coord::try_new(self.x as i32 + dx, self.y as i32 + dy)
    }

    /// Up, down, left and right neighbours that are on the board.
    pub fn orthogonal_neighbors(self) -> Vec<Coord> {
        [(0, -1), (-1, 0), (1, 0), (0, 1)]
            .into_iter()
            .filter_map(|(dx, dy)| self.offset(dx, dy))
            .collect()
    }

    /// All eight surrounding squares that are on the board.
    pub fn surrounding(self) -> Vec<Coord> {
        (-1..=1)
            .flat_map(|dy| (-1..=1).map(move |dx| (dx, dy)))
            .filter(|&(dx, dy)| (dx, dy) != (0, 0))
            .filter_map(|(dx, dy)| self.offset(dx, dy))
            .collect()
    }

    /// Human readable label such as `A1` or `J10`.
    pub fn label(self) -> String {
        format!("{}{}", (b'A' + self.x) as char, self.y + 1)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Orientation {
    Horizontal,
    Vertical,
}

impl Orientation {
    pub fn toggled(self) -> Self {
        match self {
            Orientation::Horizontal => Orientation::Vertical,
            Orientation::Vertical => Orientation::Horizontal,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ShipKind {
    Carrier,
    Battleship,
    Cruiser,
    Submarine,
    Destroyer,
}

/// The Milton Bradley fleet, largest first.
pub const FLEET: [ShipKind; 5] = [
    ShipKind::Carrier,
    ShipKind::Battleship,
    ShipKind::Cruiser,
    ShipKind::Submarine,
    ShipKind::Destroyer,
];

impl ShipKind {
    pub fn size(self) -> u8 {
        match self {
            ShipKind::Carrier => 5,
            ShipKind::Battleship => 4,
            ShipKind::Cruiser => 3,
            ShipKind::Submarine => 3,
            ShipKind::Destroyer => 2,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            ShipKind::Carrier => "Carrier",
            ShipKind::Battleship => "Battleship",
            ShipKind::Cruiser => "Cruiser",
            ShipKind::Submarine => "Submarine",
            ShipKind::Destroyer => "Destroyer",
        }
    }

    pub fn from_name(name: &str) -> Option<ShipKind> {
        FLEET.into_iter().find(|kind| kind.name() == name)
    }
}

impl fmt::Display for ShipKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Where a ship sits: its top/left-most square and direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Placement {
    pub kind: ShipKind,
    pub origin: Coord,
    pub orientation: Orientation,
}

impl Placement {
    pub fn new(kind: ShipKind, origin: Coord, orientation: Orientation) -> Self {
        Placement {
            kind,
            origin,
            orientation,
        }
    }

    /// The squares covered, or `None` if the ship would stick out of the board.
    pub fn cells(&self) -> Option<Vec<Coord>> {
        let (dx, dy) = match self.orientation {
            Orientation::Horizontal => (1, 0),
            Orientation::Vertical => (0, 1),
        };
        (0..self.kind.size() as i32)
            .map(|i| self.origin.offset(dx * i, dy * i))
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShotResult {
    Miss,
    Hit,
    Sunk { kind: ShipKind, cells: Vec<Coord> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaceError {
    OutOfBounds,
    Overlaps,
    TouchesAnotherShip,
    AlreadyPlaced,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FireError {
    AlreadyFiredAt,
}

#[derive(Debug, Clone)]
struct Ship {
    placement: Placement,
    cells: Vec<Coord>,
    hits: HashSet<Coord>,
}

impl Ship {
    fn is_sunk(&self) -> bool {
        self.hits.len() == self.cells.len()
    }
}

/// A player's own ocean: their ships and the shots received.
#[derive(Debug, Clone, Default)]
pub struct Board {
    ships: Vec<Ship>,
    shots: HashSet<Coord>,
}

impl Board {
    pub fn new() -> Self {
        Self::default()
    }

    /// A board with the full fleet placed at random, obeying the placement rules.
    pub fn random(rng: &mut Rng) -> Self {
        'attempt: loop {
            let mut board = Board::new();
            for kind in FLEET {
                let options: Vec<Placement> = Coord::all()
                    .flat_map(|origin| {
                        [Orientation::Horizontal, Orientation::Vertical]
                            .map(|o| Placement::new(kind, origin, o))
                    })
                    .filter(|p| board.check_placement(p).is_ok())
                    .collect();
                match rng.pick(&options) {
                    Some(placement) => board.place(*placement).expect("checked placement"),
                    None => continue 'attempt,
                }
            }
            return board;
        }
    }

    pub fn check_placement(&self, placement: &Placement) -> Result<(), PlaceError> {
        if self.has_ship(placement.kind) {
            return Err(PlaceError::AlreadyPlaced);
        }
        let cells = placement.cells().ok_or(PlaceError::OutOfBounds)?;
        if cells.iter().any(|c| self.ship_at(*c).is_some()) {
            return Err(PlaceError::Overlaps);
        }
        if cells
            .iter()
            .flat_map(|c| c.surrounding())
            .any(|c| self.ship_at(c).is_some())
        {
            return Err(PlaceError::TouchesAnotherShip);
        }
        Ok(())
    }

    pub fn place(&mut self, placement: Placement) -> Result<(), PlaceError> {
        self.check_placement(&placement)?;
        self.ships.push(Ship {
            placement,
            cells: placement.cells().expect("checked placement"),
            hits: HashSet::new(),
        });
        Ok(())
    }

    /// Takes a ship back off the board (only while setting up).
    pub fn remove(&mut self, kind: ShipKind) -> Option<Placement> {
        let index = self.ships.iter().position(|s| s.placement.kind == kind)?;
        Some(self.ships.remove(index).placement)
    }

    pub fn placements(&self) -> Vec<Placement> {
        self.ships.iter().map(|s| s.placement).collect()
    }

    pub fn ship_at(&self, coord: Coord) -> Option<Placement> {
        self.ships
            .iter()
            .find(|s| s.cells.contains(&coord))
            .map(|s| s.placement)
    }

    pub fn has_ship(&self, kind: ShipKind) -> bool {
        self.ships.iter().any(|s| s.placement.kind == kind)
    }

    /// All ships of the fleet are on the board.
    pub fn is_fleet_complete(&self) -> bool {
        FLEET.iter().all(|kind| self.has_ship(*kind))
    }

    pub fn fire(&mut self, coord: Coord) -> Result<ShotResult, FireError> {
        if !self.shots.insert(coord) {
            return Err(FireError::AlreadyFiredAt);
        }
        let Some(ship) = self.ships.iter_mut().find(|s| s.cells.contains(&coord)) else {
            return Ok(ShotResult::Miss);
        };
        ship.hits.insert(coord);
        if ship.is_sunk() {
            Ok(ShotResult::Sunk {
                kind: ship.placement.kind,
                cells: ship.cells.clone(),
            })
        } else {
            Ok(ShotResult::Hit)
        }
    }

    pub fn was_shot(&self, coord: Coord) -> bool {
        self.shots.contains(&coord)
    }

    pub fn is_sunk(&self, kind: ShipKind) -> bool {
        self.ships
            .iter()
            .any(|s| s.placement.kind == kind && s.is_sunk())
    }

    pub fn all_sunk(&self) -> bool {
        !self.ships.is_empty() && self.ships.iter().all(Ship::is_sunk)
    }
}

#[cfg(test)]
mod tests {
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
}
