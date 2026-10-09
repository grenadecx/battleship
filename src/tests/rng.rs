use super::*;

const DRAWS: usize = 1000;

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
    assert_ne!(Rng::new(1).next_u64(), Rng::new(2).next_u64());
}

#[test]
fn below_stays_in_range() {
    let mut rng = Rng::new(7);
    assert!((0..DRAWS).all(|_| rng.below(6) < 6));
}

#[test]
fn below_covers_the_whole_range() {
    let mut rng = Rng::new(7);
    let mut seen = [false; 6];
    for _ in 0..DRAWS {
        seen[rng.below(6)] = true;
    }
    assert_eq!(seen, [true; 6]);
}

#[test]
fn next_f32_is_in_unit_interval() {
    let mut rng = Rng::new(9);
    assert!((0..DRAWS).all(|_| (0.0..1.0).contains(&rng.next_f32())));
}

#[test]
fn pick_from_empty_slice_is_none() {
    let empty: [u8; 0] = [];
    assert_eq!(Rng::new(3).pick(&empty), None);
}

#[test]
fn pick_from_a_single_item_returns_it() {
    assert_eq!(Rng::new(3).pick(&[5]), Some(&5));
}

#[test]
fn clock_seeded_generators_differ() {
    let mut a = Rng::from_time();
    std::thread::sleep(std::time::Duration::from_millis(2));
    let mut b = Rng::from_time();
    let draws = |rng: &mut Rng| (0..4).map(|_| rng.next_u64()).collect::<Vec<_>>();
    assert_ne!(draws(&mut a), draws(&mut b));
}
