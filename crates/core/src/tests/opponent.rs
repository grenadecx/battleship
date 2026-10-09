use super::*;
use crate::ai::choose_target;
use crate::domain::{Coord, ShotResult};

const HUMAN_WINS_SEED: u64 = 0;
const COMPUTER_WINS_SEED: u64 = 4;

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
    next_message(&mut computer);
    human.opponent_ready().unwrap();
    computer.send(Message::Ready);
    (human, computer)
}

/// Plays a whole game, the human aiming like the computer does.
/// Returns every message the computer sent once the game was over.
fn play_out(human: &mut Session, computer: &mut ComputerOpponent, seed: u64) -> Vec<Message> {
    let mut aim = Rng::new(seed + 2);
    while !human.is_over() {
        if human.phase() == Phase::MyTurn {
            let target = choose_target(human.enemy_grid(), &mut aim);
            human.fire(target).unwrap();
            computer.send(Message::Fire(target));
        }
        match next_message(computer) {
            Message::Result(result) => human.receive_result(result).unwrap(),
            Message::Fire(target) => {
                let result = human.receive_fire(target).unwrap();
                computer.send(Message::Result(result));
            }
            other => panic!("unexpected {other:?} in {:?}", human.phase()),
        }
    }
    computer.send(Message::Reveal(human.my_board().placements()));
    std::iter::from_fn(|| computer.poll())
        .map(|event| match event {
            OpponentEvent::Message(message) => message,
            other => panic!("unexpected {other:?} after the game"),
        })
        .collect()
}

fn revealed_ships(after_game: &[Message]) -> Option<usize> {
    after_game.iter().find_map(|message| match message {
        Message::Reveal(placements) => Some(placements.len()),
        _ => None,
    })
}

/// Plays out the game for `seed`, which must end in `outcome` for the human.
/// Returns the human session and every message the computer sent afterwards.
fn finished_game(seed: u64, outcome: Phase) -> (Session, ComputerOpponent, Vec<Message>) {
    let (mut human, mut computer) = start_game(seed);
    let after_game = play_out(&mut human, &mut computer, seed);
    assert_eq!(
        human.phase(),
        outcome,
        "seed {seed} no longer ends this way"
    );
    (human, computer, after_game)
}

/// A lost game after which the human has asked for a rematch with a fresh fleet.
fn rematch_requested() -> ComputerOpponent {
    let (mut human, mut computer, _) = finished_game(COMPUTER_WINS_SEED, Phase::Lost);
    human.new_round().unwrap();
    human.set_board(Board::random(&mut Rng::new(99))).unwrap();
    human.ready().unwrap();
    computer.send(Message::Ready);
    computer
}

#[test]
fn computer_announces_it_is_ready_straight_away() {
    let mut computer = ComputerOpponent::new(Rng::new(1));
    assert_eq!(next_message(&mut computer), Message::Ready);
}

#[test]
fn computer_says_nothing_more_before_the_game_starts() {
    let mut computer = ComputerOpponent::new(Rng::new(1));
    next_message(&mut computer);
    assert_eq!(computer.poll(), None);
}

#[test]
fn computer_waits_for_the_human_to_open() {
    let (_, mut computer) = start_game(1);
    assert_eq!(computer.poll(), None);
}

#[test]
fn computer_answers_a_shot_with_its_result() {
    let (_, mut computer) = start_game(2);
    computer.send(Message::Fire(Coord::new(0, 0)));
    assert!(matches!(next_message(&mut computer), Message::Result(_)));
}

#[test]
fn computer_fires_back_after_answering() {
    let (_, mut computer) = start_game(2);
    computer.send(Message::Fire(Coord::new(0, 0)));
    next_message(&mut computer);
    assert!(matches!(next_message(&mut computer), Message::Fire(_)));
}

#[test]
fn computer_reveals_its_fleet_when_it_loses() {
    let (_, _, after_game) = finished_game(HUMAN_WINS_SEED, Phase::Won);
    assert_eq!(revealed_ships(&after_game), Some(5), "{after_game:?}");
}

#[test]
fn computer_reveals_its_fleet_when_it_wins() {
    let (_, _, after_game) = finished_game(COMPUTER_WINS_SEED, Phase::Lost);
    assert_eq!(revealed_ships(&after_game), Some(5), "{after_game:?}");
}

#[test]
fn computer_gets_ready_for_a_rematch() {
    let mut computer = rematch_requested();
    assert_eq!(next_message(&mut computer), Message::Ready);
}

#[test]
fn computer_opens_the_rematch() {
    let mut computer = rematch_requested();
    next_message(&mut computer);
    assert!(matches!(next_message(&mut computer), Message::Fire(_)));
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

fn gave_up(computer: &mut ComputerOpponent) -> bool {
    matches!(
        computer.poll(),
        Some(OpponentEvent::Disconnected(why)) if why.starts_with("Computer gave up: not allowed")
    )
}

#[test]
fn computer_gives_up_on_a_second_ready_mid_game() {
    let (_, mut computer) = start_game(5);
    computer.send(Message::Ready);
    assert!(gave_up(&mut computer));
}

#[test]
fn computer_gives_up_on_a_shot_out_of_turn() {
    let (_, mut computer) = start_game(6);
    computer.send(Message::Fire(Coord::new(0, 0)));
    next_message(&mut computer);
    next_message(&mut computer);
    // The computer has fired back and awaits our report, not another shot.
    computer.send(Message::Fire(Coord::new(1, 0)));
    assert!(gave_up(&mut computer));
}

#[test]
fn computer_has_no_latency() {
    let (_, computer) = start_game(7);
    assert_eq!(computer.latency(), None);
}
