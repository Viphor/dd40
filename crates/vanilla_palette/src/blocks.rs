//! Vanilla block definitions.
//!
//! Provides [`VanillaBlocksPlugin`] (registers in [`BlockRegistrySet`]) and
//! the [`VanillaBlocks`] constant struct for access to vanilla [`BlockId`]
//! values elsewhere.
//!
//! # Block IDs
//!
//! | Constant              | ID |
//! |-----------------------|----|
//! | `VanillaBlocks::AIR`  | 0  |
//! | `VanillaBlocks::STONE`| 1  |
//! | `VanillaBlocks::DIRT` | 2  |
//! | `VanillaBlocks::GRASS`| 3  |
//! | `VanillaBlocks::SAND` | 4  |
//! | `VanillaBlocks::WOOD` | 5  |
//! | `VanillaBlocks::LEAVES`| 6 |
//! | `VanillaBlocks::COBBLESTONE`| 7 |
//!
//! Custom-crate blocks should start at ID `1000` to leave room for future
//! vanilla additions.
//!
//! # Optional `textures` feature
//!
//! When the `textures` Cargo feature is enabled, every renderable vanilla
//! block additionally gets a [`BlockTextures`] entry attached via
//! [`BlockDefinition::with_data`].  The texture names follow the Minecraft
//! convention `minecraft:block/<block_name>` so a vanilla Minecraft
//! resource pack drops in unmodified.  Per-face textures (different top
//! / bottom / sides) are applied where appropriate (currently: `grass`,
//! `wood`).
//!
//! Without the feature the crate behaves exactly as before — blocks
//! ship colour-only and no `dd40_texture_core` dependency is pulled in.
//!
//! [`BlockRegistrySet`]: dd40_core::block::registry::BlockRegistrySet
//! [`BlockTextures`]: dd40_texture_core::BlockTextures

use bevy::prelude::*;
use dd40_core::{
    block::{
        BlockId,
        registry::{BlockDefinition, BlockRegistrySet},
    },
    prelude::*,
};
use dd40_loot_core::table::{LootEntry, LootTable};

use crate::items::VanillaItems;
use crate::tools::VanillaToolKinds;

// ── Constants ─────────────────────────────────────────────────────────────────

/// [`BlockId`] constants for the vanilla blocks.
pub struct VanillaBlocks;

impl VanillaBlocks {
    /// Air — the engine invariant (registered by `CorePlugin`, re-exported here
    /// for convenience).
    pub const AIR: BlockId = BlockId(0);
    /// Stone — mined with a pickaxe.
    pub const STONE: BlockId = BlockId(1);
    /// Dirt — mined with a shovel.
    pub const DIRT: BlockId = BlockId(2);
    /// Grass — mined with a shovel.
    pub const GRASS: BlockId = BlockId(3);
    /// Sand — mined with a shovel.
    pub const SAND: BlockId = BlockId(4);
    /// Wood (log) — mined with an axe.
    pub const WOOD: BlockId = BlockId(5);
    /// Leaves — mined with shears (or bare hands, slowly).
    pub const LEAVES: BlockId = BlockId(6);
    /// Cobblestone — drops when stone is broken without silk touch.
    pub const COBBLESTONE: BlockId = BlockId(7);
}

// ── Texture helpers ───────────────────────────────────────────────────────────

/// Extension trait used during vanilla block registration so call sites
/// can chain `.with_vanilla_texture(...)` without `#[cfg]` litter.
///
/// With the `textures` feature: attaches a [`BlockTextures`] with the
/// given single name for all six faces.
///
/// Without the feature: a no-op that returns `self` unchanged.
trait BlockDefinitionTextureExt: Sized {
    /// Attach a single texture name to all six faces (or no-op without
    /// `textures`).
    ///
    /// `tinted` controls whether the renderer multiplies the per-block
    /// colour into the sampled texel.  Use `true` for foliage-style
    /// blocks (grass, leaves, water) and `false` for everything else.
    fn with_vanilla_texture(self, name: &str, tinted: bool) -> Self;

