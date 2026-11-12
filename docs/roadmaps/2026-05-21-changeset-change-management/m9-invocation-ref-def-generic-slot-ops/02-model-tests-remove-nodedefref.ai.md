# Phase 02 — Model Tests + NodeDefRef Removal

**Dispatch:** sub-agent: yes | parallel: - | **Depends on:** 01

## Scope of phase

Fix all **`lpc-model`** callers and tests for `Ref | Def` API. Remove remaining
`NodeDefRef` references inside `lpc-model`.

**In scope:**

- `nodes/project/project_def.rs` tests — `ref =` wire
- `nodes/playlist/playlist_entry.rs` tests — `node.ref` / `[node.def]`
- `nodes/node_def.rs` if it references `NodeDefRef`
- Grep `lpc-model` for `NodeDefRef`, `.def`, `def_locator`, `inline_def` on old struct

**Out of scope:** `lpc-node-registry`, `lpc-engine`, examples.

## TOML test migrations

| Old | New |
|-----|-----|
| `def = { path = "./x.toml" }` | `ref = "./x.toml"` |
| `def = { kind = "Clock" }` | `[nodes.clock.def]` or nested def table |
| `[entries.2.node.def] kind = ...` | unchanged path for inline |

## Implementation details

Update match sites:

```rust
// before
match &invocation.def { NodeDefRef::Path(l) => ..., NodeDefRef::Inline(d) => ... }
// after
match &invocation { NodeInvocation::Ref(l) => ..., NodeInvocation::Def(d) => ... }
```

Update `def_shell.rs` in model if any — likely none in lpc-model.

Ensure `collect_invocations` consumers in model tests use new helpers.

## Validate

```bash
cargo test -p lpc-model
cargo clippy -p lpc-model --all-targets --no-deps -- -D warnings
```

Other workspace crates may still fail — OK until phase 03.
