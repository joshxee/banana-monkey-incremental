# Banana varieties and peeling

Five banana types, based on the user's three shape references in `docs/references/bananas/`: **original yellow, blue, red, huge, shiny**. Rounded, elongated bodies have a narrow stalk, a full curved belly, and a blunt blossom end. Each bunch has four overlapping fingers connected to one crown.

The latest shape pass reduces the single banana's center thickness by about 20% and uses rounded end caps instead of pointed tips. Bunch fingers and peeling states use the same slimmer profile so the blue and red variants remain identifiable through their silhouette.

## Deliverables

For each `TYPE` (`yellow`, `blue`, `red`, `huge`, `shiny`):

| State | Transparent game asset | Editable source |
| --- | --- | --- |
| Skin-on single | `banana-TYPE.png` | `banana-TYPE.aseprite` |
| Bunch | `banana-TYPE-bunch.png` | `banana-TYPE-bunch.aseprite` |
| Half peeled | `banana-TYPE-half-peeled.png` | `banana-TYPE-half-peeled.aseprite` |
| Peeling action | `banana-TYPE-peel-sheet.png` | `banana-TYPE-peel.aseprite` |

The peeling action has **12 frames / 1,300 ms** and plays **once**, holding its last frame. The first frame exactly equals the skin-on single; the final frame exactly equals the half-peeled sprite. Three peel strips fold away while the body stays in place. It ends half peeled, not fully skinned. Skin, flesh, peel strips, stems, blossom scars, and shiny glints are separate source layers.

## Placement and compatibility

All cells are **96 × 96**, with shared ground anchor **(48, 80)** measured from the top-left. Preserve full cells and use nearest-neighbor sampling. Huge uses a separately rasterized 1.45× native silhouette with full-resolution pixels. No backdrop, text, worker, or ground shadow is baked into game exports.

This **schemaVersion 2** replaces the earlier 64 × 64 variety drafts. Existing variety single filenames remain, but consumers must use the new dimensions and anchor. The original yellow is included here as `banana-yellow`; the older `assets/Banana/banana-bunch.*` lifecycle assets remain separate and unchanged. No runtime integration or economy values are introduced.

`banana-varieties-atlas.png` is **480 × 288**: columns are yellow, blue, red, huge, shiny; rows are skin-on, bunch, half-peeled. `banana-varieties.json` contains exact rectangles, inclusive opaque bounds, animation timing, one-based Aseprite tag ranges, and the common anchor. Each action sheet is **1152 × 96**, ordered left-to-right. `banana-varieties-data.js` exposes the same metadata for the local review page.

## Color and editable art

The palette contains the existing 31 colors plus **15 explicitly added fruit colors**: richer yellow, brighter blue and red ramps, and warm cream flesh. `banana-palette.png` and `banana-palette.gpl` contain the combined 46-color palette. This is an asset-local extension; the original project palette image is unchanged. The shiny banana uses a brighter moving peel highlight; no floating star particles are used. Game sprites have binary alpha and crisp pixel clusters.

## Regeneration and validation

From the repository root with `ASEPRITE_BIN` pointing to your installed executable:

```text
sprite-axi run tools/art/banana-varieties.lua
```

Checks cover all 75 source frames: palette membership, binary transparency, clear margins, reopened master and PNG/sheet/atlas pixel agreement, saved tags and durations, exact static/action endpoints, and fixed lower-body contact. The generator uses integer geometry and fixed curve samples so the remaining skin does not shift as peeling advances.

## Human art review

Open the local interactive viewer:

```powershell
Start-Process -FilePath (Resolve-Path 'assets/Banana/Varieties/preview.html').Path
```

Watch two replays (about 6 seconds), then scrub the 12 frames and inspect at 1× on mint and dark ground. Pass: the fruit looks rounded and elongated, four fingers are legible in each bunch, cream flesh appears as the strips fold outward, and the stationary lower body does not jump. The huge type should be visibly larger, and blue/red should separate clearly from the grounds. Fail: a pointed bowl-like silhouette, overlapping peel that hides the flesh, edge jitter, clipping, or a mismatch when the action settles into the half-peeled state.

The viewer can pause, scrub, change speed and pixel scale, and switch grounds. Replaying resets to a closed banana after a hold for inspection; that reset is not part of the one-shot game animation. Human review remains a visual acceptance step; these assets are not wired into the playable game yet.

Static previews: `banana-varieties-native-preview.png` (1×), `banana-varieties-preview.png` (2×), and `banana-varieties-dark-preview.png` (2×). `banana-peeling-preview.gif` replays with longer endpoint holds; its editable review master includes the worker and backdrop. `banana-peeling-contact.png` shows every action frame. Preview-only worker pixels come from the current Spider Worker idle reference without rescaling at native size.

## Movement on every banana, shine only on shiny

All **15 idle states** (five types × skin-on single, bunch, half-peeled) have a **12-frame / 1,600 ms looping idle**. The reference strip in `docs/references/bananas/idle-reference.png` shows a brief lift and a light patch travelling down the fruit. Our idles use a **one-native-pixel upward lift** in frames 2–3, settle on frame 4, and remain still through the rest of the loop. Only shiny bananas then pass a highlight down the colored peel. There are no external sparkle stars. Yellow, blue, red, and huge have no animated highlight. Shiny uses a cream-white light patch. The entire bunch moves together; the anatomy and shape never stretch.

The source PNG has no frame timing metadata. The 1.6-second playback is an authored interpretation, with short lift frames and a longer rest at the end. Every loop starts and ends exactly on its matching static image. Start peeling from that resting pose and enter the half-peeled idle on its first frame. During the lift the rendered pixels sit one pixel above the shared anchor; do not apply a second offset in the engine. `offsetY` in the metadata documents the already-baked movement.

Each state has `-idle.aseprite` and `-idle-sheet.png` outputs, for example `banana-blue-bunch-idle-sheet.png`. Sheets are **1152 × 96**. `banana-idle.json` supplies rectangles, timing, loop flags, and the unchanged **(48, 80)** anchor. Animation keys are `TYPE-FORM`, such as `yellow-single` and `shiny-half-peeled`. All source layers remain editable; the travelling light has its own layer.

The previous three `banana-shiny*-shine` filenames remain aliases of the corresponding new shiny idles. Their metadata is regenerated with **12 frames / 1,600 ms** and the `idle` tag, replacing the old 16-frame star effect. Consumers must read the updated metadata rather than retaining the old timing or sheet width.

`tools/art/banana-idle.lua` runs at the end of the main generator, or separately after the base exports. `tools/art/banana-shine.lua` is a compatibility entry point to the same generator. Checks cover **180 idle frames**: exact one-pixel translation, no shine on non-shiny fruit, no silhouette changes or effects outside the fruit, palette, alpha, margins, saved source and sheet agreement, tag ranges, timing, and exact static/loop endpoints. Base static and peeling checks remain separate.

Refresh `preview.html` to animate all 15 cells; Pause, speed, replay, and frame scrub also control the idle poses. Watch three cycles (about five seconds) at 1×, then enlarge: pass is a tiny lift and a quiet seam, with a passing highlight only on shiny bananas. Shiny should read brighter without a large flash. Fail is jumping several pixels, shape distortion, permanent star particles, or a visible reset. Browser interaction remains unverified because the embedded browser blocks local-file automation; viewer syntax and asset links are checked.

`banana-idle-preview.gif` shows all types in rows (yellow, blue, red, huge, shiny) and forms in columns (single, bunch, half-peeled). `banana-shiny-shine-preview.gif` isolates the three shiny states. Both have matching editable preview masters and contact sheets.
