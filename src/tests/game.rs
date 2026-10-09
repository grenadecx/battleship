use super::*;
use crate::opponent::ComputerOpponent;
use std::cell::RefCell;
use std::rc::Rc;

/// Both ends of a scripted connection: what we sent and what arrives next.
#[derive(Default)]
struct Wire {
    sent: Vec<Message>,
    incoming: VecDeque<OpponentEvent>,
}

struct Scripted(Rc<RefCell<Wire>>);

impl Opponent for Scripted {
    fn send(&mut self, message: Message) {
        self.0.borrow_mut().sent.push(message);
    }

    fn poll(&mut self) -> Option<OpponentEvent> {
        self.0.borrow_mut().incoming.pop_front()
    }
}

fn scripted(i_go_first: bool) -> (Game, Rc<RefCell<Wire>>) {
    let wire = Rc::new(RefCell::new(Wire::default()));
    let game = Game::new(
        Box::new(Scripted(wire.clone())),
        i_go_first,
        false,
        Rng::new(7),
    );
    (game, wire)
}

fn say(wire: &Rc<RefCell<Wire>>, message: Message) {
    wire.borrow_mut()
        .incoming
        .push_back(OpponentEvent::Message(message));
}

/// A scripted game where both sides are deployed and the battle is on.
fn started(i_go_first: bool) -> (Game, Rc<RefCell<Wire>>) {
    let (mut game, wire) = scripted(i_go_first);
    say(&wire, Message::Ready);
    game.update(0.0);
    assert!(game.deploy());
    game.take_events();
    wire.borrow_mut().sent.clear();
    (game, wire)
}

/// A square of our board without a ship on it.
fn open_water(game: &Game) -> Coord {
    Coord::all()
        .find(|c| game.session().my_board().ship_at(*c).is_none())
        .unwrap()
}

#[test]
fn starts_in_placement_with_a_complete_random_fleet() {
    let (game, _) = scripted(true);
    assert_eq!(game.session().phase(), Phase::Placement);
    assert!(game.editor().is_complete());
    assert!(!game.can_fire());
    assert_eq!((game.shots(), game.hits(), game.accuracy()), (0, 0, 0));
}

#[test]
fn deploying_announces_readiness_and_waits_for_the_opponent() {
    let (mut game, wire) = scripted(true);
    assert!(game.deploy());
    assert_eq!(wire.borrow().sent, vec![Message::Ready]);
    assert_eq!(game.session().phase(), Phase::WaitingForOpponent);
    assert!(game.take_events().is_empty());

    say(&wire, Message::Ready);
    game.update(0.1);
    assert_eq!(
        game.take_events(),
        vec![GameEvent::Started { i_fire_first: true }]
    );
    assert!(game.can_fire());
}

#[test]
fn opponent_ready_first_is_shown_until_we_deploy() {
    let (mut game, wire) = scripted(false);
    say(&wire, Message::Ready);
    game.update(0.0);
    assert!(game.opponent_is_ready());
    assert!(game.take_events().is_empty());

    assert!(game.deploy());
    assert!(!game.opponent_is_ready());
    assert_eq!(
        game.take_events(),
        vec![GameEvent::Started {
            i_fire_first: false
        }]
    );
    assert_eq!(game.session().phase(), Phase::TheirTurn);
}

#[test]
fn an_incomplete_fleet_cannot_be_deployed() {
    let (mut game, wire) = scripted(true);
    game.editor_mut().clear();
    assert!(!game.deploy());
    assert!(wire.borrow().sent.is_empty());
    game.randomize_fleet();
    assert!(game.deploy());
}

#[test]
fn firing_sends_the_shot_and_launches_a_missile() {
    let (mut game, wire) = started(true);
    let target = Coord::new(4, 4);
    game.fire(target, 1.0);
    assert_eq!(wire.borrow().sent, vec![Message::Fire(target)]);
    assert_eq!(game.shots(), 1);
    assert_eq!(
        game.take_events(),
        vec![GameEvent::Launched(Shot::Outgoing(target))]
    );
    assert_eq!(game.missile().map(|m| m.shot), Some(Shot::Outgoing(target)));
    assert!(!game.can_fire());

    // A second shot while the first is in the air is ignored.
    game.fire(Coord::new(5, 5), 1.1);
    assert_eq!(game.shots(), 1);
    assert_eq!(wire.borrow().sent.len(), 1);
}

