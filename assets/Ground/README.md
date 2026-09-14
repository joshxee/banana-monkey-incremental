# Jungle and town ground

128 × 64 pixel isometric floor tiles, authored in sprite-axi / Aseprite Lua. Pale sage jungle soil and muted clay town dirt keep the dark Spider Worker readable and reserve bright gold for bananas. The Hyper Light Drifter studies informed broad quiet fields, sparse flat clusters, expressive color relationships, and broken material edges. Ground uses only four colors from the project's canonical palette; there are no gradients, outlines, bevels, or partial alpha.

## Files

- `jungle-floor.png` / `dirt-floor.png`: convenient individual base tiles (variant 2).
- `ground-atlas.png`: 1024 × 512 transparent atlas, 8 columns × 8 rows.
- `ground.aseprite`: editable atlas with separate terrain, border moss, and surface-detail layers.
- `ground-atlas.json`: authoritative rectangles, corner bits, variants, filenames, and placement metadata.
- `tiles/ground-MM-vV.png`: 64 individual untrimmed tiles, all 16 terrain combinations with four detail variations each.
- `ground-context.png` / `.aseprite`: native-scale comparison with existing vegetation, five unchanged Spider Workers and banana references. Review props are excluded from tile exports.
- `ground-assembled.png`: the same ground with all review props removed.
- `preview.html`: local art inspection with native and enlarged views.
- `validation.txt`: checks emitted by the drawing recipe.

## Joining tiles

Assign jungle or dirt to **shared grid vertices**, then select each tile from its four corners. Set bits for dirt, clear bits for jungle:

| Corner | Bit | Grid vertex |
| --- | --- | --- |
| Top | 1 | `(i, j)` |
| Right | 2 | `(i+1, j)` |
| Bottom | 4 | `(i+1, j+1)` |
| Left | 8 | `(i, j+1)` |

Mask 0 is all jungle; mask 15 is all dirt. The remaining masks cover straight margins, outer and inner corners, and opposite-corner junctions. At masks 5 and 10, dirt connects through the center. One shared vertex grid guarantees compatible neighboring masks; arbitrary independently selected masks do not.

Choose detail variant 0–3 independently per tile. Variant 0 is completely quiet on the two base materials; the other variants add leaf fragments or swept soil marks. All variants preserve exactly the same boundary strips. Use a stable spatial hash or authored placement to avoid a visible repeating sequence. Border moss is deliberately broken and does not make a raised lip.

For a screen-space top grid vertex `(originX, originY)`, place the tile image's top-left at:

```text
x = originX + 64 * (i - j) - 64
y = originY + 32 * (i + j)
```

The image-center anchor is `(64, 32)`. Grid steps are `(64, 32)` and `(-64, 32)`. Each tile has exactly 4,096 opaque pixels. Keep the full 128 × 64 canvas: transparent triangle corners are intentional. Use nearest-neighbor filtering, pixel-aligned placement and no mipmaps; the packed atlas has no filter-extrusion gutters. Render ground below props and characters, at the same native pixel scale. The worker's 64 × 64 canvas contains a 38 × 55 silhouette including its tail, with a roughly 44-pixel standing body.

## Regenerate and inspect

From the repository root, with `ASEPRITE_BIN` set to your installed Aseprite executable:

```powershell
sprite-axi run tools/art/ground.lua
Start-Process (Resolve-Path 'assets/Ground/preview.html').Path
```

Spend about 30 seconds on **With subjects** at 1×, then **Ground only** and **Tile atlas** at 2×. Pass: tails, hands and bananas are easy to find on both materials; the assembled ground has no diamond grid, gaps or repeated scalloped border; moss edges read as ground growth. Fail: subjects merge with the floor, soil flecks resemble collectible fruit, or a visible seam repeats between neighboring tiles. This is the human art acceptance step. In the game the tiles are drawn by `ground_tile_mesh` in `src/isometric.rs` (D31): dirt on the ring path, the depot plaza and the worked route, floor elsewhere; `./play --scenario support` shows them.

The recipe checks palette membership, binary alpha, master/atlas/PNG equality, all 128 compatible mask-edge pairs at 129 samples per edge, identical variant boundary strips, and gap-free/overlap-free coverage of every pixel in a 960 × 640 assembly. It also checks exact worker reference pixels and limits surface marks to less than 3.5% of any tile. These checks support visual review; they do not replace it.
