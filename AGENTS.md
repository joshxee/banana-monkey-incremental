# Banana Monkey Incremental

This is a Rust and Bevy incremental game. Favor a small, deterministic core and a distinctive, playful player experience.

## Build environment

Bevy programs compile and run inside the Nix shell: `nix-shell --run "cargo test"`.

## Development loop

The economy runs headless. Test there first, and put a human in front of it before calling material work done. `docs/testing.md` explains the three tiers, the `Headless` driver and the scenario registry.

1. `cargo test` runs the unit tests and the headless economy contracts in `src/sim_tests.rs`, in seconds. A change to simulation behaviour lands with a contract there, pinned to the architecture decision it holds to and asserting the exact tick and banana.
2. `cargo clippy --all-features --all-targets -- -D warnings` and `cargo fmt --all -- --check`, both clean.
3. Material work ends with a human playtest. Hand over the whole command, how long to watch, and what pass and fail look like, for example: `./play --scenario one-worker --speed 5`, watch ten seconds, pass is +5 then -1.5 on the counter with the balance flat between. Saving is off in a scenario run. `./play --scenarios` lists every named state with its pass criterion; add one to `src/scenario.rs` when a feature needs a state worth looking at.
4. `./serve` opens the same scenarios in a browser at `http://127.0.0.1:5173/?scenario=NAME&speed=N`, for a playtest of the shipped wasm build or of touch input.
5. Playwright (`tests/e2e/`) is for what only a browser can verify: pointer and touch input, DPR, localStorage, the diagnostics panel. Run `npm run test:e2e:fast` when touching those; the full matrix belongs to CI.

Simulation state is written only in `FixedUpdate`, inside `SimulationPlugin`. Anything a player sees lives in `PresentationPlugin` and reads the `Settled` message rather than reaching into the tick. Keep that seam: it is what lets the economy run with no window.

## Specialist review

- After implementing and locally verifying material game work, ask `mathematician`, `game-designer`, and `game-developer` to review the diff and evidence in parallel. Resolve findings before completion.
- Material work changes runtime behavior, balance, architecture, UI, or tests. Skip review for questions, text or metadata edits, CI, releases, repository hygiene, infrastructure, and build fixes unless game behavior or design changes.
- Specialists advise and review; they edit only when explicitly delegated. The main agent owns implementation and final decisions.

The specialist definitions and their project-source indexes live in `.claude/agents/`.
