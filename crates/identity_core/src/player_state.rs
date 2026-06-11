use bevy::math::Vec3;
use serde::{Deserialize, Serialize};

/// Persisted player state, stored at `<world_dir>/players/<sub>.bin`.
///
/// Loaded on authentication and applied to the spawned character entity.
/// Saved on clean disconnect.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PlayerSaveState {
    /// Last known world-space position.
    pub last_position: Vec3Serde,
    /// Inventory contents. Empty until the inventory system is wired in.
    pub inventory: Vec<InventorySlot>,
}

/// `Vec3` serialisation shim.
///
/// Bevy's `Vec3` does not implement `serde::Serialize`/`Deserialize` in all
/// feature configurations, so we use this plain-struct newtype for the save
/// file format and convert with [`From`] impls.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct Vec3Serde {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl From<Vec3> for Vec3Serde {
    fn from(v: Vec3) -> Self {
        Self {
            x: v.x,
            y: v.y,
            z: v.z,
        }
    }
}

impl From<Vec3Serde> for Vec3 {
    fn from(v: Vec3Serde) -> Self {
        Vec3::new(v.x, v.y, v.z)
    }
}

/// Placeholder inventory slot.
///
/// Carries no data until the inventory system defines a concrete type.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct InventorySlot;
