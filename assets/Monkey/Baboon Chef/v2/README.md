# Baboon chef — anatomy rebuild

This is a new pixel construction. It does not reuse the rejected version's
character pixels or drawing functions. The user's three photographs are the
anatomical references; the earlier hamadryas species choice and white mantle
have been dropped. No exact species identification is claimed from the photos.

Start with **preview.html**, or **anatomy-and-chef-4x.png**: Spider Worker,
unclothed baboon, then chef, with every pixel enlarged equally. The unclothed
view is included so the animal can be judged without the costume.

## What the photographs establish

- The brow overhangs a deep-set eye; a long bridge slopes down to a blunt muzzle.
  The nose is part of the bare face. A substantial lower jaw sits below it.
- Brown-grey fur covers the crown, cheek and rounded shoulder. The bare face is
  dark and the ear is small, high, and behind the eye.
- The standing pose has flexed knees, a pelvis behind the shoulders, substantial
  haunches and broad planted feet. It is not an upright human stance.
- Long arms hang toward the knees. The working pose bends one elbow to reach the
  grill; the other arm hangs alongside the body.
- The tail emerges from the back of the rump, descends, and trails behind the feet.

Reference files: [profile](../../../../docs/references/baboon/profile-user.png),
[standing](../../../../docs/references/baboon/standing-user.png),
[walking](../../../../docs/references/baboon/walking-user.png).
An [ImageGen anatomy study](../../../../docs/references/baboon/generated-anatomy-study.png)
helped break away from the rejected construction. It is a design study, not a
game texture. Its prompt and provenance are recorded with the references.

## Assets

The latest revision uses a close warm-grey fur ramp, with ten additional
character colors authorized by the user. The shared project palette is intact.
Front views expose both brow planes, a broad muzzle end, more chest and
diagonally staggered feet. Rear views have a broad back and receding tools.
Left/right filenames indicate screen direction; all chef poses are three-quarter
views. Crisp pixel clusters provide the softer shading without blur or dithering.

- **baboon-chef.aseprite**: 11 editable layers, 6 static frames. Frames and PNGs
  are named `anatomy-left`, `chef-left`, `chef-back-left`, `anatomy-right`,
  `chef-right`, `chef-back-right`. Right poses are complete layer mirrors.
- **baboon-sheet.png**: six 64 x 64 cells in that order. Zero-based frame indices.
- **baboon-chef.json**: cell sizes, frame order, anchors, station origins and capacity.
- **grill-station.aseprite**: 4 occupancy frames with separate chefs and grill.
  Transparent exports are **grill-0-chefs.png** through **grill-3-chefs.png**.
- **anatomy-and-chef-native.png**, **anatomy-and-chef-4x.png**, **anatomy-6x.png**,
  **grill-preview-3x.png**, **twelve-chefs.png**: review images with sage backgrounds.

The old grill prop is reused for context. A station holds three fixed seats;
the first two chefs face inward from behind the grill and the third is seen
from behind in front of it. Fill stations in order for up to 12 visible chefs.
The occupancy preview hides unused grills. Character pixels and anchors stay
fixed when occupancy changes.

Use nearest-neighbor sampling and retain the 64 x 64 canvases. Left-facing
anchors are (28,59); mirrored anchors are (35,59). Coordinates are x-right,
y-down. The new body is about 54 pixels high, slightly larger than the Spider
Worker's roughly 44-pixel body; its body mass is deliberately heavier.

The base poses remain available as static art. The separate animation master
adds idle and tongs-lift loops for all four chef angles. In the game (D31)
the chefs take the grill's three seats in order, cook while fed and idle while
hungry; `src/art.rs` holds the anchors, seats and timings to the manifests.
The simulation is not changed.

## Animation review

From the repository root, run:

```powershell
node tools/art/serve-preview.cjs
```

Open [the interactive preview](http://127.0.0.1:5181/assets/Monkey/Baboon%20Chef/v2/animation-review.html).
It offers four synchronized angles, still/idle/tongs-lift modes, pause and frame
stepping, playback speed, native and enlarged views, and a 0–12 chef scene.
The anatomy comparison and reference photographs are available below the scene.

- **baboon-animations.aseprite**: 11 editable layers, eight 16-frame loops.
- **baboon-animations.png**: 1024 x 512 atlas; one loop per row, 64 x 64 cells.
- **baboon-animations.json**: zero-based atlas rows/columns and explicitly
  one-based Aseprite frame bounds, timings and foot anchors.
- **four-angle-cook.gif**: portable looping preview of all four tongs-lift angles.
- **animation-validation.txt**: palette, alpha, margins, fixed feet, loop endpoint,
  source/export pixel and frame duration checks.

Idle loops last 2.88 seconds and add a blink and one-pixel tail-tip movement.
Tongs-lift loops last 1.92 seconds and add a two-pixel lift with the shoulder and
feet fixed. These clips do not include walking or flipping a banana.

For human review, watch both loops for 30 seconds at native and enlarged scales,
then pause and step through the tongs lift. A pass means the baboon silhouette
stays readable, the feet stay planted and the loop does not snap. Check all four
angles and counts 1, 3, 4 and 12 for chef placement and occlusion.

## Regenerate

From the repository root with `ASEPRITE_BIN` set:

```powershell
sprite-axi run tools/art/baboon-chef-v2.lua
sprite-axi run tools/art/baboon-chef-animation.lua
```

The recipe checks all transparent exports against the project palette plus the
ten documented character colors,
binary alpha and clear margins; reopens PNGs and both Aseprite masters to compare
pixels; and checks the 12/3/4 capacity calculation for counts 0–15.
See **validation.txt**. Those checks establish file integrity, not artistic success.

For the human art check, open **preview.html** for a minute. Judge the bare
baboon first against the photographs at native and enlarged scales. A pass means
the brow, downward bridge, jaw, hunched torso, bent knees and trailing tail read
as a baboon without clothing. Then check the chef and move the count through
1, 2, 3, 4 and 12: each hire should add a visible chef without moving the others.
