//! Simplified voxel-aware physics for dd40.
//!
//! This module deliberately avoids a full rigid-body simulation framework.
//! Because every block occupies exactly one 1×1×1 unit cell we can resolve
//! block collisions in **O(1)** per swept axis rather than building broad- and
//! narrow-phase pipelines.
//!
//! # Architecture
//!
//! ```text
//! PhysicsSet::Integrate   – apply gravity, accumulate velocity → tentative position
//! PhysicsSet::BlockCollision  – sweep tentative position against the block grid
//! PhysicsSet::CharacterCollision – push apart overlapping character colliders
//! PhysicsSet::Finalise    – write resolved position back to Transform
//! ```
//!
//! Each sub-module owns one of those stages:
//!
//! - [`integration`]  – gravity + velocity integration
//! - [`block_collision`] – O(1) voxel AABB resolution
//! - [`character_collision`] – character-vs-character push-apart
//!
//! # Adding a custom block collision shape
//!
//! Set the [`CollisionShape`] field on [`BlockDefinition`] when registering a
//! block type.  The block-collision solver reads the shape directly from
//! [`BlockRegistry`] — no separate shape registry is needed.  This is the
//! extensibility hook for stairs, slabs, lecterns, etc.
//!
//! ```
//! use bevy::math::Vec3;
//! use dd40_core::prelude::*;
//! use dd40_core::character::physics::CollisionShape;
//!
//! let slab = BlockDefinition::new(BlockId(1000), "oak_slab")
//!     .with_collision_shape(CollisionShape::Box {
//!         min: Vec3::ZERO,
//!         max: Vec3::new(1.0, 0.5, 1.0),
//!     });
//! ```
//!
//! [`BlockDefinition`]: crate::block::registry::BlockDefinition
//! [`BlockRegistry`]: crate::block::registry::BlockRegistry

pub mod block_collision;
pub mod character_collision;
pub mod integration;
pub mod spatial_cache;

pub use spatial_cache::CharacterSpatialCache;

use bevy::prelude::*;

use block_collision::BlockCollisionPlugin;
use character_collision::CharacterCollisionPlugin;
use integration::IntegrationPlugin;

// ---------------------------------------------------------------------------
// System ordering
// ---------------------------------------------------------------------------

/// Labels the four ordered stages of one physics tick.
///
/// Configure your own systems against these labels if you need to hook into
/// the pipeline (e.g. custom force applicators before [`PhysicsSet::Integrate`],
/// or post-solve callbacks after [`PhysicsSet::Finalise`]).
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum PhysicsSet {
    /// Apply external forces (gravity, impulses) and integrate velocity into
    /// a **tentative** new world position stored in [`TentativePosition`].
    Integrate,
    /// Resolve the tentative position against the solid block grid.
    /// Sweeps each axis independently for correct corner behaviour.
    BlockCollision,
    /// Push overlapping character colliders apart.
    CharacterCollision,
    /// Copy the resolved tentative position back into the entity's
    /// [`Transform`] and clear per-frame transient state.
    Finalise,
}

// ---------------------------------------------------------------------------
// Core components
// ---------------------------------------------------------------------------

/// Axis-Aligned Bounding Box expressed as **half-extents** from the entity
/// origin.
///
/// The entity origin sits at the **bottom-centre** of the AABB by convention
/// (matching Minecraft's coordinate system where `y` is the foot position).
///
/// # Examples
///
/// A standard player capsule approximation (0.6 wide, 1.8 tall):
/// ```
/// use dd40_core::character::physics::Aabb;
/// let player = Aabb::new(0.3, 0.9, 0.3);
/// ```
#[derive(Debug, Clone, Copy, Component, Reflect)]
#[reflect(Component)]
pub struct Aabb {
    /// Half-width along X.
    pub half_x: f32,
    /// Half-height along Y (measured from origin upward — see note above).
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

