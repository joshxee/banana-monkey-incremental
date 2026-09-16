# Jungle village expansion validation

## Rebase integration

Rebased onto origin/main at 6217114. Preserved main's ground atlas, dirt paths, UI changes, animated baboon chefs, approved grill and feeding behavior. Deep green floor is limited to deep jungle; building floors sit above the ground atlas and below actors. The open kitchen follows the current chef station at (-1.75, 9.25), with peripheral preparation counters around the approved grill. No economy rules or support positions changed.

Nine layered masters, transparent exports, palette, binary alpha, margins and exact source/export agreement passed sprite-axi verification after kitchen regeneration. Building floor/structure recombination is covered by a Rust asset contract. The static kitchen-context composition now uses main's approved baboons and grill.

## Current checks

Final Nix checks passed: cargo fmt --all -- --check; cargo test (207 passed, zero failed); cargo clippy --all-features --all-targets -- -D warnings. Results are in rebase-checks.txt. Mathematician, game-developer and game-designer reviews all passed after semantic integration. The kitchen bounds test excludes flat floor paint and includes all standing counter pixels.

## Historical evidence

Other test and Clippy logs describe the pre-rebase revision and its older baseline failures; they are not results for current main. runtime-before-clearance.png also predates both foliage clearance and this rebase. It is not current visual acceptance. contact-sheet.png and kitchen-context.png are static art studies.

## Human visual acceptance outstanding

The user stopped Computer Use with physical Escape. No further desktop interaction or alternative runtime capture was performed.

Run `./play --scenario jungle-village --speed 1` in Nix (Windows: `cargo play --scenario jungle-village --speed 1`). Watch 30 seconds, pan through town and forest, and drag a home banana into the depot.

Pass: all nine additions appear; ground paths remain visible; baboon chefs and grill remain clear inside the preparation area; the distribution shed, banana targets and worker route remain readable; delivery succeeds. Fail: missing textures, hidden feet, overlapping counters, target occlusion or blocked route readability. A human must make this visual acceptance decision.
