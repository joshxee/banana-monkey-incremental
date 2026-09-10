# Banana bunch

A compact, three-finger bunch in the project palette: warm gold faces, ochre undersides, small cream highlights and a green cut stalk. The fruit is 25 × 22 native pixels, around half the Spider Worker's standing body height. Quiet overlapping crescents keep the bunch readable at 1:1.

The new set is `banana-bunch.*`; the older `Banana.aseprite` and `Banana.png` remain the existing single-banana study. These assets are ready for integration; this change does not modify game runtime code.

| State | Export | Frames | Timing | Playback |
| --- | --- | --- | --- | --- |
| Still | `banana-bunch-still.png` | 1 | Hold indefinitely | Static |
| Spawn | `banana-bunch-spawn-sheet.png` | 9 | 600 ms | Once, then idle or still |
| Idle | `banana-bunch-idle-sheet.png` | 12 | 2800 ms | Loop |
| Despawn | `banana-bunch-despawn-sheet.png` | 8 | 490 ms | Once, then remove |

Spawn grows into a brief squash and settles. Idle holds the silhouette and ground contact still while a two-pixel glint passes along the front fruit. Despawn gives a short anticipation, lifts and shrinks, then leaves two small flecks before becoming fully transparent. Spawn and despawn are actions, not seamless loops.

## Files and placement

- `banana-bunch.aseprite`: editable six-layer master, with `still`, `spawn`, `idle`, and `despawn` tags. Shadow, three fruit fingers, stalk, and effects are separate layers.
- `banana-bunch.json`: frame rectangles, individual durations, loop flags, and source tag ranges. `asepriteFrames` uses Aseprite's **one-based inclusive** numbering; sheet frames are in zero-based, left-to-right order.
- Every game frame uses a **48 × 48** transparent cell and **(24, 36)** ground anchor measured from the top-left. Keep full cells and use nearest-neighbor sampling. Do not independently trim or center frames.
- Exports include the opaque palette-colored ground shadow on its own source layer. Hide `01 Ground shadow` in the master when an engine supplies its own shadow.
- `banana-bunch-native-preview.png`: exact 1:1 worker comparison; the worker is copied without resizing.
- `banana-bunch-preview.png`, `.gif`, and `.aseprite`: review-only 4× nearest-neighbor worker comparison and full lifecycle. Backdrop and worker are excluded from all game PNGs and the game master. The GIF pauses for 700 ms after disappearing for review; game despawn ends with its documented 100 ms empty frame.

## Regeneration and checks

From the repository root, set `ASEPRITE_BIN` to your installed executable, then run:

```text
sprite-axi run tools/art/bananas.lua
```

The Lua recipe checks all 30 frames against the canonical 31-color palette, binary transparency, clear margins, PNG/source pixel agreement, saved-master pixel agreement, saved tag ranges and timings, fixed idle silhouette, and matching state endpoints. Six palette colors are used. Spawn's last frame, idle's first and last frames, and despawn's first frame equal the still exactly. Spawn's first and despawn's last frames are empty.

For a human visual review on Windows, run this from the repository root:

```powershell
Start-Process -FilePath (Resolve-Path 'assets/Banana/banana-bunch-preview.gif').Path
```

Watch two cycles (about 9 seconds), and inspect `banana-bunch-native-preview.png` at 100%. Pass: the bunch reads as bananas beside the actual worker; the spawn settles without an anchor jump; the idle stays planted with a slight highlight; despawn clears the fruit, shadow, and flecks. Fail: a clipped edge, vibrating outline, distracting idle flash, or residue after despawn. This is an asset preview, not an in-game playtest.
