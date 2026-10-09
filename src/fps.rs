//! Frame rate readout for the UI.

/// Frame rate averaged over half a second, so the readout is steady enough to read.
#[derive(Debug, Default)]
pub struct FpsMeter {
    frames: u32,
    since: f64,
    shown: Option<u32>,
}

impl FpsMeter {
    pub const WINDOW_SECONDS: f64 = 0.5;

    /// Counts a frame drawn at `now` (in seconds) and returns the rate to show,
    /// once a full window has passed.
    pub fn tick(&mut self, now: f64) -> Option<u32> {
        self.frames += 1;
        let elapsed = now - self.since;
        if elapsed >= Self::WINDOW_SECONDS {
            self.shown = Some((self.frames as f64 / elapsed).round() as u32);
            self.frames = 0;
            self.since = now;
        }
        self.shown
    }
}

#[cfg(test)]
#[path = "tests/fps.rs"]
mod tests;
