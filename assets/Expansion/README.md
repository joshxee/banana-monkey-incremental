# Jungle village expansion

Nine static assets, built sequentially with sprite-axi. The existing approved art is unchanged.

| Asset | Purpose |
| --- | --- |
| fan-palm | Narrow curved trunk, radial fan leaves |
| buttress-tree | Broad rooted trunk, tiered dark canopy |
| bamboo | Thin jointed culms and open leaf sprays |
| vine-tree | Forked trunk and hanging leafy vines |
| fallen-log | Split wood and mossy low cover |
| deep-jungle-floor | Quiet darker-green forest ground |
| research-hut | Small leaf-roof laboratory with telescope |
| distribution-center | Open sorting shed, crates and loading ramp |
| chef-kitchen | Open preparation counters framing the approved baboon chefs and grill |

## Source and regeneration

Every asset has a layered `.aseprite` master and transparent `.png`, plus a `-preview.png` with the unchanged 64x64 Spider Worker reference. `contact-sheet.png` is an art comparison, not a runtime screenshot.

Run from the repository root with sprite-axi and Aseprite installed:

```powershell
$env:JUNGLE_ASSET='fan-palm'
sprite-axi run tools/art/expansion.lua
sprite-axi run tools/art/expansion-preview.lua
```

On POSIX: `JUNGLE_ASSET=fan-palm sprite-axi run tools/art/expansion.lua`.
Repeat for the names in the table before regenerating the contact sheet.
The recipe validates canonical palette membership, binary transparency, unclipped margins (except the tiling floor), and exact source/export agreement on every run.

## Placement contract

All standing assets: canvas 320x352, ground anchor (160,316), `ART_SCALE=0.5`, no independent resizing. Buildings stand in the town as decorative architecture; existing support stations, purchases, research, routes and economy are unchanged. They have no new buttons or collision.

The 128x64 floor tile uses the same art scale: one diamond spans a 2x2 group of board tiles. One mesh overlays deep jungle only, preserving main's ground atlas on open terrain and forest edges. Staggered deterministic interior planting supplements the existing forest edge. The simulation map is unchanged.

## Human visual acceptance

`./play --scenario jungle-village --speed 1` (Windows: `cargo play --scenario jungle-village --speed 1`). Watch 30 seconds; pan around the town and toward the worked grove, then drag a home banana into the bins.

Pass: all five new jungle forms can be found; foliage fills the depths; dark ground lies only in the jungle; all three buildings are readable; workers, support silhouettes, home banana and drop target stay clear; delivery still works. Fail: missing textures, clipped exports, buildings covering interactive targets, floating feet, or scenery hiding the working route. Check portrait and landscape via `./serve` with `?scenario=jungle-village&speed=1` for final touch acceptance.

Human visual acceptance remains outstanding until a person runs this check.

## Rendering and review evidence

The three buildings have separate `-ground.png` and `-structure.png` exports. Floors render below actors; only standing props participate in depth sorting. `kitchen-context.png` is a static native-pixel study of three chefs at the runtime station coordinates, not a game screenshot. This separation fixes the designer's finding that the combined kitchen floor could hide rear chefs' feet.

`manifest.json` records dimensions, anchors, opaque bounds and layer counts. Run `sprite-axi run tools/art/verify-expansion.lua` to revalidate all current masters without redrawing them, and `sprite-axi run tools/art/kitchen-context.lua` to reproduce the kitchen study.

Mathematician, game-developer and game-designer reviews: PASS after the floor-layer fix. Runtime visual acceptance is still separate from these reviews.
