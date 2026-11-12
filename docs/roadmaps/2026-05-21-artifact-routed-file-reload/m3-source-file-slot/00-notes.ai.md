# M3 Plan Notes — SourceFileSlot + SourceFileRef

## Scope of work

Add **`SourceFileSlot`** (authored, `lpc-model`) and **`SourceFileRef` +
materialize** (resolved, `lpc-node-registry`) in the **parallel stack**. Production
`ShaderSource` / `ShaderDef.source` / fixture mapping fields **unchanged until M6**.

In scope:

- `SourceFileSlot` custom codec: `$path`, shorthand string, extension-key inline
  (`glsl`, `svg`, …).
- `SourceFileRef` enum: file artifact / inline / URL stub (stub only in M3).
- **Resolve** authored slot → ref (acquire file artifact in M1 store).
- **Materialize** ref (+ authored slot for inline body) →
  `{ version, text, diagnostic_name }`.
- Tests: codec round-trips (`lpc-model`); resolve + materialize + version bump
  (`lpc-node-registry`).
- Optional harness-only test def shape exercising `SourceFileSlot` in a parsed
  `NodeDef` (not production `ShaderDef`).

Out of scope:

- Production `ShaderDef` / `ComputeShaderDef` / fixture `MappingConfig` migration
  (**M6**).
- `lpc-engine` / node compile paths (**M6**).
- `BinaryFileSlot` (**future**).
- ChangeSet / AssetView (**M5**).

## Current state

### Authored sources today

- `ShaderSource` enum: `Path(SourcePathSlot)` | `Glsl(ValueSlot<String>)` in
  `lpc-model/src/nodes/shader/shader_source.rs`.
- Fixture mapping uses separate `MappingConfig` variants (`SvgPath`, etc.).
- Engine reads bytes at load time (`read_shader_source`, `resolve_fixture_mapping`).

### Slot infrastructure

- Custom codec dispatch in `slot_codec/custom_slot_codec.rs` (pattern:
  `NodeInvocation` + `NODE_INVOCATION_CODEC_ID`).
- Semantic leaves live under `lpc-model/src/slots/`.
- `SourcePath` / `SourcePathSlot` exist for path-only leaves.

### Parallel stack (M1–M2 done)

- `ArtifactStore`: acquire/release, `read_bytes`, `revision`, `apply_fs_changes`.
- `NodeDefRegistry`: `load_root`, `sync`, `NodeDefUpdates`.
- `lpc-node-registry/src/source/` is a stub.

### Roadmap encoding (agreed)

```toml
# file
source = { $path = "./shader.glsl" }
source = "./shader.glsl"

# inline
[source]
glsl = """
void main() {}
"""
```

Exactly one of `$path`, shorthand string, or extension-key inline table.

## Architecture sketch (proposed)

```
TOML parse ──► SourceFileSlot (authored backing in NodeDef)
                    │
                    ▼ resolve_source_file(store, containing_file, slot, frame)
              SourceFileRef (handle — no text)
                    │
                    ▼ materialize_source(store, fs, ref, slot, diagnostic_ctx)
              MaterializedSource { version, text, diagnostic_name }
```

- **File-backed:** resolve acquires `ArtifactLocation::file(resolved_path)`;
  ref holds `ArtifactId` + authored relative path + optional extension hint.
- **Inline:** ref holds extension + slot revision; text read from authored slot
  at materialize (not stored in ref).
- **Effective version:** combine slot revision + artifact revision for file mode;
  slot revision only for inline. M4 uses version change without def TOML change.

## Open questions

### Q1 — Authored vs resolved split

**Context:** Roadmap: nodes hold `SourceFileRef`, not text. Authored defs hold
`SourceFileSlot`.

**Suggested:** M3 implements both types. **Resolve** is explicit (not implicit at
parse). **Materialize** takes ref + authored slot (for inline body + slot revision).

### Q2 — Extension keys

**Context:** Inline format identified by table key (`glsl`, `svg`, `wgsl`, …).

**Suggested:** M3 accepts any non-reserved string key as inline extension; no
fixed allowlist yet. `$path` is reserved; `path` field name left free for future
`.path` artifact type.

### Q3 — Diagnostic names

**Context:** Compile errors need stable labels.

**Suggested:**

| Backing | `diagnostic_name` |
|---------|---------------------|
| File | Project-relative path as authored (e.g. `./shader.glsl`) |
| Inline | `{containing_file}:{slot_path}.{ext}` (e.g. `/shader.toml:source.glsl`) |

Pass `containing_file` + optional `SlotPath` into materialize/resolve context.

### Q4 — Test def shape

**Context:** Milestone allows harness-only defs; production `ShaderDef` untouched.

**Suggested:** M3 gate tests use:

1. Direct `SourceFileSlot` codec round-trips in `lpc-model`.
2. `lpc-node-registry` tests with inline TOML fixtures embedding a minimal
   **`TestSourceDef`** (`kind = "TestSource"`) registered only for tests — **or**
   standalone resolve/materialize tests without a new `NodeKind` if we can avoid
   registry coupling.

Prefer **standalone materialize tests** + codec tests; add `TestSourceDef` only if
needed for end-to-end `load_root` integration.

### Q5 — URL stub

**Context:** `SourceFileRef` includes future URL variant.

**Suggested:** `SourceFileRef::Url { .. }` stub variant; resolve returns
`MaterializeError::Unsupported` in M3.

### Q6 — Shorthand string disambiguation

**Context:** `source = "./shader.glsl"` must not collide with inline string that
looks like a path.

**Suggested:** Shorthand string form is **only** valid when the TOML value is a
**string scalar** (not inline table). Inline must use extension-key table form.

## Resolved decisions (roadmap)

- Parallel build; no `lpc-engine` edits until M6.
- No long-lived source text in refs or slot resolved values.
- File artifact registration via M1 store on resolve.
- Hard cut TOML encoding for new slot; example project migration at M6.

## Dependencies

- M1 `ArtifactStore` (done).
- M2 `NodeDefRegistry` (done) — optional for integrated tests.

## Notes

- M4 scenario: GLSL file edit bumps artifact → materialize version changes,
  `NodeDefUpdates` empty if node TOML unchanged.
- M5 `AssetView` will feed materialize for uncommitted asset replaces; M3 uses
  store + fs only.
