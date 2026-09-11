# Banana cart

Simple open wooden cart with four solid wooden wheels, a front driver holding a wooden tiller, two seated pedallers, and a rear tipping banana bed. The crew uses the approved Spider Worker's unscaled directional heads and palette, with new seated bodies, bent legs, gripping arms, and curled tails.

## Files

- `banana-cart.aseprite`: layered master, 576 frames, 32 named tags. Each direction has separate deck, wheels, three crew members, cargo bed, and ground shadow layers, ordered for that view.
- `banana-cart-travel_empty.png` / `banana-cart-travel_full.png`: 12 columns × 8 rows, transparent 2496 × 1408 atlases.
- `banana-cart-fill.png` / `banana-cart-offload.png`: 24 columns × 8 rows, transparent 4992 × 1408 atlases.
- `banana-cart.json`: exact rectangles, durations, loop flags, directions, and master frame ranges.
- `preview.html`: playable preview with eight simultaneous views, frame scrubbing, and 1×/2×/3× pixel scale.
- `banana-cart-lifecycle.gif`: looping SE lifecycle beside the unscaled worker, displayed at 2×.
- `banana-cart-contact.png`: native-scale overview. Columns N, NE, E, SE, S, SW, W, NW. Rows empty travel, filling, full travel, offloading.
- `banana-cart-preview.png`: still of the loaded cart beside the worker at 2×.

## Playback

All frames use **208 × 176** cells and ground anchor **(104, 126)**, measured from the top left. Rows follow screen compass **N, NE, E, SE, S, SW, W, NW**; N is screen up and diagonals use the game's 2:1 ground projection. Sample with nearest-neighbor filtering and preserve the whole cell.

| Tag prefix | Frames | Duration | Playback |
| --- | ---: | ---: | --- |
| `travel_empty_` | 12 | 960 ms | Loop |
| `fill_` | 24 | 2400 ms | Once, hold final frame |
| `travel_full_` | 12 | 960 ms | Loop |
| `offload_` | 24 | 2400 ms | Once, hold final frame |

Append the direction, for example `travel_full_SE`. JSON sheet rectangles are zero-based; Aseprite frame ranges are one-based and inclusive. Travel loops have matching pedal and wheel phase across empty/full states. Switch from travel phase zero into fill/offload for an exact seam. Fill begins at empty phase zero and finishes at full phase zero; offload does the reverse. The cart remains anchored while parked and the chassis stays still during travel; runtime translation supplies movement.

The six rendered bunches are visual cargo stages, not an economy capacity declaration. Filling drops bunches into the rear bed. Offloading opens the rear gate, tips the bed, and sends upper bunches down first, then returns to the closed empty pose. Departing fruit is part of the clip and disappears at the receiving edge; there is no persistent ground pile. Default exports include the solid palette-color shadow; the shadow has separate per-direction master layers for export customization.

## Regenerate and verify

From the repository root with `ASEPRITE_BIN` pointing to an installed Aseprite executable:

```sh
sprite-axi run tools/art/banana-cart.lua
```

The full render and verification take about a minute. On slower machines, set `SPRITE_AXI_TIMEOUT=120000` before running to allow two minutes for the bounded export checks.

Inputs are the repository palette, Spider Worker directional Aseprite, and banana-bunch Aseprite. No machine-specific input paths are used. The recipe checks all 576 frames for palette membership, binary transparency, and clipping; verifies 32 exact lifecycle seams; reopens the saved master to compare pixels, tags, and timing; and compares every exported PNG pixel to its source.

Latest verification: 576 frames, 32 clips, 12 canonical colors, binary alpha, inclusive occupied bounds `(13,18)` to `(193,160)`, all lifecycle seams and saved/exported pixels matching. Mathematician, game-designer, and game-developer reviews passed after correcting forward wheel/pedal rotation. Browser verification covered the complete-cycle preview, direction selection, one-shot final-frame hold, and scrubbing. Human motion acceptance is the review below.

For a human motion review, open `assets/BananaCart/preview.html` in a browser, or serve the repository root with `python -m http.server 8765 --bind 127.0.0.1` and visit `http://127.0.0.1:8765/assets/BananaCart/preview.html`. Watch one complete cycle (about nine seconds), then select E, N, and SW and scrub offload. Pass: one driver, two pedallers; feet stay attached to rotating pedals; wheels turn during travel and stop when parked; bananas fill, travel, and exit the rear gate; no cart-anchor jump at state changes. Fail: a missing crew member, detached feet, cargo passing through a closed gate, clipping, or a transition pop.

This is an asset pack; no runtime systems or balance have been changed.
