# dd40_rng

Pluggable random-number generation resource for dd40 crates.

See the crate-level Rust docs (`cargo doc -p dd40_rng --open`) for the
authoritative API. In short:

- `RngPlugin::default()` inserts a `GameRng` backed by `StdRng` seeded
  from OS entropy.
- `RngPlugin::with_factory(|| Box::new(MyRng::seed_from_u64(42)))`
  swaps in any custom `RngCore + Send + Sync`.
- Consumers borrow `ResMut<GameRng>` and call `rng.as_mut()` to get a
  `&mut dyn RngCore` they can use through the `rand::Rng` trait.
