//! The [`GameRng`] resource.

use bevy::ecs::resource::Resource;
use rand::RngCore;

/// Shared random-number generator inserted as a Bevy [`Resource`].
///
/// Wraps a `Box<dyn RngCore + Send + Sync>` so the concrete implementation
/// can be chosen at app construction time and swapped freely between
/// binaries (server vs. client, production vs. test) without changing any
/// consumer code.
///
/// # Threading
///
/// Bevy schedules every system that takes `ResMut<GameRng>` serially with
/// respect to every other such system. This is intentional: it preserves
/// the option to make rolls deterministic by seeding the RNG, since
/// parallel access would otherwise produce nondeterministic call order.
///
/// # Usage
///
/// ```
/// use dd40_rng::GameRng;
/// use rand::{Rng, SeedableRng, rngs::StdRng};
///
/// let mut rng = GameRng::new(StdRng::seed_from_u64(7));
/// let n: u32 = rng.as_mut().random_range(0..100);
/// assert!(n < 100);
/// ```
#[derive(Resource)]
pub struct GameRng(Box<dyn RngCore + Send + Sync>);

impl RngCore for GameRng {
    fn next_u32(&mut self) -> u32 {
        self.0.next_u32()
    }

    fn next_u64(&mut self) -> u64 {
        self.0.next_u64()
    }

    fn fill_bytes(&mut self, dst: &mut [u8]) {
        self.0.fill_bytes(dst)
    }
}

impl GameRng {
    /// Wraps any [`RngCore`] implementation in a [`GameRng`].
    pub fn new<R: RngCore + Send + Sync + 'static>(rng: R) -> Self {
        Self(Box::new(rng))
    }

    /// Constructs a [`GameRng`] from an already-boxed RNG.
    ///
    /// Useful when the concrete type is itself behind a trait object, for
    /// example when a factory closure returns
    /// `Box<dyn RngCore + Send + Sync>`.
    pub fn from_boxed(rng: Box<dyn RngCore + Send + Sync>) -> Self {
        Self(rng)
    }

    /// Borrows the inner RNG as a mutable trait object.
    ///
    /// Consumers should bring [`rand::Rng`] into scope and call its
    /// methods (`random`, `random_range`, `gen_bool`, …) through the
    /// returned reference.
    pub fn as_mut(&mut self) -> &mut (dyn RngCore + Send + Sync) {
        &mut *self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::{Rng, SeedableRng, rngs::StdRng};

    #[test]
    fn seeded_rng_is_deterministic() {
        let mut a = GameRng::new(StdRng::seed_from_u64(42));
        let mut b = GameRng::new(StdRng::seed_from_u64(42));
        let xs: Vec<u32> = (0..16).map(|_| a.as_mut().random()).collect();
        let ys: Vec<u32> = (0..16).map(|_| b.as_mut().random()).collect();
        assert_eq!(xs, ys);
    }

    #[test]
    fn distinct_seeds_diverge() {
        let mut a = GameRng::new(StdRng::seed_from_u64(1));
        let mut b = GameRng::new(StdRng::seed_from_u64(2));
        let xs: Vec<u32> = (0..16).map(|_| a.as_mut().random()).collect();
        let ys: Vec<u32> = (0..16).map(|_| b.as_mut().random()).collect();
        assert_ne!(xs, ys);
    }

    #[test]
    fn from_boxed_round_trips() {
        let boxed: Box<dyn RngCore + Send + Sync> = Box::new(StdRng::seed_from_u64(3));
        let mut rng = GameRng::from_boxed(boxed);
        let _: u32 = rng.as_mut().random();
    }
}