#[test]
fn firing_out_of_turn_does_nothing() {
    let (mut game, wire) = started(false);
    game.fire(Coord::new(0, 0), 1.0);
    assert_eq!(game.shots(), 0);
    assert!(wire.borrow().sent.is_empty());
    assert!(game.missile().is_none());
}

#[test]
fn the_result_is_shown_after_our_missile_lands() {
    let (mut game, wire) = started(true);
    let target = Coord::new(2, 3);
    game.fire(target, 1.0);
    game.take_events();
    say(&wire, Message::Result(ShotResult::Hit));

    game.update(1.0 + MISSILE_SECONDS / 2.0);
    assert!(game.take_events().is_empty(), "missile still in flight");
    assert_eq!(game.hits(), 0);

    game.update(1.0 + MISSILE_SECONDS);
    assert!(game.missile().is_none());
    assert_eq!(
        game.take_events(),
        vec![GameEvent::Impact {
            target,
            result: ShotResult::Hit,
            on_me: false
        }]
    );
    assert_eq!((game.shots(), game.hits(), game.accuracy()), (1, 1, 100));
    assert_eq!(game.session().phase(), Phase::TheirTurn);
}

#[test]
fn misses_do_not_count_as_hits() {
    let (mut game, wire) = started(true);
    game.fire(Coord::new(0, 0), 0.0);
    say(&wire, Message::Result(ShotResult::Miss));
    game.update(MISSILE_SECONDS);
    assert_eq!((game.shots(), game.hits(), game.accuracy()), (1, 0, 0));
}

#[test]
fn incoming_fire_is_answered_once_the_missile_lands() {
    let (mut game, wire) = started(false);
    let target = open_water(&game);
    say(&wire, Message::Fire(target));
    game.update(1.0);
    assert_eq!(
        game.take_events(),
        vec![GameEvent::Launched(Shot::Incoming(target))]
    );
    assert!(wire.borrow().sent.is_empty(), "answered before landing");

    game.update(1.0 + MISSILE_SECONDS);
    assert_eq!(wire.borrow().sent, vec![Message::Result(ShotResult::Miss)]);
    assert_eq!(
        game.take_events(),
        vec![GameEvent::Impact {
            target,
            result: ShotResult::Miss,
            on_me: true
        }]
    );
    assert!(game.session().my_board().was_shot(target));
    assert!(game.can_fire());
}

#[test]
fn the_next_message_waits_for_the_impact_to_play_out() {
    let (mut game, wire) = started(true);
    game.fire(Coord::new(0, 0), 0.0);
    say(&wire, Message::Result(ShotResult::Miss));
    let target = open_water(&game);
    say(&wire, Message::Fire(target));

    let landed = MISSILE_SECONDS;
    game.update(landed);
    game.take_events();
    game.update(landed + 0.3);
    assert!(game.take_events().is_empty(), "online pause is 0.35s");
    game.update(landed + 0.35);
    assert_eq!(
        game.take_events(),
        vec![GameEvent::Launched(Shot::Incoming(target))]
    );
}

#[test]
fn games_against_the_computer_pause_longer_between_shots() {
    let wire = Rc::new(RefCell::new(Wire::default()));
    let mut game = Game::new(Box::new(Scripted(wire.clone())), true, true, Rng::new(1));
    say(&wire, Message::Ready);
    game.update(0.0);
    game.deploy();
    game.fire(Coord::new(0, 0), 0.0);
    say(&wire, Message::Result(ShotResult::Miss));
    say(&wire, Message::Fire(open_water(&game)));
    game.update(MISSILE_SECONDS);
    game.take_events();
    game.update(MISSILE_SECONDS + 0.8);
    assert!(game.take_events().is_empty());
    game.update(MISSILE_SECONDS + 0.85);
    assert_eq!(game.take_events().len(), 1);
}

#[test]
fn firing_out_of_turn_stops_the_game() {
    let (mut game, wire) = started(true);
    say(&wire, Message::Fire(Coord::new(0, 0)));
    game.update(0.0);
    assert_eq!(
        game.disconnected(),
        Some("Game stopped: Opponent fired at A1 out of turn")
    );
    assert!(!game.can_fire());
}

