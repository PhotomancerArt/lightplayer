# SlotCodec Domain Serialization Roadmap

## Motivation

Serde-generated code for large domain models can become a major contributor to
embedded firmware size, and LightPlayer needs that flash and RAM budget for the
on-device GLSL JIT. The slot system already models the domain: records, values,
maps, options, enums, defaults, revisions, editor hints, and paths.
Serialization for slot-authored project/node definitions should therefore be a
projection of that slot model instead of a parallel Serde model.

The migration is practical, not ceremonial. The goal is not to delete Serde
everywhere; it is to keep per-domain-model firmware cost low by moving authored
slot data onto generic SlotCodec paths. Serde remains acceptable for protocol
shells, tests, host tooling, and small non-slot surfaces when measurement shows
the cost is flat.

## Architecture

The desired shape is:

```text
JSON / TOML
    |
    v
SyntaxEventSource
    |
    v
SlotReader / SlotWriter
    |
    v
SlotShapeRegistry
    |
    v
SlotAccess / SlotMutAccess
    |
    v
Concrete model object
```

For reads, the registry creates a default object for a shape id, then generic
slot mutation applies parsed fields. For writes, generic slot writers walk
`SlotAccess` against the registered shape.

Codegen should generate shape, access, factory, and view machinery. It should
not generate full per-type JSON/TOML parsers unless a small custom semantic leaf
handler is truly needed.

Structured enum fields use `EnumSlot<T>`. Atomic enum values use
`ValueSlot<T>` backed by `LpValue::Enum`. Raw Rust enums do not carry slot
revision state.

## Serialization Policy

- Slot-authored domain data loads and writes through SlotCodec on firmware.
- Serde is allowed for wire envelopes, small protocol wrappers, tests, and host
  tooling.
- Avoid adding serde-derived firmware parse paths for full slot/domain trees.
- Keep large payloads on manual streaming writers where buffering would hurt
  RAM.
- Treat firmware size as a measurement gate, not a theory.

## Migration Strategy

Use "switch it and fix it":

1. Keep serde derives and annotations in place temporarily.
2. Pick a real path.
3. Switch that path from serde to slot codec.
4. Fix tests and behavior until the path works.
5. Repeat for the next path.
6. Remove serde-derived behavior only when a concrete path no longer needs it
   and measurement supports the cleanup.

## Alternatives Considered

- Remove serde derives first: rejected because it creates too much breakage
  before proving the replacement paths.
- Remove Serde wholesale from `lpc-model`: deferred/rejected for now. Firmware
  measurement after M3 showed `serde_core` is a modest flat cost while
  SlotCodec already reduced `lpc_model` code size substantially.
- Generate Serde-like per-type parsers: rejected because binary size is a core
  motivation and generic slot mutation now exists.
- Keep `#[slot(enum)]` for raw enum fields: rejected because raw enum fields
  have no honest active-variant revision boundary.

## Risks

- Slot writer syntax may diverge from current authored TOML or wire JSON in
  ways that break higher layers.
- Semantic leaf codecs may need more custom syntax than expected.
- Message envelopes may live partly outside `lpc-model`, so M2 may uncover
  cross-crate boundaries.
- Keeping Serde available can accidentally reintroduce per-type firmware bloat
  if new authored domain paths use serde-derived parsing instead of SlotCodec.
- Slot infrastructure and wire snapshots still derive serde in places; this is
  acceptable while bloat remains flat, but should be revisited if measurements
  regress.

## Scope Estimate

This is a four-milestone migration:

- M1 removes confusing enum transitional code.
- M2 switches one real JSON/message path.
- M3 switches authored definition/artifact loading.
- M4 stabilizes the policy, removes obsolete source crates, validates firmware
  size, and records the decision not to remove Serde wholesale yet.
