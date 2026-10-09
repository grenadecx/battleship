//! The graphical game's own modules, plus the shared game logic from
//! `battleship-core` under the same paths. `main.rs` is a thin graphical shell
//! around them. Unit tests for each module live in `src/tests/`.

pub use battleship_core::{
    ai, domain, game, grid, opponent, protocol, relay, relay_link, rng, session, setup, sound,
};

pub mod fps;
pub mod layout;
/// Direct TCP play between two desktop games.
#[cfg(not(target_arch = "wasm32"))]
pub mod net;
/// The browser's WebSocket, for online play through the relay server.
#[cfg(target_arch = "wasm32")]
pub mod web_socket;
