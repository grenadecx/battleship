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
mod tests {
    use super::*;
    use Orientation::*;
    use ShipKind::*;

    fn c(x: u8, y: u8) -> Coord {
        Coord::new(x, y)
    }

    #[test]
    fn starts_with_the_carrier_selected_horizontally() {
        let editor = FleetEditor::new();
        assert_eq!(editor.selected(), Some(Carrier));
        assert_eq!(editor.orientation(), Horizontal);
        assert!(!editor.is_complete());
    }

    #[test]
    fn clicking_drops_the_selected_ship_and_selects_the_next() {
        let mut editor = FleetEditor::new();
        editor.click(c(0, 0));
        assert_eq!(
            editor.board().ship_at(c(4, 0)).map(|p| p.kind),
            Some(Carrier)
        );
        assert_eq!(editor.selected(), Some(Battleship));
    }

    #[test]
    fn illegal_drop_keeps_the_ship_selected() {
        let mut editor = FleetEditor::new();
        editor.click(c(0, 0));
        editor.click(c(0, 1));
        assert!(!editor.board().has_ship(Battleship));
        assert_eq!(editor.selected(), Some(Battleship));
    }

    #[test]
    fn rotate_changes_the_direction_of_the_next_drop() {
        let mut editor = FleetEditor::new();
        editor.rotate();
        assert_eq!(editor.orientation(), Vertical);
        editor.click(c(0, 0));
        assert_eq!(
            editor.board().ship_at(c(0, 4)).map(|p| p.kind),
            Some(Carrier)
        );
    }

    #[test]
    fn preview_shows_where_the_ship_would_go_and_if_it_fits() {
        let mut editor = FleetEditor::new();
        assert_eq!(
            editor.preview(c(1, 1)),
            Some((Placement::new(Carrier, c(1, 1), Horizontal), true))
        );
        assert_eq!(
            editor.preview(c(8, 1)),
            Some((Placement::new(Carrier, c(8, 1), Horizontal), false))
        );
        editor.randomize(&mut Rng::new(1));
        assert_eq!(editor.preview(c(1, 1)), None);
    }

    #[test]
    fn clicking_a_placed_ship_picks_it_up_with_its_orientation() {
        let mut editor = FleetEditor::new();
        editor.rotate();
        editor.click(c(0, 0));
        editor.rotate();
        assert_eq!(editor.orientation(), Horizontal);
        editor.click(c(0, 3));
        assert!(!editor.board().has_ship(Carrier));
        assert_eq!(editor.selected(), Some(Carrier));
        assert_eq!(editor.orientation(), Vertical);
    }

    #[test]
    fn selecting_a_ship_already_on_the_board_is_ignored() {
        let mut editor = FleetEditor::new();
        editor.click(c(0, 0));
        editor.select(Carrier);
        assert_eq!(editor.selected(), Some(Battleship));
        editor.select(Destroyer);
        assert_eq!(editor.selected(), Some(Destroyer));
    }

    #[test]
    fn placing_every_ship_by_hand_completes_the_fleet() {
        let mut editor = FleetEditor::new();
        for row in [0, 2, 4, 6, 8] {
            editor.click(c(0, row));
        }
        assert!(editor.is_complete());
        assert_eq!(editor.selected(), None);
    }

    #[test]
    fn randomize_completes_the_fleet_and_clear_empties_it() {
        let mut editor = FleetEditor::new();
        editor.randomize(&mut Rng::new(3));
        assert!(editor.is_complete());
        assert_eq!(editor.selected(), None);
        editor.clear();
        assert!(editor.board().placements().is_empty());
        assert_eq!(editor.selected(), Some(Carrier));
    }

    #[test]
    fn clicking_with_nothing_selected_on_open_water_does_nothing() {
        let mut editor = FleetEditor::new();
        editor.randomize(&mut Rng::new(3));
        let open = Coord::all()
            .find(|co| editor.board().ship_at(*co).is_none())
            .unwrap();
        editor.click(open);
        assert!(editor.is_complete());
    }

    #[test]
    fn every_ship_kind_is_reachable() {
        let mut editor = FleetEditor::new();
        for kind in FLEET {
            editor.select(kind);
            assert_eq!(editor.selected(), Some(kind));
        }
    }

    #[test]
    fn default_editor_starts_empty() {
        let editor = FleetEditor::default();
        assert!(!editor.is_complete());
        assert!(editor.board().placements().is_empty());
    }
}
