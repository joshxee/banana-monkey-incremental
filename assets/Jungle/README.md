# Jungle and banana vegetation

Five static sprite assets drawn in **sprite-axi / Aseprite Lua** to match the accepted town-center art and the supplied Spider Worker: broad flat foliage, selective underside shadows, broken leaf edges, and no enclosing outlines. All colors come from `docs/references/palette.png`. Banana fruit alone uses the gold/cream highlights.

| Asset | Transparent PNG | Editable master | Design |
| --- | --- | --- | --- |
| Broad jungle tree | [PNG](jungle-broad.png) | [Aseprite](jungle-broad.aseprite) | Large quiet crown, branching trunk, buttress roots and lianas |
| Leaning jungle tree | [PNG](jungle-leaning.png) | [Aseprite](jungle-leaning.aseprite) | Lighter offset crown, open fork and bent trunk |
| Fruiting banana plant | [PNG](banana-fruiting.png) | [Aseprite](banana-fruiting.aseprite) | Green pseudostem, torn leaf paddles, hanging ripe bunch and flower bud |
| Harvested banana plant | [PNG](banana-harvested.png) | [Aseprite](banana-harvested.aseprite) | Same plant, with the bunch removed and a cut stalk |
| Large jungle fern | [PNG](jungle-fern.png) | [Aseprite](jungle-fern.aseprite) | Low center, wide pinnate fronds and a curled new shoot |

## Scale and placement

Every master and individual PNG uses a **320 × 352** transparent canvas and a shared ground anchor of **(160, 316)**. A circular ground shadow is projected into a 2:1 ellipse, consistent with the town center's isometric ground plane. Do not scale each image to fit its opaque bounds: place them at the same pixel-to-world scale as the workers.

The original Spider Worker has a **64 × 64** frame, a roughly **44-pixel standing body**, and a **38 × 55** visible silhouette including its curled tail. The same untouched reference appears beside every asset in `jungle-scale-preview.png`; the preview has separate plant, worker, background, and label layers in `jungle-scale-preview.aseprite`.

Approximate height above the shared ground anchor: broad jungle crown **290 px**, leaning tree **242 px**, banana leaves **220 px**, and the fern's highest arch **54 px**. The fern's center stays near the ground; it is a large spreading jungle fern. These are stylized relative proportions, not botanical measurements.

The banana states are pixel-identical outside the bunch and cut-stalk area, so harvesting does not move or reshape the plant. Keep their anchors identical when switching states.

## Atlas

`jungle-atlas.png` is a **960 × 704**, 3-column/2-row sheet. `jungle-atlas.json` records names, rectangles, shared anchor, and inclusive opaque bounds. The final cell is empty. Cell order is broad jungle, leaning jungle, fruiting banana, harvested banana, fern.

The ground shadow is its own editable layer. Collision and occlusion should use a small base footprint rather than the full canopy rectangle. These assets have not been connected to runtime rendering or harvesting logic. The static originals are preserved; very slight idle animations are provided separately below.

## Regeneration and checks

From the repository root, with `ASEPRITE_BIN` set to the installed Aseprite executable:

```powershell
sprite-axi run tools/art/jungle.lua
```

The recipe redraws the layered masters, individual PNGs, atlas, metadata and scale preview. It checks exact reference-palette membership, binary alpha, canvas margins, PNG/master equality, the localized harvest-state change, and native worker-pixel preservation. Polygon points are snapped to whole pixels before calling the drawing helpers.

The worker reference is `docs/references/spider-worker.png`, the byte-identical snapshot of `assets/Monkey/Spider Worker/spider_monkey_idle.png` used for the town center. SHA-256: `42b604ab1f2fce72636c0f970bd804758d4b287ff4c95bac14a0034fb76496ef`.

## Very slight idle animations

All five plants have **16-frame, 3.2-second idle loops** (200 ms per frame), with a maximum displacement of **one native pixel**. Leaf tips sway horizontally; there is no vertical bob. Trunks, pseudostems, roots, ground shadows, fruit and the native-scale reference workers remain steady. The two jungle trees have slightly different sway timing, while both banana states use identical motion so harvesting stays aligned.

For each sprite, `<name>-idle.aseprite` is the layered animation tagged `idle`, and `<name>-idle-sheet.png` is its transparent 4 × 4 atlas (**1280 × 1408**, 320 × 352 cells). `jungle-idle.json` provides all five animations' rectangles, durations, shared `(160, 316)` anchor and looping metadata.

`jungle-idle-preview.gif` shows the full set at native scale with the unchanged workers. Its editable version is `jungle-idle-preview.aseprite`. `jungle-idle-motion.png` compares opposite poses, frames 5 and 13.

```powershell
sprite-axi run tools/art/jungle-idle.lua
```

The animation recipe verifies the one-pixel motion cap, unchanged first frames, stable ground contact and static layers, palette membership, binary alpha, canvas margins, loop boundaries, and fruiting/harvested alignment throughout all 16 frames. The animations are supplied as assets; runtime playback is not integrated.
