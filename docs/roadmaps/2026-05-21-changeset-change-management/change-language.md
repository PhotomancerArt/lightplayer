# ChangeSet Change Language (v1)

Canonical edit vocabulary for client-driven changes. Lives in
`lpc-node-registry/src/edit/` — **serde types, not part of the slot system**.
Apply uses slot mut access + overlay tables; ops themselves are not `SlotData`.

## Top level

```text
ChangeSet { id, changes: Vec<ArtifactChange> }
```

Changes are **grouped by artifact**. Each block targets one file and lists ops
for that file only.

## Target

```rust
enum ArtifactTarget {
    Id(ArtifactId),     // committed artifact (optional; harness/wire rarely)
    Path(LpPathBuf),    // absolute project path — primary authoring form
}
```

**Implicit create:** resolving `Path(p)` get-or-creates a pending overlay entry
when `p` is not in base or overlay. No explicit `Create` op.

Overlay does not use base-store refcount rules; pending paths exist until commit
or discard.

## Ops (per artifact)

```rust
ArtifactChange {
    target: ArtifactTarget,
    ops: Vec<ArtifactOp>,
}
```

### File-level (`ArtifactOp`)

| Op | Use |
|----|-----|
| `Delete` | Remove this path on commit |
| `SetBytes(text)` | Whole-file body — GLSL, SVG, etc.; optional TOML import escape hatch |

Normal **node TOML** bodies are **not** authored with `SetBytes`. They come from
slot ops + slot codec serialize on commit.

### Slot-level (`ArtifactOp`)

Node defs are slots. All node editing is slot ops at a `SlotPath` **within** the
target artifact:

| Op | Use |
|----|-----|
| `VariantSet { path, variant }` | Enum variant switch (node kind, `Ref`/`Def`, nested enums) |
| `SetSlot { path, value }` | Value leaves only (scalars, path strings, etc.) |
| `MapInsert { path, key, … }` | Map entry |
| `MapRemove { path, key }` | Map entry |
| `OptionSet { path, present }` | Option some/none (`some` → shape default) |

Examples:

- Standalone shader file: `VariantSet(root, "Shader")` then scalar `SetSlot`s on `/shader.toml`
- Inline child: ops under `entries[2].node.def.Shader…` on `/playlist.toml`
- Wire child to file: `VariantSet(nodes[shader], "Ref")` + `SetSlot(nodes[shader].ref, "./shader.toml")`

Relative locators in slot values resolve against the **containing artifact path**
(same as `resolve_node_locator` today).

## Node invocation TOML (authored)

```toml
[nodes.shader]
ref = "./shader.toml"

[nodes.clock.def]
kind = "Clock"
```

Legacy `def = { path = … }` is rejected.

## Node TOML vs assets

Same `ArtifactChange` shape. Convention:

- **`.toml`** — slot ops; serialize to text on commit
- **`.glsl`, `.svg`, …** — typically `SetBytes` / `Delete`

## Creatability

Every `examples/*` project must be reachable from blank via a finite
`ChangeSet` sequence using only:

- `ArtifactChange { target: Path(...), ops: [...] }` (implicit create)
- Slot ops + `SetBytes` for assets
- No `CreateDef`; no pre-populated def blobs as the primary path

New node at artifact root: `VariantSet(root, "Shader")` (applies variant default),
then patch value leaves with `SetSlot`.

## Apply / commit

1. **Apply** — merge each `ArtifactChange` into path-keyed overlay; base untouched
2. **View** — `NodeDefView` / read API resolves `(path, slot_path)` over overlay ∪ base
3. **Commit** — serialize overlay TOML + assets → store/fs; registry re-derive →
   `SyncResult`
4. **Discard** — drop overlay; reads = base

Dangling path refs (wire before target file exists) are OK mid-sequence.

Unreferenced overlay paths may exist on disk after commit; registry only registers
defs reachable from root (same as filesystem reality).

## Example (add shader to project)

```text
ArtifactChange { target: Path("/shader.glsl"), ops: [ SetBytes("…") ] }

ArtifactChange {
  target: Path("/shader.toml"),
  ops: [
    VariantSet(root, "Shader"),
    SetSlot(root.source.path, "./shader.glsl"),
    …
  ],
}

ArtifactChange {
  target: Path("/project.toml"),
  ops: [
    VariantSet(nodes[shader], "Ref"),
    SetSlot(nodes[shader].ref, "./shader.toml"),
  ],
}
```

Order of blocks may vary.
