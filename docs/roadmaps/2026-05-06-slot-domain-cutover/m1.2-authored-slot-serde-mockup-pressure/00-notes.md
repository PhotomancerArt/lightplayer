# M1.2 Authored Slot Serde Mockup Pressure Notes

## Scope Of Work

M1.2 is pre-work for M2. It should use `lpc-slot-mockup` to pressure authored
source-data needs before converting real `lpc-source` definitions.

In scope:

- Make typed slot wrappers usable as authored serde fields:
  - `ValueSlot<T>`
  - `MapSlot<K,V>`
  - `OptionSlot<T>`
  - semantic slots under `lpc-model/src/slot/slots/`
- Prove that mockup source defs can deserialize from authored TOML-like data,
  serialize back to clean authored data, and still expose `SlotAccess`.
- Add mockup source fixtures/tests that resemble the real source model more
  closely before applying the pattern to `lpc-source`.
- Use the mockup to pressure:
  - string/path/ref semantic leaves,
  - maps as authored tables,
  - options as authored optional tables/values,
  - enums as authored discriminated structures,
  - stable-key maps instead of arrays for source-shaped collections.
- Keep `lpc-model` `no_std + alloc`.

Out of scope:

- Converting real `lpc-source` defs.
- Changing the engine project loader.
- Replacing legacy project sync.
- Client mutation.
- Final generic UI work.

## User Notes And Decisions

- User suggested this is probably M1.2 rather than M2.
- User wants the real M2 cutover to be driven by mockup pressure first, so the
  domain needs are shaken out before the real source model is churned.
- M1.1 made derive inference clean enough that mockup source structs can look
  like the intended Rust domain model without field-level macro noise.
- The architectural direction remains: source defs should become slot-aware
  authored domain objects, not plain structs plus permanent adapters.
- The mockup is allowed to be a pressure harness. It can be more experimental
  than `lpc-source`, but it should use real `lpc-model` APIs.
- The repo prefers filesystem-oriented, concept-per-file organization.

## Current Codebase State

### M1.1 Outcome

- `FieldSlot` exists.
- `#[derive(SlotRecord)]` includes fields by default and supports
  `#[slot(root)]`.
- `ValueSlot<T>`, `MapSlot<K,V>`, `OptionSlot<T>`, derived records, and
  semantic slots implement field access.
- `lpc-slot-mockup` source derives are clean and use slot wrappers directly.
- Semantic slot files are terse under `lpc-model/src/slot/slots/`, for example
  `ratio.rs`, `source_path.rs`, and `resource_ref.rs`.

### Missing For Authored Source Data

- `ValueSlot<T>` currently stores `Versioned<T>` and exposes slot access, but
  does not serialize/deserialize as the authored inner value.
- `MapSlot<K,V>` currently stores `BTreeMap<K,V>` plus a key-set version, but
  does not serialize/deserialize as a normal authored map.
- `OptionSlot<T>` currently stores an optional value plus a presence version,
  but does not serialize/deserialize as a normal authored option.
- Semantic slots wrap `Versioned<T>` and expose metadata/access, but do not yet
  deserialize from authored strings/scalars/records.
- `SourcePathSlot` and `ArtifactPathSlot` currently store `String`; real source
  uses `LpPathBuf` and `ArtifactLocator`, so the mockup can either stay stringy
  for M1.2 or pressure more precise semantic wrappers.

### Mockup Source Model

- `lpc-slot-mockup/src/source` currently contains clean slot-aware source defs:
  - `ProjectDef`
  - `NodeInvocationDef`
  - `ShaderDef`
  - `ShaderParamDef`
  - `FixtureDef`
  - `OutputDef`
  - `TextureDef`
- The mockup is already traversed, synced, diffed, and mutated through real
  slot APIs.
- The mockup does not yet prove authored TOML serde for those defs.
- Mockup fixture mapping is still simpler than real `lpc-source`.

### Real Source Model Pressure

Real `lpc-source` remains plain serde structs:

- `ProjectDef.nodes: BTreeMap<NodeName, NodeInvocation>`
- `NodeInvocation.artifact: ArtifactLocator`
- `TextureDef.width/height`
- `ShaderDef.glsl_path: LpPathBuf`
- `ShaderDef.texture_loc: RelativeNodeRef`
- `OutputDef::GpioStrip { pin, options }`
- `FixtureDef.mapping` contains `Vec<PathSpec>` and `Vec<u32>` ring counts.

The real conversion should wait until M1.2 proves that the slot wrappers can
act as authored serde fields without awkward custom code in every source def.

