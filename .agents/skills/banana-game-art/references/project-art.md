# Project art references

Resolve the paths below from the Banana Monkey Incremental repository root, not from this skill directory. Treat existing assets as useful working examples rather than immutable templates: the user's latest visual feedback takes precedence.

## Preserved user reference studies

View these when establishing a new asset's style. These are the screenshots the user supplied to correct the initial town-center direction. Create original assets informed by them.

- [Forest and ruins](hld-forest-ruins.png): broad teal foliage fields, selective underside shadows and recessed architecture. A small warm character reads against quiet cool ground. Observe how little enclosing outline work the environment needs.
- [Red forest](hld-red-forest.png): expressive nonliteral color, strong silhouettes, substantial torn leaf clusters, hanging growth, and calm interior areas. Color relationships matter more than realistic tree color.
- [Snow clearing](hld-snow-clearing.png): generous quiet space, sharp value separation, and sparse saturated accents. The small characters remain readable among larger environmental masses.

The desired game mood is friendlier and cuter than these scenes. Their graphic clarity is the reference, not a mandate for gloom or copied motifs.

## Canonical local inputs

| Input | Repository path | How to use it |
| --- | --- | --- |
| Palette | `docs/references/palette.png` | Full 31-color source palette. Stay with these colors unless the current request changes the direction; naturalistic color is not a requirement. |
| Worker snapshot | `docs/references/spider-worker.png` | Approved 64 x 64 Spider Worker standing study. Its visible silhouette is 38 x 55 including the tail; the standing body is roughly 44 px high. Copy without rescaling into a comparison. |
| Preferred character source when present | `assets/Monkey/Spider Worker/` | Use the latest approved source here if it has since been added or revised. The stored snapshot was copied from `spider_monkey_idle.png` in another worktree. |
| Older character assets | `assets/Monkey/Character Aseprites/` | Not the Spider Worker scale reference. Using their 32 x 32 frame previously caused a wrong-size building. |
| World layout study | `docs/references/lofi-isometric.png` | Useful for broad layout and perspective. It does not override the newer pixel-art style references. |

Avoid hardcoded machine or other-worktree paths in new recipes. The repository snapshot exists so local regeneration does not depend on another checkout.

## Approved working examples

| Asset | Visual evidence | Implementation / placement |
| --- | --- | --- |
| Town center | `assets/TownCenter/town-center-scale-preview.png` and `town-center.png` | `tools/art/town-center.lua`; 672 x 704 master, ground origin (330, 440), 54 px entrance. Approximately a four-bedroom monkey house, not a measured interior plan. |
| Reduced town idle | `assets/TownCenter/town-center-idle-preview.gif` | `tools/art/town-center-idle.lua`; 16 frames / 2.4 s; half the original motion strength, about 1-2 px at outer boughs, no visible vertical bob. |
| Jungle and banana plants | `assets/Jungle/jungle-scale-preview.png` | `tools/art/jungle.lua`; five static assets, 320 x 352 cells, shared ground anchor (160, 316). |
| Very slight plant idles | `assets/Jungle/jungle-idle-preview.gif` | `tools/art/jungle-idle.lua`; 16 frames / 3.2 s, at most 1 px horizontal tip displacement; roots, fruit and structure steady. |

The assets' READMEs and JSON files contain the exact export rectangles, timings, layers, and bounds. Read them when adapting or integrating an existing asset. Canvas sizes, frame counts, and loop periods are examples for those assets, not universal art requirements.

## Lessons from the actual revisions

- The first town-center draft had promising scale and projection but failed stylistically. Thick dark borders, evenly traced roof tiles, faceted canopy highlights, and mechanically repeated details made it read as a blocky 3D model. Passing geometry and palette checks did not make it good art.
- The accepted overhaul used quieter color masses, an exposed forked trunk, uneven sagging roof fronds, a small stitched repair, subdued windows, and brighter bananas against cool bins. Workers were shown on the deck and beside the stairs and bins, not only isolated on an unrelated background.
- The user later asked to reduce idle movement. Keep the forest alive through subtle local motion and varied forms, rather than moving every object or making the entire crown bounce.
- Fruiting and harvested banana plants share the same plant pixels and anchor outside the fruit/cut-stalk region. Their animation phases stay aligned as well.

## Drawing helper detail

Sprite-axi's polygon helper also draws its edges using an integer Bresenham line loop. Passing fractional endpoints caused a nonterminating render in this project. Snap vertices before calling it, even if the intended polygon is filled:

```lua
local snapped = {}
for _,p in ipairs(points) do
  snapped[#snapped+1] = {math.floor(p[1]), math.floor(p[2])}
end
axi.poly(cel, snapped, color, {fill=true})
```

Run recipes from the repository root with `ASEPRITE_BIN` set to the available Aseprite executable. If a render hangs, inspect the script and confirm that the old run has stopped before starting another writer against the same files. Do not just raise a timeout for an unbounded drawing loop.
