use bevy::{
    ecs::{lifecycle::HookContext, world::DeferredWorld},
    prelude::*,
};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Geometry
// ---------------------------------------------------------------------------

/// Axis-Aligned Bounding Box expressed as **half-extents** from the entity
/// origin.
///
/// The entity origin sits at the **bottom-centre** of the AABB (matching
/// Minecraft's coordinate system where `y` is the foot position).
///
/// # Examples
///
/// A standard player capsule approximation (0.6 wide, 1.8 tall):
/// ```
/// use dd40_physics_core::components::Aabb;
/// let player = Aabb::new(0.3, 0.9, 0.3);
/// ```
#[derive(Debug, Clone, Copy, Component, Reflect)]
#[reflect(Component)]
pub struct Aabb {
    /// Half-width along X.
    pub half_x: f32,
    /// Half-height along Y (measured from origin upward).
    pub half_y: f32,
    /// Half-depth along Z.
    pub half_z: f32,
}

impl Aabb {
    /// Creates a new AABB with the given half-extents.
    pub fn new(half_x: f32, half_y: f32, half_z: f32) -> Self {
        Self {
            half_x,
            half_y,
            half_z,
        }
    }

    /// Returns a player-shaped AABB (0.3 × 0.9 × 0.3 half-extents).
    pub fn player() -> Self {
        Self::new(0.3, 0.9, 0.3)
    }

    /// Minimum corner relative to an origin point.
    #[inline]
    pub fn min(&self, origin: Vec3) -> Vec3 {
        Vec3::new(origin.x - self.half_x, origin.y, origin.z - self.half_z)
    }

    /// Maximum corner relative to an origin point.
    #[inline]
    pub fn max(&self, origin: Vec3) -> Vec3 {
        Vec3::new(
            origin.x + self.half_x,
            origin.y + self.half_y * 2.0,
            origin.z + self.half_z,
        )
    }

    /// Returns `true` when the AABB overlaps with `other`.
    pub fn overlaps(&self, self_origin: Vec3, other: &Aabb, other_origin: Vec3) -> bool {
        let a_min = self.min(self_origin);
        let a_max = self.max(self_origin);
        let b_min = other.min(other_origin);
        let b_max = other.max(other_origin);

        a_min.x < b_max.x
            && a_max.x > b_min.x
            && a_min.y < b_max.y
            && a_max.y > b_min.y
            && a_min.z < b_max.z
            && a_max.z > b_min.z
    }

    /// Returns the penetration vector (smallest separating axis) when the two
    /// AABBs overlap, or `None` if they are separated.
    ///
    /// The returned vector points **from `other` toward `self`** — adding it
    /// to `self_origin` resolves the overlap.
    pub fn penetration(&self, self_origin: Vec3, other: &Aabb, other_origin: Vec3) -> Option<Vec3> {
        let a_min = self.min(self_origin);
        let a_max = self.max(self_origin);
        let b_min = other.min(other_origin);
        let b_max = other.max(other_origin);

        let overlap_x = f32::min(a_max.x - b_min.x, b_max.x - a_min.x);
        let overlap_y = f32::min(a_max.y - b_min.y, b_max.y - a_min.y);
        let overlap_z = f32::min(a_max.z - b_min.z, b_max.z - a_min.z);

        if overlap_x <= 0.0 || overlap_y <= 0.0 || overlap_z <= 0.0 {
            return None;
        }

        if overlap_x <= overlap_y && overlap_x <= overlap_z {
            let sign = if self_origin.x > other_origin.x {
                1.0
            } else {
                -1.0
            };
            Some(Vec3::new(overlap_x * sign, 0.0, 0.0))
        } else if overlap_z <= overlap_x && overlap_z <= overlap_y {
            let sign = if self_origin.z > other_origin.z {
                1.0
            } else {
                -1.0
            };
            Some(Vec3::new(0.0, 0.0, overlap_z * sign))
        } else {
            let sign = if self_origin.y > other_origin.y {
                1.0
            } else {
                -1.0
            };
            Some(Vec3::new(0.0, overlap_y * sign, 0.0))
        }
    }
}

// ---------------------------------------------------------------------------
// Velocity & forces
// ---------------------------------------------------------------------------

/// Linear velocity of a physics body, in **world units per second**.
///
/// Registered as a predicted component in the network protocol so lightyear
/// includes it in rollback snapshots. This is essential for correct prediction:
/// restoring only position but not velocity causes re-simulation to diverge on
/// gravity-affected or collision-affected entities.
#[derive(
    Debug,
    Default,
    Clone,
    Copy,
    Component,
    Reflect,
    Deref,
    DerefMut,
    PartialEq,
    Serialize,
    Deserialize,
)]
#[reflect(Component)]
pub struct Velocity(pub Vec3);

impl Velocity {
    /// Zero velocity.
    pub const ZERO: Self = Self(Vec3::ZERO);

    /// Returns the underlying [`Vec3`].
    #[inline]
    pub fn vec(&self) -> Vec3 {
        self.0
    }
}

// ---------------------------------------------------------------------------

/// Scale factor applied to gravity for this entity.
///
/// - `1.0` — standard gravity (default)
/// - `0.0` — flying / no gravity
/// - negative — inverted gravity
///
/// The gravity constant itself lives in [`PhysicsConfig`].
#[derive(Debug, Clone, Copy, Component, Reflect)]
#[reflect(Component)]
pub struct GravityScale(pub f32);

impl Default for GravityScale {
    fn default() -> Self {
        Self(1.0)
    }
}

// ---------------------------------------------------------------------------

