# Milestone 2 — Texture Filetests Design

# Scope of Work

Extend `lps-filetests` so backend-neutral `.glsl` tests can declare texture
binding specs, inline texture fixture data, and texture-specific diagnostics.

This milestone proves the filetest-facing texture resource model. It does not
implement texture sampling behavior; `texelFetch` / `texture` execution belongs
to later milestones.

# File Structure

```text
lp-shader/
├── lpvm/
│   └── src/
│       └── set_uniform.rs                 # UPDATE: accept typed Texture2D uniform values
├── lps-filetests/
│   ├── filetests/
│   │   └── textures/                      # NEW: fixture + diagnostic .glsl tests
│   └── src/
│       ├── parse/
│       │   ├── mod.rs                     # UPDATE: parse texture directives
│       │   ├── test_type.rs               # UPDATE: TestFile texture specs/fixtures
│       │   └── parse_texture.rs           # NEW: texture-spec/data grammar
│       └── test_run/
│           ├── filetest_lpvm.rs           # UPDATE: retain engine memory for fixture alloc
│           ├── run_detail.rs              # UPDATE: bind fixtures before set_uniform/run
│           └── texture_fixture.rs         # NEW: validate, encode, allocate, bind fixtures
└── lps-shared/
    └── src/
        └── texture_format.rs              # likely unchanged; reuse shared texture types
```

# Filetest Surface

Minimal intended shape:

```glsl
// test run
// target: rv32n.q32
// @unimplemented(rv32n.q32): texelFetch lowering lands in M3

// texture-spec: inputColor format=rgba16unorm filter=nearest wrap=clamp shape=2d
// texture-data: inputColor 2x1 rgba16unorm
//   1.0,0.0,0.0,1.0 0.0,1.0,0.0,1.0

uniform sampler2D inputColor;

vec4 sample_first() {
    return texelFetch(inputColor, ivec2(0, 0), 0);
}

// run: sample_first() ~= vec4(1.0, 0.0, 0.0, 1.0)
```

`texture-spec` declares the compile-time binding contract:

- `format=r16unorm|rgb16unorm|rgba16unorm`
- `filter=nearest|linear`
- `wrap=clamp|repeat|mirror-repeat` as shorthand for both axes
- optional `wrap_x=` / `wrap_y=` for axis-specific policy if cheap to support
- `shape=2d|height-one|height_one`

`texture-data` declares a runtime fixture:

- Header: `// texture-data: <name> <width>x<height> <format>`
- Pixels are separated by whitespace.
- Channels inside a pixel are comma-separated with no spaces.
- Normalized float channels are preferred for readable tests.
- Exact hex channels are accepted for precision/boundary cases.
- For M2 formats, exact hex channels are 4-digit unorm16 storage values. Future
  unorm8 formats should use exact hex width appropriate to the storage channel.

# Conceptual Architecture

```text
.glsl file
  ├─ texture-spec directives
  ├─ texture-data directives + pixel rows
  └─ run directives

parse_test_file
  └─ TestFile {
       texture_specs: BTreeMap<String, TextureBindingSpec>,
       texture_fixtures: BTreeMap<String, TextureFixture>,
       run_directives: [...]
     }

compile_for_target
  ├─ lps_frontend::compile/lower
  ├─ validate texture specs against LpsModuleSig Texture2D uniforms
  └─ compile LPVM module while retaining backend engine memory

per // run:
  ├─ allocate shared memory for each fixture
  ├─ encode fixture pixels into texture storage bytes
  ├─ build LpsTexture2DDescriptor { ptr, width, height, row_stride }
  ├─ set_uniform("inputColor", LpsValueF32::Texture2D(descriptor))
  ├─ apply normal // set_uniform:
  └─ execute assertion
```

# Main Components

## Parser Model

`TestFile` gains file-level texture metadata:

- `texture_specs: BTreeMap<String, TextureBindingSpec>`
- `texture_fixtures: BTreeMap<String, TextureFixture>`

Texture specs are file-level because they describe the shader interface and are
validated at compile time. Texture fixtures are also file-level for M2; all
runs in the file use the same fixture set.

## Fixture Encoder

`texture_fixture.rs` owns fixture validation and byte encoding for:

- `R16Unorm`
- `Rgb16Unorm`
- `Rgba16Unorm`

Float channels convert through canonical unorm storage conversion. Hex channels
are exact stored values. The encoder validates pixel count, channel count, and
format consistency before any run executes.

## Compile-Time Texture Spec Validation

`lps-filetests` compiles through `lps-frontend` and LPVM directly, not through
`lp-shader::compile_px_desc`. The filetest harness therefore needs equivalent
validation after lowering:

- Every declared `sampler2D` / `LpsType::Texture2D` uniform must have a spec.
- Every spec name must match a declared texture uniform.
- Unsupported spellings in directives should fail with line-aware parse errors.

This should reuse or mirror the M1 validation behavior without pulling in
unnecessary pixel-shader synthesis.

## Runtime Binding

Runtime fixture descriptors use the normal typed uniform API:

```rust
set_uniform("inputColor", LpsValueF32::Texture2D(descriptor))
```

`lpvm::encode_uniform_write` and `encode_uniform_write_q32` should accept typed
`Texture2D` values. They should continue rejecting raw `UVec4` descriptor-shaped
stand-ins and opaque subpaths like `tex.ptr`.

## Diagnostics

Diagnostics split by source:

- Directive parse errors: malformed `texture-spec`, malformed `texture-data`,
  unsupported format/filter/wrap/shape spellings.
- Interface validation errors: missing spec, extra spec.
- Runtime fixture validation errors: missing fixture, format mismatch,
  `HeightOne` promised but fixture height is not 1.

Negative filetests should exercise these paths. Sampling execution tests can be
annotated `@unimplemented` until M3 implements `texelFetch`.

