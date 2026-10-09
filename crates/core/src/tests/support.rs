//! Builders shared by the unit tests.

use crate::domain::{Board, Coord, FLEET, Orientation, Placement, ShipKind, ShotResult};

pub fn c(x: u8, y: u8) -> Coord {
    Coord::new(x, y)
}

pub fn sunk(kind: ShipKind, cells: &[Coord]) -> ShotResult {
    ShotResult::Sunk {
        kind,
        cells: cells.to_vec(),
    }
}

/// The whole fleet laid out horizontally, one ship per even row, starting at column 0.
pub fn row_fleet_placements() -> Vec<Placement> {
    FLEET
        .iter()
        .enumerate()
        .map(|(i, kind)| Placement::new(*kind, c(0, i as u8 * 2), Orientation::Horizontal))
        .collect()
}

pub fn row_fleet() -> Board {
    board_with(&row_fleet_placements())
}

pub fn board_with(placements: &[Placement]) -> Board {
    let mut board = Board::new();
    for placement in placements {
        board.place(*placement).unwrap();
    }
    board
}

/// Every square occupied by a ship, ship by ship.
pub fn ship_squares(board: &Board) -> Vec<Coord> {
    board
        .placements()
        .iter()
        .flat_map(|placement| placement.cells().unwrap())
        .collect()
}

pub fn open_water(board: &Board) -> Vec<Coord> {
    Coord::all()
        .filter(|co| board.ship_at(*co).is_none())
        .collect()
}
