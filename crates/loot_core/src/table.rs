//! [`LootTable`] and its [`LootEntry`] / [`LootMode`] vocabulary.
//!
//! A loot table is a sequence of [`LootEntry`] values plus a
//! [`LootMode`] that decides how they combine when rolled.  The result
//! of a roll is a `Vec<ItemStack>` — the same shape consumed by
//! [`DropItems`][dd40_inventory_core::DropItems] (in
//! `dd40_inventory_core`).
//!
//! # Determinism
//!
//! `roll` takes `&mut dyn RngCore`.  Pair it with
//! [`GameRng`][dd40_rng::GameRng] for server-side, seedable
//! determinism.  All numeric ranges are inclusive on both ends so a
//! `Range { min: 1, max: 1 }` is identical to a `Fixed { count: 1 }`.

use std::any::{Any, TypeId};

use rand::Rng;
use rand::RngCore;
use serde::{Deserialize, Serialize};

use dd40_core::block::BlockData;
use dd40_item_core::active_item::ItemStack;
use dd40_item_core::registry::ItemId;

/// One entry in a [`LootTable`].
///
/// Entries are rolled independently.  An entry that contributes zero
/// items (a [`Range`][LootEntry::Range] rolling `0`, or a
/// [`Chance`][LootEntry::Chance] that misses) simply contributes
/// nothing to the resulting `Vec<ItemStack>`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum LootEntry {
    /// Always produce exactly `count` copies of `item`.
    ///
    /// `count = 0` is a no-op and contributes nothing to the roll.
    Fixed {
        /// The item to produce.
        item: ItemId,
        /// How many copies to add.
        count: u16,
    },
    /// Produce a uniform-random number of items in `[min, max]`
    /// inclusive.  When the roll is `0`, the entry contributes
    /// nothing.  `max < min` is treated as `min`.
    Range {
        /// The item to produce.
        item: ItemId,
        /// Inclusive lower bound on the roll.
        min: u16,
        /// Inclusive upper bound on the roll.
        max: u16,
    },
    /// With probability `probability` (clamped to `[0.0, 1.0]`),
    /// produce `count` copies of `item`.  Otherwise produce nothing.
    Chance {
        /// The item to produce.
        item: ItemId,
        /// How many copies to add on success.
        count: u16,
        /// Success probability, clamped into `[0.0, 1.0]`.
        probability: f32,
    },
}

impl LootEntry {
    fn roll(&self, rng: &mut dyn RngCore) -> Option<ItemStack> {
        match *self {
            LootEntry::Fixed { item, count } => non_zero(item, count),
            LootEntry::Range { item, min, max } => {
                let lo = min;
                let hi = max.max(min);
                let n = rng.random_range(lo..=hi);
                non_zero(item, n)
            }
            LootEntry::Chance {
                item,
                count,
                probability,
            } => {
                let p = probability.clamp(0.0, 1.0) as f64;
                if rng.random_bool(p) {
                    non_zero(item, count)
                } else {
                    None
                }
            }
        }
    }
}

fn non_zero(item: ItemId, count: u16) -> Option<ItemStack> {
    std::num::NonZero::new(count).map(|n| ItemStack::new(item, n))
}

/// How a [`LootTable`] combines its entries.
///
/// Only [`LootMode::All`] exists for now — every entry is rolled
/// independently and the results are concatenated.  Future modes
/// (weighted single-pick, "one of N", …) can be added without
/// breaking the existing wire format thanks to the `non_exhaustive`
/// attribute.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum LootMode {
    /// Roll every entry independently and concatenate the results.
    #[default]
    All,
}

/// A runtime-rollable loot table.
///
/// Construct via [`LootTable::new`] or [`LootTable::with_entries`],
/// then roll via [`LootTable::roll`] passing any
/// [`rand::RngCore`][rand::RngCore].  In server-side game code that
/// RNG is normally [`GameRng`][dd40_rng::GameRng]; in tests use a
/// seeded `StdRng` for reproducibility.
///
/// `LootTable` implements [`BlockData`] so it can be stored as default
/// block data on a
/// [`BlockDefinition`][dd40_core::block::BlockDefinition] and looked
/// up by the loot system on block-break.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LootTable {
    /// The entries that will be rolled, in order.
    pub entries: Vec<LootEntry>,
    /// How entries combine.  Currently only [`LootMode::All`].
    pub mode: LootMode,
}

impl LootTable {
    /// Constructs an empty loot table.
    pub fn new() -> Self {
        Self::default()
    }

    /// Constructs a loot table from a list of entries, defaulting to
    /// [`LootMode::All`].
    pub fn with_entries(entries: Vec<LootEntry>) -> Self {
        Self {
            entries,
            mode: LootMode::All,
        }
    }

    /// Sets the mode, returning `self` for chaining.
    pub fn with_mode(mut self, mode: LootMode) -> Self {
        self.mode = mode;
        self
    }

