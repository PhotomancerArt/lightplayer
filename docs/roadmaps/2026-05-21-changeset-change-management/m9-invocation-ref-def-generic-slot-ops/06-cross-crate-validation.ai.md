# Phase 06 — Cross-Crate Validation

**Dispatch:** sub-agent: supervised | parallel: - | **Depends on:** 05

## Scope of phase

Fix remaining workspace references and run broader test gate **before** touching
`examples/`.

**In scope:**

- Grep repo for `NodeDefRef`, `def = { path`, `NodeInvocation { def`
- Fix: `lpc-wire/tests`, `lpa-server/template.rs`, any stray engine nodes
- `fw-tests` / `scene_render_emu` compile if example projects referenced — may still
  fail until phase 07; note which failures are example-TOML-only

**Out of scope:** `examples/` (phase 07).

## Grep commands

```bash
rg 'NodeDefRef|def = \{ path' lp-core lp-app lp-cli lp-fw
rg 'inline_def|def_locator' lp-core
```

## Expected fixes

- `lpc-wire` source slot sync test paths if node path strings changed
- Engine runtime tests constructing `NodeInvocation::path(...)` — should still work

## Validate

```bash
cargo test -p lpc-model
cargo test -p lpc-node-registry
cargo test -p lpc-engine
cargo test -p fw-tests --test scene_render_emu 2>&1 | tail -30  # may fail on examples
cargo clippy -p lpc-model -p lpc-node-registry -p lpc-engine --all-targets --no-deps -- -D warnings
```

Document any remaining failures blocked on phase 07 in phase PR notes.
