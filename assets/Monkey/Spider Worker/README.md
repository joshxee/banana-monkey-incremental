# Spider worker monkey

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
