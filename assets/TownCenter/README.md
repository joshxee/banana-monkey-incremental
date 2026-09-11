# Town center

A large central treehouse drawn with **sprite-axi / Aseprite Lua**, scaled to the supplied Spider Worker. The house wraps around a forked living tree, with a pale timber deck, long open stairs, a repaired roof made from oversized dried fronds, four windows, a civic pennant, and three outdoor banana bins.

## Reference study and art direction

This overhaul follows the three Hyper Light Drifter screenshots supplied in the conversation:

- **Forest and ruins:** trees read as broad, mostly uninterrupted color masses. Dark branches and broken underside shadows establish volume; a dark border does not surround every object. Quiet ground separates characters from the environment.
- **Red forest:** bold foliage silhouettes do more work than internal shading. Torn clusters and hanging foliage punctuate the edges. Buildings use flat planes and a few recessed details, rather than outlined surfaces.
- **Snow scene:** strong value separation and selective saturated accents keep the characters readable. The environment leaves space around small focal objects.

Applied here: no enclosing contour strokes; a connected, uneven foliage field with selective underside shadows; quiet and ragged edge sections; sparse roof repairs and frond slits instead of a tile grid; broad wall colors instead of plank outlines. The roof sags around the tree and has unequal eaves. Railings and braces are physical structures, not outline treatments.

The original dark monkeys are unchanged. Muted light timber and ground give their silhouettes contrast. Windows use cool shadow colors. **Gold and cream are reserved for actual bananas**, set against cool, dark bin interiors. The pennant emblem uses muted tan.

## Deliverables

- `town-center.aseprite`: editable **672 × 704** master, 11 named layers.
- `town-center.png`: native transparent game export.
- `town-center-structure.png`, `town-center-ground.png`, `town-center-bins.png`:
  the three sprites the game actually draws, split out of the master. Same
  canvas, same coordinates; drawn bins over structure over ground they are
  `town-center.png`, pixel for pixel, and `art.rs` holds that at test time.
- `town-center-preview.png`: 2× nearest-neighbor inspection export.
- `town-center-scale-preview.png` / `.aseprite`: native-scale context with three unchanged Spider Workers: on the porch, beside the stairs, and by the collection bins. Backdrop and workers are separate preview layers and absent from the game export.

## Scale and placement

Ground axes use **2:1 pixel isometric projection**, with vertical edges remaining vertical. The enclosed plan is **224 × 168 axis pixels**, with 68-pixel walls and a **54-pixel door**. This is an exterior concept approximately the scale of a four-bedroom monkey house, not a measured interior plan.

The reference worker has a **64 × 64 canvas**, a roughly 44-pixel standing body, and a **38 × 55** visible silhouette including its curled tail. It is the original standing study `assets/Monkey/Spider Worker/spider_monkey_idle.png`, found in the `codex/spider-monkey-gait` worktree. `docs/references/spider-worker.png` is a byte-identical snapshot for reproducible comparison. SHA-256: `42b604ab1f2fce72636c0f970bd804758d4b287ff4c95bac14a0034fb76496ef`.

Use the same nearest-neighbor display scale for the building and workers. The projected ground origin is **(330, 440)**. The stair approach is around **(88, 513)**. The twelve 8-pixel risers descend from the 96-pixel deck to ground level. Collision and occlusion require footprint-aware integration; these coordinates are placement guides. The asset is not connected to the runtime scene.

## The runtime split

The game draws this building as three sprites rather than one, because the three
sort at three different depths and a single sprite can only carry one:

| Sprite | Layers | Drawn |
| --- | --- | --- |
| `town-center-ground.png` | `01 Quiet ground and cast shade` | flat on the ground, under the depot glow and the crowd's shadows |
| `town-center-structure.png` | the house, tree, deck and everything above them | at the house's own depth |
| `town-center-bins.png` | `10 Collection bins and golden bananas` | at the ground the *bins* stand on, so the queue in front of them draws in front of them |

The bins are the delivery point, and the game stands a second set of them
elsewhere in the village once the carts are unlocked, so they have to be a sprite
of their own. They keep the master's canvas and the master's ground origin, so
nothing about their placement is re-derived: dropped on the delivery point they
land exactly where they are drawn here.

`python3 tools/art/split-bins.py`, from the repository root, re-cuts the bins out
of the structure after the master is regenerated. It needs Pillow, not Aseprite,
and running it twice writes nothing the second time. The ground split is not
scripted; it was exported from the master's own bottom layer.

## Verification and regeneration

The master embeds the reference's full 31-color palette and uses **19 visible colors**, all exact members of `docs/references/palette.png`. Opaque art bounds are **(63, 38)–(574, 606)** inclusive. Alpha is binary (0 or 255); no antialiasing or gradient colors.

The recipe validates palette membership, alpha, unclipped margins, pixel equality between the PNG and layered master, and every opaque pixel of all three reference workers at native scale. Visual review remains necessary for art quality.

From the repository root, with `ASEPRITE_BIN` pointing to the installed Aseprite executable:

```powershell
sprite-axi run tools/art/town-center.lua
sprite-axi show assets/TownCenter/town-center.aseprite --scale 2 --png assets/TownCenter/town-center-preview.png
```

Regeneration overwrites the generated town-center files. Edit the Lua recipe or preserve a separate working copy before manual edits.

## Building idle animation

`town-center-idle.aseprite` contains **16 frames at 150 ms**, a seamless **2.4-second** forward loop tagged `idle`. The breeze is set to half its original strength: about 1–2 native pixels at the outer boughs, with reduced banner flutter and no visible vertical bob. The banner stays fixed along its attachment seam. The house, trunk, roots, collection bins, and reference monkeys remain still. Frame 1 exactly matches the approved static master.

- `town-center-idle-preview.gif`: looping **704 × 672** preview with native-scale workers.
- `town-center-idle-preview.aseprite`: editable layered context animation.
- `town-center-idle-sheet.png`: transparent 4 × 4 atlas (**2688 × 2816**).
- `town-center-idle.json`: fixed frame rectangles, durations, and looping metadata.
- `town-center-idle-motion.png`: still comparison of frames 1, 5, and 13.

Regenerate from the static master:

```powershell
sprite-axi run tools/art/town-center-idle.lua -f assets/TownCenter/town-center.aseprite
```

The animation recipe checks static-layer equality, palette membership, binary alpha, margins, first-frame equality, and the wraparound transition against ordinary adjacent transitions. The exported GIF was independently checked: 16 frames, 2,400 ms total duration, infinite looping. This remains an asset export; runtime playback is not integrated.
