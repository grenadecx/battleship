//! Sound effects synthesised at start-up, so the game ships as a single binary
//! without any asset files. Each effect is rendered to an in-memory WAV file.

use crate::rng::Rng;

pub const SAMPLE_RATE: u32 = 44_100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Effect {
    Click,
    Launch,
    Splash,
    Explosion,
    Sunk,
    Victory,
    Defeat,
}

pub const ALL_EFFECTS: [Effect; 7] = [
    Effect::Click,
    Effect::Launch,
    Effect::Splash,
    Effect::Explosion,
    Effect::Sunk,
    Effect::Victory,
    Effect::Defeat,
];

/// Mono samples in `-1.0..=1.0`.
pub fn samples(effect: Effect) -> Vec<f32> {
    let mut rng = Rng::new(effect as u64 + 1);
    let mut s = match effect {
        Effect::Click => tone(0.04, |_| 880.0, |t| (-t * 90.0).exp()),
        Effect::Launch => {
            let whistle = tone(0.5, |t| 1400.0 - 1500.0 * t, |t| (t * 8.0).min(1.0) * 0.5);
            let hiss = noise(&mut rng, 0.5, 0.15, |t| (1.0 - t * 2.0).max(0.0) * 0.4);
            mix(&whistle, &hiss)
        }
        Effect::Splash => {
            let wash = noise(&mut rng, 0.7, 0.35, |t| (-t * 6.0).exp());
            let plop = tone(0.15, |t| 300.0 - 1200.0 * t, |t| (-t * 25.0).exp() * 0.6);
            mix(&wash, &plop)
        }
        Effect::Explosion => explosion(&mut rng, 1.0),
        Effect::Sunk => {
            let boom = explosion(&mut rng, 1.4);
            let groan = tone(
                1.4,
                |t| 140.0 - 70.0 * t,
                |t| (t * 4.0).min(1.0) * (1.4 - t) * 0.35,
            );
            mix(&boom, &groan)
        }
        Effect::Victory => melody(&[(523.3, 0.13), (659.3, 0.13), (784.0, 0.13), (1046.5, 0.6)]),
        Effect::Defeat => melody(&[(392.0, 0.3), (370.0, 0.3), (349.2, 0.3), (329.6, 0.9)]),
    };
    finish(&mut s);
    s
}

/// 16-bit mono PCM WAV file.
pub fn wav(effect: Effect) -> Vec<u8> {
    encode_wav(&samples(effect))
}

pub fn encode_wav(samples: &[f32]) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut bytes = Vec::with_capacity(44 + data_len as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes()); // PCM
    bytes.extend_from_slice(&1u16.to_le_bytes()); // mono
    bytes.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    bytes.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    for sample in samples {
        let value = (sample.clamp(-1.0, 1.0) * 32767.0).round() as i16;
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes
}

fn sample_count(seconds: f32) -> usize {
    (seconds * SAMPLE_RATE as f32) as usize
}

/// A sine-ish tone with a little overtone, frequency and envelope over time in seconds.
fn tone(seconds: f32, frequency: impl Fn(f32) -> f32, envelope: impl Fn(f32) -> f32) -> Vec<f32> {
    let mut phase = 0.0f32;
    (0..sample_count(seconds))
        .map(|i| {
            let t = i as f32 / SAMPLE_RATE as f32;
            phase += std::f32::consts::TAU * frequency(t).max(20.0) / SAMPLE_RATE as f32;
            waveform(phase) * envelope(t)
        })
        .collect()
}

/// The fundamental with a quieter overtone an octave up.
fn waveform(phase: f32) -> f32 {
    phase.sin() * 0.8 + (phase * 2.0).sin() * 0.2
}

/// Low-passed white noise; `brightness` in `0..1` is the filter coefficient.
fn noise(rng: &mut Rng, seconds: f32, brightness: f32, envelope: impl Fn(f32) -> f32) -> Vec<f32> {
    let mut filtered = 0.0f32;
    (0..sample_count(seconds))
        .map(|i| {
            let t = i as f32 / SAMPLE_RATE as f32;
            let white = rng.next_f32() * 2.0 - 1.0;
            filtered += brightness * (white - filtered);
            filtered * envelope(t) * 3.0
        })
        .collect()
}

fn explosion(rng: &mut Rng, seconds: f32) -> Vec<f32> {
    let rumble = noise(rng, seconds, 0.04, |t| (-t * 3.5).exp() * 1.6);
    let crack = noise(rng, 0.12, 0.6, |t| (-t * 40.0).exp() * 0.5);
    let thump = tone(seconds, |t| 70.0 - 30.0 * t, |t| (-t * 5.0).exp() * 0.7);
    mix(&mix(&rumble, &crack), &thump)
}

fn melody(notes: &[(f32, f32)]) -> Vec<f32> {
    notes
        .iter()
        .flat_map(|&(frequency, seconds)| {
            tone(
                seconds,
                move |_| frequency,
                move |t| (t * 200.0).min(1.0) * (1.0 - t / seconds).max(0.0).powf(0.6),
            )
        })
        .collect()
}

fn mix(a: &[f32], b: &[f32]) -> Vec<f32> {
    (0..a.len().max(b.len()))
        .map(|i| a.get(i).unwrap_or(&0.0) + b.get(i).unwrap_or(&0.0))
        .collect()
}

/// Normalises the peak and fades the last few milliseconds to avoid clicks.
fn finish(samples: &mut [f32]) {
    let peak = samples.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    if peak > 0.0 {
        let gain = 0.85 / peak;
        samples.iter_mut().for_each(|v| *v *= gain);
    }
    let fade = sample_count(0.015).min(samples.len());
    let len = samples.len();
    for (i, v) in samples[len - fade..].iter_mut().enumerate() {
        *v *= 1.0 - (i + 1) as f32 / fade as f32;
    }
}

#[cfg(test)]
#[path = "tests/sound.rs"]
mod tests;
