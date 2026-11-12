# M1.1 Notes: Manual Mockup Shape Slice

## Scope Of Work

M1 proved the low-level streaming reader/writer foundation. M1.1 should prove
that foundation against a mockup shape that looks like the real authored domain
before M2 starts codegen.

The goal is a hand-written serialization target in `lpc-slot-mockup` that
exercises the same concepts codegen will need to emit:

- slot roots
- node definition variants and node invocations
- nested records
- maps with string and numeric keys
- options, including present and absent values
- enum discriminators, including unit and record variants
- scalar leaves: `f32`, `u32`, `bool`, `String`
- fixed numeric arrays such as xy/white-point values
- vector/list values
- binding definitions and binding endpoints
- length-prefixed base64 binary payloads if the mockup keeps a wire payload

The plan should not implement codegen. The point is to write manual functions
that are intentionally shaped like the codegen target, then decide whether the
API still feels correct.

## Current Code State

Generic codec foundation now lives in `lp-core/lpc-model/src/slot_codec/`:

- `syntax.rs`: `SyntaxEvent`, `SyntaxEventSource`, `SyntaxError`, `SourceSpan`
- `json_syntax_source.rs`: streaming JSON event source
- `toml_syntax_source.rs`: TOML value adapter
- `slot_reader.rs`: streaming semantic reader
- `slot_json_writer.rs`: slot JSON writer plus `SlotJsonWrite`

`lpc-wire` re-exports these types from `lpc_wire::slot` for existing call sites.

The current mockup proof is
`lp-core/lpc-slot-mockup/src/tests/native_stream.rs`. It round-trips one small
`ManualWireConfig` through JSON and reads the same shape from TOML. It covers
objects, scalars, arrays, binary tuples, and a basic discriminator error, but it
does not cover the actual mockup source domain.

Existing mockup source/domain files:

- `lp-core/lpc-slot-mockup/src/source/project_def.rs`
- `lp-core/lpc-slot-mockup/src/source/shader_def.rs`
- `lp-core/lpc-slot-mockup/src/source/texture_def.rs`
- `lp-core/lpc-slot-mockup/src/source/fixture_def.rs`
- `lp-core/lpc-slot-mockup/src/source/output_def.rs`
- `lp-core/lpc-slot-mockup/src/source/mapping.rs`

Existing slot-shape TOML/JSON tests live in:

- `lp-core/lpc-slot-mockup/src/tests/storage_codec.rs`
- `lp-core/lpc-slot-mockup/src/tests/authored_serde.rs`

Those tests are useful references, but they exercise the old `SlotData` and
Serde paths. M1.1 should exercise `SlotReader` / `SlotJsonWriter` with manual
typed functions.

## User Notes

- The user wants an M1.1 before M2.
- Keep the proof in the mockup crate for clarity.
- The proof should manually read/write all basic shapes so we know the API
  makes sense before generating it.
- After this slice, M2 can move to codegen.

## Open Questions

### Q1. Should M1.1 write TOML, JSON, or both?

Suggested answer: write JSON with `SlotJsonWriter`, read JSON with
`JsonSyntaxSource`, and read TOML with `TomlSyntaxSource`. For TOML output,
write manual `toml::Value` builders in the mockup only if needed for evidence.
TOML authored data is expected to be small, so M1.1 does not need a streaming
TOML writer.

### Q2. Should the manual functions construct real mockup source structs or
parallel test-only structs?

Suggested answer: use real mockup source structs wherever their fields are
accessible enough. If private fields make that awkward, add narrow test-facing
constructors/accessors or create a test-only source bundle that mirrors the real
shape. The plan should record any mismatch as a domain-shape rough point.

### Q3. How complete is "all basic shapes"?

Suggested answer: cover the concepts listed in the scope matrix, not every
field of every real model type. M1.1 should prove representative coverage and
rough-codegen ergonomics, not duplicate the entire production loader by hand.
