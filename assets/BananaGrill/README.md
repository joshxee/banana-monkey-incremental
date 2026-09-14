# Banana grill

The standalone grill used by the current baboon chefs:

- `banana-grill.aseprite`: editable master with five layers.
- `banana-grill.png`: transparent 128 × 112 sprite, ground anchor (64,100).

Use nearest-neighbor sampling. The current chef sprites, occupancy composites,
placement metadata and twelve-chef preview are in
[Baboon Chef/v2](../Monkey/Baboon%20Chef/v2/README.md).

Rebuild just the grill from the repository root with `ASEPRITE_BIN` set:

```powershell
sprite-axi run tools/art/banana-grill.lua
```

The recipe checks palette, alpha, margins and source/export agreement. It does
not recreate rejected chef art.
