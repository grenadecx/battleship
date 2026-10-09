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

#[test]
fn finishing_silence_keeps_it_silent() {
    let mut samples = vec![0.0; 1000];
    finish(&mut samples);
    assert_eq!(peak(&samples), 0.0);
}
