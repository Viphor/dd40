# Debug UI Crate

This crate provides debug UI elements for the dd40 game client.

## Features

- **FPS Counter**: Displays current frames per second in the top-left corner
  - Green text for good performance (≥60 FPS)
  - Yellow text for moderate performance (30-59 FPS)
  - Red text for low performance (<30 FPS)

- **Block Statistics**: Displays information about blocks in the world
  - **Loaded Blocks**: Total number of non-air blocks currently loaded in the world (light blue text)
  - **Rendered Blocks**: Number of blocks actually being rendered (orange text)
  - **Culling Efficiency**: Percentage of loaded blocks that are culled (not rendered) due to occlusion (cyan text)

## Usage

Add the `DebugUiPlugin` to your Bevy app:

```rust
use bevy::prelude::*;
use dd40_debug_ui::DebugUiPlugin;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(DebugUiPlugin)
        .run();
}
```

The FPS counter will automatically appear in the top-left corner of the window.

## Implementation Details

The plugin:
1. Adds Bevy's `FrameTimeDiagnosticsPlugin` for frame timing data
2. Creates a UI overlay with debug text elements arranged in a column
3. Updates the FPS display every frame with smoothed values
4. Color-codes the FPS based on performance thresholds
5. Reads `BlockStatistics` resource from the world crate to display block counts
6. Calculates and displays culling efficiency percentage

## Future Enhancements

Planned additions to the debug UI:
- Position/velocity display
- Block type under cursor
- Chunk coordinates
- Memory usage
- Entity count
- Toggle visibility with hotkey (F3)
- Performance graphs
- Debug console
- Networking statistics
- World generation progress

## Performance

The debug UI has minimal performance impact:
- Single UI container with four text elements
- Updates only when diagnostics are available
- Uses Bevy's built-in diagnostics system
- Block statistics are updated via efficient query counting

## Dependencies

- `bevy` - Core game engine
- `dd40_world` - For accessing `BlockStatistics` resource