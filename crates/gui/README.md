# dd40_gui

In-game HUD crate for dd40. Provides the crosshair and any other persistent
heads-up-display elements needed during gameplay.

Depends only on `dd40_core`. Replace this crate with a custom HUD by swapping
the plugin in `dd40_client`.

## Module overview

```
src/
├── lib.rs          — module declarations
├── plugin.rs       — GuiPlugin
└── crosshair.rs    — Crosshair UI element
```
