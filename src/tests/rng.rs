use super::*;

#[test]
fn same_seed_gives_same_sequence() {
    let mut a = Rng::new(42);
    let mut b = Rng::new(42);
    for _ in 0..100 {
        assert_eq!(a.next_u64(), b.next_u64());
    }
}

#[test]
fn different_seeds_give_different_sequences() {
    let mut a = Rng::new(1);
    let mut b = Rng::new(2);
    assert_ne!(a.next_u64(), b.next_u64());
}

#[test]
fn below_stays_in_range_and_covers_it() {
    let mut rng = Rng::new(7);
    let mut seen = [false; 6];
    for _ in 0..1000 {
        let v = rng.below(6);
        assert!(v < 6);
        seen[v] = true;
    }
    assert!(seen.iter().all(|s| *s));
}

#[test]
fn next_f32_is_in_unit_interval() {
    let mut rng = Rng::new(9);
    for _ in 0..1000 {
        let v = rng.next_f32();
        assert!((0.0..1.0).contains(&v));
    }
}

#[test]
fn pick_from_empty_slice_is_none() {
    let mut rng = Rng::new(3);
    let empty: [u8; 0] = [];
    assert_eq!(rng.pick(&empty), None);
    assert_eq!(rng.pick(&[5]), Some(&5));
}

#[test]
fn clock_seeded_generators_differ() {
    let mut a = Rng::from_time();
    std::thread::sleep(std::time::Duration::from_millis(2));
    let mut b = Rng::from_time();
    assert_ne!(
        (0..4).map(|_| a.next_u64()).collect::<Vec<_>>(),
        (0..4).map(|_| b.next_u64()).collect::<Vec<_>>()
    );
}
