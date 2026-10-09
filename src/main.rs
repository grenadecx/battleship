//! Graphical shell: screens, drawing, animation and sound. All game rules live
//! in the library crate and are covered by its tests.

use battleship::domain::{BOARD_SIZE, Coord, FLEET, Orientation, Placement, ShipKind, ShotResult};
use battleship::grid::Knowledge;
use battleship::net::{self, DEFAULT_PORT, Host, Joining};
use battleship::opponent::{ComputerOpponent, Opponent, OpponentEvent};
use battleship::protocol::Message;
use battleship::rng::Rng;
use battleship::session::{Phase, Session};
use battleship::setup::FleetEditor;
use battleship::sound::{self, Effect};
use macroquad::audio::{PlaySoundParams, Sound, load_sound_from_bytes, play_sound};
use macroquad::prelude::*;
use std::cell::Cell;
use std::collections::{HashMap, VecDeque};

static FONT_BYTES: &[u8] = include_bytes!("../assets/DejaVuSans-Bold.ttf");

thread_local! {
    /// Leaked on purpose: dropping the font's texture during thread-local teardown,
    /// after macroquad has destroyed the GL context, segfaults on exit.
    static FONT: Cell<Option<&'static Font>> = const { Cell::new(None) };
}

fn load_font() {
    if let Ok(mut font) = load_ttf_font_from_bytes(FONT_BYTES) {
        font.set_filter(FilterMode::Linear);
        FONT.set(Some(Box::leak(Box::new(font))));
    }
}

/// Draws text with the embedded font; `y` is the baseline.
fn text(s: &str, x: f32, y: f32, size: f32, color: Color) -> TextDimensions {
    let font_size = (size * 0.8).round().max(1.0) as u16;
    draw_text_ex(
        s,
        x,
        y,
        TextParams {
            font: FONT.get(),
            font_size,
            color,
            ..Default::default()
        },
    )
}

fn measure(s: &str, size: f32) -> TextDimensions {
    let font_size = (size * 0.8).round().max(1.0) as u16;
    measure_text(s, FONT.get(), font_size, 1.0)
}

// ---------------------------------------------------------------------------
// Layout (virtual canvas, scaled to the window)
// ---------------------------------------------------------------------------

const VW: f32 = 1200.0;
const VH: f32 = 760.0;
const CELL: f32 = 38.0;
const BOARD_PX: f32 = CELL * BOARD_SIZE as f32;
const LEFT_BOARD: Vec2 = Vec2::new(110.0, 165.0);
const RIGHT_BOARD: Vec2 = Vec2::new(710.0, 165.0);

const MISSILE_SECONDS: f32 = 0.65;

// Palette
const BG_TOP: Color = Color::new(0.02, 0.06, 0.13, 1.0);
const BG_BOTTOM: Color = Color::new(0.03, 0.15, 0.27, 1.0);
const INK: Color = Color::new(0.88, 0.94, 1.0, 1.0);
const MUTED: Color = Color::new(0.55, 0.66, 0.78, 1.0);
const ACCENT: Color = Color::new(1.0, 0.78, 0.25, 1.0);
const DANGER: Color = Color::new(0.95, 0.30, 0.25, 1.0);
const GOOD: Color = Color::new(0.35, 0.85, 0.5, 1.0);
const HULL: Color = Color::new(0.56, 0.62, 0.68, 1.0);
const HULL_DARK: Color = Color::new(0.33, 0.38, 0.44, 1.0);

