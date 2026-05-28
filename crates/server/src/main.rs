use bevy::{diagnostic::DiagnosticsPlugin, prelude::*};
use dd40_character_interaction::CharacterInteractionPlugin;
use dd40_chunk_storage::plugin::DiskStoragePlugin;
use dd40_core::{
    common::log_plugin, graceful_shutdown::GracefulShutdownPlugin, plugin::CorePlugin,
};
use dd40_integration_character_physics::IntegrationCharacterPhysicsPlugin;
use dd40_integration_loose_item_pickup::LooseItemPickupPlugin;
use dd40_loose_items::LooseItemsPlugin;
use dd40_loot::LootPlugin;
use dd40_network::{
    ServerInventoryNetworkPlugin, ServerNetworkPlugin,
    server::connection::{DDServer, LinkConditionerConfig, RecvLinkConditioner},
    shared::connection::SHARED_SETTINGS,
};
use dd40_physics::PhysicsPlugin;
use dd40_vanilla_inventory::{VanillaInventoryPlugin, VanillaInventoryRulesPlugin};
use dd40_vanilla_palette::{VanillaBlocks, VanillaPalettePlugin};
use dd40_world::{
    WorldPlugin,
    generators::bowl::{BowlWorldGenerator, Layer},
};

fn main() {
    App::new()
        // MinimalPlugins gives us ECS, scheduling, and time – but no window or rendering.
        .add_plugins(MinimalPlugins)
        .add_plugins(log_plugin())
        .add_plugins(DiagnosticsPlugin)
        .add_plugins((
            CorePlugin,
            // Headless: translate Ctrl-C / SIGTERM into AppExit so the
            // Last schedule (which flushes entity sidecars) actually
            // runs before the process exits.
            GracefulShutdownPlugin,
            PhysicsPlugin,
            IntegrationCharacterPhysicsPlugin,
            VanillaPalettePlugin,
            DiskStoragePlugin::new("world_data/chunks"),
            WorldPlugin::new(BowlWorldGenerator(vec![
                Layer {
                    block_id: VanillaBlocks::STONE,
                    height_range: 0..58,
                },
                Layer {
                    block_id: VanillaBlocks::DIRT,
                    height_range: 58..62,
                },
                Layer {
                    block_id: VanillaBlocks::GRASS,
                    height_range: 62..63,
                },
            ])),
            // Authoritative block-targeting, mining, and placement for every
            // connected character.  The server owns the truth; clients render
            // the result that comes back over the wire.
            CharacterInteractionPlugin,
            // Server-only: turn accepted ChunkChange::Remove into DropItems.
            LootPlugin,
            // Server-only: spawn LooseItem entities from DropItems and tick
            // their despawn / pickup-cooldown timers.
            LooseItemsPlugin,
            // Server-only: grant LooseItems to characters in contact with
            // an empty inventory slot.
            LooseItemPickupPlugin,
            // Authoritative inventory rules: drains SlotInteraction
            // messages and mutates each Character's InventoryComponent
            // + HeldStackComponent.  Clients send their slot clicks as
            // NetSlotInteraction; ServerInventoryNetworkPlugin
            // (below) translates them onto the local bus.
            VanillaInventoryPlugin,
            VanillaInventoryRulesPlugin,
            // Server-only: drain NetSlotInteraction messages from
            // lightyear, resolve the controlling Character via
            // ControlledBy, and re-emit a local SlotInteraction so the
            // VanillaInventoryRulesPlugin apply system runs unchanged.
            ServerInventoryNetworkPlugin,
            ServerNetworkPlugin(DDServer {
                conditioner: Some(RecvLinkConditioner::new(
                    LinkConditionerConfig::average_condition(),
                )),
                port: 6969,
                shared: SHARED_SETTINGS,
            }),
        ))
        .add_systems(Update, server_tick)
        .run();
}

/// Placeholder server tick system – extend with network and game-logic code.
fn server_tick(_time: Res<Time>) {}
