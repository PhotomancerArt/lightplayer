# Phase 01 — NodeInvocation Ref|Def Model

**Dispatch:** sub-agent: yes | parallel: -

## Scope of phase

Rewrite `NodeInvocation` as a **`Slotted` enum** with `Ref` and `Def` variants and
new TOML read/write.

**In scope:**

- `lpc-model/src/node/node_invocation.rs` — enum, helpers, TOML codec
- Remove `NodeDefRef` type from this file
- Remove `def_slot: ArtifactPathSlot` and record/custom shape hack
- `node/mod.rs` — stop exporting `NodeDefRef`
- `lib.rs` — drop `NodeDefRef` re-export

**Out of scope:** registry, engine, examples, `VariantSet`, diff.

## TOML contract

```toml
ref = "./shader.toml"          # Ref variant at current table

[....def]                      # Def variant
kind = "Clock"
```

Read: if table has `ref` key → `Ref`; if has `def` subtable or inline def fields → `Def`.
Error if both.

## Implementation details

**Enum shape (target):**

```rust
#[derive(Clone, Debug, PartialEq, Slotted)]
pub enum NodeInvocation {
    Ref(ArtifactSpecifier),  // or Ref { locator } if slotted derive needs named fields
    Def(NodeDef),
}
```

Use `#[slotted(...)]` / derive patterns consistent with `NodeDef` enum in
`nodes/node_def.rs`. Variant wire names: **`Ref`**, **`Def`**.

**TOML mapping:**

| Variant | Authored form |
|---------|----------------|
| `Ref` | `ref = "<locator>"` (string at invocation table) |
| `Def` | `[parent.def]` table with `kind = ...` |

Implement via slotted enum codec if possible; otherwise minimal custom
`FieldSlot` read/write on the enum only (not whole fake record).

**Helpers (replace old API):**

```rust
impl NodeInvocation {
    pub fn path(locator: ArtifactSpecifier) -> Self;
    pub fn inline(def: NodeDef) -> Self;
    pub fn ref_specifier(&self) -> Option<&ArtifactSpecifier>;
    pub fn inline_def(&self) -> Option<&NodeDef>;
}
```

Keep `path()` / `inline()` constructors for call-site ergonomics.

**Delete from `custom_slot_codec.rs`:** `NODE_INVOCATION_CODEC_ID` branches if enum
no longer uses custom codec.

**Default:** `Ref(ArtifactSpecifier::path(""))` or `Def(NodeDef::default())` — match
prior default behavior.

## Tests (this phase)

Add/update in `node_invocation.rs` `#[cfg(test)]`:

- `ref = "./texture.toml"` loads `Ref`
- `[def] kind = "Clock"` loads `Def`
- reject `ref` + `[def]` together
- reject legacy `def = { path = ... }`
- round-trip write/read for both variants

## Validate

```bash
cargo test -p lpc-model node_invocation
cargo check -p lpc-model
```

Expect compile failures in other crates until phase 03 — OK.
