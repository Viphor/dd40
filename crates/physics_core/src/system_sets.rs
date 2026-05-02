use bevy::prelude::*;

/// Labels the ordered stages of one physics tick.
///
/// Configure your own systems against these labels to hook into the pipeline.
///
/// **Expected order:**
/// `InputSync` → `Integrate` → `BlockCollision` → `CharacterCollision` → `Finalise`
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum PhysicsSet {
    /// External input injection phase.
    ///
    /// Network systems (and any other non-local input source) that write to
    /// [`CharacterInput`] must run here so the character controller always
    /// sees up-to-date intent before translating it into physics impulses.
    InputSync,
    /// Apply external forces (gravity, impulses) and integrate velocity into
    /// a **tentative** new position refined by the collision stages.
    Integrate,
    /// Resolve the tentative position against the solid block grid.
    BlockCollision,
    /// Push overlapping character colliders apart.
    CharacterCollision,
    /// Copy the resolved tentative position back into [`Transform`] and
    /// clear per-frame transient state.
    Finalise,
}