    /// Returns a player-shaped AABB (0.3 × 0.9 × 0.3 half-extents, yielding a
    /// 0.6 × 1.8 × 0.6 bounding box).
    pub fn player() -> Self {
        Self::new(0.3, 0.9, 0.3)
    }

    /// Minimum corner relative to an origin point.
    #[inline]
    pub fn min(&self, origin: Vec3) -> Vec3 {
        Vec3::new(
            origin.x - self.half_x,
            origin.y, // bottom of box is at origin
            origin.z - self.half_z,
        )
    }

    /// Maximum corner relative to an origin point.
    #[inline]
    pub fn max(&self, origin: Vec3) -> Vec3 {
        Vec3::new(
            origin.x + self.half_x,
            origin.y + self.half_y * 2.0, // top is 2 * half_y above origin
            origin.z + self.half_z,
        )
    }

    /// Returns `true` when the AABB (placed at `self_origin`) overlaps with
    /// the AABB placed at `other_origin`.
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

    /// Returns the penetration vector (smallest separating axis) when two
    /// AABBs overlap, or `None` if they do not overlap.
    ///
    /// The returned vector points **from `other` toward `self`** — i.e. adding
    /// it to `self_origin` resolves the overlap.
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

        // Push along the axis of least penetration.
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

/// Collision shape for a block type.
///
/// All variants must stay **within** the 1×1×1 block cell.  The physics
/// solver reads this directly from [`BlockDefinition::collision_shape`] via
/// [`BlockRegistry`] — it is part of the block definition rather than a
/// separate resource.
///
/// This is the extensibility point for stairs, slabs, lecterns, etc.
///
/// [`BlockDefinition::collision_shape`]: crate::block::registry::BlockDefinition::collision_shape
/// [`BlockRegistry`]: crate::block::registry::BlockRegistry
#[derive(Debug, Clone, Reflect)]
pub enum CollisionShape {
    /// Solid unit cube — the default for all opaque blocks.
    FullCube,
    /// No collision at all (air, torches, etc.).
    None,
    /// An AABB within the cell, specified as min/max in **cell-local** space
    /// (i.e. each component in `[0, 1]`).  The cell origin is the
    /// block's minimum corner.
    ///
    /// Example — a half-slab occupying the bottom half of the cell:
    /// ```
    /// use bevy::math::Vec3;
    /// use dd40_core::character::physics::CollisionShape;
    /// let slab = CollisionShape::Box { min: Vec3::ZERO, max: Vec3::new(1.0, 0.5, 1.0) };
    /// ```
    Box {
        /// Minimum corner in cell-local coordinates (`[0, 1]` range).
        min: Vec3,
        /// Maximum corner in cell-local coordinates (`[0, 1]` range).
        max: Vec3,
    },
}

impl Default for CollisionShape {
    fn default() -> Self {
        Self::FullCube
    }
}

// ---------------------------------------------------------------------------

/// Linear velocity of a physics body, in **world units per second**.
///
/// Add this component to any entity that should be moved by the physics
/// pipeline.  Entities without [`Velocity`] are treated as static.
#[derive(Debug, Default, Clone, Copy, Component, Reflect)]
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
/// - `1.0` → standard gravity (default)
/// - `0.0` → flying / no gravity
/// - negative → inverted gravity (ceiling walker)
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

/// Tracks whether the entity is currently resting on a solid surface.
///
/// Set to `true` by the block-collision solver when the entity's AABB bottom
/// is flush with (or slightly below) a solid block's top face after Y-axis
/// resolution.  Reset to `false` at the start of each [`PhysicsSet::Integrate`]
/// frame.
///
/// Use this to determine whether a jump impulse may be applied.
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

/// Internal scratch component that holds the **tentative** world position
/// produced by [`PhysicsSet::Integrate`] and refined by the collision stages
/// before being written back to [`Transform`] during [`PhysicsSet::Finalise`].
///
/// This component is managed entirely by the physics pipeline; do **not**
/// read or write it from outside the physics module.
#[derive(Debug, Default, Clone, Copy, Component)]
pub(crate) struct TentativePosition(pub Vec3);

// ---------------------------------------------------------------------------

/// A **required** marker component that opts an entity into the physics
/// pipeline.
///
/// Insert this alongside [`Aabb`], [`Velocity`], and (optionally)
/// [`GravityScale`] to make an entity participate in physics simulation.
///
/// Characters that should generate collisions with other characters should
/// also derive [`CharacterCollider`].
///
/// # Required components
///
/// Bevy's `#[require]` attribute ensures that inserting `PhysicsBody`
/// automatically inserts default values for all of its dependencies so the
/// pipeline never panics on missing components.
#[derive(Debug, Default, Component, Reflect)]
#[reflect(Component)]
#[require(Velocity, GravityScale, Grounded, TentativePosition)]
pub struct PhysicsBody;

// ---------------------------------------------------------------------------

/// Marker component that opts this entity into **character-vs-character**
/// collision resolution.
///
/// Entities with [`PhysicsBody`] but without [`CharacterCollider`] are still
/// resolved against the block grid; they simply do not push other characters
/// away.
#[derive(Debug, Default, Component, Reflect)]
#[reflect(Component)]
pub struct CharacterCollider;

// ---------------------------------------------------------------------------

/// Global physics configuration, available as a Bevy [`Resource`].
#[derive(Resource, Debug, Clone, Reflect)]
#[reflect(Resource)]
pub struct PhysicsConfig {
    /// Gravitational acceleration in world units per second².
    /// Positive values pull entities **downward** (−Y).
    /// Default: `20.0` (roughly twice Earth gravity for snappier feel).
    pub gravity: f32,
    /// Horizontal velocity damping factor applied each second (0 = no friction,
    /// 1 = instant stop).  Used when the entity is grounded.
    pub ground_friction: f32,
    /// Horizontal velocity damping factor applied each second when airborne.
    pub air_friction: f32,
    /// Maximum downward velocity (terminal velocity), in world units/s.
    pub terminal_velocity: f32,
}

impl Default for PhysicsConfig {
    fn default() -> Self {
        Self {
            gravity: 20.0,
            ground_friction: 10.0,
            air_friction: 0.5,
            terminal_velocity: 60.0,
        }
    }
}

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

/// Registers all physics systems and resources.
///
/// Add this plugin to your [`App`] to enable the physics pipeline.  It is
/// already included by [`CorePlugin`].
///
/// [`CorePlugin`]: crate::plugin::CorePlugin
pub struct PhysicsPlugin;

impl Plugin for PhysicsPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Aabb>()
            .register_type::<Velocity>()
            .register_type::<GravityScale>()
            .register_type::<Grounded>()
            .register_type::<PhysicsBody>()
            .register_type::<CharacterCollider>()
            .register_type::<PhysicsConfig>()
            .register_type::<CollisionShape>()
            .init_resource::<PhysicsConfig>()
            // Order the four stages inside FixedUpdate so physics runs at a
            // deterministic rate decoupled from the render frame rate.
            .configure_sets(
                FixedUpdate,
                (
                    PhysicsSet::Integrate,
                    PhysicsSet::BlockCollision,
                    PhysicsSet::CharacterCollision,
                    PhysicsSet::Finalise,
                )
                    .chain(),
            )
            .add_plugins((
                IntegrationPlugin,
                BlockCollisionPlugin,
                CharacterCollisionPlugin,
            ));
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------
    // Aabb geometry helpers
    // ------------------------------------------------------------------

