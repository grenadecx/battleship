//! One player's view of a match: turn order, own board, knowledge of the enemy.
//! Transport agnostic: the same rules drive games against the computer and over TCP.

use crate::domain::{Board, Coord, ShotResult};
use crate::grid::TargetGrid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Arranging the fleet.
    Placement,
    /// Fleet placed, waiting for the opponent to finish theirs.
    WaitingForOpponent,
    MyTurn,
    /// We fired at this square and wait for the opponent to report the result.
    AwaitingResult(Coord),
    TheirTurn,
    Won,
    Lost,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionError {
    NotAllowedNow(Phase),
    FleetIncomplete,
    AlreadyTargeted(Coord),
    BadReport(String),
}

impl std::fmt::Display for SessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionError::NotAllowedNow(phase) => write!(f, "not allowed during {phase:?}"),
            SessionError::FleetIncomplete => write!(f, "the fleet is not fully placed"),
            SessionError::AlreadyTargeted(c) => write!(f, "{} was already targeted", c.label()),
            SessionError::BadReport(why) => write!(f, "bad report: {why}"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Session {
    phase: Phase,
    board: Board,
    enemy: TargetGrid,
    i_go_first: bool,
    opponent_ready: bool,
}

impl Session {
    pub fn new(i_go_first: bool) -> Self {
        Session {
            phase: Phase::Placement,
            board: Board::new(),
            enemy: TargetGrid::new(),
            i_go_first,
            opponent_ready: false,
        }
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn my_board(&self) -> &Board {
        &self.board
    }

    pub fn enemy_grid(&self) -> &TargetGrid {
        &self.enemy
    }

    pub fn i_go_first(&self) -> bool {
        self.i_go_first
    }

    pub fn is_over(&self) -> bool {
        matches!(self.phase, Phase::Won | Phase::Lost)
    }

    fn require(&self, allowed: bool) -> Result<(), SessionError> {
        if allowed {
            Ok(())
        } else {
            Err(SessionError::NotAllowedNow(self.phase))
        }
    }

    /// Replaces our fleet while still arranging it.
    pub fn set_board(&mut self, board: Board) -> Result<(), SessionError> {
        self.require(self.phase == Phase::Placement)?;
        self.board = board;
        Ok(())
    }

    /// We have finished placing our fleet.
    pub fn ready(&mut self) -> Result<(), SessionError> {
        self.require(self.phase == Phase::Placement)?;
        if !self.board.is_fleet_complete() {
            return Err(SessionError::FleetIncomplete);
        }
        self.phase = Phase::WaitingForOpponent;
        self.start_if_both_ready();
        Ok(())
    }

    /// The opponent finished placing their fleet. May arrive before we are ready,
    /// or even while we are still looking at the previous game's result.
    pub fn opponent_ready(&mut self) -> Result<(), SessionError> {
        self.require(
            matches!(self.phase, Phase::Placement | Phase::WaitingForOpponent) || self.is_over(),
        )?;
        self.opponent_ready = true;
        self.start_if_both_ready();
        Ok(())
    }

    fn start_if_both_ready(&mut self) {
        if self.phase == Phase::WaitingForOpponent && self.opponent_ready {
            // Consumed, so that the next round waits for a fresh READY.
            self.opponent_ready = false;
            self.phase = if self.i_go_first {
                Phase::MyTurn
            } else {
                Phase::TheirTurn
            };
        }
    }

    /// We fire at a square of the enemy ocean.
    pub fn fire(&mut self, target: Coord) -> Result<(), SessionError> {
        self.require(self.phase == Phase::MyTurn)?;
        if !self.enemy.can_target(target) {
            return Err(SessionError::AlreadyTargeted(target));
        }
        self.phase = Phase::AwaitingResult(target);
        Ok(())
    }

    /// The opponent reports the result of our last shot.
    pub fn receive_result(&mut self, result: ShotResult) -> Result<(), SessionError> {
        let Phase::AwaitingResult(target) = self.phase else {
            return Err(SessionError::NotAllowedNow(self.phase));
        };
        self.enemy
            .record(target, &result)
            .map_err(|e| SessionError::BadReport(format!("{e:?} for {}", target.label())))?;
        self.phase = if self.enemy.fleet_destroyed() {
            Phase::Won
        } else {
            Phase::TheirTurn
        };
        Ok(())
    }

    /// The opponent fires at our ocean; returns what they hit.
    pub fn receive_fire(&mut self, target: Coord) -> Result<ShotResult, SessionError> {
        self.require(self.phase == Phase::TheirTurn)?;
        let result = self
            .board
            .fire(target)
            .map_err(|_| SessionError::AlreadyTargeted(target))?;
        self.phase = if self.board.all_sunk() {
            Phase::Lost
        } else {
            Phase::MyTurn
        };
        Ok(result)
    }

    /// Starts another game against the same opponent. The other player opens this time.
    pub fn new_round(&mut self) -> Result<(), SessionError> {
        self.require(self.is_over())?;
        let opponent_ready = self.opponent_ready;
        *self = Session::new(!self.i_go_first);
        self.opponent_ready = opponent_ready;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Orientation, Placement, ShipKind};
    use crate::rng::Rng;

    fn c(x: u8, y: u8) -> Coord {
        Coord::new(x, y)
    }

    /// Full fleet, one ship per even row, starting at column 0.
    fn row_fleet() -> Board {
        let mut board = Board::new();
        for (i, kind) in crate::domain::FLEET.iter().enumerate() {
            board
                .place(Placement::new(
                    *kind,
                    c(0, i as u8 * 2),
                    Orientation::Horizontal,
                ))
                .unwrap();
        }
        board
    }

    fn ready_session(i_go_first: bool) -> Session {
        let mut s = Session::new(i_go_first);
        s.set_board(row_fleet()).unwrap();
        s.ready().unwrap();
        s.opponent_ready().unwrap();
        s
    }

    #[test]
    fn starts_in_placement() {
        let s = Session::new(true);
        assert_eq!(s.phase(), Phase::Placement);
        assert!(!s.is_over());
    }

    #[test]
    fn cannot_be_ready_with_an_incomplete_fleet() {
        let mut s = Session::new(true);
        assert_eq!(s.ready(), Err(SessionError::FleetIncomplete));
        assert_eq!(s.phase(), Phase::Placement);
    }

    #[test]
    fn waits_for_the_opponent_after_getting_ready() {
        let mut s = Session::new(true);
        s.set_board(row_fleet()).unwrap();
        s.ready().unwrap();
        assert_eq!(s.phase(), Phase::WaitingForOpponent);
    }

    #[test]
    fn first_player_starts_when_both_are_ready() {
        assert_eq!(ready_session(true).phase(), Phase::MyTurn);
        assert_eq!(ready_session(false).phase(), Phase::TheirTurn);
    }

    #[test]
    fn opponent_may_be_ready_first() {
        let mut s = Session::new(false);
        s.opponent_ready().unwrap();
        assert_eq!(s.phase(), Phase::Placement);
        s.set_board(row_fleet()).unwrap();
        s.ready().unwrap();
        assert_eq!(s.phase(), Phase::TheirTurn);
    }

    #[test]
    fn board_cannot_change_after_getting_ready() {
        let mut s = ready_session(true);
        assert!(matches!(
            s.set_board(row_fleet()),
            Err(SessionError::NotAllowedNow(_))
        ));
    }

    #[test]
    fn firing_waits_for_the_result() {
        let mut s = ready_session(true);
        s.fire(c(4, 4)).unwrap();
        assert_eq!(s.phase(), Phase::AwaitingResult(c(4, 4)));
    }

    #[test]
    fn cannot_fire_out_of_turn() {
        let mut s = ready_session(false);
        assert_eq!(
            s.fire(c(4, 4)),
            Err(SessionError::NotAllowedNow(Phase::TheirTurn))
        );
        let mut placing = Session::new(true);
        assert!(placing.fire(c(4, 4)).is_err());
    }

    #[test]
    fn cannot_fire_twice_at_the_same_square() {
        let mut s = ready_session(true);
        s.fire(c(4, 4)).unwrap();
        s.receive_result(ShotResult::Miss).unwrap();
        s.receive_fire(c(9, 9)).unwrap();
        assert_eq!(s.fire(c(4, 4)), Err(SessionError::AlreadyTargeted(c(4, 4))));
        assert_eq!(s.phase(), Phase::MyTurn);
    }

    #[test]
    fn turn_passes_after_our_shot_is_reported() {
        let mut s = ready_session(true);
        s.fire(c(4, 4)).unwrap();
        s.receive_result(ShotResult::Hit).unwrap();
        assert_eq!(s.phase(), Phase::TheirTurn);
        assert_eq!(s.enemy_grid().open_hits(), vec![c(4, 4)]);
    }

    #[test]
    fn unexpected_result_is_rejected() {
        let mut s = ready_session(false);
        assert!(s.receive_result(ShotResult::Miss).is_err());
    }

    #[test]
    fn inconsistent_sunk_report_is_rejected() {
        let mut s = ready_session(true);
        s.fire(c(4, 4)).unwrap();
        let bogus = ShotResult::Sunk {
            kind: ShipKind::Destroyer,
            cells: vec![c(0, 0), c(1, 0)],
        };
        assert!(matches!(
            s.receive_result(bogus),
            Err(SessionError::BadReport(_))
        ));
    }

    #[test]
    fn incoming_fire_is_resolved_against_our_board_and_passes_the_turn() {
        let mut s = ready_session(false);
        assert_eq!(s.receive_fire(c(0, 0)), Ok(ShotResult::Hit));
        assert_eq!(s.phase(), Phase::MyTurn);
        assert!(s.my_board().was_shot(c(0, 0)));
    }

    #[test]
    fn incoming_fire_out_of_turn_is_rejected() {
        let mut s = ready_session(true);
        assert!(s.receive_fire(c(0, 0)).is_err());
    }

    #[test]
    fn repeated_incoming_fire_is_rejected() {
        let mut s = ready_session(false);
        s.receive_fire(c(9, 9)).unwrap();
        s.fire(c(0, 0)).unwrap();
        s.receive_result(ShotResult::Miss).unwrap();
        assert_eq!(
            s.receive_fire(c(9, 9)),
            Err(SessionError::AlreadyTargeted(c(9, 9)))
        );
    }

    #[test]
    fn losing_the_last_ship_loses_the_game() {
        let mut s = ready_session(false);
        let targets: Vec<Coord> = s
            .my_board()
            .placements()
            .iter()
            .flat_map(|pl| pl.cells().unwrap())
            .collect();
        let free: Vec<Coord> = Coord::all()
            .filter(|co| s.my_board().ship_at(*co).is_none())
            .collect();
        let mut free_squares = free.into_iter();
        for (i, target) in targets.iter().enumerate() {
            s.receive_fire(*target).unwrap();
            if i + 1 < targets.len() {
                s.fire(free_squares.next().unwrap()).unwrap();
                s.receive_result(ShotResult::Miss).unwrap();
            }
        }
        assert_eq!(s.phase(), Phase::Lost);
        assert!(s.is_over());
    }

    #[test]
    fn sinking_the_last_enemy_ship_wins_the_game() {
        // The enemy uses the same layout as ours; we sink it ship by ship.
        let enemy = row_fleet();
        let mut s = ready_session(true);
        let free: Vec<Coord> = Coord::all()
            .filter(|co| s.my_board().ship_at(*co).is_none())
            .collect();
        let mut our_free = free.into_iter();
        let mut enemy_board = enemy.clone();
        let targets: Vec<Coord> = enemy
            .placements()
            .iter()
            .flat_map(|pl| pl.cells().unwrap())
            .collect();
        for target in targets {
            s.fire(target).unwrap();
            let result = enemy_board.fire(target).unwrap();
            s.receive_result(result).unwrap();
            if s.phase() == Phase::Won {
                break;
            }
            s.receive_fire(our_free.next().unwrap()).unwrap();
        }
        assert_eq!(s.phase(), Phase::Won);
        assert!(s.enemy_grid().fleet_destroyed());
    }

    #[test]
    fn new_round_is_only_possible_after_the_game() {
        let mut s = ready_session(true);
        assert!(s.new_round().is_err());
    }

    fn finished_session(i_go_first: bool) -> Session {
        // Lose quickly: the opponent hits every ship square while we keep missing.
        let mut s = ready_session(i_go_first);
        let targets: Vec<Coord> = s
            .my_board()
            .placements()
            .iter()
            .flat_map(|pl| pl.cells().unwrap())
            .collect();
        let free: Vec<Coord> = Coord::all()
            .filter(|co| s.my_board().ship_at(*co).is_none())
            .collect();
        let mut free = free.into_iter();
        for target in targets {
            if s.phase() == Phase::MyTurn {
                s.fire(free.next().unwrap()).unwrap();
                s.receive_result(ShotResult::Miss).unwrap();
            }
            s.receive_fire(target).unwrap();
        }
        assert_eq!(s.phase(), Phase::Lost);
        s
    }

    #[test]
    fn new_round_resets_the_boards_and_swaps_who_starts() {
        let mut s = finished_session(true);
        s.new_round().unwrap();
        assert_eq!(s.phase(), Phase::Placement);
        assert!(!s.my_board().is_fleet_complete());
        assert!(s.enemy_grid().sunk_ships().is_empty());
        assert!(!s.i_go_first());
        s.set_board(Board::random(&mut Rng::new(1))).unwrap();
        s.ready().unwrap();
        s.opponent_ready().unwrap();
        assert_eq!(s.phase(), Phase::TheirTurn);
    }

    #[test]
    fn opponent_can_get_ready_for_the_next_round_before_we_leave_the_result_screen() {
        let mut s = finished_session(false);
        s.opponent_ready().unwrap();
        s.new_round().unwrap();
        s.set_board(row_fleet()).unwrap();
        s.ready().unwrap();
        assert_eq!(s.phase(), Phase::MyTurn);
    }

    #[test]
    fn readiness_from_the_previous_round_does_not_carry_over() {
        let mut s = finished_session(true);
        s.new_round().unwrap();
        s.set_board(row_fleet()).unwrap();
        s.ready().unwrap();
        assert_eq!(s.phase(), Phase::WaitingForOpponent);
    }

    #[test]
    fn opponent_ready_mid_battle_is_rejected() {
        let mut s = ready_session(true);
        assert!(s.opponent_ready().is_err());
    }
}
