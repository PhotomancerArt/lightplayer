# M1.1 Design: Manual Mockup Shape Slice

## Scope Of Work

Build a hand-written mockup source serialization slice that exercises the slot
codec against realistic domain shapes before M2 codegen.

In scope:

- Add a focused manual codec test surface in `lpc-slot-mockup`.
- Read/write a mock source bundle with multiple slot roots.
- Cover roots, nested records, maps, options, enums, arrays, scalar leaves,
  bindings, and discriminator errors.
- Use the same reader/writer style that M2 codegen should emit.
- Record rough points where the real/mockup domain shape resists clean codec
  generation.

Out of scope:

- Generated code.
- Production loader/message adoption.
- Removing Serde from existing mockup/domain paths.
- A general TOML writer in `lpc-model` unless the manual slice proves it is
  immediately necessary.

## File Structure

```text
lp-core/lpc-slot-mockup/src/
  tests/
    native_stream.rs              existing small codec foundation proof
    manual_shape_codec.rs         new M1.1 hand-written source-domain proof
    mod.rs                        add test module

docs/roadmaps/2026-05-13-slot-native-streaming-serialization/
  m1-parser-generator-foundation/
    m1.1-manual-mockup-shape-slice/
      00-notes.md
      00-design.md
      01-shape-coverage-fixture.md
      02-manual-reader-writer.md
      03-cleanup-validation.md
      summary.md                  created after implementation
```

## Architecture Summary

M1.1 adds a test-only manual codec layer that is deliberately boring and
codegen-shaped:

```text
sample mock source bundle
        │
        ├── write_*_json(&mut SlotJsonWriter)
        │       └── JSON bytes
        │             └── JsonSyntaxSource
        │                   └── read_*(&mut SlotReader)
        │
        └── manual TOML Value sample
                └── TomlSyntaxSource
                      └── same read_*(&mut SlotReader)
```

The important property is that JSON and TOML share the same typed read
functions. The writer side starts with JSON because the embedded RAM pressure is
mainly on streaming wire messages. TOML may use `toml::Value` samples/builders
inside the mockup because disk-authored files are small and already parsed into
a value tree.

## Main Components

### Source Bundle Fixture

Create a representative test fixture containing:

- a `ProjectDef`-like root with `name` and `nodes`
- `NodeInvocationDef` entries pointing at node artifacts
- a node-definition enum wrapper with variants for output, texture, shader,
  and fixture
- non-empty bindings using both ref-style endpoints and explicit value/literal
  endpoints if the current model supports them cleanly
- fixture mapping variants that cover a record enum variant and a unit enum
  variant
- nested path specs with numeric-key maps and ring lamp counts
- output options as an option-wrapped record
- one absent option field

### Manual Readers

Manual readers should be shaped like generated code:

- `read_project_def(reader) -> Result<ProjectDefLike, SyntaxError>`
- `read_node_def(reader) -> Result<NodeDefLike, SyntaxError>`
- `read_mapping_config(reader) -> Result<MappingConfigLike, SyntaxError>`
- `read_path_spec(reader) -> Result<PathSpecLike, SyntaxError>`
- helper functions for optional fields, maps, arrays, binding endpoints, and
  required-field errors

Records should use `object().next_prop()?` plus a generated-looking `match`.
Enums should use `start_object()` followed by
`expect_discriminator("kind")?.string()?`.

### Manual Writers

Manual JSON writers should mirror the reader:

- `write_project_def_json(value, object)`
- `write_node_def_json(value, object)`
- `write_mapping_config_json(value, object)`
- `write_path_spec_json(value, object)`
- helper functions for maps, arrays, options, bindings, and scalar leaves

The JSON output should be parsed back with `JsonSyntaxSource` and compared to
the original fixture.

### Rough-Point Notes

The implementation should add a short `summary.md` after the work lands. It
should list:

- APIs that felt codegen-friendly
- APIs that felt awkward
- domain shapes that required test-only workarounds
- deviations from current real TOML/JSON worth considering before M2
