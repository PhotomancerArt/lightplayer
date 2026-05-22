# ChangeSet Change Language (v1)

Canonical edit vocabulary for client-driven changes. Lives in
`lpc-node-registry/src/edit/` — **serde types, not part of the slot system**.
Apply uses slot mut access + overlay tables; ops themselves are not `SlotData`.

## Top level

```text
EditBatch { id, edits: Vec<ArtifactEdit> }
```

Changes are **grouped by artifact**. Each block targets one file and is either
**slot-structured** (`.toml` defs) or **asset/opaque** (GLSL, SVG, delete, TOML
import escape hatch) — never both in one block.

## Target

```rust
enum EditTarget {
    Id(ArtifactId),     // committed artifact (optional; harness/wire rarely)
    Path(LpPathBuf),    // absolute project path — primary authoring form
}
```

**Implicit create:** resolving `Path(p)` get-or-creates a pending overlay entry
when `p` is not in base or overlay. No explicit `Create` op.

Overlay does not use base-store refcount rules; pending paths exist until commit
or discard.

## Artifact blocks

```rust
#[serde(tag = "kind", rename_all = "snake_case")]
enum ArtifactEdit {
    Slot { target: EditTarget, ops: Vec<SlotEdit> },
    Asset { target: EditTarget, ops: Vec<AssetEdit> },
}
```

Wire example:

```json
{ "kind": "slot", "target": { "path": "/shader.toml" }, "ops": [ … ] }
{ "kind": "asset", "target": { "path": "/shader.glsl" }, "ops": [ … ] }
```

### Slot ops (`SlotEdit`)

Node defs are slots. All normal node editing is slot ops at a `SlotPath`
**within** the target `.toml` artifact:

| Op | Use |
|----|-----|
| `UseEnumVariant { path, variant }` | Enum variant switch (node kind, `Unset`/`Ref`/`Def`, nested enums) |
| `AssignValue { path, value }` | Value leaves only (scalars, path strings, etc.) |
| `MapInsert { path, key, … }` | Map entry |
| `MapRemove { path, key }` | Map entry |
| `UseOption { path, present }` | Option some/none (`present = true` → shape default) |

Examples:

- Standalone shader file: `UseEnumVariant(root, "Shader")` then scalar `AssignValue`s on `/shader.toml`
- Inline child: ops under `entries[2].node.def.Shader…` on `/playlist.toml`
- Wire child to file: `UseEnumVariant(nodes[shader], "Ref")` + `AssignValue(nodes[shader].ref, "./shader.toml")`

Relative locators in slot values resolve against the **containing artifact path**
(same as `resolve_node_locator` today).

### Asset ops (`AssetEdit`)

Path-level file body edits:

| Op | Use |
|----|-----|
| `Delete` | Remove this path on commit |
| `ReplaceBody(text)` | Whole-file body — GLSL, SVG, etc.; optional TOML import escape hatch |

Normal **node TOML** bodies are **not** authored with `ReplaceBody`. They come from
slot ops + slot codec serialize on commit.

## Node invocation TOML (authored)

```toml
[nodes.placeholder]
unset = {}

[nodes.shader]
ref = "./shader.toml"

[nodes.clock.def]
kind = "Clock"
```

Legacy `def = { path = … }` is rejected.

## Node TOML vs assets

Same `ArtifactEdit` envelope; **`kind`** selects the op vocabulary:

- **`.toml`** — `kind: "slot"`; serialize to text on commit
- **`.glsl`, `.svg`, …** — `kind: "asset"` with `ReplaceBody` / `Delete`

## Creatability

Every `examples/*` project must be reachable from blank via a finite
`EditBatch` sequence using only:

- `ArtifactEdit::Slot` / `ArtifactEdit::Asset` (implicit create via `Path`)
- No `CreateDef`; no pre-populated def blobs as the primary path

New node at artifact root: `UseEnumVariant(root, "Shader")` (applies variant default),
then patch value leaves with `AssignValue`.

## Apply / commit

1. **Apply** — merge each `ArtifactEdit` into path-keyed overlay; base untouched
2. **View** — `NodeDefView` / read API resolves `(path, slot_path)` over overlay ∪ base
3. **Commit** — serialize overlay TOML + assets → store/fs; registry re-derive →
   `SyncResult`
4. **Discard** — drop overlay; reads = base

Dangling path refs (wire before target file exists) are OK mid-sequence.

Unreferenced overlay paths may exist on disk after commit; registry only registers
defs reachable from root (same as filesystem reality).

## Example (add shader to project)

```text
ArtifactEdit::Asset(Path("/shader.glsl"), [ ReplaceBody("…") ])

ArtifactEdit::Slot(Path("/shader.toml"), [
  UseEnumVariant(root, "Shader"),
  AssignValue(root.source.path, "./shader.glsl"),
  …
])

ArtifactEdit::Slot(Path("/project.toml"), [
  UseEnumVariant(nodes[shader], "Ref"),
  AssignValue(nodes[shader].ref, "./shader.toml"),
])
```

Order of blocks may vary.
