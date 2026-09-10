---
name: banana-game-art
description: Design, create, edit, animate, or visually review game assets for Banana Monkey Incremental. Use for its isometric pixel-art characters, bananas, buildings, jungle vegetation, props, and environmental scenes. Does not apply to unrelated games or nonvisual gameplay code.
---

# Banana Monkey game art

Create thoughtful, original pixel art for a friendly, fun, cute, living jungle. The visual language is strongly influenced by Hyper Light Drifter: broad color fields, confident silhouettes, selective shadow masses, and expressive color relationships. Borrow that graphic language while keeping this game's playful mood. Colors can be deliberately unlike real life.

**Monkeys and bananas are the visual priority.** A beautiful environment still fails if it competes with them or swallows their silhouettes. Judge the composition at gameplay scale, with the actual worker present.

## Start with visual evidence

- Study the user's current images before designing. Describe the few visual decisions they demonstrate; naming a style is not a substitute for looking at it.
- Read [references/project-art.md](references/project-art.md) to locate the canonical palette, worker, approved examples, and preserved reference images. View relevant images, not just scripts or file metadata. For a new design direction, inspect the preserved reference studies as well as the closest existing asset; for a small edit, focus on the asset and requested change.
- Use the repository palette unless the requested direction changes it. Choose colors by their role in the scene, not literal botanical or material accuracy.
- Match the actual Spider Worker at native pixel scale. Measure its visible body separately from its canvas and curled tail. Do not substitute the older, smaller generic monkey. A newer user-approved character reference takes precedence over the stored snapshot.

## Art direction

### Make the subjects pop

Give monkeys readable silhouettes against quiet ground, decks, and nearby surfaces. Use value and hue separation; solve weak contrast in the environment before adding character outlines, glows, or unrequested recolors. Reserve the strongest bright, warm, saturated accents for ripe bananas and meaningful focal points. Avoid filling background windows and foliage with equally bright gold highlights.

Keep open space around limbs, tails, fruit clusters, and interactions. A banana bunch should read as fruit at native scale, not as yellow noise. Background foliage can form large calm masses so small subjects remain clear.

### Build forms without enclosing outlines

Use overlapping shapes, material color changes, recessed openings, and selective cast shadows to explain depth. Avoid dark contour bands around every tree, leaf, roof, or wall. A dark trunk, doorway, underside, or physical railing is legitimate structure; it is not a reason to trace the rest of the object.

Foliage should have a dominant color field with broken shadow clusters near the underside. Alternate quiet edge stretches with larger torn clusters, not uniform perimeter stippling. Avoid separately shaded polygon lobes, concentric highlight bands, and faceted shapes that resemble a low-poly model.

Buildings may use isometric construction underneath, but their character comes from authored silhouettes and materials: uneven growth, sagging fronds, a worn repair, a crooked bough, or a purposeful handmade detail. Avoid mechanical tile grids, outlined planks, fence-like wireframes, and repeated beveled surfaces. A mathematically consistent box is not a finished artistic design.

### Keep the jungle warm and alive

Favor friendly proportions, soft or rounded masses, expressive curves, and occasional playful asymmetry. Let variations differ in canopy shape, trunk lean, negative space, and growth, rather than merely rotating or recoloring one template. Use small signs of life where they support the scene; do not fill every surface with decoration.

For banana plants, use broad leaf paddles, a visible green pseudostem, selective leaf tears, and a legible hanging bunch. Follow new supplied references when the user wants a different species or interpretation.

## Pixel craft and projection

Use top-down isometric composition with consistent 2:1 ground axes and vertical structural edges. Organic silhouettes need not follow straight isometric edges everywhere; their bases, overlap, and perspective must agree with the scene.

Draw intentional pixel clusters and controlled stepped curves. Keep pixels crisp with nearest-neighbor presentation. Avoid automatic smoothing, blurred shading, gradients that introduce stray colors, and indiscriminate dithering. Apply texture sparingly where it explains material or breaks an edge.

Use sprite-axi and the installed sprite-axi skill for editable pixel art and animation. Favor a layered Aseprite master plus a reproducible Lua recipe. Use procedural helpers to construct deliberate forms; repeated formulas do not replace visual authorship. Snap coordinates to integers before sprite-axi polygon and line calls.

## Animation when requested

Ambient idles should be very slight. The user explicitly reduced an earlier building animation for moving too much. Start vegetation with movement capped around one native pixel at leaf tips, slow timing, and no whole-object bob. Keep root contacts and structural anchors steady. Fruit should remain a clear focal point rather than constantly wobbling with the scenery.

Use the accepted animations in the reference index as starting points, not fixed timing rules for every future animation. Preserve the underlying art and scale in an idle; use deliberate larger motion for an action when that is the request. Keep matching resource states aligned through the same phase so a harvest switch does not make the plant jump. Inspect both the animation and its loop boundary.

## Finish with evidence

Export the requested asset scope: usually a layered master and transparent PNG, plus animation sheets, timing/anchor metadata, and a looping preview when relevant. Keep review backgrounds and reference workers out of game exports. Preserve common anchors between variants; do not resize each sprite independently to its opaque bounds.

Check palette membership, crisp transparency, margins, export/source agreement, and state alignment where applicable. Inspect native-scale images with the real worker and bananas present; also inspect enlarged pixels or motion extremes when needed. Numerical checks cannot establish taste. Revise work that reads as generic blocky 3D, is over-textured, or hides the main subjects even if every technical check passes.

Follow the repository's AGENTS.md review requirements for material game work. Deliver a visual preview and links to usable files. Creating assets does not by itself require wiring them into runtime systems or adding extra animations or variants.
