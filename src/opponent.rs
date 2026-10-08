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
mod tests {
    use super::*;
    use crate::domain::{Coord, ShotResult};

    fn next_message(computer: &mut ComputerOpponent) -> Message {
        match computer.poll() {
            Some(OpponentEvent::Message(m)) => m,
            other => panic!("expected a message, got {other:?}"),
        }
    }

    /// Human session that is ready and has received the computer's READY.
    fn start_game(seed: u64) -> (Session, ComputerOpponent) {
        let mut human = Session::new(true);
        human.set_board(Board::random(&mut Rng::new(seed))).unwrap();
        human.ready().unwrap();
        let mut computer = ComputerOpponent::new(Rng::new(seed + 1));
        assert_eq!(next_message(&mut computer), Message::Ready);
        human.opponent_ready().unwrap();
        computer.send(Message::Ready);
        (human, computer)
    }

    /// Plays a whole game with the human shooting squares in order.
    /// Returns every message the computer sent after the game ended.
    fn play_out(human: &mut Session, computer: &mut ComputerOpponent) -> Vec<Message> {
        let mut human_targets = Coord::all();
        let mut after_game = Vec::new();
        for _ in 0..500 {
            if human.phase() == Phase::MyTurn {
                let target = human_targets
                    .find(|c| human.enemy_grid().can_target(*c))
                    .unwrap();
                human.fire(target).unwrap();
                computer.send(Message::Fire(target));
            }
            match computer.poll() {
                Some(OpponentEvent::Message(message)) if human.is_over() => {
                    after_game.push(message)
                }
                Some(OpponentEvent::Message(Message::Result(result))) => {
                    human.receive_result(result).unwrap();
                    if human.is_over() {
                        computer.send(Message::Reveal(human.my_board().placements()));
                    }
                }
                Some(OpponentEvent::Message(Message::Fire(target))) => {
                    let result = human.receive_fire(target).unwrap();
                    computer.send(Message::Result(result));
                    if human.is_over() {
                        computer.send(Message::Reveal(human.my_board().placements()));
                    }
                }
                Some(other) => panic!("unexpected {other:?} in {:?}", human.phase()),
                None if human.is_over() => return after_game,
                None => {}
            }
        }
        panic!("game did not finish");
    }

    #[test]
    fn computer_is_ready_straight_away() {
        let mut computer = ComputerOpponent::new(Rng::new(1));
        assert_eq!(
            computer.poll(),
            Some(OpponentEvent::Message(Message::Ready))
        );
        assert_eq!(computer.poll(), None);
    }

    #[test]
    fn computer_waits_for_the_human_to_open() {
        let (_, mut computer) = start_game(1);
        assert_eq!(computer.poll(), None);
    }

    #[test]
    fn computer_answers_a_shot_and_fires_back() {
        let (mut human, mut computer) = start_game(2);
        human.fire(Coord::new(0, 0)).unwrap();
        computer.send(Message::Fire(Coord::new(0, 0)));
        let Message::Result(result) = next_message(&mut computer) else {
            panic!("expected a result");
        };
        human.receive_result(result).unwrap();
        let Message::Fire(target) = next_message(&mut computer) else {
            panic!("expected the computer to fire back");
        };
        assert!(human.receive_fire(target).is_ok());
    }

    #[test]
    fn a_game_against_the_computer_always_finishes_with_a_reveal() {
        for seed in 0..10 {
            let (mut human, mut computer) = start_game(seed * 10);
            let after = play_out(&mut human, &mut computer);
            assert!(human.is_over());
            let reveal = after.iter().find_map(|m| match m {
                Message::Reveal(placements) => Some(placements.len()),
                _ => None,
            });
            assert_eq!(reveal, Some(5), "seed {seed}: {after:?}");
        }
    }

    #[test]
    fn computer_beats_a_human_who_shoots_in_reading_order_most_of_the_time() {
        let wins = (0..10)
            .filter(|seed| {
                let (mut human, mut computer) = start_game(seed * 7 + 3);
                play_out(&mut human, &mut computer);
                human.phase() == Phase::Lost
            })
            .count();
        assert!(wins >= 7, "computer only won {wins}/10");
    }

    #[test]
    fn computer_plays_a_rematch_and_opens_it() {
        let (mut human, mut computer) = start_game(4);
        play_out(&mut human, &mut computer);
        human.new_round().unwrap();
        human.set_board(Board::random(&mut Rng::new(99))).unwrap();
        human.ready().unwrap();
        computer.send(Message::Ready);
        assert_eq!(next_message(&mut computer), Message::Ready);
        human.opponent_ready().unwrap();
        assert_eq!(human.phase(), Phase::TheirTurn);
        let Message::Fire(target) = next_message(&mut computer) else {
            panic!("computer should open the second round");
        };
        human.receive_fire(target).unwrap();
    }

    #[test]
    fn computer_reports_nonsense_as_a_disconnect() {
        let (_, mut computer) = start_game(5);
        computer.send(Message::Result(ShotResult::Miss));
        assert!(matches!(
            computer.poll(),
            Some(OpponentEvent::Disconnected(_))
        ));
    }
}
