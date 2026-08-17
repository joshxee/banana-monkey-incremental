# Banana Monkey Incremental

This is a Rust and Bevy incremental game. Favor a small, deterministic core and a distinctive, playful player experience.

## Build environment

Bevy programs can be compiled and run inside this Nix shell with `nix-shell --run "cargo run"`.

## Specialist gates

- The main agent owns planning, implementation, synthesis, and final decisions.
- After drafting a material implementation plan, consult `mathematician`, `game-designer`, and `game-developer` in parallel. Reconcile their findings before implementation.
- After implementing and locally verifying a material change set, ask all three to review the diff and evidence. Resolve findings before completion.
- Specialists advise and review. They edit only when the user explicitly delegates implementation to them, and return `PASS` when they have no domain finding.
- Material work changes runtime behavior, balance, architecture, UI, or tests. Skip the gates for questions and tiny text or metadata edits.

The specialist definitions and their project-source indexes live in `.claude/agents/`.
