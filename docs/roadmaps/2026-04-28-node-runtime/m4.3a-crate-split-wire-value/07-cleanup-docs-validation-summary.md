# Phase 7 — Cleanup, Docs, Validation, Summary

## Scope of phase

Perform final cleanup, update roadmap/design docs, validate the split, and
write `summary.md`.

This phase is tagged `main`: the main agent should do it directly because it
requires cross-cutting judgment and final review.

Out of scope:

- Do not introduce new behavior.
- Do not rename existing non-core `lp-*` crates.
- Do not do broad aesthetic cleanup beyond stale references introduced or
  exposed by this milestone.
- Do not commit until all validation passes.

## Code organization reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a `TODO` comment so it can be found later.

## Final cleanup checklist

Search the full diff and repository for stale names and temporary code:

- `lpc-model::LpsValue`
- `lpc_model::LpsValue`
- `lpc-model::LpsType`
- `lpc_model::LpsType`
- `lpc-protocol`
- `lpc_protocol`
- `lpc-artifact`
- `lpc_artifact`
- `lpc-engine`
- `lpc_runtime`
- `lp-wire`
- `lp-source`
- `lp-artifact`
- `todo!`
- `unimplemented!`
- `dbg!`
- `println!` in tests or production code added by this plan
- new `TODO` comments without a clear reason
- new `#[allow(...)]` attributes
- commented-out code

Use `rg` for searches, not shell `grep`.

## Documentation updates

Update these docs to reflect the final crate names and boundaries:

- `docs/roadmaps/2026-04-28-node-runtime/m4.3a-crate-split-wire-value/plan.md`
  - Replace the stale placeholder with a short current status note pointing to
    `00-notes.md`, `00-design.md`, the numbered phase files, and `summary.md`.
- `docs/roadmaps/2026-04-28-node-runtime/m4.3-runtime-spine/plan.md`
  - Update any cross-link that still says M4.3a happens later.
- `docs/roadmaps/2026-04-28-node-runtime/m4.4-domain-sync/plan.md`
  - Update to say M4.4 builds on `lpc-wire::WireValue` / `WirePropAccess`.
- Design docs under `docs/roadmaps/2026-04-28-node-runtime/design/` where
  they mention `LpsValue` at wire boundaries, `lpc-model` as the source/wire
  owner, `lpc-artifact`, or `lpc-protocol`.

Keep doc edits concise. Do not rewrite the whole roadmap.

## Summary file

Create:

```text
docs/roadmaps/2026-04-28-node-runtime/m4.3a-crate-split-wire-value/summary.md
```

Use this exact shape:

```markdown
### What was built

- ...

### Decisions for future reference

#### <short title>

- **Decision:** ...
- **Why:** ...
- **Rejected alternatives:** ...
- **Revisit when:** ...
```

Capture only high-signal decisions, likely:

- `lpc-source` naming over `lpc-artifact`/`lpc-document`.
- `lpc-wire` over `lpc-protocol`.
- `WireValue` in `lpc-model`; **`LpsValueF32`/`LpsType` conversion**
  **`lpc-engine` boundary** (not generic `lpc_wire` payloads).
- `RuntimePropAccess` vs `WirePropAccess`.
- `lpc-model` has no `lps-shared` dependency.

## Validate

**(Documentation-only milestones:** skip the commands below until the Rust
split lands; Phase 7 in-repo tracks executable validation.)

Run formatting:

```bash
cargo +nightly fmt
```

Run focused checks:

```bash
cargo check -p lpc-model -p lpc-source -p lpc-wire -p lpc-engine -p lpc-view
cargo test -p lpc-model
cargo test -p lpc-source
cargo test -p lpc-wire
cargo test -p lpc-engine
cargo test -p lpc-view
```

Run broader host validation:

```bash
just build-host
```

If shader/runtime pipeline files were touched in a way that affects firmware
or `lp-shader`, also run:

```bash
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
```

If validation fails due to a non-trivial design/compile issue, stop and ask the
user rather than grinding on main-agent debugging.
