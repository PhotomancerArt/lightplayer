# M9 — NodeInvocation Ref|Def + Generic Slot Edit Ops

Collapse `NodeInvocation` to a slotted **`Ref | Def`** enum, add **`VariantSet`**
edit op, and remove registry shortcuts that paper over the old custom codec.

**Prerequisite:** M8 unified sync (done).

**Plan:** [`m9-invocation-ref-def-generic-slot-ops/`](m9-invocation-ref-def-generic-slot-ops/)

## Phases

| # | Title | Focus |
|---|--------|--------|
| 01 | Invocation Ref\|Def model | `lpc-model` enum + TOML codec |
| 02 | Model tests + remove NodeDefRef | Tests, exports, callers in `lpc-model` |
| 03 | Engine + registry consumers | `def_walker`, `def_shell`, `effective_read`, registration |
| 04 | VariantSet + thin slot_apply | `lpc-node-registry` edit apply |
| 05 | Generic diff + test TOML | `def_diff`, harness/fixtures, integration tests |
| 06 | Cross-crate validation | `lpc-model`, `lpc-engine`, `lpc-node-registry`, `fw-tests` |
| 07 | Examples + docs | `examples/`, `change-language.md`, roadmap summaries |
| 08 | Cleanup + CI gate | `just check`, plan `summary.md` |

Rust and in-repo tests through phase 06; examples and docs in phase 07.

## TOML wire (breaking, no dual-read)

```toml
[nodes.shader]
ref = "./shader.toml"

[nodes.clock.def]
kind = "Clock"
```

Playlist:

```toml
[entries.2.node]
ref = "./active.toml"

[entries.2.node.def]
kind = "Shader"
```
