# Review evidence — 2026-09-14

Rebuild command: `sprite-axi run tools/art/baboon-chef-v2.lua`.
Final file-integrity report: `validation.txt`.

- Mathematician: PASS. Three fixed seats per grill, four grills, twelve visible
  chefs; mirrored anchors and station coordinates match the metadata. No economy change.
- Game developer: PASS. Pose labels, mirrors, layer ordering, anchors and exported
  source agreement are consistent. No runtime integration in this asset change.
- Game designer: initially requested a broader rounded chest and shoulder merging
  into the upper arm. Those shapes were revised in both anatomy and chef poses,
  all outputs were regenerated, and the follow-up visual review returned PASS.

All three reviews used Astra. Human visual acceptance is pending. Automated
checks and specialist reviews do not establish that the user accepts the art.

## Animation preview

Rebuild command: `sprite-axi run tools/art/baboon-chef-animation.lua`.
Integrity checks passed for all 128 frames across eight loops; see
`animation-validation.txt`.

- Game designer: PASS for restrained movement, fixed feet/shoulder and matching
  loop endpoints, after inspecting the atlas, GIF and recipe.
- Game developer and mathematician: identified ambiguous frame-index metadata.
  Atlas coordinates and Aseprite frame bounds now declare their separate bases;
  regenerated files passed both follow-up reviews.
- Browser check: page and assets load; pause/resume, angle and animation changes,
  frame stepping including last-to-first wrap, scrubbing, speed, zoom and foot
  anchor controls work. The chef slider updates the scene and label at 4 and 12.
  The preview was left playing the tongs-lift loop at normal speed.

No runtime code changed. Human anatomy and motion acceptance remain pending;
the animation review procedure is in `README.md`.
