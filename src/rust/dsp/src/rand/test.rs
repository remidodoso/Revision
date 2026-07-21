use super::*;

#[test]
fn the_generator_is_uniform_and_stays_in_range() {
    let mut random = Random::new(12_345);
    let mut bucket = [0usize; 10];
    for _ in 0..100_000 {
        let value = random.next_f64();
        assert!((0.0..1.0).contains(&value), "out of range: {value}");
        bucket[(value * 10.0) as usize] += 1;
    }
    // A tenth of 100 000 is 10 000; anything within a few percent is fine. This
    // is not a randomness test, it is a "the shift is not off by one" test.
    for count in bucket {
        assert!((9_000..11_000).contains(&count), "lumpy: {bucket:?}");
    }
}

#[test]
fn the_same_seed_gives_the_same_sequence() {
    let take = |seed| {
        let mut random = Random::new(seed);
        (0..8).map(|_| random.next_u64()).collect::<Vec<_>>()
    };
    assert_eq!(take(7), take(7));
    assert_ne!(take(7), take(8));
}

#[test]
fn the_hash_is_written_out_because_the_standard_one_is_not_stable() {
    // The point of writing FNV-1a by hand is that this number is fixed forever:
    // `DefaultHasher` may change between releases, and a bake seed that moved
    // with the toolchain would silently re-colour every table in every project.
    let mut hasher = Hasher::default();
    hasher.write(b"revision");
    assert_eq!(hasher.finish(), 15_466_728_656_978_478_680);
}

#[test]
fn different_input_hashes_differently() {
    let hash = |bytes: &[u8]| {
        let mut hasher = Hasher::default();
        hasher.write(bytes);
        hasher.finish()
    };
    assert_ne!(hash(b"saw"), hash(b"pulse"));
    assert_ne!(hash(b"ab"), hash(b"ba"));
}
