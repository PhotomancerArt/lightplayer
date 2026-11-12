# SlotData, SlotTree, And Validation

## Scope Of Phase

Add slot data instance types, rooted slot trees, traversal, and basic
shape/data validation.

In scope:

- Add `slot_data.rs`.
- Add `slot_tree.rs`.
- Add `SlotData`, `SlotRecord`, `SlotMap`, `SlotEnum`, `SlotOption`.
- Add `SlotTree`.
- Add traversal by `SlotPath`.
- Add recursive validation against `SlotRegistry`.

Out of scope:

- Rich static/dynamic authoring helpers.
- Derive macros.
- Wire deltas/patches.
- Runtime mutation APIs.

## Code Organization Reminders

- Keep instance data in `slot_data.rs`.
- Keep rooted traversal in `slot_tree.rs`.
- Keep validation helpers near the type that owns the validation operation
  unless a separate private helper makes readability much better.
- Tests stay at the bottom of files.
- Add comments only for non-obvious invariants.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-model/src/slot/slot_data.rs`
- `lp-core/lpc-model/src/slot/slot_tree.rs`
- `lp-core/lpc-model/src/slot/slot_shape.rs`
- `lp-core/lpc-model/src/slot/slot_registry.rs`
- `lp-core/lpc-model/src/slot/mod.rs`
- `lp-core/lpc-model/src/lib.rs`
- `lp-core/lpc-model/src/prop/model_value.rs`
- `lp-core/lpc-model/src/prop/model_type.rs`

Suggested types:

```rust
pub enum SlotData {
    Value(Versioned<ModelValue>),
    Record(SlotRecord),
    Map(SlotMap),
    Enum(SlotEnum),
    Option(SlotOption),
}

pub struct SlotRecord {
    pub fields: Vec<SlotData>,
}

pub struct SlotMap {
    pub entries: BTreeMap<SlotMapKey, SlotData>,
}

pub struct SlotEnum {
    pub variant: SlotName,
    pub data: Box<SlotData>,
}

pub enum SlotOption {
    None,
    Some(Box<SlotData>),
}

pub struct SlotTree {
    pub shape: SlotShapeId,
    pub root: SlotData,
}
```

Map key warning:

- `SlotMapKey` is intentionally constrained; do not use `ModelValue` as the map
  key.
- Start with:

```rust
pub enum SlotMapKey {
    String(String),
    I32(i32),
    U32(u32),
}
```

- The roadmap wants maps for stable identity, not arbitrary JSON-like key
  values.

Traversal:

- `SlotTree::get(&SlotRegistry, &SlotPath)` should at least traverse records
  and maps.
- Traversal takes the registry because record field names live in `SlotShape`,
  while `SlotRecord` stores only indexed field data.
- For enum/option traversal, implement the minimal clear behavior:
  - root path returns the enum/option node,
  - deeper traversal through active variant/some value is acceptable if simple,
  - avoid clever path encodings that M2 may regret.

Validation:

Add an API similar to:

```rust
impl SlotRegistry {
    pub fn validate_tree(&self, tree: &SlotTree) -> Result<(), SlotError>;
}
```

Validation should check:

- tree shape exists,
- data variant matches shape variant,
- `ModelValue` matches `ModelType`,
- record field count matches shape fields and indexed fields match the corresponding shape field,
- map keys match SlotMapKeyShape and values match value shape,
- enum variant exists and data matches variant shape,
- option some data matches the some shape.

Error shape:

- Add a focused slot error type if needed, e.g. `SlotError`.
- It can start simple but should include enough context for tests and debugging.

Tests:

- Validate a simple value tree.
- Validate nested record.
- Validate map with stable keys.
- Validate enum variant and option none/some.
- Reject missing shape id.
- Reject variant mismatch, missing/unknown record fields, bad map key, and bad
  model value type.

## Validate

```bash
cargo test -p lpc-model
```
