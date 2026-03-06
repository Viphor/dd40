# Debug UI Quick Start

## Add to Your App

```rust
use dd40_debug_ui::DebugUiPlugin;

App::new()
    .add_plugins(DefaultPlugins)
    .add_plugins(DebugUiPlugin)  // Add this line
    .run();
```

## What You Get

An FPS counter in the top-left corner that shows:
- **Green text** = Good performance (≥60 FPS)
- **Yellow text** = Moderate performance (30-59 FPS)
- **Red text** = Low performance (<30 FPS)

## Run the Demo

```bash
cargo run --example debug_ui_demo
```

## That's It!

The FPS counter appears automatically. No configuration needed.

## Coming Soon

Future debug UI features:
- Player position/velocity
- Block under cursor
- Chunk coordinates
- Memory usage
- Entity count
- Toggle with F3 key
- Debug console

See [README.md](README.md) for more details.