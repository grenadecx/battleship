//! Battleship game logic. Everything here is UI-agnostic and test-driven;
//! `main.rs` is a thin graphical shell around it. Unit tests for each module
//! live in `src/tests/`.

pub mod ai;
pub mod domain;
pub mod game;
pub mod grid;
pub mod layout;
pub mod net;
pub mod opponent;
pub mod protocol;
pub mod rng;
pub mod session;
pub mod setup;
pub mod sound;
#[cfg(test)]
#[path = "tests/support.rs"]
mod test_support;
