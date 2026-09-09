# Testing and playtesting

The harness has three tiers. Each answers a different question, and the
cheaper tier always runs first.

| Tier | Question | Command | Warm cost |
|---|---|---|---|
| Headless | Does the economy do what the architecture doc says, tick for tick? | `cargo test` | seconds |
| Playtest | Does it feel and look right? | `./play --scenario NAME` | one build, then a human |
| Browser | Does the pointer, the touch screen, the save, the diagnostics panel work in Chromium? | `npm run test:e2e:fast`, full suite in CI | minutes to tens of minutes |

Both wrappers, `./play` and `./serve`, exist because the build needs the Nix
shell: outside it `wayland-client`, `libudev` and `alsa` are missing and the
build fails in their build scripts long before the game is reached.

The middle tier is the one that matters. Automated tests hold the numbers
still; a person decides whether the game is good.

## Headless: `cargo test`

`src/game.rs` is two plugins. `SimulationPlugin` is the 20 Hz economy:
`FixedUpdate` only, no window, no assets. `PresentationPlugin` is everything
drawn or touched. The split is what makes the first tier possible: a test adds
`SimulationPlugin` to `MinimalPlugins` and steps it.

`src/headless.rs` is the driver. `Headless::scenario("one-worker")` opens a
named state; `tick()` runs exactly one fixed tick, because the app is on
`TimeUpdateStrategy::FixedTimesteps(1)`; `hire`, `harvest` and `restart` push
the same requests the shop and menu push; `settled()` returns every `Settled`
message the simulation wrote since the last call, which is its ledger.

`src/sim_tests.rs` holds the contracts. A test names the decision in
`docs/banana-architecture-v2.md` it holds to and asserts the exact tick and the
exact banana. A worker's trip is a thousand ticks and runs in under a
millisecond, so a test can afford twenty of them.

Two boundaries a test can hit:

- The worker spawns on tick one, not tick zero. Sample after `tick()`.
- A segment flips on the tick that completes it. The 400th tick of the
  outbound walk already reads as `Pick`; the delivery is tick 950 and the meal
  is tick 1000.

The suite is deterministic: a seeded `Restored` placement and a fixed feeding
order mean two runs of the same scenario are bit-identical, and a test asserts
so.

## The map: `assets/maps/start.txt`

The village is a grid of jungle, ring path and town, one character per tile,
compiled in with `include_str!` rather than loaded as an asset — the headless
economy needs it, and `cargo test` has no asset server. `src/map.rs` parses it
and answers routes: A* over the passable tiles, pulled taut so the length is the
walk a monkey covers rather than the staircase a grid search returns.

`GROVE_DISTANCE` is a measurement of this file (D24). The economy still *holds*
it as a constant and a test asserts the two agree, so the map is what the
constant is checked against rather than what supplies it at runtime — redrawing
the map fails `cargo test` instead of quietly moving the balance. That flips
when a second node goes live and travel stops being one number.

`./play --map` is the readout to run first when you have edited it:

```text
map: 69x69 tiles of 2 m

terrain:  2842 jungle  328 path  1521 town  70 grove
centre:   (44, 30)

banana nodes, nearest first:
  (20, 12)    60.00 m over 1 leg(s)   <- worked
  (20, 58)    73.76 m over 1 leg(s)
  (39, 30)    10.00 m               <- home tree, hand-picked only

travel leg: the worked node is 60.0 m; GROVE_DISTANCE is 60.0 m
```

A disagreement is called out on that last line, and the travel leg is printed
at full precision rather than rounded, because the number is there to be pasted
into `GROVE_DISTANCE`. Re-derive the balance before you go any further:
distance and both speeds move together or D17's cart advantage moves with them.

The home tree is listed but never worked. It is the node the *player* drags
from, kept a few tiles out so both ends of the drag are on screen at a zoom
where a monkey is legible; being nearer than the worked node, counting it among
them would silently take over as the travel leg.

## Scenarios: `src/scenario.rs`

