//! One match against an opponent, as the UI plays it: relays moves between our
//! session and the opponent, paces them so every shot can be animated, and
//! keeps score across rounds. The shell renders the state and reacts to the
//! [`GameEvent`]s with sound and effects.

use crate::domain::{Coord, Placement, ShotResult};
use crate::opponent::{Opponent, OpponentEvent};
use crate::protocol::Message;
use crate::rng::Rng;
use crate::session::{Phase, Session};
use crate::setup::FleetEditor;
use crate::sound::Effect;
use std::collections::VecDeque;

/// Time a missile is in the air, in seconds.
pub const MISSILE_SECONDS: f64 = 0.65;
/// Delay between the deciding shot and the victory or defeat fanfare.
pub const FANFARE_DELAY: f64 = 1.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shot {
    /// Our shell, flying at a square of the enemy ocean.
    Outgoing(Coord),
    /// The opponent's shell, flying at a square of ours.
    Incoming(Coord),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Missile {
    pub shot: Shot,
    pub started: f64,
}

impl Missile {
    /// Flight progress at `now`, from 0.0 at launch to 1.0 on landing.
    pub fn progress(&self, now: f64) -> f32 {
        ((now - self.started) / MISSILE_SECONDS).clamp(0.0, 1.0) as f32
    }
}

/// Something the UI should show or play.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GameEvent {
    Launched(Shot),
    /// A shot has been resolved. `on_me` when it landed on our board.
    Impact {
        target: Coord,
        result: ShotResult,
        on_me: bool,
    },
    /// Both fleets are deployed and the battle begins.
    Started {
        i_fire_first: bool,
    },
    /// Victory or defeat, a moment after the game was decided.
    Fanfare(Effect),
}

pub struct Game {
    opponent: Box<dyn Opponent>,
    vs_computer: bool,
    session: Session,
    editor: FleetEditor,
    rng: Rng,
    inbox: VecDeque<Message>,
    missile: Option<Missile>,
    hold_until: f64,
    opponent_is_ready: bool,
    enemy_fleet: Vec<Placement>,
    disconnected: Option<String>,
    game_over_at: Option<f64>,
    pending_fanfare: Option<(f64, Effect)>,
    shots: u32,
    hits: u32,
    wins: u32,
    losses: u32,
    events: Vec<GameEvent>,
}

impl Game {
    pub fn new(
        opponent: Box<dyn Opponent>,
        i_go_first: bool,
        vs_computer: bool,
        mut rng: Rng,
    ) -> Self {
        let mut editor = FleetEditor::new();
        editor.randomize(&mut rng);
        Game {
            opponent,
            vs_computer,
            session: Session::new(i_go_first),
            editor,
            rng,
            inbox: VecDeque::new(),
            missile: None,
            hold_until: 0.0,
            opponent_is_ready: false,
            enemy_fleet: Vec::new(),
            disconnected: None,
            game_over_at: None,
            pending_fanfare: None,
            shots: 0,
            hits: 0,
            wins: 0,
            losses: 0,
            events: Vec::new(),
        }
    }

    pub fn session(&self) -> &Session {
        &self.session
    }

    pub fn editor(&self) -> &FleetEditor {
        &self.editor
    }

    pub fn editor_mut(&mut self) -> &mut FleetEditor {
        &mut self.editor
    }

    pub fn missile(&self) -> Option<&Missile> {
        self.missile.as_ref()
    }

    /// The opponent has deployed and waits for us.
    pub fn opponent_is_ready(&self) -> bool {
        self.opponent_is_ready
    }

    /// The enemy fleet, once revealed at the end of a game.
    pub fn enemy_fleet(&self) -> &[Placement] {
        &self.enemy_fleet
    }

    /// Why the game cannot continue, if it cannot.
    pub fn disconnected(&self) -> Option<&str> {
        self.disconnected.as_deref()
    }

    pub fn game_over_at(&self) -> Option<f64> {
        self.game_over_at
    }

    pub fn shots(&self) -> u32 {
        self.shots
    }

    pub fn hits(&self) -> u32 {
        self.hits
    }

    /// Percentage of this round's shots that hit, rounded down.
    pub fn accuracy(&self) -> u32 {
        (100 * self.hits).checked_div(self.shots).unwrap_or(0)
    }

    pub fn wins(&self) -> u32 {
        self.wins
    }

    pub fn losses(&self) -> u32 {
        self.losses
    }

    /// Whether we may pick a target right now.
    pub fn can_fire(&self) -> bool {
        self.session.phase() == Phase::MyTurn
            && self.missile.is_none()
            && self.disconnected.is_none()
    }

    /// Events since the last call, oldest first.
    pub fn take_events(&mut self) -> Vec<GameEvent> {
        std::mem::take(&mut self.events)
    }

    pub fn randomize_fleet(&mut self) {
        self.editor.randomize(&mut self.rng);
    }