    /// Attach distinct top, bottom and side textures (or no-op without
    /// `textures`).  `tinted` has the same meaning as on
    /// [`Self::with_vanilla_texture`].
    fn with_vanilla_pillar_texture(
        self,
        top: &str,
        bottom: &str,
        sides: &str,
        tinted: bool,
    ) -> Self;

    /// Attach the grass-block texture set: a tinted overlay on the
    /// four side faces and a tinted top, with an untinted bottom.
    ///
    /// `top` is sampled and multiplied by the per-block tint (matches
    /// Minecraft's biome-coloured `grass_block_top`).  Each side face
    /// uses `sides` as the base (shown as-authored) and
    /// `sides_overlay` as an alpha-masked overlay whose RGB is
    /// multiplied by the per-block tint before being composited on top.
    /// `bottom` is shown untinted.
    ///
    /// Without the `textures` feature: returns `self` unchanged.
    fn with_vanilla_grass_textures(
        self,
        top: &str,
        bottom: &str,
        sides: &str,
        sides_overlay: &str,
    ) -> Self;
}

#[cfg(feature = "textures")]
impl BlockDefinitionTextureExt for BlockDefinition {
    fn with_vanilla_texture(self, name: &str, tinted: bool) -> Self {
        use dd40_texture_core::{BlockTextures, TextureRef};
        self.with_data(
            BlockTextures::all(TextureRef::named(format!("minecraft:block/{name}")))
                .with_tint(tinted),
        )
    }

    fn with_vanilla_pillar_texture(
        self,
        top: &str,
        bottom: &str,
        sides: &str,
        tinted: bool,
    ) -> Self {
        use dd40_texture_core::{BlockTextures, TextureRef};
        self.with_data(
            BlockTextures::top_bottom_sides(
                TextureRef::named(format!("minecraft:block/{top}")),
                TextureRef::named(format!("minecraft:block/{bottom}")),
                TextureRef::named(format!("minecraft:block/{sides}")),
            )
            .with_tint(tinted),
        )
    }

    fn with_vanilla_grass_textures(
        self,
        top: &str,
        bottom: &str,
        sides: &str,
        sides_overlay: &str,
    ) -> Self {
        use dd40_texture_core::{BlockTextures, Face, TextureRef};
        // Per-face tint model for grass:
        // - Top is greyscale `grass_block_top` and must be tinted by
        //   the biome colour (whole-output tint).
        // - Sides have a base (`grass_block_side`, shown as-authored)
        //   plus a greyscale overlay (`grass_block_side_overlay`)
        //   whose RGB is multiplied by the biome colour and composited
        //   on top.  The whole-output side tint must therefore be off.
        // - Bottom is plain `dirt`, untinted.
        self.with_data(
            BlockTextures::top_bottom_sides(
                TextureRef::named(format!("minecraft:block/{top}")),
                TextureRef::named(format!("minecraft:block/{bottom}")),
                TextureRef::named(format!("minecraft:block/{sides}")),
            )
            .with_side_overlay(TextureRef::named(format!(
                "minecraft:block/{sides_overlay}"
            )))
            .with_tint(false)
            .with_tint_for(Face::Top, Some(true)),
        )
    }
}

#[cfg(not(feature = "textures"))]
impl BlockDefinitionTextureExt for BlockDefinition {
    fn with_vanilla_texture(self, _name: &str, _tinted: bool) -> Self {
        self
    }
    fn with_vanilla_pillar_texture(
        self,
        _top: &str,
        _bottom: &str,
        _sides: &str,
        _tinted: bool,
    ) -> Self {
        self
    }
    fn with_vanilla_grass_textures(
        self,
        _top: &str,
        _bottom: &str,
        _sides: &str,
        _sides_overlay: &str,
    ) -> Self {
        self
    }
}

// ── Registration system ───────────────────────────────────────────────────────

