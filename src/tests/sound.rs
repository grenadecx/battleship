use super::*;

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

fn u16_at(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap())
}

#[test]
fn wav_header_describes_16_bit_mono_pcm() {
    let bytes = encode_wav(&[0.0, 0.5, -0.5]);
    assert_eq!(&bytes[0..4], b"RIFF");
    assert_eq!(u32_at(&bytes, 4) as usize, bytes.len() - 8);
    assert_eq!(&bytes[8..12], b"WAVE");
    assert_eq!(&bytes[12..16], b"fmt ");
    assert_eq!(u32_at(&bytes, 16), 16);
    assert_eq!(u16_at(&bytes, 20), 1, "PCM");
    assert_eq!(u16_at(&bytes, 22), 1, "mono");
    assert_eq!(u32_at(&bytes, 24), SAMPLE_RATE);
    assert_eq!(u32_at(&bytes, 28), SAMPLE_RATE * 2, "byte rate");
    assert_eq!(u16_at(&bytes, 32), 2, "block align");
    assert_eq!(u16_at(&bytes, 34), 16, "bits per sample");
    assert_eq!(&bytes[36..40], b"data");
    assert_eq!(u32_at(&bytes, 40), 6);
    assert_eq!(bytes.len(), 44 + 6);
}

#[test]
fn samples_are_scaled_and_clamped_to_16_bits() {
    let bytes = encode_wav(&[1.0, -1.0, 2.0, -2.0, 0.0]);
    let pcm: Vec<i16> = bytes[44..]
        .chunks(2)
        .map(|b| i16::from_le_bytes([b[0], b[1]]))
        .collect();
    assert_eq!(pcm, vec![32767, -32767, 32767, -32767, 0]);
}

#[test]
fn every_effect_is_audible_short_and_within_range() {
    for effect in ALL_EFFECTS {
        let s = samples(effect);
        let seconds = s.len() as f32 / SAMPLE_RATE as f32;
        assert!(
            (0.02..=3.0).contains(&seconds),
            "{effect:?} lasts {seconds}s"
        );
        let peak = s.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(peak > 0.1, "{effect:?} is too quiet ({peak})");
        assert!(peak <= 1.0, "{effect:?} clips ({peak})");
        assert!(s.iter().all(|v| v.is_finite()));
    }
}

#[test]
fn effects_fade_out_instead_of_clicking() {
    for effect in ALL_EFFECTS {
        let s = samples(effect);
        let tail = &s[s.len() - 10..];
        assert!(
            tail.iter().all(|v| v.abs() < 0.05),
            "{effect:?} ends abruptly"
        );
    }
}

#[test]
fn effects_are_deterministic() {
    for effect in ALL_EFFECTS {
        assert_eq!(wav(effect), wav(effect));
    }
}

#[test]
fn effects_sound_different() {
    assert_ne!(samples(Effect::Splash), samples(Effect::Explosion));
    assert_ne!(samples(Effect::Victory), samples(Effect::Defeat));
}

#[test]
fn effects_decode_with_the_audio_backend_decoder() {
    for effect in ALL_EFFECTS {
        let mut reader = audrey::Reader::new(std::io::Cursor::new(wav(effect)))
            .unwrap_or_else(|e| panic!("{effect:?}: {e:?}"));
        let description = reader.description();
        assert_eq!(description.channel_count(), 1);
        assert_eq!(description.sample_rate(), SAMPLE_RATE);
        let decoded = reader.samples::<f32>().filter_map(Result::ok).count();
        assert_eq!(decoded, samples(effect).len(), "{effect:?}");
    }
}

#[test]
fn finishing_silence_keeps_it_silent() {
    let mut samples = vec![0.0; 1000];
    finish(&mut samples);
    assert!(samples.iter().all(|v| *v == 0.0));
}
