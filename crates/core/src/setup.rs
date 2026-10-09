//! Interactive fleet placement: pick a ship, rotate it, drop it on the board,
//! or pick a placed ship back up.

use crate::domain::{Board, Coord, FLEET, Orientation, Placement, ShipKind};
use crate::rng::Rng;

#[derive(Debug, Clone)]
pub struct FleetEditor {
    board: Board,
    selected: Option<ShipKind>,
    orientation: Orientation,
}

impl Default for FleetEditor {
    fn default() -> Self {
        Self::new()
    }
}

impl FleetEditor {
    pub fn new() -> Self {
        FleetEditor {
            board: Board::new(),
            selected: Some(FLEET[0]),
            orientation: Orientation::Horizontal,
        }
    }

    pub fn board(&self) -> &Board {
        &self.board
    }

    pub fn selected(&self) -> Option<ShipKind> {
        self.selected
    }

    pub fn orientation(&self) -> Orientation {
        self.orientation
    }

    pub fn is_complete(&self) -> bool {
        self.board.is_fleet_complete()
    }

    pub fn rotate(&mut self) {
        self.orientation = self.orientation.toggled();
    }

    /// Selects a ship that is not on the board yet.
    pub fn select(&mut self, kind: ShipKind) {
        if !self.board.has_ship(kind) {
            self.selected = Some(kind);
        }
    }

    /// Where the selected ship would go with its bow at `at`, and whether that is legal.
    pub fn preview(&self, at: Coord) -> Option<(Placement, bool)> {
        let placement = Placement::new(self.selected?, at, self.orientation);
        Some((placement, self.board.check_placement(&placement).is_ok()))
    }

    /// Picks up the ship under the cursor, or drops the selected ship there.
    pub fn click(&mut self, at: Coord) {
        if let Some(placed) = self.board.ship_at(at) {
            self.board.remove(placed.kind);
            self.selected = Some(placed.kind);
            self.orientation = placed.orientation;
        } else if let Some((placement, true)) = self.preview(at) {
            self.board.place(placement).expect("preview said it fits");
            self.selected = FLEET.into_iter().find(|kind| !self.board.has_ship(*kind));
        }
    }

    pub fn randomize(&mut self, rng: &mut Rng) {
        self.board = Board::random(rng);
        self.selected = None;
    }

    pub fn clear(&mut self) {
        let orientation = self.orientation;
        *self = FleetEditor::new();
        self.orientation = orientation;
    }
}

#[cfg(test)]
#[path = "tests/setup.rs"]
mod tests;
