//! Online play in the browser: both players open a WebSocket to the relay on the
//! server that served the page, and meet in a room with a short code.

use super::*;
use battleship::relay::{CODE_LENGTH, normalize_code};
use battleship::relay_link::{Progress, RelayLink};
use battleship::web_socket::JsSocket;

type Link = RelayLink<JsSocket>;

pub enum Online {
    /// `link` is handed to the match once the opponent arrives.
    Hosting {
        link: Option<Link>,
        error: Option<String>,
    },
    JoinSetup {
        code: TextInput,
        error: Option<String>,
    },
    Joining {
        link: Option<Link>,
        code: String,
    },
}

impl Online {
    pub fn host() -> Self {
        Online::Hosting {
            link: Some(RelayLink::host(JsSocket::open())),
            error: None,
        }
    }

    pub fn join(last_code: &str) -> Self {
        Online::JoinSetup {
            code: TextInput::new(last_code, CODE_LENGTH),
            error: None,
        }
    }

    /// Whether a text field has the keyboard.
    pub fn is_typing(&self) -> bool {
        matches!(self, Online::JoinSetup { .. })
    }

    /// Runs one frame. `last_join` remembers the code the player joined with.
    pub fn frame(&mut self, ui: &mut Ui, last_join: &mut String) -> Option<Screen> {
        match self {
            Online::Hosting { link, error } => hosting(ui, link, error),
            Online::JoinSetup { code, error } => {
                let next = join_setup(ui, code, error);
                if next.is_some() {
                    *last_join = code.text.clone();
                }
                next
            }
            Online::Joining { link, code } => joining(ui, link, code),
        }
    }
}

fn start_match(link: &mut Option<Link>) -> Option<Screen> {
    let link = link.take()?;
    let i_go_first = link.i_go_first();
    Some(Screen::Match(Box::new(Match::new(
        Box::new(link),
        "Opponent".into(),
        i_go_first,
        false,
    ))))
}

fn hosting(ui: &mut Ui, link: &mut Option<Link>, error: &mut Option<String>) -> Option<Screen> {
    if back_button(ui) {
        return Some(Screen::Menu);
    }
    draw_text_centered("HOST A GAME", VW / 2.0, 150.0, 60.0, ACCENT);
    if let Some(e) = error {
        draw_text_centered(e, VW / 2.0, 300.0, 26.0, DANGER);
        if ui.button(
            Rect::new(VW / 2.0 - 140.0, 360.0, 280.0, 58.0),
            "Try again",
            true,
        ) {
            return Some(Screen::Online(Online::host()));
        }
        return None;
    }
    match link.as_mut()?.progress() {
        Progress::Ready => return start_match(link),
        Progress::Failed(e) => {
            *error = Some(e);
            *link = None;
            return None;
        }
        Progress::Waiting => {}
    }
    draw_radar(VW / 2.0, 300.0, 90.0, ui.time as f32);
    match link.as_ref().and_then(|l| l.room()) {
        Some(code) => {
            draw_text_centered("Your room code", VW / 2.0, 440.0, 26.0, INK);
            draw_text_centered(code, VW / 2.0 - 70.0, 505.0, 60.0, GOOD);
            if ui.button(Rect::new(VW / 2.0 + 90.0, 470.0, 120.0, 44.0), "Copy", true) {
                macroquad::miniquad::window::clipboard_set(code);
            }
            draw_text_centered(
                "Ask your opponent to open this page, choose Join Online Game and enter the code.",
                VW / 2.0,
                570.0,
                22.0,
                MUTED,
            );
        }
        None => draw_text_centered("Contacting the game server ...", VW / 2.0, 470.0, 28.0, INK),
    }
    None
}

fn join_setup(ui: &mut Ui, code: &mut TextInput, error: &mut Option<String>) -> Option<Screen> {
    if back_button(ui) {
        return Some(Screen::Menu);
    }
    draw_text_centered("JOIN A GAME", VW / 2.0, 150.0, 60.0, ACCENT);
    draw_text_centered(
        "Enter the room code your opponent sees. Ctrl+V pastes.",
        VW / 2.0,
        250.0,
        26.0,
        INK,
    );
    code.update(ui);
    code.text = normalize_code(&code.text);
    code.draw(Rect::new(VW / 2.0 - 120.0, 280.0, 240.0, 60.0), ui.time);
    if let Some(e) = error {
        draw_text_centered(e, VW / 2.0, 380.0, 24.0, DANGER);
    }
    let go = ui.button(
        Rect::new(VW / 2.0 - 140.0, 420.0, 280.0, 58.0),
        "Join",
        true,
    ) || is_key_pressed(KeyCode::Enter)
        || is_key_pressed(KeyCode::KpEnter);
    if go {
        if code.text.len() != CODE_LENGTH {
            *error = Some(format!("Room codes are {CODE_LENGTH} letters and digits"));
            return None;
        }
        return Some(Screen::Online(Online::Joining {
            link: Some(RelayLink::join(JsSocket::open(), &code.text)),
            code: code.text.clone(),
        }));
    }
    None
}

fn joining(ui: &mut Ui, link: &mut Option<Link>, code: &str) -> Option<Screen> {
    if back_button(ui) {
        return Some(Screen::Menu);
    }
    match link.as_mut()?.progress() {
        Progress::Ready => return start_match(link),
        Progress::Failed(e) => {
            return Some(Screen::Online(Online::JoinSetup {
                code: TextInput::new(code, CODE_LENGTH),
                error: Some(e),
            }));
        }
        Progress::Waiting => {}
    }
    draw_text_centered("CONNECTING", VW / 2.0, 150.0, 60.0, ACCENT);
    draw_radar(VW / 2.0, 330.0, 90.0, ui.time as f32);
    draw_text_centered(
        &format!("Joining room {code} ..."),
        VW / 2.0,
        480.0,
        30.0,
        INK,
    );
    None
}
