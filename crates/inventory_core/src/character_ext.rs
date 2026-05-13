//! Builder extension trait that adds an [`Inventory`] to any
//! [`AddExtra`][dd40_core::builder_extra::AddExtra] builder.
//!
//! [`CharacterInventoryExt`] is blanket-implemented on every type that
//! implements [`AddExtra`][dd40_core::builder_extra::AddExtra], so
//! [`CharacterBuilder`][] (in `dd40_character_core`) — and any other future
//! builder that opts into the extras protocol — gains
//! [`with_inventory`][CharacterInventoryExt::with_inventory] and
//! [`with_inventory_component`][CharacterInventoryExt::with_inventory_component]
//! without `dd40_inventory_core` having to depend on `dd40_character_core`.
//!
//! [`CharacterBuilder`]: https://docs.rs/dd40_character_core

use dd40_core::builder_extra::AddExtra;

use crate::inventory::Inventory;

/// Adds an [`Inventory`] to any builder that implements
/// [`AddExtra`][dd40_core::builder_extra::AddExtra].
pub trait CharacterInventoryExt {
    /// Attaches an empty [`Inventory`] of the given capacity.
    fn with_inventory(self, capacity: usize) -> Self;

    /// Attaches a pre-constructed [`Inventory`].
    ///
    /// Useful when the inventory must be populated before spawn (loading a
    /// save, restoring a snapshot).  After spawn, mutate via the
    /// event-firing methods on [`Inventory`] so observers stay in sync.
    fn with_inventory_component(self, inventory: Inventory) -> Self;
}

impl<T> CharacterInventoryExt for T
where
    T: AddExtra,
{
    fn with_inventory(mut self, capacity: usize) -> Self {
        self.add_extra(move |entity| {
            entity.insert(Inventory::with_capacity(capacity));
        });
        self
    }

    fn with_inventory_component(mut self, inventory: Inventory) -> Self {
        self.add_extra(move |entity| {
            entity.insert(inventory);
        });
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::EntityCommands;
    use bevy::prelude::*;

    /// Minimal in-test builder mirroring the pattern used by
    /// `crates/physics_core/src/character_ext.rs` so this test does not
    /// require a dependency on `dd40_character_core`.
    struct TestBuilder {
        extras: Vec<Box<dyn FnOnce(&mut EntityCommands) + Send + 'static>>,
    }

    impl TestBuilder {
        fn new() -> Self {
            Self { extras: Vec::new() }
        }

        fn spawn(self, commands: &mut Commands) -> Entity {
            let mut entity = commands.spawn_empty();
            for extra in self.extras {
                extra(&mut entity);
            }
            entity.id()
        }
    }

    impl AddExtra for TestBuilder {
        fn add_extra<F>(&mut self, f: F) -> &mut Self
        where
            F: FnOnce(&mut EntityCommands) + Send + 'static,
        {
            self.extras.push(Box::new(f));
            self
        }
    }

    #[test]
    fn with_inventory_attaches_component_of_requested_capacity() {
        let mut app = App::new();
        let entity_id = std::sync::Arc::new(std::sync::Mutex::new(None::<Entity>));
        let id_clone = entity_id.clone();
        app.add_systems(Startup, move |mut commands: Commands| {
            let id = TestBuilder::new().with_inventory(9).spawn(&mut commands);
            *id_clone.lock().unwrap() = Some(id);
        });
        app.update();
        let id = entity_id.lock().unwrap().expect("entity spawned");
        let inv = app.world().get::<Inventory>(id).expect("Inventory present");
        assert_eq!(inv.capacity(), 9);
    }

    #[test]
    fn with_inventory_component_attaches_supplied_inventory() {
        let mut app = App::new();
        let entity_id = std::sync::Arc::new(std::sync::Mutex::new(None::<Entity>));
        let id_clone = entity_id.clone();
        let mut prefilled = Inventory::with_capacity(2);
        prefilled.set_slot_without_event(
            0,
            Some(dd40_item_core::active_item::ItemStack::single(
                dd40_item_core::registry::ItemId(7),
            )),
        );
        app.add_systems(Startup, move |mut commands: Commands| {
            let id = TestBuilder::new()
                .with_inventory_component(prefilled.clone())
                .spawn(&mut commands);
            *id_clone.lock().unwrap() = Some(id);
        });
        app.update();
        let id = entity_id.lock().unwrap().expect("entity spawned");
        let inv = app.world().get::<Inventory>(id).expect("Inventory present");
        assert_eq!(inv.capacity(), 2);
        assert_eq!(
            inv.slot(0).unwrap().item,
            dd40_item_core::registry::ItemId(7)
        );
    }
}
