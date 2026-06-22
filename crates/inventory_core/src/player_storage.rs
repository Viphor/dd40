use bevy::prelude::*;
use dd40_player_storage::PlayerStateContributor;

use crate::component::InventoryComponent;
use crate::inventory::Inventory;

/// Contributes `InventoryComponent` to the player state registry.
pub(crate) struct InventoryContributor;

impl PlayerStateContributor for InventoryContributor {
    fn kind(&self) -> &'static str {
        "inventory"
    }

    fn current_version(&self) -> u16 {
        1
    }

    fn save(&self, entity: &EntityRef) -> Vec<u8> {
        entity
            .get::<InventoryComponent>()
            .and_then(|c| bincode::serialize(c.inventory()).ok())
            .unwrap_or_default()
    }

    fn load(&self, entity: Entity, version: u16, data: &[u8], commands: &mut Commands) {
        match version {
            1 => match bincode::deserialize::<Inventory>(data) {
                Ok(inv) => {
                    commands
                        .entity(entity)
                        .insert(InventoryComponent::from_inventory(inv));
                }
                Err(e) => {
                    warn!(entity = ?entity, error = %e, "failed to deserialise inventory save; starting with empty inventory");
                }
            },
            v => {
                warn!(entity = ?entity, version = v, "unknown inventory save version; starting with empty inventory");
            }
        }
    }
}