## Key Implementation Tensions

### Version After Deserialization

Deserializing authored data must create versioned slot fields. The likely
default is to stamp deserialized fields with `current_state_version()`.

Suggested direction: implement deserialization for typed wrappers using the
ambient version. Tests can set `set_current_state_version(FrameId::new(N))`
before parsing and assert parsed fields carry version `N`.

### Authored Serde Versus Wire Serde

`SlotData` is the wire/snapshot representation and includes versions. Typed
slot wrappers should serialize as clean authored values, not as `Versioned<T>`
objects.

Suggested direction: implement serde on wrappers as transparent authored data:

- `ValueSlot<T>` -> `T`
- `MapSlot<K,V>` -> map entries
- `OptionSlot<T>` -> `Option<T>`
- semantic slots -> their underlying authored representation

### Semantic Slot Inner Types

Some semantic slots currently store convenient model-facing types:

- `SourcePathSlot(String)`
- `ArtifactPathSlot(String)`
- `RelativeNodeRefSlot(RelativeNodeRef)`
- `ColorOrderSlot(ColorOrderValue)`
- `ResourceRefSlot(ResourceRef)`

M1.2 needs to decide whether to keep source/artifact paths string-backed for
the mockup or add more precise wrappers before M2.

Suggested direction: keep mockup path slots string-backed for the first serde
slice, but add tests that make the authored format explicit. Add a future note
or M2 task to decide whether real source needs `LpPathBufSlot` and
`ArtifactLocatorSlot`.

### Arrays Versus Stable-Key Maps

Real fixture mapping uses arrays, but the slot model intentionally favors
stable-key maps for versioned structure. M1.2 can pressure this without changing
real examples.

Suggested direction: reshape or extend the mockup fixture mapping to include a
source-like `PathPoints` variant with keyed path maps and keyed ring-count maps.
Use this to prove the authored TOML syntax for maps before changing
`examples/basic`.

## Open Questions

### Q1. Should M1.2 strictly avoid real `lpc-source` changes?

Context: The point of M1.2 is to shake out model/wrapper/serde needs in the
mockup before applying them to real defs.

Answer: yes. This is prep-work. Limit real production code changes to
`lpc-model` and `lpc-slot-mockup`. Do not convert `lpc-source` until M2.

### Q2. Should deserialized slot wrappers use `current_state_version()`?

Context: Authored file load needs a version boundary. Passing a mutation/load
context everywhere would be noisy and conflicts with the current ambient
version direction.

Answer: yes. `Deserialize` for typed slot wrappers should stamp data with
`current_state_version()`.

### Q3. Should M1.2 add serde for all semantic slots or only the ones used by
the mockup?

Context: Adding serde for all semantic slots is broader, but it prevents M2
from discovering obvious gaps one at a time.

Answer: add serde for all current semantic slots in
`lpc-model/src/slot/slots/`, with tests near each concept or grouped in a slot
serde test. The default rule should be that every semantic slot leaf is
authorable unless explicitly documented otherwise.

### Q4. Should mockup fixture mapping become more source-like in M1.2?

Context: Real fixture mapping is the hardest source shape because it uses
arrays and nested enums. If the mockup does not pressure that shape, M2 will
still carry the largest uncertainty.

Answer: yes, but keep it bounded. Add a source-like mapping variant with keyed
maps rather than trying to mirror every real fixture detail. The array-to-map
conversion is accepted pain: stable keys are the slot-domain rule, and M1.2
should prove the authored TOML shape before M2 changes real source examples.

### Q5. Should M1.2 include a TOML fixture directory for the mockup?

Context: String-based tests are quick, but files make it easier to see the
authored shape directly and compare it to `examples/basic`.

Answer: generate evidence files rather than hand-maintained fixtures. We want
to see what the authored TOML looks like, but generated fixtures are better
than another source of truth. Build representative Rust mockup source defs,
serialize them to TOML, write the evidence to a gitignored path, print the path
or TOML summary in tests, parse it back, and verify slot traversal/sync survives
the round trip. Hand-authored fixture files remain an option if generated output
is hard to inspect.

## Suggested Deliverable

At the end of M1.2, the mockup should have a test that:

1. sets an ambient source-load version,
2. loads mockup project/shader/fixture/output/texture defs from TOML fixtures,
3. registers shapes,
4. snapshots source roots through the real slot access sync path,
5. verifies the authored shape is clean TOML while the runtime slot snapshot has
   versions,
6. round-trips at least one source def back to TOML without serializing
   `Versioned<T>` internals.
