# Banana Monkey Incremental

This is a Rust and Bevy incremental game. Favor a small, deterministic core and a distinctive, playful player experience.

## Build environment

Bevy programs can be compiled and run inside this Nix shell with `nix-shell --run "cargo run"`.

## Specialist review

- After implementing and locally verifying material game work, ask `mathematician`, `game-designer`, and `game-developer` to review the diff and evidence in parallel. Resolve findings before completion.
- Material work changes runtime behavior, balance, architecture, UI, or tests. Skip review for questions, text or metadata edits, CI, releases, repository hygiene, infrastructure, and build fixes unless game behavior or design changes.
- Specialists advise and review; they edit only when explicitly delegated. The main agent owns implementation and final decisions.

The specialist definitions and their project-source indexes live in `.claude/agents/`.
