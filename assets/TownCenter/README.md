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
- `town-center-preview.png`: 2× nearest-neighbor inspection export.
- `town-center-scale-preview.png` / `.aseprite`: native-scale context with three unchanged Spider Workers: on the porch, beside the stairs, and by the collection bins. Backdrop and workers are separate preview layers and absent from the game export.

## Scale and placement

Ground axes use **2:1 pixel isometric projection**, with vertical edges remaining vertical. The enclosed plan is **224 × 168 axis pixels**, with 68-pixel walls and a **54-pixel door**. This is an exterior concept approximately the scale of a four-bedroom monkey house, not a measured interior plan.

The reference worker has a **64 × 64 canvas**, a roughly 44-pixel standing body, and a **38 × 55** visible silhouette including its curled tail. It is the original standing study `assets/Monkey/Spider Worker/spider_monkey_idle.png`, found in the `codex/spider-monkey-gait` worktree. `docs/references/spider-worker.png` is a byte-identical snapshot for reproducible comparison. SHA-256: `42b604ab1f2fce72636c0f970bd804758d4b287ff4c95bac14a0034fb76496ef`.

Use the same nearest-neighbor display scale for the building and workers. The projected ground origin is **(330, 440)**. The stair approach is around **(88, 513)**. The twelve 8-pixel risers descend from the 96-pixel deck to ground level. Collision and occlusion require footprint-aware integration; these coordinates are placement guides. The asset is not connected to the runtime scene.

## Verification and regeneration

The master embeds the reference's full 31-color palette and uses **19 visible colors**, all exact members of `docs/references/palette.png`. Opaque art bounds are **(63, 38)–(574, 606)** inclusive. Alpha is binary (0 or 255); no antialiasing or gradient colors.

The recipe validates palette membership, alpha, unclipped margins, pixel equality between the PNG and layered master, and every opaque pixel of all three reference workers at native scale. Visual review remains necessary for art quality.

From the repository root, with `ASEPRITE_BIN` pointing to the installed Aseprite executable:

```powershell
sprite-axi run tools/art/town-center.lua
sprite-axi show assets/TownCenter/town-center.aseprite --scale 2 --png assets/TownCenter/town-center-preview.png
```

Regeneration overwrites the generated town-center files. Edit the Lua recipe or preserve a separate working copy before manual edits.
