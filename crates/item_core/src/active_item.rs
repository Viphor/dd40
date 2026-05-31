//! The per-character [`ActiveItem`] component and its [`ItemStack`] payload.
//!
//! # Role in the architecture
//!
//! [`ActiveItem`] is the **single contract** every gameplay system reads when
//! it asks "what is this character holding right now?".  Mining reads it for
//! the tool kind/tier; placement reads it for the placeable block; future
//! "use item" code paths will read it for consumable / weapon behaviour.
//!
//! Inventory crates (`dd40_inventory`, hypothetical `dd40_multi_equip`,
//! etc.) attach [`ActiveItem`] with a concrete
//! [`ActiveItemProvider`] that knows how to read and consume from
//! whatever storage they use.  A character without an [`ActiveItem`]
//! component, or with an empty cache, is considered to be holding
//! nothing — bare hands.
//!
//! # Provider model
//!
//! [`ActiveItem`] wraps a `Box<dyn ActiveItemProvider>` plus a
//! `cached: Option<ItemStack>` snapshot.  Read-side callers see only
//! the cache via [`ActiveItem::peek`] / [`ActiveItem::item`] — cheap,
//! synchronous, and free of ECS access.
//!
//! Write-side callers (e.g. block placement on commit) ask the
//! provider to [`consume`][ActiveItemProvider::consume]; the provider
//! is responsible for actually decrementing the underlying storage.
//! A refresh system in the inventory crate keeps `cached` in sync
//! with the provider on `Changed<InventoryComponent>`.

use std::num::NonZero;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::registry::ItemId;

/// A non-empty stack of identical items.
///
/// Inventory slots that are empty store `Option::None` rather than a stack
/// with `count = 0`; the [`NonZero`] count makes the
/// "empty stack" state unrepresentable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Reflect, Serialize, Deserialize)]
pub struct ItemStack {
    /// Which item this stack holds.
    pub item: ItemId,
    /// How many copies are in the stack.
    ///
    /// Always `>= 1`.  Inventory crates are responsible for capping this
    /// at the item's [`ItemDefinition::max_stack`][crate::registry::ItemDefinition::max_stack];
    /// consumers may assume it falls within `1..=max_stack`.
    pub count: NonZero<u16>,
}

impl ItemStack {
    /// Creates a stack of `count` copies of `item`.
    ///
    /// Use [`ItemStack::try_new`] when `count` is a runtime [`u16`] that
    /// might be zero.
    pub fn new(item: ItemId, count: NonZero<u16>) -> Self {
        Self { item, count }
    }

    /// Creates a stack from a runtime [`u16`] count, returning [`None`] if
    /// `count == 0`.
    pub fn try_new(item: ItemId, count: u16) -> Option<Self> {
        NonZero::new(count).map(|count| Self { item, count })
    }

    /// Convenience constructor for a single-item stack.
    pub fn single(item: ItemId) -> Self {
        Self::new(item, NonZero::<u16>::MIN)
    }
}

/// Source of the "currently held" stack and sink for "consume one /
/// many" operations.
///
/// Inventory crates implement this to bridge their storage layout to
/// the gameplay-side [`ActiveItem`] contract.  Two implementations
/// ship in-tree:
///
/// - [`EmptyProvider`] — always empty, consume is a no-op.  Used as
///   the default when an `ActiveItem` is freshly inserted but no
///   inventory has claimed it yet.
/// - `dd40_inventory::InventorySlotProvider` — reads/mutates
///   the entity's `InventoryComponent` at its current `active_slot`.
///
/// Future providers might wrap a creative-mode infinite stack, a
/// menu-driven selection, or an AI policy.
///
/// # Why `&World` / `&mut World`?
///
/// Providers typically need to look up component state on other
/// entities (the inventory holder), or to mutate that state when
/// consuming.  Taking the world explicitly makes that obvious and
/// keeps the trait usable from exclusive systems where the writer
/// already has the world.
pub trait ActiveItemProvider: Send + Sync + 'static {
    /// Returns the currently-active stack, or `None` if empty.
    ///
    /// Called by the refresh system in the inventory crate to update
    /// [`ActiveItem`]'s cache.
    fn current(&self, world: &World) -> Option<ItemStack>;

    /// Consumes up to `count` items from the active stack and returns
    /// how many were actually removed.
    ///
    /// Implementations may be no-ops (creative-mode providers return
    /// `0`).  Implementations that *do* mutate state must mutate the
    /// underlying storage so a subsequent `current` reflects the
    /// change.
    fn consume(&mut self, world: &mut World, count: u16) -> u16;
}

/// The "empty hands" provider — always returns `None` and never
/// consumes.  Used as the default for a freshly-inserted
/// [`ActiveItem`].
#[derive(Debug, Default, Clone, Copy)]
pub struct EmptyProvider;

impl ActiveItemProvider for EmptyProvider {
    fn current(&self, _world: &World) -> Option<ItemStack> {
        None
    }

    fn consume(&mut self, _world: &mut World, _count: u16) -> u16 {
        0
    }
}

/// The item a character is currently holding.
///
/// Attach to any `Character` entity.  Gameplay systems read this
/// component to determine behaviour:
///
/// - **Mining** looks up the cached item's
///   [`tool`][crate::registry::ItemDefinition::tool] field for the
///   speed-bonus kind/tier.
/// - **Placement** looks up the cached item's
///   [`placeable`][crate::registry::ItemDefinition::placeable] field
///   for the block to place, and asks the provider to consume one on
///   commit.
///
/// An empty cache (or no component) means bare hands — no tool bonus,
/// nothing to place.
///
/// This component is **not** replicated.  Each side (client, server)
/// attaches its own `ActiveItem` and refreshes the cache from local
/// state; the underlying inventory data is what travels across the
/// wire.
#[derive(Component)]
pub struct ActiveItem {
    cached: Option<ItemStack>,
    provider: Box<dyn ActiveItemProvider>,
}

