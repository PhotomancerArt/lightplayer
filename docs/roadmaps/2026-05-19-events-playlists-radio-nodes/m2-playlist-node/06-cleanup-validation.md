# Phase 6: Cleanup And Final Validation

- parallel: -
- sub-agent: main

## Scope Of Phase

Clean up temporary code, finish docs/examples consistency, and run the final validation set for a
shader-pipeline change.

In scope:

- remove temporary TODOs or replace them with real future notes;
- ensure `idle`/`active` naming is consistent;
- ensure playlist entry children do not leak default visual output bindings;
- ensure formatter/lints/tests pass;
- write a short summary after implementation.

Out of scope:

- Radio integration.
- New transition types.
- Large unrelated refactors.

## Code Organization Reminders

- Keep tests at the bottom of Rust files.
- Do not leave commented-out experiments in shaders or Rust.
- Do not run `cargo build --workspace` or `cargo test --workspace`; this repo has RV32-only members.
- Preserve the on-device GLSL JIT path. Do not feature-gate compiler execution behind `std`.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Review:

```text
lp-core/lpc-model/src/nodes/playlist/
lp-core/lpc-engine/src/nodes/playlist/
lp-core/lpc-engine/src/engine/project_loader.rs
lp-core/lpc-engine/src/node/contexts.rs
lp-core/lpc-engine/src/dataflow/resolver/
examples/button-playlist/
```

Checklist:

- `Playlist` is exported from `lpc-model` and `lpc-engine` module trees.
- Generated slot views/shapes compile.
- Recursive loader handles path-backed and inline playlist entry nodes.
- Relative parent ref `..#entry_time` works for entry child shaders.
- Default visual output binding is suppressed for playlist entry children.
- Top-level visual output default still works.
- `entry_time` is the only public playlist-local time slot.
- Entry-local trigger binding targets `entries[N].trigger`; there is no first-slice generic
  playlist-level trigger.
- Crossfade uses outgoing entry `fade_after` or playlist `default_fade`.
- Entry trigger restart and duration behavior match the notes.
- Example names are `idle` and `active`, not `chill` or `triggered`.

Write/update a summary when implementation is complete:

```text
docs/roadmaps/2026-05-19-events-playlists-radio-nodes/m2-playlist-node/summary.md
```

## Validate

Run targeted checks first:

```bash
cargo fmt --check
cargo test -p lpc-model playlist
cargo test -p lpc-engine playlist
cargo test -p lpc-engine project_loader
cargo check -p lpc-engine
cargo check -p lpa-server
cargo test -p lpa-server --no-run
```

Then run the shader-pipeline validation required for this repo:

```bash
cargo test -p fw-tests --test scene_render_emu --test profile_alloc_emu
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu
```

Before pushing the implementation branch, run the CI gate if time allows:

```bash
rustup update nightly
just check
just build-ci
just test
```