fn window_conf() -> Conf {
    Conf {
        window_title: "Battleship".to_owned(),
        window_width: VW as i32,
        window_height: VH as i32,
        window_resizable: true,
        sample_count: 4,
        ..Default::default()
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    load_font();
    let mut audio = Audio::load().await;
    let mut app = App::new();
    loop {
        let camera = letterbox_camera();
        set_camera(&camera);
        let mut ui = Ui::begin(&camera);
        draw_background(ui.time);

        if is_key_pressed(KeyCode::M) && !app.is_typing() {
            audio.muted = !audio.muted;
        }
        app.frame(&mut ui, &audio);
        draw_text_right(
            if audio.muted {
                "Sound off (M)"
            } else {
                "Sound on (M)"
            },
            VW - 16.0,
            VH - 14.0,
            18.0,
            MUTED,
        );
        let fps = text(&format!("{} FPS", get_fps()), 16.0, VH - 14.0, 18.0, MUTED);
        if let Some(latency) = app.latency() {
            text(
                &format!("Ping {} ms", latency.as_millis()),
                16.0 + fps.width + 20.0,
                VH - 14.0,
                18.0,
                MUTED,
            );
        }

        if ui.clicked_button {
            audio.play(Effect::Click);
        }
        if app.quit {
            break;
        }
        next_frame().await;
    }
}

/// Maps the fixed virtual canvas onto the window, keeping the aspect ratio.
fn letterbox_camera() -> Camera2D {
    let (sw, sh) = (screen_width().max(1.0), screen_height().max(1.0));
    let scale = (sw / VW).min(sh / VH);
    let (w, h) = (sw / scale, sh / scale);
    Camera2D {
        target: vec2(VW / 2.0, VH / 2.0),
        zoom: vec2(2.0 / w, 2.0 / h),
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// Input and widgets
// ---------------------------------------------------------------------------

struct Ui {
    mouse: Vec2,
    clicked: bool,
    right_clicked: bool,
    time: f64,
    chars: Vec<char>,
    clicked_button: bool,
}

impl Ui {
    fn begin(camera: &Camera2D) -> Self {
        let mut chars = Vec::new();
        while let Some(c) = get_char_pressed() {
            chars.push(c);
        }
        Ui {
            mouse: camera.screen_to_world(mouse_position().into()),
            clicked: is_mouse_button_pressed(MouseButton::Left),
            right_clicked: is_mouse_button_pressed(MouseButton::Right),
            time: get_time(),
            chars,
            clicked_button: false,
        }
    }

    fn button(&mut self, rect: Rect, label: &str, enabled: bool) -> bool {
        let hovered = enabled && rect.contains(self.mouse);
        let fill = match (enabled, hovered) {
            (false, _) => Color::new(0.15, 0.2, 0.27, 0.8),
            (true, true) => Color::new(0.22, 0.45, 0.7, 1.0),
            (true, false) => Color::new(0.13, 0.3, 0.5, 1.0),
        };
        draw_rectangle(
            rect.x,
            rect.y + 3.0,
            rect.w,
            rect.h,
            Color::new(0.0, 0.0, 0.0, 0.3),
        );
        draw_rectangle(rect.x, rect.y, rect.w, rect.h, fill);
        draw_rectangle_lines(
            rect.x,
            rect.y,
            rect.w,
            rect.h,
            2.0,
            if hovered {
                ACCENT
            } else {
                Color::new(0.4, 0.6, 0.8, 0.6)
            },
        );
        let color = if enabled { INK } else { MUTED };
        draw_text_centered(
            label,
            rect.x + rect.w / 2.0,
            rect.y + rect.h / 2.0 + 8.0,
            24.0,
            color,
        );
        let clicked = hovered && self.clicked;
        if clicked {
            self.clicked_button = true;
            self.clicked = false;
        }
        clicked
    }
}

struct TextInput {
    text: String,
    max_len: usize,
}

impl TextInput {
    fn new(text: &str, max_len: usize) -> Self {
        TextInput {
            text: text.to_string(),
            max_len,
        }
    }

    fn update(&mut self, ui: &Ui) {
        let ctrl = is_key_down(KeyCode::LeftControl)
            || is_key_down(KeyCode::RightControl)
            || is_key_down(KeyCode::LeftSuper)
            || is_key_down(KeyCode::RightSuper);
        if ctrl && is_key_pressed(KeyCode::V) {
            if let Some(pasted) = macroquad::miniquad::window::clipboard_get() {
                self.push_str(pasted.trim());
            }
        } else if !ctrl {
            for c in &ui.chars {
                self.push_str(&c.to_string());
            }
        }
        if is_key_pressed(KeyCode::Backspace) {
            self.text.pop();
        }
    }

    fn push_str(&mut self, s: &str) {
        for c in s.chars().filter(char::is_ascii_graphic) {
            if self.text.len() < self.max_len {
                self.text.push(c);
            }
        }
    }

    fn draw(&self, rect: Rect, time: f64) {
        draw_rectangle(
            rect.x,
            rect.y,
            rect.w,
            rect.h,
            Color::new(0.02, 0.08, 0.15, 0.9),
        );
        draw_rectangle_lines(rect.x, rect.y, rect.w, rect.h, 2.0, ACCENT);
        let dims = text(
            &self.text,
            rect.x + 14.0,
            rect.y + rect.h / 2.0 + 10.0,
            30.0,
            INK,
        );
        if (time * 2.0).fract() < 0.5 {
            let x = rect.x + 16.0 + dims.width;
            draw_line(x, rect.y + 10.0, x, rect.y + rect.h - 10.0, 2.0, INK);
        }
    }
}

// ---------------------------------------------------------------------------
// Audio
// ---------------------------------------------------------------------------

struct Audio {
    sounds: HashMap<Effect, Sound>,
    muted: bool,
}

impl Audio {
    async fn load() -> Self {
        let mut sounds = HashMap::new();
        for effect in sound::ALL_EFFECTS {
            if let Ok(s) = load_sound_from_bytes(&sound::wav(effect)).await {
                sounds.insert(effect, s);
            }
        }
        Audio {
            sounds,
            muted: false,
        }
    }

    fn play(&self, effect: Effect) {
        if self.muted {
            return;
        }
        if let Some(s) = self.sounds.get(&effect) {
            let volume = match effect {
                Effect::Click => 0.35,
                Effect::Launch => 0.45,
                _ => 0.7,
            };
            play_sound(
                s,
                PlaySoundParams {
                    looped: false,
                    volume,
                },
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Screens
// ---------------------------------------------------------------------------

enum Screen {
    Menu,
    HostSetup {
        port: TextInput,
        error: Option<String>,
    },
    Hosting {
        host: Host,
        addresses: Vec<String>,
    },
    JoinSetup {
        address: TextInput,
        error: Option<String>,
    },
    Joining {
        joining: Joining,
        address: String,
    },
    Match(Box<Match>),
}

struct App {
    screen: Screen,
    quit: bool,
    last_join_address: String,
}

impl App {
    fn new() -> Self {
        App {
            screen: Screen::Menu,
            quit: false,
            last_join_address: String::new(),
        }
    }

    fn is_typing(&self) -> bool {
        matches!(
            self.screen,
            Screen::HostSetup { .. } | Screen::JoinSetup { .. }
        )
    }

    /// Round-trip time to a network opponent, while in a match.
    fn latency(&self) -> Option<std::time::Duration> {
        match &self.screen {
            Screen::Match(game) => game.opponent.latency(),
            _ => None,
        }
    }

    fn frame(&mut self, ui: &mut Ui, audio: &Audio) {
        let next = match &mut self.screen {
            Screen::Menu => self.menu(ui),
            Screen::HostSetup { port, error } => host_setup(ui, port, error),
            Screen::Hosting { host, addresses } => hosting(ui, host, addresses),
            Screen::JoinSetup { address, error } => {
                let next = join_setup(ui, address, error);
                if next.is_some() {
                    self.last_join_address = address.text.clone();
                }
                next
            }
            Screen::Joining { joining, address } => joining_screen(ui, joining, address),
            Screen::Match(game) => game.frame(ui, audio),
        };
        if let Some(next) = next {
            self.screen = next;
        }
    }

    fn menu(&mut self, ui: &mut Ui) -> Option<Screen> {
        let t = ui.time as f32;
        let bob = (t * 1.5).sin() * 6.0;
        draw_text_centered(
            "BATTLESHIP",
            VW / 2.0 + 4.0,
            194.0 + bob,
            110.0,
            Color::new(0.0, 0.0, 0.0, 0.4),
        );
        draw_text_centered("BATTLESHIP", VW / 2.0, 190.0 + bob, 110.0, ACCENT);
        draw_text_centered(
            "Sink the enemy fleet before it sinks yours",
            VW / 2.0,
            240.0,
            26.0,
            MUTED,
        );
        draw_menu_ship(VW / 2.0, 300.0, t);

        let x = VW / 2.0 - 170.0;
        let mut next = None;
        if ui.button(Rect::new(x, 360.0, 340.0, 58.0), "Play vs Computer", true) {
            next = Some(Screen::Match(Box::new(Match::new(
                Box::new(ComputerOpponent::new(Rng::from_time())),
                "Computer".into(),
                true,
                true,
            ))));
        }
        if ui.button(Rect::new(x, 432.0, 340.0, 58.0), "Host Online Game", true) {
            next = Some(Screen::HostSetup {
                port: TextInput::new(&DEFAULT_PORT.to_string(), 5),
                error: None,
            });
        }
        if ui.button(Rect::new(x, 504.0, 340.0, 58.0), "Join Online Game", true) {
            next = Some(Screen::JoinSetup {
                address: TextInput::new(&self.last_join_address, 64),
                error: None,
            });
        }
        if ui.button(Rect::new(x, 576.0, 340.0, 58.0), "Quit", true) {
            self.quit = true;
        }
        draw_text_centered(
            "Fleet: Carrier 5 · Battleship 4 · Cruiser 3 · Submarine 3 · Destroyer 2 - ships never touch",
            VW / 2.0,
            690.0,
            20.0,
            MUTED,
        );
        next
    }
}

fn back_button(ui: &mut Ui) -> bool {
    ui.button(Rect::new(30.0, 24.0, 120.0, 44.0), "Back", true) || is_key_pressed(KeyCode::Escape)
}

fn host_setup(ui: &mut Ui, port: &mut TextInput, error: &mut Option<String>) -> Option<Screen> {
    if back_button(ui) {
        return Some(Screen::Menu);
    }
    draw_text_centered("HOST A GAME", VW / 2.0, 150.0, 60.0, ACCENT);
    draw_text_centered(
        "Choose the TCP port to listen on",
        VW / 2.0,
        250.0,
        26.0,
        INK,
    );
    port.update(ui);
    port.text.retain(|c| c.is_ascii_digit());
    let field = Rect::new(VW / 2.0 - 120.0, 280.0, 240.0, 60.0);
    port.draw(field, ui.time);
    if let Some(e) = error {
        draw_text_centered(e, VW / 2.0, 380.0, 24.0, DANGER);
    }
    let start = ui.button(
        Rect::new(VW / 2.0 - 140.0, 420.0, 280.0, 58.0),
        "Start hosting",
        true,
    ) || is_key_pressed(KeyCode::Enter)
        || is_key_pressed(KeyCode::KpEnter);
    if start {
        let Ok(number) = port.text.parse::<u16>() else {
            *error = Some("Enter a port between 1 and 65535".into());
            return None;
        };
        if number == 0 {
            *error = Some("Enter a port between 1 and 65535".into());
            return None;
        }
        match Host::start(number) {
            Ok(host) => {
                let mut addresses: Vec<String> = net::local_ip_addresses()
                    .into_iter()
                    .map(|ip| match ip {
                        std::net::IpAddr::V4(v4) => format!("{v4}:{number}"),
                        std::net::IpAddr::V6(v6) => format!("[{v6}]:{number}"),
                    })
                    .collect();
                if addresses.is_empty() {
                    addresses.push(format!("<this computer's IP>:{number}"));
                }
                return Some(Screen::Hosting { host, addresses });
            }
            Err(e) => *error = Some(format!("Could not listen on port {number}: {e}")),
        }
    }
    None
}

fn hosting(ui: &mut Ui, host: &mut Host, addresses: &[String]) -> Option<Screen> {
    if back_button(ui) {
        return Some(Screen::Menu);
    }
    if let Some(link) = host.poll() {
        let label = link.peer().ip().to_string();
        return Some(Screen::Match(Box::new(Match::new(
            Box::new(link),
            label,
            true,
            false,
        ))));
    }
    draw_text_centered("WAITING FOR AN OPPONENT", VW / 2.0, 150.0, 54.0, ACCENT);
    draw_radar(VW / 2.0, 300.0, 90.0, ui.time as f32);
    draw_text_centered(
        &format!(
            "Listening on TCP port {}. Ask your opponent to join one of:",
            host.port()
        ),
        VW / 2.0,
        440.0,
        26.0,
        INK,
    );
    let mut y = 490.0;
    for address in addresses.iter().take(3) {
        draw_text_centered(address, VW / 2.0 - 70.0, y, 34.0, GOOD);
        if ui.button(
            Rect::new(VW / 2.0 + 150.0, y - 30.0, 120.0, 40.0),
            "Copy",
            true,
        ) {
            macroquad::miniquad::window::clipboard_set(address);
        }
        y += 52.0;
    }
    draw_text_centered(
        "Same network: use the address above. Over the internet: forward this TCP port on your",
        VW / 2.0,
        y + 30.0,
        20.0,
        MUTED,
    );
    draw_text_centered(
        "router to this computer and share your public IP. Allow the game through your firewall.",
        VW / 2.0,
        y + 54.0,
        20.0,
        MUTED,
    );
    None
}

fn join_setup(ui: &mut Ui, address: &mut TextInput, error: &mut Option<String>) -> Option<Screen> {
    if back_button(ui) {
        return Some(Screen::Menu);
    }
    draw_text_centered("JOIN A GAME", VW / 2.0, 150.0, 60.0, ACCENT);
    draw_text_centered(
        &format!("Host address as IP or IP:port (default port {DEFAULT_PORT}). Ctrl+V pastes."),
        VW / 2.0,
        250.0,
        26.0,
        INK,
    );
    address.update(ui);
    address.draw(Rect::new(VW / 2.0 - 260.0, 280.0, 520.0, 60.0), ui.time);
    if let Some(e) = error {
        draw_text_centered(e, VW / 2.0, 380.0, 24.0, DANGER);
    }
    let go = ui.button(
        Rect::new(VW / 2.0 - 140.0, 420.0, 280.0, 58.0),
        "Connect",
        true,
    ) || is_key_pressed(KeyCode::Enter)
        || is_key_pressed(KeyCode::KpEnter);
    if go {
        match net::parse_address(&address.text) {
            Ok(normalised) => {
                return Some(Screen::Joining {
                    joining: net::join(&normalised),
                    address: normalised,
                });
            }
            Err(e) => *error = Some(e),
        }
    }
    None
}

fn joining_screen(ui: &mut Ui, joining: &mut Joining, address: &str) -> Option<Screen> {
    if back_button(ui) {
        return Some(Screen::Menu);
    }
    match joining.poll() {
        Some(Ok(link)) => {
            let label = link.peer().ip().to_string();
            return Some(Screen::Match(Box::new(Match::new(
                Box::new(link),
                label,
                false,
                false,
            ))));
        }
        Some(Err(e)) => {
            return Some(Screen::JoinSetup {
                address: TextInput::new(address, 64),
                error: Some(e),
            });
        }
        None => {}
    }
    draw_text_centered("CONNECTING", VW / 2.0, 150.0, 60.0, ACCENT);
    draw_radar(VW / 2.0, 330.0, 90.0, ui.time as f32);
    draw_text_centered(
        &format!("Calling {address} ..."),
        VW / 2.0,
        480.0,
        30.0,
        INK,
    );
    None
}

// ---------------------------------------------------------------------------
// A match against one opponent (computer or remote), possibly several rounds
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
enum Shot {
    Outgoing,
    Incoming(Coord),
}

struct Missile {
    from: Vec2,
    to: Vec2,
    started: f64,
    shot: Shot,
}

struct Match {
    opponent: Box<dyn Opponent>,
    opponent_name: String,
    vs_computer: bool,
    session: Session,
    editor: FleetEditor,
    inbox: VecDeque<Message>,
    missile: Option<Missile>,
    hold_until: f64,
    opponent_is_ready: bool,
    enemy_fleet: Vec<Placement>,
    disconnected: Option<String>,
    game_over_at: Option<f64>,
    pending_sound: Option<(f64, Effect)>,
    shots: u32,
    hits: u32,
    wins: u32,
    losses: u32,
    fx: Fx,
    rng: Rng,
}

impl Match {
    fn new(
        opponent: Box<dyn Opponent>,
        opponent_name: String,
        i_go_first: bool,
        vs_computer: bool,
    ) -> Self {
        let mut rng = Rng::from_time();
        let mut editor = FleetEditor::new();
        editor.randomize(&mut rng);
        Match {
            opponent,
            opponent_name,
            vs_computer,
            session: Session::new(i_go_first),
            editor,
            inbox: VecDeque::new(),
            missile: None,
            hold_until: 0.0,
            opponent_is_ready: false,
            enemy_fleet: Vec::new(),
            disconnected: None,
            game_over_at: None,
            pending_sound: None,
            shots: 0,
            hits: 0,
            wins: 0,
            losses: 0,
            fx: Fx::default(),
            rng,
        }
    }

    fn frame(&mut self, ui: &mut Ui, audio: &Audio) -> Option<Screen> {
        let now = ui.time;
        self.update(now, audio);

        let shake = self.fx.shake_offset(now, &mut self.rng);
        let mut camera = letterbox_camera();
        camera.target -= shake;
        set_camera(&camera);

        let next = if self.session.phase() == Phase::Placement {
            self.placement_screen(ui)
        } else {
            self.battle_screen(ui, audio)
        };
        self.fx.draw(now);
        set_camera(&letterbox_camera());

        if let Some(next) = next {
            return Some(next);
        }
        if let Some(reason) = self.disconnected.clone() {
            return self.disconnected_overlay(ui, &reason);
        }
        if self.game_over_at.is_some_and(|t| now > t + 1.3) {
            return self.game_over_overlay(ui, audio);
        }
        None
    }

    // --- rules plumbing ---------------------------------------------------

    fn update(&mut self, now: f64, audio: &Audio) {
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
        if let Some((at, effect)) = self.pending_sound
            && now >= at
        {
            audio.play(effect);
            self.pending_sound = None;
        }
        if self
            .missile
            .as_ref()
            .is_some_and(|m| now >= m.started + MISSILE_SECONDS as f64)
        {
            let missile = self.missile.take().expect("checked above");
            self.land(missile.shot, now, audio);
        }
        if self.missile.is_none()
            && now >= self.hold_until
            && let Some(message) = self.inbox.pop_front()
        {
            self.handle(message, now, audio);
        }
    }

    fn handle(&mut self, message: Message, now: f64, audio: &Audio) {
        match message {
            Message::Ready => match self.session.opponent_ready() {
                Ok(()) => {
                    self.opponent_is_ready = true;
                    self.on_maybe_started(now);
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
                let from = RIGHT_BOARD + vec2(BOARD_PX / 2.0, -120.0);
                self.launch(
                    from,
                    cell_center(LEFT_BOARD, target),
                    Shot::Incoming(target),
                    now,
                    audio,
                );
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
                self.impact(RIGHT_BOARD, target, &result, false, now, audio);
                self.check_game_over(now);
            }
            Message::Reveal(fleet) => self.enemy_fleet = fleet,
            Message::Hello { .. } | Message::Ping(_) | Message::Pong(_) | Message::Bye => {}
        }
    }

    fn launch(&mut self, from: Vec2, to: Vec2, shot: Shot, now: f64, audio: &Audio) {
        audio.play(Effect::Launch);
        self.missile = Some(Missile {
            from,
            to,
            started: now,
            shot,
        });
    }

    fn land(&mut self, shot: Shot, now: f64, audio: &Audio) {
        // Our own shell lands silently; the opponent's report triggers the effect.
        if let Shot::Incoming(target) = shot {
            match self.session.receive_fire(target) {
                Ok(result) => {
                    self.opponent.send(Message::Result(result.clone()));
                    self.impact(LEFT_BOARD, target, &result, true, now, audio);
                    self.check_game_over(now);
                }
                Err(e) => self.broken(e.to_string()),
            }
        }
    }

    fn impact(
        &mut self,
        board: Vec2,
        target: Coord,
        result: &ShotResult,
        on_me: bool,
        now: f64,
        audio: &Audio,
    ) {
        let center = cell_center(board, target);
        let toast_at = board + vec2(BOARD_PX / 2.0, BOARD_PX / 2.0);
        match result {
            ShotResult::Miss => {
                audio.play(Effect::Splash);
                self.fx.splash(center, now, &mut self.rng);
                self.fx
                    .toast("Miss", toast_at, Color::new(0.7, 0.85, 1.0, 1.0), now);
            }
            ShotResult::Hit => {
                audio.play(Effect::Explosion);
                self.fx.explode(center, 1.0, now, &mut self.rng);
                self.fx.shake(now, 7.0);
                self.fx.toast("HIT!", toast_at, ACCENT, now);
            }
            ShotResult::Sunk { kind, cells } => {
                audio.play(Effect::Sunk);
                for cell in cells {
                    self.fx
                        .explode(cell_center(board, *cell), 0.8, now, &mut self.rng);
                }
                self.fx.shake(now, 14.0);
                let text = if on_me {
                    format!("Your {kind} was sunk!")
                } else {
                    format!("You sank the {kind}!")
                };
                self.fx.toast(&text, toast_at, DANGER, now);
            }
        }
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
        if self.session.phase() == Phase::Won {
            self.wins += 1;
            self.pending_sound = Some((now + 1.0, Effect::Victory));
        } else {
            self.losses += 1;
            self.pending_sound = Some((now + 1.0, Effect::Defeat));
        }
    }

    fn on_maybe_started(&mut self, now: f64) {
        match self.session.phase() {
            Phase::MyTurn => {
                self.opponent_is_ready = false;
                self.fx
                    .toast("You fire first!", vec2(VW / 2.0, VH / 2.0), GOOD, now);
            }
            Phase::TheirTurn => {
                self.opponent_is_ready = false;
                let text = format!("{} fires first", self.opponent_name);
                self.fx.toast(&text, vec2(VW / 2.0, VH / 2.0), ACCENT, now);
            }
            _ => {}
        }
    }

    fn broken(&mut self, why: String) {
        self.disconnected
            .get_or_insert(format!("Game stopped: {why}"));
    }

    fn fire(&mut self, target: Coord, now: f64, audio: &Audio) {
        if self.missile.is_some() || self.session.fire(target).is_err() {
            return;
        }
        self.shots += 1;
        self.opponent.send(Message::Fire(target));
        let from = LEFT_BOARD + vec2(BOARD_PX / 2.0, BOARD_PX + 40.0);
        self.launch(
            from,
            cell_center(RIGHT_BOARD, target),
            Shot::Outgoing,
            now,
            audio,
        );
    }

    fn new_round(&mut self) {
        if self.session.new_round().is_ok() {
            self.editor = FleetEditor::new();
            self.editor.randomize(&mut self.rng);
            self.enemy_fleet.clear();
            self.game_over_at = None;
            self.pending_sound = None;
            self.shots = 0;
            self.hits = 0;
            self.fx = Fx::default();
        }
    }

    // --- screens ------------------------------------------------------------

    fn leave_button(&mut self, ui: &mut Ui) -> bool {
        ui.button(Rect::new(30.0, 24.0, 120.0, 44.0), "Leave", true)
    }

    fn placement_screen(&mut self, ui: &mut Ui) -> Option<Screen> {
        if self.leave_button(ui) {
            return Some(Screen::Menu);
        }
        draw_text_centered("DEPLOY YOUR FLEET", VW / 2.0, 70.0, 50.0, ACCENT);
        draw_text_centered(
            &format!("Opponent: {}", self.opponent_name),
            VW / 2.0,
            105.0,
            22.0,
            MUTED,
        );
        draw_board_frame(LEFT_BOARD, "YOUR FLEET", ui.time);

        let hovered = board_cell_at(LEFT_BOARD, ui.mouse);
        for placement in self.editor.board().placements() {
            let lifted = hovered
                .is_some_and(|h| placement.cells().unwrap_or_default().contains(&h))
                && self.editor.selected().is_none();
            draw_ship(
                LEFT_BOARD,
                &placement,
                if lifted {
                    HULL
                } else {
                    HULL_DARK.lerp_to(HULL, 0.6)
                },
            );
        }
        if let Some(at) = hovered {
            if let Some((placement, ok)) = self.editor.preview(at) {
                if let Some(cells) = placement.cells() {
                    let tint = if ok { GOOD } else { DANGER };
                    draw_ship(
                        LEFT_BOARD,
                        &placement,
                        Color::new(tint.r, tint.g, tint.b, 0.55),
                    );
                    for c in cells {
                        let p = cell_origin(LEFT_BOARD, c);
                        draw_rectangle_lines(
                            p.x + 1.0,
                            p.y + 1.0,
                            CELL - 2.0,
                            CELL - 2.0,
                            2.0,
                            tint,
                        );
                    }
                } else {
                    let p = cell_origin(LEFT_BOARD, at);
                    draw_rectangle_lines(p.x, p.y, CELL, CELL, 2.0, DANGER);
                }
            }
            if ui.clicked {
                self.editor.click(at);
                ui.clicked_button = true;
            }
        }
        if ui.right_clicked || is_key_pressed(KeyCode::R) {
            self.editor.rotate();
        }

        // Right hand panel: ship picker and actions.
        let px = RIGHT_BOARD.x;
        text("Ships", px, 190.0, 30.0, INK);
        for (i, kind) in FLEET.iter().enumerate() {
            let rect = Rect::new(px, 205.0 + i as f32 * 50.0, 380.0, 42.0);
            let placed = self.editor.board().has_ship(*kind);
            let selected = self.editor.selected() == Some(*kind);
            let fill = if selected {
                Color::new(0.25, 0.45, 0.25, 1.0)
            } else if rect.contains(ui.mouse) && !placed {
                Color::new(0.2, 0.35, 0.55, 1.0)
            } else {
                Color::new(0.1, 0.2, 0.32, 0.9)
            };
            draw_rectangle(rect.x, rect.y, rect.w, rect.h, fill);
            let label_color = if placed { MUTED } else { INK };
            text(kind.name(), rect.x + 14.0, rect.y + 28.0, 26.0, label_color);
            for s in 0..kind.size() {
                let x = rect.x + 180.0 + s as f32 * 24.0;
                draw_rectangle(
                    x,
                    rect.y + 12.0,
                    20.0,
                    18.0,
                    if placed { HULL_DARK } else { HULL },
                );
            }
            if placed {
                draw_text_right("placed", rect.x + rect.w - 12.0, rect.y + 27.0, 18.0, GOOD);
            }
            if ui.clicked && rect.contains(ui.mouse) && !placed {
                self.editor.select(*kind);
                ui.clicked_button = true;
            }
        }
        let orientation = match self.editor.orientation() {
            Orientation::Horizontal => "horizontal",
            Orientation::Vertical => "vertical",
        };
        text(
            &format!("Placing {orientation}  (R / right-click rotates)"),
            px,
            475.0,
            20.0,
            MUTED,
        );
        text(
            "Click a placed ship to pick it up again.",
            px,
            500.0,
            20.0,
            MUTED,
        );

        if ui.button(Rect::new(px, 520.0, 180.0, 50.0), "Random", true)
            || is_key_pressed(KeyCode::Space)
        {
            self.editor.randomize(&mut self.rng);
        }
        if ui.button(Rect::new(px + 200.0, 520.0, 180.0, 50.0), "Clear", true) {
            self.editor.clear();
        }
        let complete = self.editor.is_complete();
        let ready_clicked = ui.button(
            Rect::new(px, 590.0, 380.0, 60.0),
            "Ready for battle!",
            complete,
        ) || (complete
            && (is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::KpEnter)));
        if self.opponent_is_ready {
            draw_text_centered(
                &format!("{} is ready and waiting", self.opponent_name),
                px + 190.0,
                685.0,
                22.0,
                GOOD,
            );
        }
        if ready_clicked
            && self.session.set_board(self.editor.board().clone()).is_ok()
            && self.session.ready().is_ok()
        {
            self.opponent.send(Message::Ready);
            self.on_maybe_started(ui.time);
        }
        None
    }

    fn battle_screen(&mut self, ui: &mut Ui, audio: &Audio) -> Option<Screen> {
        if self.leave_button(ui) {
            return Some(Screen::Menu);
        }
        let now = ui.time;
        let phase = self.session.phase();
        draw_text_centered(
            &format!("You {} - {} {}", self.wins, self.losses, self.opponent_name),
            VW / 2.0,
            52.0,
            26.0,
            MUTED,
        );

        draw_board_frame(LEFT_BOARD, "YOUR FLEET", now);
        draw_board_frame(RIGHT_BOARD, "ENEMY WATERS", now);
        self.draw_my_board(now);
        self.draw_enemy_board(now);

        let my_turn =
            phase == Phase::MyTurn && self.missile.is_none() && self.disconnected.is_none();
        if my_turn {
            let pulse = 0.5 + 0.5 * (now as f32 * 4.0).sin();
            draw_rectangle_lines(
                RIGHT_BOARD.x - 6.0,
                RIGHT_BOARD.y - 6.0,
                BOARD_PX + 12.0,
                BOARD_PX + 12.0,
                3.0,
                Color::new(ACCENT.r, ACCENT.g, ACCENT.b, 0.4 + 0.5 * pulse),
            );
            if let Some(target) = board_cell_at(RIGHT_BOARD, ui.mouse)
                && self.session.enemy_grid().can_target(target)
            {
                draw_crosshair(cell_center(RIGHT_BOARD, target), now);
                if ui.clicked {
                    self.fire(target, now, audio);
                }
            }
        }
        if let Phase::AwaitingResult(target) = phase
            && self.missile.is_none()
        {
            draw_crosshair(cell_center(RIGHT_BOARD, target), now);
        }

        let status = match phase {
            Phase::WaitingForOpponent => format!(
                "Waiting for {} to deploy their fleet...",
                self.opponent_name
            ),
            Phase::MyTurn => "Your turn: pick a target in enemy waters".to_string(),
            Phase::AwaitingResult(target) => format!("Firing at {}...", target.label()),
            Phase::TheirTurn => format!("{} is taking aim...", self.opponent_name),
            Phase::Won => "Victory!".to_string(),
            Phase::Lost => "Defeat".to_string(),
            Phase::Placement => String::new(),
        };
        let status_color = if my_turn { ACCENT } else { INK };
        draw_text_centered(&status, VW / 2.0, 105.0, 30.0, status_color);

        // Fleet status under each board.
        let my_board = self.session.my_board();
        draw_fleet_status(LEFT_BOARD.x, |k| my_board.is_sunk(k));
        let sunk = self.session.enemy_grid().sunk_ships().to_vec();
        draw_fleet_status(RIGHT_BOARD.x, |k| sunk.contains(&k));

        if phase == Phase::WaitingForOpponent {
            draw_radar(
                RIGHT_BOARD.x + BOARD_PX / 2.0,
                RIGHT_BOARD.y + BOARD_PX / 2.0,
                120.0,
                now as f32,
            );
        }

        if let Some(missile) = &self.missile {
            let p = ((now - missile.started) as f32 / MISSILE_SECONDS).clamp(0.0, 1.0);
            draw_missile(missile.from, missile.to, p);
        }
        None
    }

    fn draw_my_board(&self, now: f64) {
        let board = self.session.my_board();
        for placement in board.placements() {
            let color = if board.is_sunk(placement.kind) {
                Color::new(0.35, 0.2, 0.18, 1.0)
            } else {
                HULL
            };
            draw_ship(LEFT_BOARD, &placement, color);
        }
        for c in Coord::all() {
            if !board.was_shot(c) {
                continue;
            }
            let center = cell_center(LEFT_BOARD, c);
            match board.ship_at(c) {
                Some(p) if board.is_sunk(p.kind) => draw_wreck_mark(center),
                Some(_) => draw_flames(center, now, c),
                None => draw_peg(center, Color::new(0.85, 0.92, 1.0, 0.85)),
            }
        }
    }

    fn draw_enemy_board(&self, now: f64) {
        let grid = self.session.enemy_grid();
        // Ships revealed after the game, drawn as ghosts.
        for placement in &self.enemy_fleet {
            let afloat = placement
                .cells()
                .unwrap_or_default()
                .iter()
                .any(|c| grid.get(*c) != Knowledge::Sunk);
            if afloat {
                draw_ship(RIGHT_BOARD, placement, Color::new(0.6, 0.65, 0.7, 0.5));
            }
        }
        for c in Coord::all() {
            let center = cell_center(RIGHT_BOARD, c);
            match grid.get(c) {
                Knowledge::Unknown => {}
                Knowledge::Miss => draw_peg(center, Color::new(0.85, 0.92, 1.0, 0.85)),
                Knowledge::Water => {
                    draw_circle(center.x, center.y, 2.5, Color::new(0.6, 0.75, 0.9, 0.35))
                }
                Knowledge::Hit => draw_flames(center, now, c),
                Knowledge::Sunk => {
                    let o = cell_origin(RIGHT_BOARD, c);
                    draw_rectangle(
                        o.x + 3.0,
                        o.y + 3.0,
                        CELL - 6.0,
                        CELL - 6.0,
                        Color::new(0.35, 0.2, 0.18, 1.0),
                    );
                    draw_wreck_mark(center);
                }
            }
        }
    }

    fn disconnected_overlay(&mut self, ui: &mut Ui, reason: &str) -> Option<Screen> {
        draw_rectangle(0.0, 0.0, VW, VH, Color::new(0.0, 0.0, 0.0, 0.65));
        draw_text_centered("CONNECTION CLOSED", VW / 2.0, 300.0, 56.0, DANGER);
        draw_text_centered(reason, VW / 2.0, 360.0, 26.0, INK);
        if ui.button(
            Rect::new(VW / 2.0 - 140.0, 420.0, 280.0, 58.0),
            "Main menu",
            true,
        ) {
            return Some(Screen::Menu);
        }
        None
    }

    fn game_over_overlay(&mut self, ui: &mut Ui, _audio: &Audio) -> Option<Screen> {
        let won = self.session.phase() == Phase::Won;
        let t = ui.time as f32;
        draw_rectangle(0.0, 230.0, VW, 300.0, Color::new(0.0, 0.02, 0.06, 0.82));
        let pulse = 1.0 + 0.04 * (t * 3.0).sin();
        let (title, color) = if won {
            ("VICTORY", ACCENT)
        } else {
            ("DEFEAT", DANGER)
        };
        draw_text_centered(title, VW / 2.0, 330.0, 100.0 * pulse, color);
        let accuracy = (100 * self.hits).checked_div(self.shots).unwrap_or(0);
        draw_text_centered(
            &format!(
                "{} shots fired, {}% hit rate   ·   Score: You {} - {} {}",
                self.shots, accuracy, self.wins, self.losses, self.opponent_name
            ),
            VW / 2.0,
            385.0,
            26.0,
            INK,
        );
        if won {
            for _ in 0..2 {
                let x = self.rng.next_f32() * VW;
                self.fx.confetti(vec2(x, 230.0), ui.time, &mut self.rng);
            }
        }
        if ui.button(
            Rect::new(VW / 2.0 - 300.0, 430.0, 280.0, 58.0),
            "Play again",
            true,
        ) {
            self.new_round();
        }
        if ui.button(
            Rect::new(VW / 2.0 + 20.0, 430.0, 280.0, 58.0),
            "Main menu",
            true,
        ) {
            return Some(Screen::Menu);
        }
        None
    }
}

// ---------------------------------------------------------------------------
// Effects
// ---------------------------------------------------------------------------

struct Particle {
    pos: Vec2,
    vel: Vec2,
    born: f64,
    life: f32,
    size: f32,
    color: Color,
    gravity: f32,
}

struct Ripple {
    center: Vec2,
    born: f64,
}

struct Toast {
    text: String,
    at: Vec2,
    color: Color,
    born: f64,
}

#[derive(Default)]
struct Fx {
    particles: Vec<Particle>,
    ripples: Vec<Ripple>,
    toasts: Vec<Toast>,
    shake_started: f64,
    shake_strength: f32,
    last_update: f64,
}

impl Fx {
    fn splash(&mut self, center: Vec2, now: f64, rng: &mut Rng) {
        self.ripples.push(Ripple { center, born: now });
        for _ in 0..22 {
            self.particles.push(Particle {
                pos: center,
                vel: vec2(
                    rng.next_f32() * 160.0 - 80.0,
                    -120.0 - rng.next_f32() * 200.0,
                ),
                born: now,
                life: 0.5 + rng.next_f32() * 0.4,
                size: 1.5 + rng.next_f32() * 2.5,
                color: Color::new(0.75, 0.9, 1.0, 1.0),
                gravity: 700.0,
            });
        }
    }

    fn explode(&mut self, center: Vec2, power: f32, now: f64, rng: &mut Rng) {
        for _ in 0..(45.0 * power) as usize {
            let angle = rng.next_f32() * std::f32::consts::TAU;
            let speed = 60.0 + rng.next_f32() * 220.0 * power;
            let palette = [
                Color::new(1.0, 0.85, 0.3, 1.0),
                Color::new(1.0, 0.55, 0.1, 1.0),
                Color::new(0.95, 0.25, 0.1, 1.0),
            ];
            self.particles.push(Particle {
                pos: center,
                vel: vec2(angle.cos(), angle.sin()) * speed,
                born: now,
                life: 0.4 + rng.next_f32() * 0.6,
                size: 2.0 + rng.next_f32() * 3.5,
                color: palette[rng.below(3)],
                gravity: 180.0,
            });
        }
        for _ in 0..10 {
            self.particles.push(Particle {
                pos: center + vec2(rng.next_f32() * 16.0 - 8.0, 0.0),
                vel: vec2(rng.next_f32() * 40.0 - 20.0, -30.0 - rng.next_f32() * 50.0),
                born: now,
                life: 1.0 + rng.next_f32() * 0.8,
                size: 6.0 + rng.next_f32() * 6.0,
                color: Color::new(0.3, 0.3, 0.32, 0.6),
                gravity: -20.0,
            });
        }
    }

    fn confetti(&mut self, at: Vec2, now: f64, rng: &mut Rng) {
        let palette = [ACCENT, GOOD, DANGER, Color::new(0.4, 0.7, 1.0, 1.0)];
        self.particles.push(Particle {
            pos: at,
            vel: vec2(rng.next_f32() * 80.0 - 40.0, 40.0 + rng.next_f32() * 80.0),
            born: now,
            life: 2.5,
            size: 3.0 + rng.next_f32() * 2.0,
            color: palette[rng.below(palette.len())],
            gravity: 60.0,
        });
    }

    fn toast(&mut self, text: &str, at: Vec2, color: Color, now: f64) {
        self.toasts.push(Toast {
            text: text.to_string(),
            at,
            color,
            born: now,
        });
    }

    fn shake(&mut self, now: f64, strength: f32) {
        self.shake_started = now;
        self.shake_strength = strength;
    }

    fn shake_offset(&self, now: f64, rng: &mut Rng) -> Vec2 {
        let age = (now - self.shake_started) as f32;
        let strength = self.shake_strength * (1.0 - age / 0.4).max(0.0);
        if strength <= 0.0 {
            return Vec2::ZERO;
        }
        vec2(rng.next_f32() - 0.5, rng.next_f32() - 0.5) * strength * 2.0
    }

    fn draw(&mut self, now: f64) {
        let dt = if self.last_update == 0.0 {
            0.0
        } else {
            (now - self.last_update) as f32
        };
        let dt = dt.min(0.05);
        self.last_update = now;

        self.particles.retain(|p| ((now - p.born) as f32) < p.life);
        for p in &mut self.particles {
            p.vel.y += p.gravity * dt;
            p.pos += p.vel * dt;
            let fade = 1.0 - (now - p.born) as f32 / p.life;
            let c = Color::new(p.color.r, p.color.g, p.color.b, p.color.a * fade);
            draw_circle(p.pos.x, p.pos.y, p.size * (0.5 + 0.5 * fade), c);
        }

        self.ripples.retain(|r| now - r.born < 1.0);
        for r in &self.ripples {
            let age = (now - r.born) as f32;
            for k in 0..3 {
                let a = age - k as f32 * 0.15;
                if a > 0.0 {
                    let alpha = (1.0 - a).max(0.0) * 0.8;
                    draw_circle_lines(
                        r.center.x,
                        r.center.y,
                        4.0 + a * 40.0,
                        2.0,
                        Color::new(0.8, 0.92, 1.0, alpha),
                    );
                }
            }
        }

        self.toasts.retain(|t| now - t.born < 1.6);
        for t in &self.toasts {
            let age = (now - t.born) as f32;
            let pop = 1.0 + 0.4 * (1.0 - (age / 0.15).min(1.0));
            let alpha = (1.0 - ((age - 1.1) / 0.5).max(0.0)).clamp(0.0, 1.0);
            let y = t.at.y - age * 25.0;
            draw_text_centered(
                &t.text,
                t.at.x + 2.0,
                y + 2.0,
                44.0 * pop,
                Color::new(0.0, 0.0, 0.0, 0.6 * alpha),
            );
            draw_text_centered(
                &t.text,
                t.at.x,
                y,
                44.0 * pop,
                Color::new(t.color.r, t.color.g, t.color.b, alpha),
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Drawing helpers
// ---------------------------------------------------------------------------

trait Lerp {
    fn lerp_to(self, other: Color, t: f32) -> Color;
}

impl Lerp for Color {
    fn lerp_to(self, o: Color, t: f32) -> Color {
        Color::new(
            self.r + (o.r - self.r) * t,
            self.g + (o.g - self.g) * t,
            self.b + (o.b - self.b) * t,
            self.a + (o.a - self.a) * t,
        )
    }
}

fn draw_text_centered(s: &str, x: f32, y: f32, size: f32, color: Color) {
    let dims = measure(s, size);
    text(s, x - dims.width / 2.0, y, size, color);
}

fn draw_text_right(s: &str, x: f32, y: f32, size: f32, color: Color) {
    let dims = measure(s, size);
    text(s, x - dims.width, y, size, color);
}

fn draw_background(time: f64) {
    let bands = 24;
    for i in 0..bands {
        let t = i as f32 / bands as f32;
        let c = BG_TOP.lerp_to(BG_BOTTOM, t);
        let h = VH * 3.0 / bands as f32;
        draw_rectangle(-VW, -VH + t * VH * 3.0, VW * 3.0, h + 1.0, c);
    }
    let t = time as f32;
    for row in 0..9 {
        let y = 80.0 + row as f32 * 85.0;
        let mut prev = vec2(-40.0, y);
        for step in 1..=31 {
            let x = -40.0 + step as f32 * 42.0;
            let p = vec2(x, y + (x * 0.012 + t * 0.8 + row as f32).sin() * 6.0);
            draw_line(
                prev.x,
                prev.y,
                p.x,
                p.y,
                1.5,
                Color::new(0.4, 0.6, 0.9, 0.05),
            );
            prev = p;
        }
    }
}

fn cell_origin(board: Vec2, c: Coord) -> Vec2 {
    board + vec2(c.x as f32 * CELL, c.y as f32 * CELL)
}

fn cell_center(board: Vec2, c: Coord) -> Vec2 {
    cell_origin(board, c) + vec2(CELL / 2.0, CELL / 2.0)
}

fn board_cell_at(board: Vec2, point: Vec2) -> Option<Coord> {
    let local = (point - board) / CELL;
    if local.x < 0.0 || local.y < 0.0 {
        return None;
    }
    Coord::try_new(local.x as i32, local.y as i32)
}

fn draw_board_frame(board: Vec2, title: &str, time: f64) {
    let t = time as f32;
    draw_rectangle(
        board.x - 4.0,
        board.y - 4.0,
        BOARD_PX + 8.0,
        BOARD_PX + 8.0,
        Color::new(0.0, 0.0, 0.0, 0.35),
    );
    for c in Coord::all() {
        let (x, y) = (c.x as f32, c.y as f32);
        let wave = 0.035 * (t * 1.6 + x * 0.7 + y * 0.45).sin()
            + 0.025 * (t * 2.3 - x * 0.4 + y * 0.9).sin();
        let color = Color::new(0.06 + wave * 0.5, 0.25 + wave, 0.45 + wave * 1.5, 1.0);
        let o = cell_origin(board, c);
        draw_rectangle(o.x + 1.0, o.y + 1.0, CELL - 2.0, CELL - 2.0, color);
        // A glint travelling across the water.
        let glint = ((t * 0.9 - x * 0.35 - y * 0.2).sin() * 0.5 + 0.5).powi(12);
        if glint > 0.05 {
            draw_line(
                o.x + 8.0,
                o.y + 14.0,
                o.x + 20.0,
                o.y + 14.0,
                1.5,
                Color::new(1.0, 1.0, 1.0, glint * 0.25),
            );
        }
    }
    for i in 0..BOARD_SIZE {
        let letter = ((b'A' + i) as char).to_string();
        draw_text_centered(
            &letter,
            board.x + i as f32 * CELL + CELL / 2.0,
            board.y - 10.0,
            22.0,
            MUTED,
        );
        draw_text_right(
            &(i + 1).to_string(),
            board.x - 10.0,
            board.y + i as f32 * CELL + CELL / 2.0 + 7.0,
            22.0,
            MUTED,
        );
    }
    draw_text_centered(title, board.x + BOARD_PX / 2.0, board.y - 36.0, 28.0, INK);
}

fn draw_ship(board: Vec2, placement: &Placement, color: Color) {
    let Some(cells) = placement.cells() else {
        return;
    };
    let first = cell_origin(board, cells[0]);
    let last = cell_origin(board, *cells.last().expect("ships have cells"));
    let inset = 6.0;
    let (x, y) = (first.x + inset, first.y + inset);
    let (w, h) = (last.x + CELL - inset - x, last.y + CELL - inset - y);
    let radius = w.min(h) / 2.0;
    let dark = Color::new(color.r * 0.6, color.g * 0.6, color.b * 0.6, color.a);
    match placement.orientation {
        Orientation::Horizontal => {
            draw_rectangle(x + radius, y, w - 2.0 * radius, h, color);
            draw_circle(x + radius, y + radius, radius, color);
            // Pointed bow on the right.
            draw_triangle(
                vec2(x + w - radius, y),
                vec2(x + w - radius, y + h),
                vec2(x + w + 4.0, y + h / 2.0),
                color,
            );
            draw_line(
                x + radius,
                y + h / 2.0,
                x + w - radius,
                y + h / 2.0,
                2.0,
                dark,
            );
        }
        Orientation::Vertical => {
            draw_rectangle(x, y + radius, w, h - 2.0 * radius, color);
            draw_circle(x + radius, y + radius, radius, color);
            draw_triangle(
                vec2(x, y + h - radius),
                vec2(x + w, y + h - radius),
                vec2(x + w / 2.0, y + h + 4.0),
                color,
            );
            draw_line(
                x + w / 2.0,
                y + radius,
                x + w / 2.0,
                y + h - radius,
                2.0,
                dark,
            );
        }
    }
    for c in &cells[..cells.len().saturating_sub(1)] {
        let center = cell_center(board, *c);
        draw_circle(center.x, center.y, 5.0, dark);
    }
}

fn draw_peg(center: Vec2, color: Color) {
    draw_circle(center.x, center.y, 6.0, Color::new(0.0, 0.0, 0.0, 0.3));
    draw_circle(center.x, center.y - 1.0, 5.0, color);
}

fn draw_flames(center: Vec2, time: f64, seed: Coord) {
    let t = time as f32 * 9.0 + seed.x as f32 * 1.7 + seed.y as f32 * 2.9;
    draw_circle(center.x, center.y, 11.0, Color::new(0.85, 0.15, 0.1, 0.9));
    for k in 0..3 {
        let phase = t + k as f32 * 2.1;
        let h = 10.0 + 5.0 * phase.sin().abs();
        let dx = (k as f32 - 1.0) * 6.0 + phase.cos() * 1.5;
        let base = center + vec2(dx, 6.0);
        draw_triangle(
            base + vec2(-4.0, 0.0),
            base + vec2(4.0, 0.0),
            base + vec2(phase.cos() * 2.0, -h - 4.0),
            Color::new(1.0, 0.55 + 0.2 * phase.sin(), 0.1, 0.95),
        );
    }
    draw_circle(
        center.x,
        center.y + 2.0,
        4.0,
        Color::new(1.0, 0.95, 0.5, 0.9),
    );
}

fn draw_wreck_mark(center: Vec2) {
    let s = 10.0;
    let c = Color::new(0.95, 0.3, 0.25, 0.95);
    draw_line(
        center.x - s,
        center.y - s,
        center.x + s,
        center.y + s,
        3.5,
        c,
    );
    draw_line(
        center.x + s,
        center.y - s,
        center.x - s,
        center.y + s,
        3.5,
        c,
    );
}

fn draw_crosshair(center: Vec2, time: f64) {
    let pulse = 1.0 + 0.12 * (time as f32 * 8.0).sin();
    let r = CELL * 0.42 * pulse;
    draw_circle_lines(center.x, center.y, r, 2.0, ACCENT);
    draw_line(
        center.x - r - 5.0,
        center.y,
        center.x - r + 6.0,
        center.y,
        2.0,
        ACCENT,
    );
    draw_line(
        center.x + r - 6.0,
        center.y,
        center.x + r + 5.0,
        center.y,
        2.0,
        ACCENT,
    );
    draw_line(
        center.x,
        center.y - r - 5.0,
        center.x,
        center.y - r + 6.0,
        2.0,
        ACCENT,
    );
    draw_line(
        center.x,
        center.y + r - 6.0,
        center.x,
        center.y + r + 5.0,
        2.0,
        ACCENT,
    );
}

fn missile_position(from: Vec2, to: Vec2, p: f32) -> Vec2 {
    let arc = from.distance(to) * 0.35;
    from.lerp(to, p) - vec2(0.0, arc * 4.0 * p * (1.0 - p))
}

fn draw_missile(from: Vec2, to: Vec2, p: f32) {
    for k in 1..10 {
        let q = p - k as f32 * 0.025;
        if q > 0.0 {
            let pos = missile_position(from, to, q);
            let fade = 1.0 - k as f32 / 10.0;
            draw_circle(
                pos.x,
                pos.y,
                5.0 * fade,
                Color::new(0.85, 0.85, 0.9, 0.35 * fade),
            );
        }
    }
    let pos = missile_position(from, to, p);
    draw_circle(pos.x, pos.y, 9.0, Color::new(1.0, 0.6, 0.2, 0.35));
    draw_circle(pos.x, pos.y, 5.5, Color::new(0.15, 0.15, 0.18, 1.0));
}

fn draw_fleet_status(x: f32, is_sunk: impl Fn(ShipKind) -> bool) {
    let y = LEFT_BOARD.y + BOARD_PX + 30.0;
    for (i, kind) in FLEET.iter().enumerate() {
        let col = (i % 2) as f32;
        let row = (i / 2) as f32;
        let (px, py) = (x + col * 200.0, y + row * 30.0);
        let sunk = is_sunk(*kind);
        let color = if sunk { DANGER } else { INK };
        for s in 0..kind.size() {
            draw_rectangle(
                px + s as f32 * 9.0,
                py - 12.0,
                7.0,
                10.0,
                if sunk { DANGER } else { HULL },
            );
        }
        let dims = text(kind.name(), px + 54.0, py, 20.0, color);
        if sunk {
            draw_line(
                px + 52.0,
                py - 6.0,
                px + 56.0 + dims.width,
                py - 6.0,
                2.0,
                DANGER,
            );
        }
    }
}

fn draw_radar(x: f32, y: f32, r: f32, t: f32) {
    draw_circle(x, y, r, Color::new(0.02, 0.2, 0.12, 0.6));
    for k in 1..=3 {
        draw_circle_lines(
            x,
            y,
            r * k as f32 / 3.0,
            1.5,
            Color::new(0.3, 0.9, 0.5, 0.4),
        );
    }
    let angle = t * 2.5;
    for k in 0..20 {
        let a = angle - k as f32 * 0.04;
        let alpha = 0.8 * (1.0 - k as f32 / 20.0);
        draw_line(
            x,
            y,
            x + a.cos() * r,
            y + a.sin() * r,
            2.0,
            Color::new(0.3, 1.0, 0.5, alpha * 0.5),
        );
    }
}

fn draw_menu_ship(cx: f32, y: f32, t: f32) {
    let roll = (t * 1.3).sin() * 3.0;
    let x = cx - 110.0 + (t * 0.5).sin() * 20.0;
    draw_rectangle(x, y + roll, 220.0, 22.0, HULL_DARK);
    draw_triangle(
        vec2(x + 220.0, y + roll),
        vec2(x + 220.0, y + 22.0 + roll),
        vec2(x + 250.0, y + roll),
        HULL_DARK,
    );
    draw_rectangle(x + 60.0, y - 18.0 + roll, 80.0, 18.0, HULL);
    draw_rectangle(x + 85.0, y - 34.0 + roll, 26.0, 16.0, HULL);
    draw_line(
        x + 140.0,
        y - 10.0 + roll,
        x + 180.0,
        y - 16.0 + roll,
        4.0,
        HULL,
    );
    draw_line(
        x + 60.0,
        y - 10.0 + roll,
        x + 25.0,
        y - 16.0 + roll,
        4.0,
        HULL,
    );
    for k in 0..6 {
        let wx = cx - 200.0 + k as f32 * 80.0;
        let wy = y + 26.0 + (t * 2.0 + k as f32).sin() * 3.0;
        draw_line(wx, wy, wx + 50.0, wy, 2.0, Color::new(0.6, 0.8, 1.0, 0.5));
    }
}