    /// Rolls the table against `rng`, producing a (possibly empty)
    /// vector of stacks.
    ///
    /// The order of the produced stacks matches the order of the
    /// entries; entries that contribute nothing are skipped.
    pub fn roll(&self, rng: &mut dyn RngCore) -> Vec<ItemStack> {
        match self.mode {
            LootMode::All => self.entries.iter().filter_map(|e| e.roll(rng)).collect(),
        }
    }
}

impl BlockData for LootTable {
    fn type_key(&self) -> &'static str {
        std::any::type_name::<Self>()
    }
    fn clone_box(&self) -> Box<dyn BlockData> {
        Box::new(self.clone())
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Convenience: type-id of [`LootTable`] for callers that want to look
/// the type up in the block-data registry without naming the type.
pub fn loot_table_type_id() -> TypeId {
    TypeId::of::<LootTable>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    #[test]
    fn empty_table_yields_empty() {
        let table = LootTable::new();
        let mut rng = StdRng::seed_from_u64(0);
        assert!(table.roll(&mut rng).is_empty());
    }

    #[test]
    fn fixed_entry_is_deterministic() {
        let table = LootTable::with_entries(vec![LootEntry::Fixed {
            item: ItemId(42),
            count: 3,
        }]);
        let mut rng = StdRng::seed_from_u64(7);
        let drops = table.roll(&mut rng);
        assert_eq!(drops.len(), 1);
        assert_eq!(drops[0].item, ItemId(42));
        assert_eq!(drops[0].count.get(), 3);
    }

    #[test]
    fn fixed_entry_with_zero_count_contributes_nothing() {
        let table = LootTable::with_entries(vec![LootEntry::Fixed {
            item: ItemId(42),
            count: 0,
        }]);
        let mut rng = StdRng::seed_from_u64(7);
        assert!(table.roll(&mut rng).is_empty());
    }

    #[test]
    fn range_respects_inclusive_bounds_with_fixed_seed() {
        let table = LootTable::with_entries(vec![LootEntry::Range {
            item: ItemId(1),
            min: 2,
            max: 5,
        }]);
        for seed in 0..32u64 {
            let mut rng = StdRng::seed_from_u64(seed);
            let drops = table.roll(&mut rng);
            assert_eq!(drops.len(), 1);
            let n = drops[0].count.get();
            assert!((2..=5).contains(&n), "out of bounds: {n}");
        }
    }

    #[test]
    fn range_with_zero_min_can_drop_nothing() {
        let table = LootTable::with_entries(vec![LootEntry::Range {
            item: ItemId(1),
            min: 0,
            max: 3,
        }]);
        let mut saw_empty = false;
        let mut saw_nonempty = false;
        for seed in 0..256u64 {
            let mut rng = StdRng::seed_from_u64(seed);
            let drops = table.roll(&mut rng);
            if drops.is_empty() {
                saw_empty = true;
            } else {
                saw_nonempty = true;
            }
        }
        assert!(
            saw_empty,
            "expected at least one 0-count roll across 256 seeds"
        );
        assert!(
            saw_nonempty,
            "expected at least one nonzero roll across 256 seeds"
        );
    }

    #[test]
    fn chance_zero_probability_never_drops() {
        let table = LootTable::with_entries(vec![LootEntry::Chance {
            item: ItemId(1),
            count: 1,
            probability: 0.0,
        }]);
        for seed in 0..32u64 {
            let mut rng = StdRng::seed_from_u64(seed);
            assert!(table.roll(&mut rng).is_empty());
        }
    }

    #[test]
    fn chance_one_probability_always_drops() {
        let table = LootTable::with_entries(vec![LootEntry::Chance {
            item: ItemId(1),
            count: 1,
            probability: 1.0,
        }]);
        for seed in 0..32u64 {
            let mut rng = StdRng::seed_from_u64(seed);
            assert_eq!(table.roll(&mut rng).len(), 1);
        }
    }

    #[test]
    fn seeded_rng_makes_roll_reproducible() {
        let table = LootTable::with_entries(vec![
            LootEntry::Range {
                item: ItemId(1),
                min: 1,
                max: 10,
            },
            LootEntry::Chance {
                item: ItemId(2),
                count: 4,
                probability: 0.5,
            },
        ]);
        let mut a = StdRng::seed_from_u64(99);
        let mut b = StdRng::seed_from_u64(99);
        assert_eq!(table.roll(&mut a), table.roll(&mut b));
    }

    #[test]
    fn loot_table_implements_block_data() {
        let table = LootTable::with_entries(vec![LootEntry::Fixed {
            item: ItemId(1),
            count: 1,
        }]);
        let boxed: Box<dyn BlockData> = Box::new(table);
        assert_eq!(boxed.type_key(), std::any::type_name::<LootTable>());
        let cloned = boxed.clone_box();
        assert_eq!(cloned.type_key(), boxed.type_key());
        let back = cloned
            .as_any()
            .downcast_ref::<LootTable>()
            .expect("downcast");
        assert_eq!(back.entries.len(), 1);
    }
}
