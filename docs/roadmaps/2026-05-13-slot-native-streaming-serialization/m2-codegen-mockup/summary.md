# M2 Summary: Codegen In The Mockup

## What was built

- Added a generated slot-codec module for the mockup crate, emitted from
  `lpc-slot-codegen` into `OUT_DIR`.
- Proved generated JSON write, JSON read, and TOML read for a representative
  source bundle covering records, maps, options, discriminators, fixed arrays,
  and single-value enum-style bindings.
- Added a real mockup `ProjectDef` slice that reads current authored TOML and
  round-trips generated JSON through the slot-native codec.
- Added a real mockup `OutputDef` slice that reads current authored TOML,
  round-trips generated JSON, and proves default leaf handling by reading a
  partial `options` object over `OutputDriverOptionsConfig::default()`.
- Moved writer map and fixed-array loops into `lpc_model::slot_codec` so
  generated code can call shared helpers instead of carrying repeated loops.
- Split the mockup codec generator into named template sections so each area
  can be replaced by metadata-driven renderers independently.

## Decisions for future reference

#### Shared writer helpers

- **Decision:** Keep common writer loops in `SlotJsonValue` helpers.
- **Why:** This mirrors the reader helper shape and keeps generated code smaller
  for maps and fixed arrays.
- **Rejected alternatives:** Emitting `write_string_map` and `write_f32_array`
  into every generated module.
- **Revisit when:** Monomorphization of helper closures shows up as a binary
  size problem.

#### Real struct access

- **Decision:** The real `ProjectDef` slice uses intentional construction and
  access APIs instead of making fields public wholesale.
- **Why:** Serialization should not force the domain model to expose internal
  field layout everywhere.
- **Rejected alternatives:** Making every slot-backed model field public for
  generated codec convenience.
- **Revisit when:** Codegen moves from the mockup blob into derive or
  module-local generated code.

#### Default and missing field policy

- **Decision:** Use a default instance as the source of omitted leaf values, then
  override only fields present in the stream.
- **Why:** The generated `OutputDef` slice proves this works for nested option
  records while preserving slot-backed constructors.
- **Rejected alternatives:** Treating every missing field as an error, or
  requiring every serde default to become a bespoke generated branch.
- **Revisit when:** Generated code needs defaults that cannot be represented by
  `Default`.

## Rough edges before M3

- `render_mockup_slot_codec()` is now sectioned, but those sections are still
  hardcoded generated-output templates. The next step is replacing one section
  at a time with renderers over discovered slot metadata.
- The generated file is currently about 645 lines for the M2 slice. Some of
  that is test fixture type definition, but the real generator should keep
  pushing repeated read/write policy into shared helpers.
- Private field access is now visible as a real design pressure. `ProjectDef`
  needed only `NodeInvocationDef::artifact()`, while `OutputDef` needed
  intentional codec constructors and value accessors. Broader roots likely need
  either module-local generation, derive-emitted codec impls, or explicit
  constructors/accessors.
- Cleanup scan found existing print-heavy mockup tests outside the new codec
  path; no new scratch/debug output was added to `slot_codec` or codegen.
