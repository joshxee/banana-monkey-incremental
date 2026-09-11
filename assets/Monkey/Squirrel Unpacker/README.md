# Squirrel Unpacker

Original editable pixel art for a small courier that darts to spider workers,
takes a banana, carries it to the unload facility, and drops it into a box.
The [online reference study](../../../docs/research/squirrel-monkey-art.md)
records the zoo and primatology sources. The common squirrel monkey's pale
eye mask, dark muzzle, gray/olive back, warm limbs and dark-tipped balancing
tail guide the design. Two-handed running is deliberate cartoon stylization.

The squirrel's standing body is about 30 native pixels high beside the
approved Spider Worker's roughly 44-pixel body. Both use the same display
scale. Empty travel lowers the torso into a quadrupedal scamper; loaded
travel raises it enough to cradle the fruit. Broad color fields, small ears,
separated eyes and muzzle, and an open tail curve keep it distinct.

## Files

| File | Purpose |
| --- | --- |
| `squirrel-monkey.aseprite` | Eight editable layers, 40 named clips, 256 frames |
| `squirrel-monkey.png` | Transparent SE idle still |
| `squirrel-monkey-idle.png` | 4 columns × 8 direction rows, 256 × 512 |
| `squirrel-monkey-dart.png` | 8 columns × 8 rows, 512 × 512 |
| `squirrel-monkey-carry.png` | Same layout as empty dart |
| `squirrel-monkey-take.png` | 6 columns × 8 rows, 384 × 512 |
| `squirrel-monkey-drop.png` | Same layout as take |
| `banana-transfer.png` | Separate 24 × 24 fruit, origin (10, 10), for handoff/falling motion |
| `squirrel-monkey.json` | Rectangles, durations, shared anchor, hand sockets, transfer events |
| `preview.html` | Delivery demonstration, action/direction inspection, pause/scrub, scale and ground controls |
| `squirrel-monkey-delivery.gif` | 4.5-second route using the actual Spider Worker and collection boxes |
| `squirrel-monkey-darts.gif` | All directions; empty above loaded, at 2× |
| `squirrel-monkey-preview.png` | 3× pose comparison with the unchanged reference worker |
| `squirrel-monkey-contact.png` | Native contact sheet, action rows in table order |

All game sheets have transparent, untrimmed **64 × 64 cells**, nearest-neighbor
sampling, and the shared ground anchor **(32, 56)**. Row order is **N, NE, E,
SE, S, SW, W, NW** in screen compass directions. Diagonals follow 2:1 ground
axes. NW/W/SW are pixel reflections of NE/E/SE using `x = 63 - x`; keep the
common anchor when reflecting. Review backgrounds and reference assets are
excluded from the game sheets.

## Playback and transfer

| Tag prefix | Frames | Timing | Playback |
| --- | --- | --- | --- |
| `idle_` | 4 | 300 ms each; 1,200 ms | Loop; slight local tail adjustment |
| `dart_` | 8 | 50 ms each; 400 ms | Loop; empty quadrupedal dart |
| `carry_` | 8 | 50 ms each; 400 ms | Loop; banana held at chest |
| `take_` | 6 | 70 ms each; 420 ms | Once; attach banana at 210 ms |
| `drop_` | 6 | 70 ms each; 420 ms | Once; release banana at 210 ms |

Suffix each tag with its direction, for example `carry_SW`. Move the entity
to create travel; cycles are in place. Illustrative preview travel is about
150 native pixels/second, not an economy rule. Stop travel at the contact
point before `take` or `drop`; their fixed feet provide the action stance.
The gait and stationary interaction endpoints are different poses, so settle
to the interaction stance at arrival rather than claiming pixel-identical
transitions. Turn to face the recipient or box before starting the action.

The metadata uses **zero-based frame indexes** for events. At frame 3
(Aseprite clip frame 4), `take` gains its banana layer; `drop` loses it.
Remove the donor's fruit when the handoff begins, animate one separate fruit
to the receiver, then remove that transfer sprite at `attach_banana`. At
`release_banana`, spawn a separate fruit at the last loaded frame's
`bananaSocket`, animate it into the box, and occlude it behind the front rim.
Do not render both the transfer sprite and the attached banana after receipt.
The JSON sockets locate the fruit's (10, 10) origin inside each 64 × 64 cell.
When `bananaFlipX` is true (NW/W/SW), mirror the transfer image horizontally
and use its mirrored origin (13, 10), keeping the supplied socket unchanged.
The verifier checks exact standalone-fruit registration in every loaded frame.

## In the game

The game draws one of these per Unpacker hired, up to `support::COURIER_LIMIT`,
and places it on the line between a harvester that is unloading and the town
centre's banana bins: out empty on `dart_`, back loaded on `carry_`, and `idle_`
when there is nobody at the bins to help. The row is picked from the direction
it is actually drawn moving, projected to the screen, so the compass bearings
here are the bearings the player sees. `dart_` and `carry_` are advanced by the
distance drawn rather than by the clock, at the manifest's own 60 native pixels
per loop, so the feet grip the ground at any speed the board is zoomed to.

`take_` and `drop_` are not played: they are one-shot clips with their own
planted feet, and playing them needs per-courier playback state and a rule for
interrupting them. The banana in the courier's hands is what says which leg of
the run it is on. See D31 in `docs/banana-architecture-v2.md`.

The sheets themselves are asset exports and a review demonstration; economy
events and box occlusion are not integrated.

## Regeneration and verification

Run from the repository root with `ASEPRITE_BIN` pointing to your installed
Aseprite executable:

```powershell
sprite-axi run tools/art/squirrel-monkey.lua
sprite-axi run tools/art/verify-squirrel-monkey.lua -f 'assets/Monkey/Squirrel Unpacker/squirrel-monkey.aseprite'
sprite-axi run tools/art/squirrel-monkey-preview.lua
```

The verifier checks every master/PNG pixel, tag bounds, frame and clip timing,
binary transparency, canonical palette membership, clear canvas margins,
mirrored directions, visible fruit layers, transfer timing, hand sockets,
distinct gait poses and loop-boundary differences. All 256 frames pass;
opaque bounds are (3, 15)–(60, 61), using nine canonical colors.
The transfer verifier also checks reflected fruit/socket registration. The
delivery recipe asserts that the fruit is fully behind the box wall before
removal. Mathematician, game-designer, and game-developer reviews all pass
after correcting transfer reflection and box-rim occlusion. Preview JavaScript
syntax, metadata equality and sheet references pass static checks. Automated
HTML browser inspection was blocked by the browser's local-file URL policy;
the exported contact sheets and transfer sequence were reviewed directly.

For a human art playtest, open the complete command below from the repo root:

```powershell
Start-Process 'assets/Monkey/Squirrel Unpacker/preview.html'
```

Watch 15 seconds, then pause and inspect native scale plus the final → first
dart frame. Pass: a clearly smaller squirrel monkey, quick alternating
strides, readable fruit, one handoff followed by a drop into the open box,
no clipping or loop snap. Fail: a banana duplicates/disappears during transfer,
the face merges into the payload, or a direction loses limb/tail separation.
The demonstration resets the donor between loops; it is not an economy simulation.