    #[test]
    fn aabb_min_max_at_origin() {
        let aabb = Aabb::new(0.3, 0.9, 0.3);
        let origin = Vec3::ZERO;

        let min = aabb.min(origin);
        let max = aabb.max(origin);

        assert!((min.x - (-0.3)).abs() < 1e-5);
        assert!((min.y - 0.0).abs() < 1e-5);
        assert!((min.z - (-0.3)).abs() < 1e-5);

        assert!((max.x - 0.3).abs() < 1e-5);
        assert!((max.y - 1.8).abs() < 1e-5);
        assert!((max.z - 0.3).abs() < 1e-5);
    }

    #[test]
    fn aabb_overlaps_identical_aabbs() {
        let a = Aabb::player();
        let b = Aabb::player();
        assert!(a.overlaps(Vec3::ZERO, &b, Vec3::ZERO));
    }

    #[test]
    fn aabb_no_overlap_far_apart() {
        let a = Aabb::player();
        let b = Aabb::player();
        // 10 units apart — should not overlap.
        assert!(!a.overlaps(Vec3::ZERO, &b, Vec3::new(10.0, 0.0, 0.0)));
    }

    #[test]
    fn aabb_overlap_touching_edge_does_not_overlap() {
        let a = Aabb::new(0.5, 0.5, 0.5);
        let b = Aabb::new(0.5, 0.5, 0.5);
        // Place b exactly adjacent on the X axis: a occupies [-0.5, 0.5] and b [0.5, 1.5].
        // Touching but not overlapping.
        let pen = a.penetration(Vec3::ZERO, &b, Vec3::new(1.0, 0.0, 0.0));
        assert!(pen.is_none(), "touching AABBs should not penetrate");
    }

