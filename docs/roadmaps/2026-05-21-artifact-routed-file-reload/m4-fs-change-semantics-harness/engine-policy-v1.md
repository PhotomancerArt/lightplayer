# Engine Policy v1 (M6 reference)

**Not registry output.** How **`lpc-engine`** interprets **`SyncResult`**.

## Input

```rust
// After: let result = registry.sync(fs, changes, frame, ctx);
result.def_updates      // added / changed / removed def ids
result.source_revisions // file-backed source version bumps
result.change_details   // KindChanged, EnteredError, etc.
```

Plus engine-owned runtime binding graph (def id → live node).

## Suggested v1 policy

| `SyncResult` signal | Engine effect |
|---------------------|---------------|
| `def_updates.added` | Attach / create node |
| `def_updates.removed` | Destroy node |
| `changed` + `KindChanged` | Delete + recreate node |
| `changed` + `Content` / other | Refresh / re-prepare |
| `changed` + `EnteredError` | Destroy node; cascade parent error |
| `source_revisions` (def not in `changed`) | Re-prepare products (recompile GLSL, etc.) |

M4 tests **`SyncResult` only** — not these effects.
