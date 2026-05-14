# M2 Summary: Codegen In The Mockup

## What was built

- Added a generated slot-codec module for the mockup crate, emitted from
  `lpc-slot-codegen` into `OUT_DIR`.
- Proved generated JSON write, JSON read, and TOML read for a representative
  source bundle covering records, maps, options, discriminators, fixed arrays,
  and single-value enum-style bindings.
- Added a real mockup `ProjectDef` slice that reads current authored TOML and
  round-trips generated JSON through the slot-native codec.
- Moved writer map and fixed-array loops into `lpc_model::slot_codec` so
  generated code can call shared helpers instead of carrying repeated loops.

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

- **Decision:** The generated `ProjectDef` reader currently applies the
  universal omit-empty policy for optional/map fields and requires the
  discriminator first.
- **Why:** This matches the current authored TOML shape and keeps M2 focused on
  the streaming mechanics.
- **Rejected alternatives:** General default-instance mutation for every field
  in M2.
- **Revisit when:** `OutputDef` is generated, because it needs richer default
  leaf handling.

## Rough edges before M3

- `render_mockup_slot_codec()` is still a hardcoded generated-output template.
  The behavior is proven, but the next step is turning it into small renderers
  over discovered slot metadata.
- The generated file is currently about 455 lines for the M2 slice. Some of
  that is test fixture type definition, but the real generator should keep
  pushing repeated read/write policy into shared helpers.
- Private field access is now visible as a real design pressure. `ProjectDef`
  needed only `NodeInvocationDef::artifact()`, but broader roots likely need
  either module-local generation, derive-emitted codec impls, or explicit
  constructors/accessors.
- Cleanup scan found existing print-heavy mockup tests outside the new codec
  path; no new scratch/debug output was added to `slot_codec` or codegen.
