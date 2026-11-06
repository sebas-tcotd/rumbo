//! PCG XSH-RR 64/32 with reference two-step seeding and wrapping arithmetic.
//! Subsystem streams use initstate=seed and initseq=the fixed 63-bit ID below.
//! Increment=(ID<<1)|1; IDs are protocol values, independent of declaration order.
//! Cosmetic randomness must use a separate generator outside these streams.

/// Stable simulation stream identities; never renumber or reuse these values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub enum Subsystem {
    /// Bot decisions.
    Bots = 1,
    /// Starting grid decisions.
    Start = 2,
    /// Race rule decisions.
    Rules = 3,
}

/// Reproducible PCG32 state, independent of clocks and external RNGs.
#[derive(Clone, Debug)]
pub struct Pcg32 {
    state: u64,
    increment: u64,
}
impl Pcg32 {
    /// Reference PCG seeding. Sequence uses its low 63 bits, as in pcg32_srandom_r.
    pub fn seeded(seed: u64, sequence: u64) -> Self {
        let mut rng = Self {
            state: 0,
            increment: sequence.wrapping_shl(1) | 1,
        };
        rng.next_u32();
        rng.state = rng.state.wrapping_add(seed);
        rng.next_u32();
        rng
    }
    /// Derive a stream directly from (seed, stable subsystem ID).
    pub fn for_subsystem(seed: u64, subsystem: Subsystem) -> Self {
        Self::seeded(seed, subsystem as u64)
    }
    /// Advance once and return the reference XSH-RR output permutation.
    pub fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.state = old
            .wrapping_mul(6364136223846793005)
            .wrapping_add(self.increment);
        let shifted = (((old >> 18) ^ old) >> 27) as u32;
        shifted.rotate_right((old >> 59) as u32)
    }
}
