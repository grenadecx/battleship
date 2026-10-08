//! Battleship game logic. Everything here is UI-agnostic and test-driven;
//! `main.rs` is a thin graphical shell around it.

pub mod ai;
pub mod domain;
pub mod grid;
pub mod rng;
