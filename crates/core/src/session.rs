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
#[path = "tests/session.rs"]
mod tests;
