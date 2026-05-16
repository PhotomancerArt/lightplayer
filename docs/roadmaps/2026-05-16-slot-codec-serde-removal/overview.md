# Slot Codec Serde Removal Roadmap

## Motivation

Serde-generated code is a major contributor to embedded firmware size, and
LightPlayer needs that flash and RAM budget for the on-device GLSL JIT. The
slot system already models the domain: records, values, maps, options, enums,
defaults, revisions, editor hints, and paths. Serialization should become a
projection of that slot model instead of a parallel Serde model.

The migration should be practical, not ceremonial. The goal is not to delete
derives first; it is to stop depending on Serde for real behavior first.

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

## Migration Strategy

Use "switch it and fix it":

1. Keep serde derives and annotations in place temporarily.
2. Pick a real path.
3. Switch that path from serde to slot codec.
4. Fix tests and behavior until the path works.
5. Repeat for the next path.
6. Remove serde derives, helpers, tests, and dependencies only after the slot
   paths own behavior.

## Alternatives Considered

- Remove serde derives first: rejected because it creates too much breakage
  before proving the replacement paths.
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
- Removing serde from slot infrastructure may be harder than removing it from
  domain records because schema snapshots currently derive serde.

## Scope Estimate

This is a four-milestone migration:

- M1 removes confusing enum transitional code.
- M2 switches one real JSON/message path.
- M3 switches authored definition/artifact loading.
- M4 removes serde from `lpc-model` and validates the dependency cleanup.
