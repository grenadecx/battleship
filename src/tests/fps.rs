use super::*;

/// Ticks `frames` evenly spaced frames from `start` up to `end`, returning the last readout.
fn run(meter: &mut FpsMeter, start: f64, end: f64, frames: u32) -> Option<u32> {
    let step = (end - start) / frames as f64;
    (1..=frames)
        .map(|i| meter.tick(start + step * i as f64))
        .last()
        .flatten()
}

#[test]
fn shows_nothing_before_the_first_window_is_over() {
    let mut meter = FpsMeter::default();
    assert_eq!(meter.tick(0.1), None);
    assert_eq!(meter.tick(0.4), None);
}

#[test]
fn shows_the_average_rate_over_a_window() {
    let mut meter = FpsMeter::default();
    assert_eq!(run(&mut meter, 0.0, FpsMeter::WINDOW_SECONDS, 30), Some(60));
}

#[test]
fn keeps_showing_the_last_rate_until_the_next_window_is_over() {
    let mut meter = FpsMeter::default();
    run(&mut meter, 0.0, 0.5, 30);
    assert_eq!(meter.tick(0.6), Some(60));
    assert_eq!(meter.tick(0.9), Some(60));
}

#[test]
fn each_window_is_measured_on_its_own() {
    let mut meter = FpsMeter::default();
    run(&mut meter, 0.0, 0.5, 30);
    assert_eq!(run(&mut meter, 0.5, 1.0, 15), Some(30));
}

#[test]
fn a_slow_frame_closes_the_window_late_and_lowers_the_rate() {
    let mut meter = FpsMeter::default();
    run(&mut meter, 0.0, 0.25, 15);
    assert_eq!(meter.tick(1.0), Some(16));
}