#[test]
fn firing_twice_at_the_same_square_stops_the_game() {
    let (mut game, wire) = started(false);
    let target = open_water(&game);
    say(&wire, Message::Fire(target));
    game.update(0.0);
    game.update(MISSILE_SECONDS);
    game.fire(Coord::new(0, 0), 1.0);
    say(&wire, Message::Result(ShotResult::Miss));
    say(&wire, Message::Fire(target));
    for step in 1..20 {
        game.update(1.0 + step as f64 * 0.25);
    }
    assert!(
        game.disconnected()
            .is_some_and(|why| why.contains("out of turn")),
        "{:?}",
        game.disconnected()
    );
}

#[test]
fn a_result_for_a_shot_we_did_not_fire_stops_the_game() {
    let (mut game, wire) = started(true);
    say(&wire, Message::Result(ShotResult::Hit));
    game.update(0.0);
    assert_eq!(
        game.disconnected(),
        Some("Game stopped: Opponent reported a shot we did not fire")
    );
}

#[test]
fn an_impossible_result_stops_the_game() {
    let (mut game, wire) = started(true);
    game.fire(Coord::new(0, 0), 0.0);
    say(
        &wire,
        Message::Result(ShotResult::Sunk {
            kind: crate::domain::ShipKind::Carrier,
            cells: vec![Coord::new(9, 9)],
        }),
    );
    game.update(MISSILE_SECONDS);
    assert!(
        game.disconnected()
            .is_some_and(|why| why.starts_with("Game stopped: bad report")),
        "{:?}",
        game.disconnected()
    );
}

#[test]
fn a_second_ready_mid_game_stops_the_game() {
    let (mut game, wire) = started(true);
    say(&wire, Message::Ready);
    game.update(0.0);
    assert!(
        game.disconnected()
            .is_some_and(|why| why.starts_with("Game stopped: Unexpected READY"))
    );
}

#[test]
fn a_dropped_connection_freezes_the_game() {
    let (mut game, wire) = started(false);
    wire.borrow_mut()
        .incoming
        .push_back(OpponentEvent::Disconnected("Connection reset".into()));
    say(&wire, Message::Fire(open_water(&game)));
    game.update(0.0);
    assert_eq!(game.disconnected(), Some("Connection reset"));
    assert!(game.take_events().is_empty());
    assert!(game.missile().is_none());
}

#[test]
fn the_first_reason_to_stop_is_kept() {
    let (mut game, wire) = started(true);
    say(&wire, Message::Result(ShotResult::Hit));
    game.update(0.0);
    wire.borrow_mut()
        .incoming
        .push_back(OpponentEvent::Disconnected("later".into()));
    game.update(1.0);
    assert_eq!(
        game.disconnected(),
        Some("Game stopped: Opponent reported a shot we did not fire")
    );
}

#[test]
fn hello_and_bye_are_ignored_mid_game() {
    let (mut game, wire) = started(true);
    say(&wire, Message::Hello { version: 1 });
    say(&wire, Message::Bye);
    game.update(0.0);
    game.update(1.0);
    assert_eq!(game.disconnected(), None);
    assert!(game.can_fire());
}

#[test]
fn missile_progress_runs_from_launch_to_landing() {
    let missile = Missile {
        shot: Shot::Outgoing(Coord::new(0, 0)),
        started: 2.0,
    };
    assert_eq!(missile.progress(1.0), 0.0);
    assert_eq!(missile.progress(2.0), 0.0);
    assert!((missile.progress(2.0 + MISSILE_SECONDS / 2.0) - 0.5).abs() < 1e-6);
    assert_eq!(missile.progress(2.0 + MISSILE_SECONDS), 1.0);
    assert_eq!(missile.progress(10.0), 1.0);
}

/// Plays a whole game against the computer, firing in reading order.
fn play_against_computer(game: &mut Game, mut now: f64) -> f64 {
    let mut targets = Coord::all();
    while game.game_over_at().is_none() {
        assert_eq!(game.disconnected(), None);
        if game.can_fire() {
            let target = targets
                .by_ref()
                .find(|c| game.session().enemy_grid().can_target(*c))
                .expect("an open square while the game is on");
            game.fire(target, now);
        }
        now += 0.1;
        game.update(now);
        assert!(now < 10_000.0, "game never finished");
    }
    now
}