/// Tracks whether the entity is resting on a solid surface.
///
/// Set to `true` by the block-collision solver on Y-axis landing. Reset to
/// `false` at the start of each [`PhysicsSet::Integrate`] frame. Use this to
/// determine whether a jump impulse may be applied.
#[derive(Debug, Default, Clone, Copy, Component, Reflect)]
#[reflect(Component)]
pub struct Grounded(pub bool);

impl Grounded {
    /// Returns `true` when the entity is on the ground.
    #[inline]
    pub fn is_grounded(&self) -> bool {
        self.0
    }
}

// ---------------------------------------------------------------------------
// Position scratchpad
// ---------------------------------------------------------------------------

/// The confirmed physics position of a character, separate from [`Transform`].
///
/// This is the single source of truth for the physics solver. The pipeline
/// reads it at the start of each tick and writes the resolved position back in
/// [`PhysicsSet::Finalise`].
///
/// [`Transform`] is a **visual-only** output. For networked predicted entities,
/// the network bridge overrides [`Transform`] in `Update` with a
/// frame-interpolated value, keeping rendering smooth without contaminating
/// physics.
///
/// The `on_add` hook copies the entity's current [`Transform`] translation so
/// the solver starts at the right position without manual initialisation.
#[derive(Debug, Default, Clone, Copy, Component, Reflect, PartialEq)]
#[reflect(Component)]
#[component(on_add)]
pub struct CharacterPosition(pub Vec3);

impl CharacterPosition {
    fn on_add(mut world: DeferredWorld, context: HookContext) {
        let translation = world
            .get::<Transform>(context.entity)
            .map(|t| t.translation)
            .unwrap_or(Vec3::ZERO);
        if let Some(mut pos) = world.get_mut::<CharacterPosition>(context.entity) {
            pos.0 = translation;
        }
    }
}

// ---------------------------------------------------------------------------

/// Accumulates instantaneous velocity changes to be flushed at the start of
/// the next [`PhysicsSet::Integrate`] tick.
///
/// Any system may add to this — the character controller, explosion knockback,
/// ability systems, etc. The integration stage flushes it into [`Velocity`]
/// and resets it to zero each tick so nothing leaks across frames.
///
/// Prefer writing here over mutating [`Velocity`] directly; this keeps all
/// force sources composable and order-independent within a frame.
#[derive(Debug, Default, Clone, Copy, Component, Reflect, Deref, DerefMut)]
#[reflect(Component)]
pub struct Impulse(pub Vec3);

// ---------------------------------------------------------------------------
// Markers
// ---------------------------------------------------------------------------

/// Opts an entity into the full physics pipeline.
///
/// **Important:** [`Aabb`] is NOT auto-inserted because its dimensions are
/// entity-specific.  You must add an [`Aabb`] yourself, otherwise
/// [`PhysicsSet::BlockCollision`] will silently skip the entity and it will
/// fall through all geometry.
///
/// Entities that should collide with other characters should also add
/// [`CharacterCollider`].
#[derive(Debug, Default, Component, Reflect)]
#[reflect(Component)]
#[require(Velocity, GravityScale, Grounded, Impulse, CharacterPosition)]
pub struct PhysicsBody;

// ---------------------------------------------------------------------------

/// Opts this entity into **character-vs-character** collision resolution.
///
/// Entities with [`PhysicsBody`] but without [`CharacterCollider`] are still
/// resolved against the block grid; they do not push other characters away.
#[derive(Debug, Default, Component, Reflect)]
#[reflect(Component)]
pub struct CharacterCollider;

#[cfg(test)]
mod tests {
    use super::*;

    /// `CharacterPosition::on_add` reads the entity's `Transform.translation`
    /// at component-insertion time. When `Transform` is part of the same
    /// spawn tuple as `PhysicsBody` (which auto-requires `CharacterPosition`),
    /// the hook sees the spawn position and the physics solver starts there.
    #[test]
    fn character_position_picks_up_transform_present_in_spawn_tuple() {
        let mut app = App::new();
        let entity = app
            .world_mut()
            .spawn((Transform::from_xyz(0.0, 74.0, 0.0), PhysicsBody))
            .id();
        let cp = app.world().get::<CharacterPosition>(entity).unwrap();
        assert_eq!(cp.0, Vec3::new(0.0, 74.0, 0.0));
    }

    /// Regression test for the player-stuck-at-bottom-of-world bug.
    ///
    /// If `PhysicsBody` is inserted **before** `Transform` is on the entity,
    /// `CharacterPosition::on_add` reads no Transform and falls back to
    /// `Vec3::ZERO`. Inserting `Transform` afterwards does **not** retro-fix
    /// `CharacterPosition`. Spawn flows must include `Transform` (or another
    /// initial `CharacterPosition`) in the same tuple as `PhysicsBody`.
    #[test]
    fn character_position_is_zero_when_transform_is_inserted_after_physics_body() {
        let mut app = App::new();
        let entity = app.world_mut().spawn(PhysicsBody).id();
        // CharacterPosition was initialised to ZERO at on_add time.
        let cp_before = *app.world().get::<CharacterPosition>(entity).unwrap();
        assert_eq!(cp_before.0, Vec3::ZERO);

        // Inserting Transform afterwards does NOT update CharacterPosition.
        app.world_mut()
            .entity_mut(entity)
            .insert(Transform::from_xyz(0.0, 74.0, 0.0));
        let cp_after = *app.world().get::<CharacterPosition>(entity).unwrap();
        assert_eq!(
            cp_after.0,
            Vec3::ZERO,
            "documents the contract: CharacterPosition::on_add only fires once"
        );
    }
}
