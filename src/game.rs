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
use std::time::Duration;

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

    /// Round-trip time to a network opponent, once measured.
    pub fn latency(&self) -> Option<Duration> {
        self.opponent.latency()
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
            Message::Hello { .. } | Message::Ping(_) | Message::Pong(_) | Message::Bye => {}
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
#[path = "tests/game.rs"]
mod tests;