fn register_vanilla_blocks(mut registry: ResMut<BlockRegistry>, mut commands: Commands) {
    // Air (ID 0) is already registered by CorePlugin — skip it.

    registry.register(
        BlockDefinition::new(VanillaBlocks::STONE, "stone")
            .with_color(Color::srgb(0.5, 0.5, 0.5))
            .with_solid(true)
            .with_renderable(true)
            .with_toughness(1.5)
            .with_preferred_tool(VanillaToolKinds::PICKAXE)
            .with_vanilla_texture("stone", false)
            .with_data(LootTable::with_entries(vec![LootEntry::Fixed {
                item: VanillaItems::COBBLESTONE,
                count: 1,
            }])),
        &mut commands,
    );

    registry.register(
        BlockDefinition::new(VanillaBlocks::DIRT, "dirt")
            .with_color(Color::srgb(0.6, 0.4, 0.2))
            .with_solid(true)
            .with_renderable(true)
            .with_toughness(0.5)
            .with_preferred_tool(VanillaToolKinds::SHOVEL)
            .with_vanilla_texture("dirt", false),
        &mut commands,
    );

    registry.register(
        BlockDefinition::new(VanillaBlocks::GRASS, "grass")
            .with_color(Color::srgb(0.2, 0.8, 0.2))
            .with_solid(true)
            .with_renderable(true)
            .with_toughness(0.6)
            .with_preferred_tool(VanillaToolKinds::SHOVEL)
            .with_vanilla_grass_textures(
                "grass_block_top",
                "dirt",
                "grass_block_side",
                "grass_block_side_overlay",
            ),
        &mut commands,
    );

    registry.register(
        BlockDefinition::new(VanillaBlocks::SAND, "sand")
            .with_color(Color::srgb(0.9, 0.85, 0.6))
            .with_solid(true)
            .with_renderable(true)
            .with_toughness(0.5)
            .with_preferred_tool(VanillaToolKinds::SHOVEL)
            .with_vanilla_texture("sand", false),
        &mut commands,
    );

    registry.register(
        BlockDefinition::new(VanillaBlocks::WOOD, "wood")
            .with_color(Color::srgb(0.55, 0.35, 0.2))
            .with_solid(true)
            .with_renderable(true)
            .with_toughness(2.0)
            .with_preferred_tool(VanillaToolKinds::AXE)
            .with_vanilla_pillar_texture("oak_log_top", "oak_log_top", "oak_log", false),
        &mut commands,
    );

    registry.register(
        BlockDefinition::new(VanillaBlocks::LEAVES, "leaves")
            .with_color(Color::srgb(0.1, 0.6, 0.1))
            .with_solid(true)
            .with_renderable(true)
            .with_toughness(0.2)
            .with_preferred_tool(VanillaToolKinds::SHEARS)
            .with_vanilla_texture("oak_leaves", true),
        &mut commands,
    );

    registry.register(
        BlockDefinition::new(VanillaBlocks::COBBLESTONE, "cobblestone")
            .with_color(Color::srgb(0.4, 0.4, 0.4))
            .with_solid(true)
            .with_renderable(true)
            .with_toughness(2.0)
            .with_preferred_tool(VanillaToolKinds::PICKAXE)
            .with_vanilla_texture("cobblestone", false),
        &mut commands,
    );
}

// ── Plugin ────────────────────────────────────────────────────────────────────

/// Plugin that registers all vanilla block types during [`BlockRegistrySet`].
///
/// Added automatically by [`VanillaPalettePlugin`]; you can also add it
/// directly if you only want the vanilla blocks without the vanilla tools.
///
/// [`VanillaPalettePlugin`]: crate::VanillaPalettePlugin
pub struct VanillaBlocksPlugin;