A scenario is a `SavedRun` plus a `Placement`. `AtStall` starts every harvester
at the stall at phase zero, the way a fresh hire starts, and gives exact
figures. `Restored { seed }` drops them along the route, the way a save restore
does, and gives a populated field. A restored monkey's first arrival at the
stall pays nothing, by design: it is drawn carrying a banana it never picked.
Tell a playtester, or they will file it as a bug.

The registry serves both tiers. `Headless::scenario(name)` and `./play
--scenario name` open the same world, so a test and a playtest of a feature
start from the same place, and neither hand-harvests its way there.

`./play --scenarios` lists them with what each is for. Add a scenario when
a feature needs a state worth looking at, with a `summary` that gives the
clock and the pass criterion: what to watch, when it happens, what wrong looks
like. The registry tests check the name is kebab-case and unique, that a
scenario owning a cart has the research to own one, that research only exists
where a technologist could have produced it, and that a summary's claim about
the shop's rate column holds.

## Playtest: `./play`

`cargo play` is an alias for `cargo run --features test-hooks --`. The feature
is what makes the launch switches exist; a release build has none of them. It
has to run inside the Nix shell, which is where Bevy's system libraries live;
`./play` at the repo root wraps that, and is the form to hand to a human.

```
./play --scenario one-worker --speed 5
./play --scenario late-game --view stage
./play --scenarios
```

- `--scenario NAME` opens that state. **Saving is off** for the whole run, so
  the player's real save is never overwritten by a state they did not earn.
- `--speed N` runs the clock N times faster, from 0.1 to 60. `Time<Fixed>`
  is driven by virtual time, so the simulation is tick-for-tick identical; only
  frame-paced things (hand-harvest rate, save throttle, touch suppression)
  shrink in wall-clock terms.
- `--view stage` spawns the board and its actors with no banner, store or
  menu, and gives the board the whole window. For looking at one thing. The
  menu and the shop keys are off in this view; H and the drag still harvest.

The same three keys work on the web, as a query string. `./serve` wraps
`trunk serve --features test-hooks` in the Nix shell, and takes a minute to
build the first time:

```
./serve
# then open
http://127.0.0.1:5173/?scenario=late-game&speed=5
http://127.0.0.1:5173/?scenario=one-worker&speed=10&view=stage
```

An unknown scenario cannot stop a page from loading, so the run starts from
the save and says why in the browser console. The Playwright web server builds
with the feature already, which is how a spec can seed itself with
`?scenario=`.

When handing a playtest to a human, give the whole `./play` command with a
speed, how long to watch, and what pass and fail look like, so
they can answer with one of those words rather than "looked fine". The
scenario's `summary` is written to be that sentence. Say that saving is off,
so nothing they do in the run persists.

## Browser: Playwright

`tests/e2e/` covers what only Chromium can verify: mouse and touch input
against the canvas, device-pixel-ratio emulation, localStorage persistence
across a reload, the diagnostics panel, and visual baselines. The economy's
contracts used to live here too and waited out real 50-second cycles; they
moved to `src/sim_tests.rs`, and nothing here waits on a cycle now.

Specs read `window.__BANANA_MONKEY_TEST_STATE__`, a JSON export the game
refreshes every frame (`sync_web_test_state` in `src/game.rs`). A spec that
needs a state seeds it with `?scenario=` rather than grinding to it.

`npm run test:e2e:fast` is the desktop project alone. The full matrix with the
emulated-mobile projects runs in CI. Two things to know before blaming a spec:

- Every mobile spec must call `installDevicePixelContentBoxFix`; without it a
  layout looks broken and is not.
- The reload spec can fail on clean `main` under load. Check with `git stash`
  before attributing it to a change.

## CI

`cargo test --locked` runs both the unit tests and the headless contracts.
Clippy runs with `--all-features --all-targets`, so the launcher and the
harness are linted. The Playwright job runs the input, worker and diagnostics
specs; the visual spec stays local because its baselines are rasterised on a
developer machine.
