# M1.2 Summary

M1.2 proved the authored-serde slice in the mockup before the real source model
cutover.

## What Was Built

- `ValueSlot<T>` serializes as `T` and deserializes with the ambient
  `current_state_version()`.
- `MapSlot<K,V>` serializes as an authored map and deserializes string, `u32`,
  and `i32` keys through the slot key conversion boundary.
- `OptionSlot<T>` serializes as `Option<T>` and deserializes with a stamped
  presence version.
- All current semantic slots under `lpc-model/src/slot/slots/` support clean
  authored serde.
- Mockup source defs now derive serde directly on slot-aware Rust models.
- Mockup fixture mapping now includes a source-like `path_points` variant with
  stable-key point maps and a nested path enum.
- A generated evidence harness writes representative `project.toml`,
  `shader.toml`, `fixture.toml`, `output.toml`, and `texture.toml` to:
  `target/slot-mockup-evidence/source-basic/`.

## Evidence

The generated fixture evidence shows the stable-key authored shape:

```toml
[mapping]
kind = "path_points"

[mapping.points.1]
position = [0.1, 0.2]
intensity = 1.0

[mapping.path]
kind = "ring_array"
rings = 2
points_per_ring = 96
clockwise = true
```

The generated shader evidence shows shader param defs as an authored map:

```toml
[param_defs.exposure]
label = "Exposure"
description = "Output exposure multiplier"
value_type = "f32"
default = 1.0
```

## Notes For M2

- Real `lpc-source` conversion can use the same model: slot-aware domain
  structs as the source of truth, with serde on wrappers rather than adapter
  objects.
- TOML table keys are strings at the serde boundary, so typed numeric slot maps
  must keep using authored-key parsing.
- Source fixture arrays should become stable-key maps during the real cutover.
  This is deliberate churn, not a compatibility problem.
- The ambient state version works for loading and tests, but tests should avoid
  exact global-version assertions unless they serialize execution.

## Decisions For Future Reference

#### Authored Serde Lives On Slot Wrappers

- **Decision:** Typed slot wrappers serialize as clean authored values, not as
  `Versioned<T>` structures.
- **Why:** The Rust domain model remains the source of truth while TOML stays
  hand-authorable and readable.
- **Rejected alternatives:** Permanent source/wire adapter structs; exposing
  version internals in authored files.

#### Numeric Map Keys Parse At The Authored Boundary

- **Decision:** `MapSlot<u32, _>` and `MapSlot<i32, _>` serialize through string
  map keys and parse those keys during deserialization.
- **Why:** TOML table keys are strings, but the slot model still needs typed
  stable ids.
- **Rejected alternatives:** Downgrading all authored map ids to strings;
  avoiding numeric ids in source-like data.

#### Fixture Arrays Become Stable-Key Maps

- **Decision:** The mockup fixture mapping uses `mapping.points.<id>` tables
  rather than arrays.
- **Why:** Stable-key maps match slot versioning and client pruning semantics.
- **Revisit when:** Real fixture authoring needs an ergonomic UI projection over
  map-backed data.