impl ActiveItem {
    /// Builds an [`ActiveItem`] with the given provider.  The cache
    /// starts empty; the inventory crate's refresh system fills it in
    /// on the next tick.
    pub fn with_provider<P: ActiveItemProvider>(provider: P) -> Self {
        Self {
            cached: None,
            provider: Box::new(provider),
        }
    }

    /// Returns the currently-cached stack snapshot.
    ///
    /// This is what every read-side caller should use: cheap, no
    /// world access, kept in sync by the inventory crate's refresh
    /// system.
    pub fn peek(&self) -> Option<ItemStack> {
        self.cached
    }

    /// Returns the [`ItemId`] currently held, if any.  Equivalent to
    /// `self.peek().map(|s| s.item)`.
    pub fn item(&self) -> Option<ItemId> {
        self.cached.map(|s| s.item)
    }

    /// Overwrites the cached snapshot.
    ///
    /// Intended for the inventory crate's refresh system only;
    /// gameplay code should never call this.
    pub fn set_cache(&mut self, stack: Option<ItemStack>) {
        self.cached = stack;
    }

    /// Calls the provider's [`current`][ActiveItemProvider::current],
    /// updates the cache, and returns the new value.
    ///
    /// Convenience wrapper for the refresh system.
    pub fn refresh(&mut self, world: &World) -> Option<ItemStack> {
        let new = self.provider.current(world);
        self.cached = new;
        new
    }

    /// Asks the provider to consume `count` items.  Returns how many
    /// were actually removed.  Does **not** update the cache —
    /// callers should rely on the refresh system to do that after the
    /// underlying storage emits its own change signal.
    pub fn consume(&mut self, world: &mut World, count: u16) -> u16 {
        self.provider.consume(world, count)
    }

    /// Returns a mutable reference to the underlying provider trait
    /// object, for callers that need to invoke provider-specific
    /// methods.
    pub fn provider_mut(&mut self) -> &mut dyn ActiveItemProvider {
        &mut *self.provider
    }
}

impl Default for ActiveItem {
    fn default() -> Self {
        Self::with_provider(EmptyProvider)
    }
}

impl std::fmt::Debug for ActiveItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActiveItem")
            .field("cached", &self.cached)
            .finish_non_exhaustive()
    }
}

/// Test helper: a provider whose state is a single
/// `Option<ItemStack>`.  `current` returns it; `consume` decrements
/// its count.
///
/// Available outside `#[cfg(test)]` so integration tests in other
/// crates can wire it up without taking a dependency on a specific
/// inventory implementation.
#[derive(Debug, Default, Clone, Copy)]
pub struct FixedProvider(pub Option<ItemStack>);

impl FixedProvider {
    /// Convenience constructor that wraps a single-item stack.
    pub fn single(item: ItemId) -> Self {
        Self(Some(ItemStack::single(item)))
    }
}

impl ActiveItemProvider for FixedProvider {
    fn current(&self, _world: &World) -> Option<ItemStack> {
        self.0
    }

    fn consume(&mut self, _world: &mut World, count: u16) -> u16 {
        let Some(stack) = self.0 else {
            return 0;
        };
        let have = stack.count.get();
        let take = count.min(have);
        if take == have {
            self.0 = None;
        } else {
            self.0 = Some(ItemStack::new(
                stack.item,
                NonZero::new(have - take).expect("take < have"),
            ));
        }
        take
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nz(n: u16) -> NonZero<u16> {
        NonZero::new(n).expect("non-zero literal")
    }

    #[test]
    fn default_active_item_is_empty() {
        let active = ActiveItem::default();
        assert!(active.peek().is_none());
        assert_eq!(active.item(), None);
    }

    #[test]
    fn with_provider_starts_with_empty_cache() {
        let active = ActiveItem::with_provider(FixedProvider::single(ItemId(5)));
        assert!(
            active.peek().is_none(),
            "cache must start empty — refresh system fills it"
        );
    }

    #[test]
    fn refresh_pulls_from_provider() {
        let mut world = World::new();
        let mut active = ActiveItem::with_provider(FixedProvider::single(ItemId(5)));
        let got = active.refresh(&world);
        assert_eq!(got.unwrap().item, ItemId(5));
        assert_eq!(active.item(), Some(ItemId(5)));
        let _ = &mut world;
    }

    #[test]
    fn consume_calls_provider() {
        let mut world = World::new();
        let mut active =
            ActiveItem::with_provider(FixedProvider(Some(ItemStack::new(ItemId(5), nz(3)))));
        active.refresh(&world);
        let taken = active.consume(&mut world, 2);
        assert_eq!(taken, 2);
        // Cache is not auto-refreshed; refresh again to verify the
        // underlying provider state actually changed.
        let remaining = active.refresh(&world);
        assert_eq!(remaining.unwrap().count.get(), 1);
    }

    #[test]
    fn try_new_zero_count_returns_none() {
        assert!(ItemStack::try_new(ItemId(1), 0).is_none());
        assert_eq!(ItemStack::try_new(ItemId(1), 5).unwrap().count, nz(5));
    }

    #[test]
    fn empty_provider_consume_is_noop() {
        let mut world = World::new();
        let mut p = EmptyProvider;
        assert_eq!(p.consume(&mut world, 5), 0);
    }
}
