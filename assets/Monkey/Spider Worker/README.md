# Spider worker monkey

## Eight-direction walking and carrying

The latest correction pass reconnects every down-facing ankle and toe contour.
Rear-facing N/NE/NW arm cels now render below the body, while their dedicated
`Tail foreground` cels render above it; this keeps the crossing arm occluded and
the tail readable in front. The carrying variants use the same ordering.

`spider_monkey_directional_walks.aseprite` contains **16 clips / 192 frames**:
eight empty-handed walks and eight walks holding one banana. Each clip has
12 frames and a 720 ms loop, timed 70, 60, 50 ms repeated four times. All frames
use untrimmed transparent 64 × 64 cells with the shared ground anchor (32, 56).
The original empty-handed SE cycle is preserved pixel for pixel.

| Export | Layout |
| --- | --- |
| `spider_monkey_walk_8dir.png` | 768 × 512; 12 columns × 8 direction rows |
| `spider_monkey_carry_walk_8dir.png` | Identical layout and timing |
| `spider_monkey_directional_walks.json` | Clip names, source frames, sheet rectangles, timing and anchor |
| `spider_monkey_8dir_preview.gif` | Looping 2× comparison; walking above carrying |
| `directional_walks_preview.html` | Paired directions, pause/scrub, speed, native/2×/4× and three grounds |

Rows and preview columns run **N, NE, E, SE, S, SW, W, NW**. Directions are
screen compass directions: N is up. Diagonal motion follows 2:1 ground axes.
Tags are `walk_N` … `walk_NW`, followed by `carry_walk_N` … `carry_walk_NW`.
The five authored views are N, NE, E, SE and S; NW, W and SW are exact pixel
reflections. Reflection maps pixel x to 63−x, about the canvas boundary x=32.
Carrying changes one arm into a bent supporting pose, with separate editable
Banana and Banana grip layers. Body, tail, legs and free arm keep their phase.
Switch states at the same phase/time offset; these are in-place walks.

Open `directional_walks_preview.html` in a browser and watch for **15 seconds**.
Then pause, inspect at Native and 4×, and scrub frame 12 → 1. Pass: the feet
alternate, each direction and banana remain readable, the loop has no snap or
clipped pixels, and paired feet/tails stay aligned. Review grounds are excluded
from both game sheets. These assets are not wired into the runtime.

Regenerate from the repository root after setting `ASEPRITE_BIN` to your local
Aseprite executable:

```powershell
sprite-axi run 'assets/Monkey/Spider Worker/create_directional_walks.lua'
sprite-axi run 'assets/Monkey/Spider Worker/export_directional_walks.lua' -f 'assets/Monkey/Spider Worker/spider_monkey_directional_walks.aseprite'
sprite-axi run 'assets/Monkey/Spider Worker/verify_directional_walks.lua' -f 'assets/Monkey/Spider Worker/spider_monkey_directional_walks.aseprite'
```

The verifier checks all 192 PNG/master pairs, canonical palette membership,
binary transparency, margins, exact mirrored views, loop/tag timing, loop
boundaries, visible fruit, paired state alignment, and unchanged approved SE.

## Original down-right idle and transitions

Down-right pixel animations built with sprite-axi, revised against the user's
spider-monkey gait video and still reference. The monkey rests on all fours
with a lifted rump over bent legs and splayed palms beneath the shoulders,
releases its hands in sequence as it rises, then walks briskly with loose
outward arms, playful torso sway and gentle counterbalancing tail motion. The
tail curl opens away from the body; the chest is open with grouped lighter
tummy-fur planes rather than scattered highlights.

The layered master is `spider_monkey_animations.ase`. All frames use transparent
64 × 64 cells and the same five monkey colours. Guides and preview background
are hidden. These are animation assets; runtime integration is separate.

| Clip | Aseprite frames (1-based) | JSON frames (0-based) | Duration | Horizontal strip |
| --- | --- | --- | --- | --- |
| idle | 1–4 | 0–3 | 1,100 ms, loop | 256 × 64 |
| rise | 5–10 | 4–9 | 430 ms, play once | 384 × 64 |
| walk | 11–22 | 10–21 | 720 ms, loop | 768 × 64 |
| settle | 23–28 | 22–27 | 450 ms, play once | 384 × 64 |

Idle timings are 300, 250, 300, 250 ms. Walk repeats 70, 60, 50 ms four times,
giving quick recoveries with a small pause as the weight transfers. Both hands
remain airborne during walking; the legs provide support. Rise and settle are
transition clips, not loops. Their Aseprite tags specify forward playback;
the game must stop or switch clips at the endpoint.

Play `idle → rise → walk`, and return with `walk → settle → idle`. Rise begins
on idle frame 1 and ends on walk frame 1. Settle begins on walk frame 1 and ends
on idle frame 1, with pixel-identical boundary poses. For a clean stop, finish
the current walk cycle before starting settle. Moving the game entity produces
travel; this is an in-place down-right cycle. Keep a consistent nominal origin
of (32, 56) across cells and tune world travel speed to the stance-foot motion.

`spider_monkey_animations.png` is the packed atlas; its JSON is authoritative
for exact rectangles, durations, layers and tags. Repeated transition poses
share atlas rectangles. Individual `_sheet.png` files are horizontal,
untrimmed, transparent strips. `_idle_preview.gif` and `_walk_preview.gif`
show the loops at 4× on sand. `_gait_preview.gif` demonstrates idle, rise,
three walk cycles and settle in a seamless presentation loop.
Use the transparent PNGs in game.

The original standing study remains in `spider_monkey_idle.ase`,
`spider_monkey_idle.png` and `preview.png`; the new quadrupedal idle is in the
animation master and `spider_monkey_idle_sheet.png`.

From the repository root in PowerShell:

```powershell
$env:ASEPRITE_BIN = 'C:/Users/jbxcl/OSS/aseprite-windows-docker-build/bin/aseprite.exe'
sprite-axi run 'assets/Monkey/Spider Worker/create_animations.lua'
sprite-axi export 'assets/Monkey/Spider Worker/spider_monkey_animations.ase' sheet --out 'assets/Monkey/Spider Worker/spider_monkey_animations.png' --data 'assets/Monkey/Spider Worker/spider_monkey_animations.json'
sprite-axi run 'assets/Monkey/Spider Worker/export_animations.lua' -f 'assets/Monkey/Spider Worker/spider_monkey_animations.ase'
sprite-axi run 'assets/Monkey/Spider Worker/verify_animations.lua' -f 'assets/Monkey/Spider Worker/spider_monkey_animations.ase'
```

`create_idle.lua` holds the original contours and palette. `gait_poses.lua`
defines the revised joints, poses, timing and transitions. The verifier checks
visible colours, canvas margins, planted idle contact pixels, distinct loop
poses, loop-boundary differences, exact transition endpoints and tag timing.
Perceptual quality also requires reviewing the rendered motion.
