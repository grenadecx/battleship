//! Opponents speak the wire protocol, so the UI drives a computer player and
//! a remote human through exactly the same interface.

use crate::ai::choose_target;
use crate::domain::Board;
use crate::protocol::Message;
use crate::rng::Rng;
use crate::session::{Phase, Session};
use std::collections::VecDeque;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpponentEvent {
    Message(Message),
    Disconnected(String),
}

pub trait Opponent {
    fn send(&mut self, message: Message);
    /// Next event from the opponent, if any. Never blocks.
    fn poll(&mut self) -> Option<OpponentEvent>;
}

/// The game itself as a player. It always lets the human open the first round.
pub struct ComputerOpponent {
    session: Session,
    rng: Rng,
    outbox: VecDeque<OpponentEvent>,
}

impl ComputerOpponent {
    pub fn new(rng: Rng) -> Self {
        let mut computer = ComputerOpponent {
            session: Session::new(false),
            rng,
            outbox: VecDeque::new(),
        };
        computer.deploy_fleet();
        computer
    }

    fn deploy_fleet(&mut self) {
        let board = Board::random(&mut self.rng);
        self.session.set_board(board).expect("placing a fleet");
        self.session.ready().expect("random fleet is complete");
        self.say(Message::Ready);
    }

    fn say(&mut self, message: Message) {
        self.outbox.push_back(OpponentEvent::Message(message));
    }

    fn handle(&mut self, message: Message) -> Result<(), String> {
        match message {
            Message::Ready => {
                if self.session.is_over() {
                    self.session.new_round().map_err(|e| e.to_string())?;
                    self.deploy_fleet();
                }
                self.session.opponent_ready().map_err(|e| e.to_string())?;
            }
            Message::Fire(target) => {
                let result = self
                    .session
                    .receive_fire(target)
                    .map_err(|e| e.to_string())?;
                self.say(Message::Result(result));
            }
            Message::Result(result) => {
                self.session
                    .receive_result(result)
                    .map_err(|e| e.to_string())?;
            }
            Message::Hello { .. } | Message::Reveal(_) | Message::Bye => {}
        }
        Ok(())
    }

    fn take_turn(&mut self) {
        if self.session.phase() == Phase::MyTurn {
            let target = choose_target(self.session.enemy_grid(), &mut self.rng);
            self.session.fire(target).expect("AI picks open squares");
            self.say(Message::Fire(target));
        }
    }
}

impl Opponent for ComputerOpponent {
    fn send(&mut self, message: Message) {
        let was_over = self.session.is_over();
        match self.handle(message) {
            Ok(()) => {
                if self.session.is_over() && !was_over {
                    let fleet = self.session.my_board().placements();
                    self.say(Message::Reveal(fleet));
                }
                self.take_turn();
            }
            Err(error) => self.outbox.push_back(OpponentEvent::Disconnected(format!(
                "Computer gave up: {error}"
            ))),
        }
    }

    fn poll(&mut self) -> Option<OpponentEvent> {
        self.outbox.pop_front()
    }
}

#[cfg(test)]
#[path = "tests/opponent.rs"]
mod tests;
