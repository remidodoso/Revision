//! Determinism's two primitives: a hash to derive a seed, and a generator.
//!
//! **Not shared with `rev-engine`.** Its voice seeds its own read-head offsets
//! with six lines of LCG, and deduplicating them would mean the real-time crate
//! importing a non-audio dependency. Its independence is worth more than the
//! six lines (dsp-02 §6).
//!
//! **Not bit-compatible with the JavaScript**, either. Matching would mean
//! reproducing mulberry32, djb2 and one particular radix-2 FFT; the comparison
//! basis is magnitude spectra, which are phase-independent by design.

/// SplitMix64 — the generator, chosen because it is short enough to read.
pub struct Random(u64);

impl Random {
    pub fn new(seed: u64) -> Random {
        Random(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `[0, 1)`. The top 53 bits, so every value is representable.
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// FNV-1a, written out rather than taken from `std`.
///
/// `DefaultHasher` is explicitly not stable across releases, and a bake seed
/// that changed with the toolchain would silently re-colour every table.
pub struct Hasher(u64);

impl Default for Hasher {
    fn default() -> Hasher {
        Hasher(0xCBF2_9CE4_8422_2325)
    }
}

impl Hasher {
    pub fn write(&mut self, byte: &[u8]) -> &mut Hasher {
        for b in byte {
            self.0 = (self.0 ^ u64::from(*b)).wrapping_mul(0x0000_0100_0000_01B3);
        }
        self
    }

    /// Floats go in by bit pattern, and are rounded first by the caller where
    /// two nearly-equal patches should share a table.
    pub fn write_f64(&mut self, value: f64) -> &mut Hasher {
        self.write(&value.to_bits().to_le_bytes())
    }

    pub fn write_u64(&mut self, value: u64) -> &mut Hasher {
        self.write(&value.to_le_bytes())
    }

    pub fn finish(&self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod test;
