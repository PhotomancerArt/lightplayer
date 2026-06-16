# M4 Summary — Fs-Change Semantics Harness

<!-- Filled at completion -->

## API (target)

```rust
registry.load_root(fs, root_path, frame, ctx)? ;
let result = registry.sync(fs, changes, frame, ctx);
// result: SyncResult { def_updates, source_revisions, change_details }
```

Registry owns state. **`sync`** applies changes, updates state, returns factual diff.
M5 adds ChangeSet variants to `RegistryChange`.

## Scenarios (gate)

| ID | Input | `SyncResult` |
|----|-------|--------------|
| S1 | def TOML edit | def changed |
| S2 | GLSL only | source_revisions |
| S3 | SVG only | source_revisions |
| S4 | inline child edit | child changed |
| S5 | parse error | EnteredError |
| S6 | kind flip | KindChanged |

Engine policy: `engine-policy-v1.md`.