impl Plugin for VanillaBlocksPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(feature = "textures")]
        dd40_core::ensure_plugins!(app, dd40_texture_core::TextureCorePlugin);
        app.add_systems(Startup, register_vanilla_blocks.in_set(BlockRegistrySet));
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use dd40_core::tools::ToolRegistry;

    fn make_test_app() -> App {
        let mut app = App::new();
        app.insert_resource(BlockRegistry::new());
        app.insert_resource(ToolRegistry::new());
        // Register shears kind so preferred_tool references are valid
        app.add_systems(Startup, register_vanilla_blocks.in_set(BlockRegistrySet));
        app.configure_sets(Startup, BlockRegistrySet);
        app
    }

    #[test]
    fn vanilla_blocks_registered() {
        let mut app = make_test_app();
        app.update();

        let registry = app.world().resource::<BlockRegistry>();

        let stone = registry.get(VanillaBlocks::STONE).unwrap();
        assert_eq!(stone.name, "stone");
        assert!(stone.is_solid);
        assert!(stone.is_renderable);
        assert!(stone.is_destructible);
        assert!((stone.toughness - 1.5).abs() < 1e-6);

        let leaves = registry.get(VanillaBlocks::LEAVES).unwrap();
        assert_eq!(leaves.name, "leaves");
        assert!((leaves.toughness - 0.2).abs() < 1e-6);
    }

    #[test]
    fn all_vanilla_block_ids_exist() {
        let mut app = make_test_app();
        app.update();

        let registry = app.world().resource::<BlockRegistry>();
        for id in [
            VanillaBlocks::AIR,
            VanillaBlocks::STONE,
            VanillaBlocks::DIRT,
            VanillaBlocks::GRASS,
            VanillaBlocks::SAND,
            VanillaBlocks::WOOD,
            VanillaBlocks::LEAVES,
            VanillaBlocks::COBBLESTONE,
        ] {
            assert!(registry.get(id).is_some(), "Block {:?} not registered", id);
        }
    }

    #[test]
    fn stone_loot_table_drops_cobblestone() {
        use rand::SeedableRng;
        use rand::rngs::StdRng;
        let mut app = make_test_app();
        app.update();

        let registry = app.world().resource::<BlockRegistry>();
        let table = registry
            .block_data::<LootTable>(VanillaBlocks::STONE)
            .expect("stone should have a LootTable attached");
        let mut rng = StdRng::seed_from_u64(0);
        let stacks = table.roll(&mut rng);
        assert_eq!(stacks.len(), 1);
        assert_eq!(stacks[0].item, VanillaItems::COBBLESTONE);
    }

    #[cfg(feature = "textures")]
    #[test]
    fn renderable_blocks_have_block_textures_attached() {
        use dd40_texture_core::{BlockTextures, Face, TextureRef};
        let mut app = make_test_app();
        app.update();
        let registry = app.world().resource::<BlockRegistry>();

        let stone_tex = registry
            .block_data::<BlockTextures>(VanillaBlocks::STONE)
            .expect("stone should have BlockTextures attached");
        assert_eq!(
            stone_tex.get(Face::Top),
            Some(&TextureRef::named("minecraft:block/stone"))
        );
        assert!(stone_tex.is_complete());

        let grass_tex = registry
            .block_data::<BlockTextures>(VanillaBlocks::GRASS)
            .unwrap();
        assert_eq!(
            grass_tex.get(Face::Top),
            Some(&TextureRef::named("minecraft:block/grass_block_top"))
        );
        assert_eq!(
            grass_tex.get(Face::Bottom),
            Some(&TextureRef::named("minecraft:block/dirt"))
        );
        assert_eq!(
            grass_tex.get(Face::North),
            Some(&TextureRef::named("minecraft:block/grass_block_side"))
        );
        // Grass uses the overlay-compositing model: side faces have an
        // overlay texture, the top face is whole-output tinted (greyscale
        // greenery), and sides+bottom are NOT whole-output tinted (the
        // overlay handles biome tinting for sides; bottom is plain dirt).
        assert_eq!(
            grass_tex.overlay(Face::North),
            Some(&TextureRef::named(
                "minecraft:block/grass_block_side_overlay"
            ))
        );
        assert!(grass_tex.tinted_for(Face::Top));
        assert!(!grass_tex.tinted_for(Face::Bottom));
        assert!(!grass_tex.tinted_for(Face::North));
    }

    #[test]
    fn air_is_not_destructible() {
        let registry = BlockRegistry::new();
        let air = registry.get(VanillaBlocks::AIR).unwrap();
        assert!(!air.is_destructible);
    }
}
