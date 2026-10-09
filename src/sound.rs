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
            let wave = phase.sin() * 0.8 + (phase * 2.0).sin() * 0.2;
            wave * envelope(t)
        })
        .collect()
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
                move |t| (t * 200.0).min(1.0) * (1.0 - t / seconds).max(0.0).powf(0.6) * 0.6,
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
mod tests {
    use super::*;

    const HEADER_LEN: usize = 44;
    const SHORTEST_SECONDS: f32 = 0.02;
    const LONGEST_SECONDS: f32 = 3.0;
    const QUIETEST_PEAK: f32 = 0.1;
    const SILENT_ENOUGH: f32 = 0.05;
    const FADE_OUT_SAMPLES: usize = 10;

    fn u32_at(bytes: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
    }

    fn u16_at(bytes: &[u8], at: usize) -> u16 {
        u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap())
    }

    fn seconds(samples: &[f32]) -> f32 {
        samples.len() as f32 / SAMPLE_RATE as f32
    }

    fn peak(samples: &[f32]) -> f32 {
        samples.iter().fold(0.0, |loudest, v| loudest.max(v.abs()))
    }

    fn decoder(effect: Effect) -> audrey::Reader<std::io::Cursor<Vec<u8>>> {
        audrey::Reader::new(std::io::Cursor::new(wav(effect)))
            .unwrap_or_else(|e| panic!("{effect:?}: {e:?}"))
    }

    #[test]
    fn wav_header_describes_16_bit_mono_pcm() {
        let bytes = encode_wav(&[0.0, 0.5, -0.5]);
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(u32_at(&bytes, 4) as usize, bytes.len() - 8, "RIFF size");
        assert_eq!(&bytes[8..12], b"WAVE");
        assert_eq!(&bytes[12..16], b"fmt ");
        assert_eq!(u32_at(&bytes, 16), 16, "fmt size");
        assert_eq!(u16_at(&bytes, 20), 1, "PCM");
        assert_eq!(u16_at(&bytes, 22), 1, "mono");
        assert_eq!(u32_at(&bytes, 24), SAMPLE_RATE);
        assert_eq!(u32_at(&bytes, 28), SAMPLE_RATE * 2, "byte rate");
        assert_eq!(u16_at(&bytes, 32), 2, "block align");
        assert_eq!(u16_at(&bytes, 34), 16, "bits per sample");
        assert_eq!(&bytes[36..40], b"data");
    }

    #[test]
    fn wav_data_holds_two_bytes_per_sample() {
        let bytes = encode_wav(&[0.0, 0.5, -0.5]);
        assert_eq!(u32_at(&bytes, 40), 6);
        assert_eq!(bytes.len(), HEADER_LEN + 6);
    }

    #[test]
    fn samples_are_scaled_and_clamped_to_16_bits() {
        let bytes = encode_wav(&[1.0, -1.0, 2.0, -2.0, 0.0]);
        let pcm: Vec<i16> = bytes[HEADER_LEN..]
            .chunks(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]))
            .collect();
        assert_eq!(pcm, vec![32767, -32767, 32767, -32767, 0]);
    }

    #[test]
    fn every_effect_is_short() {
        for effect in ALL_EFFECTS {
            let length = seconds(&samples(effect));
            assert!(
                (SHORTEST_SECONDS..=LONGEST_SECONDS).contains(&length),
                "{effect:?} lasts {length}s"
            );
        }
    }

    #[test]
    fn every_effect_is_audible() {
        for effect in ALL_EFFECTS {
            let loudest = peak(&samples(effect));
            assert!(
                loudest > QUIETEST_PEAK,
                "{effect:?} is too quiet ({loudest})"
            );
        }
    }

    #[test]
    fn no_effect_clips() {
        for effect in ALL_EFFECTS {
            let loudest = peak(&samples(effect));
            assert!(loudest <= 1.0, "{effect:?} clips ({loudest})");
        }
    }

    #[test]
    fn every_sample_is_finite() {
        for effect in ALL_EFFECTS {
            assert!(samples(effect).iter().all(|v| v.is_finite()), "{effect:?}");
        }
    }

    #[test]
    fn effects_fade_out_instead_of_clicking() {
        for effect in ALL_EFFECTS {
            let s = samples(effect);
            let tail = &s[s.len() - FADE_OUT_SAMPLES..];
            assert!(
                tail.iter().all(|v| v.abs() < SILENT_ENOUGH),
                "{effect:?} ends abruptly"
            );
        }
    }

    #[test]
    fn effects_are_deterministic() {
        for effect in ALL_EFFECTS {
            assert_eq!(wav(effect), wav(effect), "{effect:?}");
        }
    }

    #[test]
    fn splash_and_explosion_sound_different() {
        assert_ne!(samples(Effect::Splash), samples(Effect::Explosion));
    }

    #[test]
    fn victory_and_defeat_sound_different() {
        assert_ne!(samples(Effect::Victory), samples(Effect::Defeat));
    }

    #[test]
    fn audio_backend_decodes_every_effect_as_mono() {
        for effect in ALL_EFFECTS {
            assert_eq!(
                decoder(effect).description().channel_count(),
                1,
                "{effect:?}"
            );
        }
    }

    #[test]
    fn audio_backend_decodes_every_effect_at_our_sample_rate() {
        for effect in ALL_EFFECTS {
            assert_eq!(
                decoder(effect).description().sample_rate(),
                SAMPLE_RATE,
                "{effect:?}"
            );
        }
    }

    #[test]
    fn audio_backend_decodes_every_sample() {
        for effect in ALL_EFFECTS {
            let decoded = decoder(effect)
                .samples::<f32>()
                .filter_map(Result::ok)
                .count();
            assert_eq!(decoded, samples(effect).len(), "{effect:?}");
        }
    }
}