    #[test]
    fn aabb_penetration_returns_none_when_separated() {
        let a = Aabb::player();
        let b = Aabb::player();
        assert!(
            a.penetration(Vec3::ZERO, &b, Vec3::new(5.0, 0.0, 0.0))
                .is_none()
        );
    }

    #[test]
    fn aabb_penetration_x_axis() {
        let a = Aabb::new(0.5, 0.5, 0.5);
        let b = Aabb::new(0.5, 0.5, 0.5);
        // Overlap by 0.2 on X, more than Y or Z overlap.
        // Positions: a at origin (X: -0.5..0.5), b at (0.8, 0, 0) (X: 0.3..1.3).
        // But Y overlap would be the full 1.0 and Z overlap the full 1.0 — so X
        // (0.2) is smallest → push along X.
        let pen = a
            .penetration(Vec3::ZERO, &b, Vec3::new(0.8, 0.0, 0.0))
            .expect("should overlap");
        // Penetration should be along X and point left (a is to the left of b).
        assert!(pen.y.abs() < 1e-5, "no Y component expected");
        assert!(pen.z.abs() < 1e-5, "no Z component expected");
        assert!(pen.x < 0.0, "should push a to the left");
        assert!((pen.x.abs() - 0.2).abs() < 1e-4);
    }

    #[test]
    fn aabb_penetration_y_axis_when_smallest() {
        let a = Aabb::new(2.0, 0.1, 2.0); // very wide, very short
        let b = Aabb::new(2.0, 0.1, 2.0);
        // Place b slightly above a so Y overlap is tiny, X/Z are huge.
        let pen = a
            .penetration(Vec3::ZERO, &b, Vec3::new(0.0, 0.15, 0.0))
            .expect("should overlap");
        assert!(pen.x.abs() < 1e-5);
        assert!(pen.z.abs() < 1e-5);
        assert!(pen.y < 0.0, "a should be pushed downward");
    }

    // ------------------------------------------------------------------
    // CollisionShape default
    // ------------------------------------------------------------------

    #[test]
    fn collision_shape_default_is_full_cube() {
        assert!(matches!(
            CollisionShape::default(),
            CollisionShape::FullCube
        ));
    }

    // ------------------------------------------------------------------
    // PhysicsConfig defaults sanity
    // ------------------------------------------------------------------

    #[test]
    fn physics_config_defaults_are_sane() {
        let cfg = PhysicsConfig::default();
        assert!(cfg.gravity > 0.0, "gravity should pull downward");
        assert!(
            cfg.terminal_velocity > cfg.gravity,
            "terminal velocity should exceed one second of freefall"
        );
        assert!(cfg.ground_friction >= 0.0);
        assert!(cfg.air_friction >= 0.0);
    }
}
