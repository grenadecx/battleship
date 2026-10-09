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
    use crate::test_support::{c, open_water};
    use Orientation::*;
    use ShipKind::*;

    fn kind_at(editor: &FleetEditor, square: Coord) -> Option<ShipKind> {
        editor
            .board()
            .ship_at(square)
            .map(|placement| placement.kind)
    }

    fn editor_with_carrier_at_origin() -> FleetEditor {
        let mut editor = FleetEditor::new();
        editor.click(c(0, 0));
        editor
    }

    fn randomized_editor() -> FleetEditor {
        let mut editor = FleetEditor::new();
        editor.randomize(&mut Rng::new(3));
        editor
    }

    /// A vertical carrier on A1-A5, then rotated back to horizontal.
    fn editor_with_vertical_carrier_and_horizontal_cursor() -> FleetEditor {
        let mut editor = FleetEditor::new();
        editor.rotate();
        editor.click(c(0, 0));
        editor.rotate();
        editor
    }

    #[test]
    fn starts_with_the_carrier_selected() {
        assert_eq!(FleetEditor::new().selected(), Some(Carrier));
    }

    #[test]
    fn starts_horizontal() {
        assert_eq!(FleetEditor::new().orientation(), Horizontal);
    }

    #[test]
    fn starts_with_an_incomplete_fleet() {
        assert!(!FleetEditor::new().is_complete());
    }

    #[test]
    fn clicking_drops_the_selected_ship() {
        assert_eq!(
            kind_at(&editor_with_carrier_at_origin(), c(4, 0)),
            Some(Carrier)
        );
    }

    #[test]
    fn dropping_a_ship_selects_the_next_one() {
        assert_eq!(editor_with_carrier_at_origin().selected(), Some(Battleship));
    }

    #[test]
    fn illegal_drop_leaves_the_board_unchanged() {
        let mut editor = editor_with_carrier_at_origin();
        editor.click(c(0, 1));
        assert!(!editor.board().has_ship(Battleship));
    }

    #[test]
    fn illegal_drop_keeps_the_ship_selected() {
        let mut editor = editor_with_carrier_at_origin();
        editor.click(c(0, 1));
        assert_eq!(editor.selected(), Some(Battleship));
    }

    #[test]
    fn rotate_turns_the_cursor_vertical() {
        let mut editor = FleetEditor::new();
        editor.rotate();
        assert_eq!(editor.orientation(), Vertical);
    }

    #[test]
    fn rotated_ship_is_dropped_vertically() {
        let mut editor = FleetEditor::new();
        editor.rotate();
        editor.click(c(0, 0));
        assert_eq!(kind_at(&editor, c(0, 4)), Some(Carrier));
    }

    #[test]
    fn preview_shows_where_a_fitting_ship_would_go() {
        assert_eq!(
            FleetEditor::new().preview(c(1, 1)),
            Some((Placement::new(Carrier, c(1, 1), Horizontal), true))
        );
    }

    #[test]
    fn preview_flags_a_ship_that_does_not_fit() {
        assert_eq!(
            FleetEditor::new().preview(c(8, 1)),
            Some((Placement::new(Carrier, c(8, 1), Horizontal), false))
        );
    }

    #[test]
    fn preview_is_empty_with_nothing_selected() {
        assert_eq!(randomized_editor().preview(c(1, 1)), None);
    }

    #[test]
    fn clicking_a_placed_ship_removes_it_from_the_board() {
        let mut editor = editor_with_vertical_carrier_and_horizontal_cursor();
        editor.click(c(0, 3));
        assert!(!editor.board().has_ship(Carrier));
    }

    #[test]
    fn clicking_a_placed_ship_selects_it() {
        let mut editor = editor_with_vertical_carrier_and_horizontal_cursor();
        editor.click(c(0, 3));
        assert_eq!(editor.selected(), Some(Carrier));
    }

    #[test]
    fn picked_up_ship_keeps_its_orientation() {
        let mut editor = editor_with_vertical_carrier_and_horizontal_cursor();
        editor.click(c(0, 3));
        assert_eq!(editor.orientation(), Vertical);
    }

    #[test]
    fn selecting_a_ship_already_on_the_board_is_ignored() {
        let mut editor = editor_with_carrier_at_origin();
        editor.select(Carrier);
        assert_eq!(editor.selected(), Some(Battleship));
    }

    #[test]
    fn any_ship_not_on_the_board_can_be_selected() {
        for kind in FLEET {
            let mut editor = FleetEditor::new();
            editor.select(kind);
            assert_eq!(editor.selected(), Some(kind));
        }
    }

    #[test]
    fn placing_every_ship_by_hand_completes_the_fleet() {
        let mut editor = FleetEditor::new();
        for row in [0, 2, 4, 6, 8] {
            editor.click(c(0, row));
        }
        assert!(editor.is_complete());
    }

    #[test]
    fn nothing_is_selected_once_the_fleet_is_complete() {
        assert_eq!(randomized_editor().selected(), None);
    }

    #[test]
    fn randomize_completes_the_fleet() {
        assert!(randomized_editor().is_complete());
    }

    #[test]
    fn clear_empties_the_board() {
        let mut editor = randomized_editor();
        editor.clear();
        assert!(editor.board().placements().is_empty());
    }

    #[test]
    fn clear_selects_the_carrier_again() {
        let mut editor = randomized_editor();
        editor.clear();
        assert_eq!(editor.selected(), Some(Carrier));
    }

    #[test]
    fn clicking_open_water_with_nothing_selected_does_nothing() {
        let mut editor = randomized_editor();
        let before = editor.board().placements();
        editor.click(open_water(editor.board())[0]);
        assert_eq!(editor.board().placements(), before);
    }
}
