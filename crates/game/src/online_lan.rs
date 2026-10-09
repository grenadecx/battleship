//! Online play on the desktop: one game hosts on a TCP port and the other joins
//! it by IP address, with no server in between.

use super::*;
use battleship::net::{self, DEFAULT_PORT, Host, Joining};

pub enum Online {
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
}

impl Online {
    pub fn host() -> Self {
        Online::HostSetup {
            port: TextInput::new(&DEFAULT_PORT.to_string(), 5),
            error: None,
        }
    }

    pub fn join(last_address: &str) -> Self {
        Online::JoinSetup {
            address: TextInput::new(last_address, 64),
            error: None,
        }
    }

    /// Whether a text field has the keyboard.
    pub fn is_typing(&self) -> bool {
        matches!(self, Online::HostSetup { .. } | Online::JoinSetup { .. })
    }

    /// Runs one frame. `last_join` remembers the address the player connected to.
    pub fn frame(&mut self, ui: &mut Ui, last_join: &mut String) -> Option<Screen> {
        match self {
            Online::HostSetup { port, error } => host_setup(ui, port, error),
            Online::Hosting { host, addresses } => hosting(ui, host, addresses),
            Online::JoinSetup { address, error } => {
                let next = join_setup(ui, address, error);
                if next.is_some() {
                    *last_join = address.text.clone();
                }
                next
            }
            Online::Joining { joining, address } => joining_screen(ui, joining, address),
        }
    }
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
                return Some(Screen::Online(Online::Hosting { host, addresses }));
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
                return Some(Screen::Online(Online::Joining {
                    joining: net::join(&normalised),
                    address: normalised,
                }));
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
            return Some(Screen::Online(Online::JoinSetup {
                address: TextInput::new(address, 64),
                error: Some(e),
            }));
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
