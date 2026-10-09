//! Battleship game logic: rules, the computer opponent and the wire protocols.
//! It has no UI, no I/O and no platform code, so the desktop game, the browser
//! build and the relay server all share it. Unit tests for each module live in
//! `src/tests/`.

pub mod ai;
pub mod domain;
pub mod game;
pub mod grid;
pub mod opponent;
pub mod protocol;
pub mod relay;
pub mod relay_link;
pub mod rng;
pub mod session;
pub mod setup;
pub mod sound;
#[cfg(test)]
#[path = "tests/support.rs"]
mod test_support;