    /// Advances to `now`: collects the opponent's messages, lands missiles and
    /// handles at most one queued message once the previous shot has played out.
    pub fn update(&mut self, now: f64) {
        while let Some(event) = self.opponent.poll() {
            match event {
                OpponentEvent::Message(message) => self.inbox.push_back(message),
                OpponentEvent::Disconnected(reason) => {
                    self.disconnected.get_or_insert(reason);
                }
            }
        }
        if self.disconnected.is_some() {
            return;
        }
        if let Some((at, effect)) = self.pending_fanfare
            && now >= at
        {
            self.events.push(GameEvent::Fanfare(effect));
            self.pending_fanfare = None;
        }
        if self
            .missile
            .is_some_and(|m| now >= m.started + MISSILE_SECONDS)
        {
            let missile = self.missile.take().expect("checked above");
            self.land(missile.shot, now);
        }
        if self.missile.is_none()
            && now >= self.hold_until
            && let Some(message) = self.inbox.pop_front()
        {
            self.handle(message, now);
        }
    }

    /// Commits the fleet in the editor and tells the opponent we are ready.
    /// Returns whether the fleet was accepted.
    pub fn deploy(&mut self) -> bool {
        if self.session.set_board(self.editor.board().clone()).is_err()
            || self.session.ready().is_err()
        {
            return false;
        }
        self.opponent.send(Message::Ready);
        self.on_maybe_started();
        true
    }

    /// Fires at `target` if it is our turn, the square is open and no missile
    /// is in the air.
    pub fn fire(&mut self, target: Coord, now: f64) {
        if self.missile.is_some() || self.session.fire(target).is_err() {
            return;
        }
        self.shots += 1;
        self.opponent.send(Message::Fire(target));
        self.launch(Shot::Outgoing(target), now);
    }

    /// Starts a rematch after a finished game. The score carries over.
    pub fn new_round(&mut self) {
        if self.session.new_round().is_ok() {
            self.editor = FleetEditor::new();
            self.editor.randomize(&mut self.rng);
            self.enemy_fleet.clear();
            self.game_over_at = None;
            self.pending_fanfare = None;
            self.shots = 0;
            self.hits = 0;
        }
    }

    fn handle(&mut self, message: Message, now: f64) {
        match message {
            Message::Ready => match self.session.opponent_ready() {
                Ok(()) => {
                    self.opponent_is_ready = true;
                    self.on_maybe_started();
                }
                Err(e) => self.broken(format!("Unexpected READY: {e}")),
            },
            Message::Fire(target) => {
                if self.session.phase() != Phase::TheirTurn
                    || self.session.my_board().was_shot(target)
                {
                    self.broken(format!("Opponent fired at {} out of turn", target.label()));
                    return;
                }
                self.launch(Shot::Incoming(target), now);
            }
            Message::Result(result) => {
                let Phase::AwaitingResult(target) = self.session.phase() else {
                    self.broken("Opponent reported a shot we did not fire".into());
                    return;
                };
                if let Err(e) = self.session.receive_result(result.clone()) {
                    self.broken(e.to_string());
                    return;
                }
                if result != ShotResult::Miss {
                    self.hits += 1;
                }
                self.impact(target, result, false, now);
                self.check_game_over(now);
            }
            Message::Reveal(fleet) => self.enemy_fleet = fleet,
            Message::Hello { .. } | Message::Bye => {}
        }
    }

    fn launch(&mut self, shot: Shot, now: f64) {
        self.events.push(GameEvent::Launched(shot));
        self.missile = Some(Missile { shot, started: now });
    }

    fn land(&mut self, shot: Shot, now: f64) {
        // Our own shell lands silently; the opponent's report triggers the impact.
        if let Shot::Incoming(target) = shot {
            match self.session.receive_fire(target) {
                Ok(result) => {
                    self.opponent.send(Message::Result(result.clone()));
                    self.impact(target, result, true, now);
                    self.check_game_over(now);
                }
                Err(e) => self.broken(e.to_string()),
            }
        }
    }

    fn impact(&mut self, target: Coord, result: ShotResult, on_me: bool, now: f64) {
        self.events.push(GameEvent::Impact {
            target,
            result,
            on_me,
        });
        // Let the effect breathe before the next shot comes in.
        let pause = if self.vs_computer { 0.85 } else { 0.35 };
        self.hold_until = now + pause;
    }

    fn check_game_over(&mut self, now: f64) {
        if !self.session.is_over() {
            return;
        }
        self.game_over_at = Some(now);
        self.opponent
            .send(Message::Reveal(self.session.my_board().placements()));
        let fanfare = if self.session.phase() == Phase::Won {
            self.wins += 1;
            Effect::Victory
        } else {
            self.losses += 1;
            Effect::Defeat
        };
        self.pending_fanfare = Some((now + FANFARE_DELAY, fanfare));
    }

    fn on_maybe_started(&mut self) {
        let i_fire_first = match self.session.phase() {
            Phase::MyTurn => true,
            Phase::TheirTurn => false,
            _ => return,
        };
        self.opponent_is_ready = false;
        self.events.push(GameEvent::Started { i_fire_first });
    }

    fn broken(&mut self, why: String) {
        self.disconnected
            .get_or_insert(format!("Game stopped: {why}"));
    }
}

#[cfg(test)]
mod tests {
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
}