fn against_computer(seed: u64) -> Game {
    let computer = ComputerOpponent::new(Rng::new(seed));
    let mut game = Game::new(Box::new(computer), true, true, Rng::new(seed + 1));
    assert!(game.deploy());
    game.update(0.0);
    assert!(game.can_fire());
    game
}

#[test]
fn a_game_against_the_computer_ends_with_score_reveal_and_fanfare() {
    for seed in 0..5 {
        let mut game = against_computer(seed);
        let over = play_against_computer(&mut game, 0.0);
        let won = game.session().phase() == Phase::Won;
        assert_eq!(
            (game.wins(), game.losses()),
            if won { (1, 0) } else { (0, 1) }
        );
        assert!(game.shots() >= game.hits());

        game.take_events();
        game.update(over + FANFARE_DELAY / 2.0);
        assert!(
            !game
                .take_events()
                .contains(&GameEvent::Fanfare(Effect::Victory))
        );
        let mut now = over;
        while now < over + FANFARE_DELAY + 1.0 {
            now += 0.1;
            game.update(now);
        }
        let fanfare = if won { Effect::Victory } else { Effect::Defeat };
        assert!(game.take_events().contains(&GameEvent::Fanfare(fanfare)));
        assert_eq!(game.enemy_fleet().len(), 5, "seed {seed}");
    }
}

#[test]
fn a_rematch_resets_the_round_but_keeps_the_score() {
    let mut game = against_computer(3);
    let over = play_against_computer(&mut game, 0.0);
    let score = (game.wins(), game.losses());
    // Let the fanfare and the computer's reveal play out, as the overlay does.
    game.update(over + FANFARE_DELAY + 1.0);
    game.update(over + FANFARE_DELAY + 2.0);

    game.new_round();
    assert_eq!(game.session().phase(), Phase::Placement);
    assert!(game.editor().is_complete());
    assert!(game.enemy_fleet().is_empty());
    assert_eq!(game.game_over_at(), None);
    assert_eq!((game.shots(), game.hits()), (0, 0));
    assert_eq!((game.wins(), game.losses()), score);

    // The computer opens the rematch.
    assert!(game.deploy());
    game.update(over + 5.0);
    assert_eq!(game.session().phase(), Phase::TheirTurn);
}

#[test]
fn new_round_is_ignored_while_a_game_is_on() {
    let (mut game, _) = started(true);
    game.new_round();
    assert_eq!(game.session().phase(), Phase::MyTurn);
}

#[test]
fn sinking_the_whole_enemy_fleet_wins() {
    let (mut game, wire) = started(true);
    let mut enemy = crate::domain::Board::random(&mut Rng::new(5));
    let mut targets = enemy
        .placements()
        .into_iter()
        .flat_map(|p| p.cells().unwrap());
    let my_board = game.session().my_board().clone();
    let mut water = Coord::all().filter(|c| my_board.ship_at(*c).is_none());
    let mut now = 0.0;
    while game.game_over_at().is_none() {
        if game.can_fire() {
            let target = targets.next().expect("fleet sunk before the game ended");
            game.fire(target, now);
            say(&wire, Message::Result(enemy.fire(target).unwrap()));
            // Like the computer, the opponent fires back right after reporting.
            if !enemy.all_sunk() {
                say(&wire, Message::Fire(water.next().unwrap()));
            }
        }
        now += 0.1;
        game.update(now);
        assert_eq!(game.disconnected(), None);
    }
    assert_eq!(game.session().phase(), Phase::Won);
    assert_eq!((game.wins(), game.losses()), (1, 0));
    assert_eq!((game.shots(), game.hits(), game.accuracy()), (17, 17, 100));
    assert!(matches!(
        wire.borrow().sent.last(),
        Some(Message::Reveal(fleet)) if fleet.len() == 5
    ));
    game.update(now + FANFARE_DELAY);
    assert!(
        game.take_events()
            .contains(&GameEvent::Fanfare(Effect::Victory))
    );
}
