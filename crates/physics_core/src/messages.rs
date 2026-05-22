//! Collision-contact messages emitted by the physics simulation.
//!
//! These messages are written **once per [`FixedUpdate`] tick** by
//! `dd40_physics`. Any system can subscribe with a
//! [`bevy::ecs::message::MessageReader`] to react to contacts — pickup
//! systems, audio cues, merging logic for loose items, etc.
//!
//! # Normal convention
//!
//! For both message types, `normal` is a unit vector pointing **from
//! the other object toward the body** (outward of the obstacle).
//! For a body resting on top of a block, `normal == +Y`.  For a body
//! pressed against the east face of a wall, `normal == -X`.
//!
//! # Determinism
//!
//! For [`BodyBodyContact`], the entity with the lower
//! [`Entity::index()`] is always placed in `a`.  This keeps
//! downstream pair handlers from having to deduplicate `(a, b)` vs
//! `(b, a)`.

use bevy::ecs::message::Message;
use bevy::prelude::*;
use dd40_core::block::BlockPos;

/// Emitted when a [`crate::components::PhysicsBody`]'s AABB is
/// touching a solid block face after block-collision resolution.
///
/// A resting body produces one message per touching face per tick
/// (typically one — the block underneath).  Walking along a wall
/// produces a stream of contacts on the wall's face.
#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub struct BodyBlockContact {
    /// The body that is touching a block face.
    pub body: Entity,
    /// The block being touched.
    pub block: BlockPos,
    /// Unit normal pointing from the block toward the body.
    pub normal: Vec3,
    /// How deeply the AABB penetrates the block face along `normal`,
    /// clamped to ≥ 0.  Typically `0.0` for a flush contact; positive
    /// values mean the contact-detection pass found residual overlap
    /// (rare, indicates a sweep edge case).
    pub penetration: f32,
}

/// Emitted when two [`crate::components::PhysicsBody`] entities overlap
/// during body-collision resolution.
///
/// The lower-indexed entity is always in `a`; this means downstream
/// pair handlers (pickup, merging, damage) never see both orderings.
#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub struct BodyBodyContact {
    /// Lower-indexed body in the pair.
    pub a: Entity,
    /// Higher-indexed body in the pair.
    pub b: Entity,
    /// Unit normal pointing from `b` toward `a`.
    pub normal: Vec3,
    /// Penetration depth along `normal` at the moment of detection.
    pub penetration: f32,
}

impl BodyBodyContact {
    /// Builds a contact with `a` / `b` ordered by [`Entity::index()`],
    /// flipping `normal` if the caller passed the pair in the opposite
    /// order.  Use this from any system that emits contacts so the
    /// ordering invariant is upheld consistently.
    pub fn new(a: Entity, b: Entity, normal_from_b_to_a: Vec3, penetration: f32) -> Self {
        if a.index() <= b.index() {
            Self {
                a,
                b,
                normal: normal_from_b_to_a,
                penetration,
            }
        } else {
            Self {
                a: b,
                b: a,
                normal: -normal_from_b_to_a,
                penetration,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_body_contact_new_orders_by_entity_index_and_flips_normal() {
        let lo = Entity::from_raw_u32(5).unwrap();
        let hi = Entity::from_raw_u32(42).unwrap();
        let normal = Vec3::new(1.0, 0.0, 0.0);

        let c1 = BodyBodyContact::new(lo, hi, normal, 0.5);
        assert_eq!(c1.a, lo);
        assert_eq!(c1.b, hi);
        assert_eq!(c1.normal, normal);

        let c2 = BodyBodyContact::new(hi, lo, normal, 0.5);
        assert_eq!(c2.a, lo);
        assert_eq!(c2.b, hi);
        assert_eq!(c2.normal, -normal);
        assert_eq!(c2.penetration, 0.5);
    }
}
