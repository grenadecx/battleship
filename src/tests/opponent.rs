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
            Some(OpponentEvent::Message(message)) if human.is_over() => after_game.push(message),
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

#[test]
fn computer_gives_up_on_a_second_ready_mid_game() {
    let (_, mut computer) = start_game(5);
    computer.send(Message::Ready);
    assert!(matches!(
        computer.poll(),
        Some(OpponentEvent::Disconnected(why)) if why.starts_with("Computer gave up: not allowed")
    ));
}

#[test]
fn computer_gives_up_on_a_shot_out_of_turn() {
    let (_, mut computer) = start_game(6);
    computer.send(Message::Fire(Coord::new(0, 0)));
    assert!(matches!(next_message(&mut computer), Message::Result(_)));
    assert!(matches!(next_message(&mut computer), Message::Fire(_)));
    // The computer has fired back and awaits our report, not another shot.
    computer.send(Message::Fire(Coord::new(1, 0)));
    assert!(matches!(
        computer.poll(),
        Some(OpponentEvent::Disconnected(why)) if why.starts_with("Computer gave up: not allowed")
    ));
}
