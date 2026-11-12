# Phase 3: TOML Parsing + Validation

## Goal
Implement two-phase manifest parsing: raw TOML deserialization into
intermediate structs, then validation + conversion into typed
`FxManifest`. All errors surface as `FxError`.

## Files

### 3.1 `lpfx/lpfx/src/parse.rs`

**Raw structs** (private):

```rust
#[derive(Deserialize)]
struct RawManifest {
    meta: RawMeta,
    resolution: Option<RawResolution>,
    input: Option<BTreeMap<String, RawInputDef>>,
}

#[derive(Deserialize)]
struct RawMeta {
    name: String,
    description: Option<String>,
    author: Option<String>,
    tags: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct RawResolution {
    width: Option<u32>,
    height: Option<u32>,
}

#[derive(Deserialize)]
struct RawInputDef {
    #[serde(rename = "type")]
    input_type: String,
    label: Option<String>,
    default: Option<toml::Value>,
    min: Option<toml::Value>,
    max: Option<toml::Value>,
    presentation: Option<String>,
    choices: Option<Vec<RawChoice>>,
    unit: Option<String>,
    role: Option<String>,
}

#[derive(Deserialize)]
struct RawChoice {
    value: i32,
    label: String,
}
```

**Public API**:

```rust
pub fn parse_manifest(toml_src: &str) -> Result<FxManifest, FxError>
```

**Validation rules**:

- `meta.name` is required and non-empty.
- Each input's `type` must be one of: `f32`, `i32`, `bool`, `vec3`,
  `Color`, `Palette`.
- If `default` is present, it must be compatible with the declared type
  (e.g. `f32` input expects a float value).
- `min`/`max` only valid for numeric types (`f32`, `i32`).
- `presentation = "choice"` requires a non-empty `choices` array.
- `choices` entries must have `value` compatible with input type.
- Resolution defaults to `{ width: 512, height: 512 }` if omitted.

### 3.2 `lpfx/lpfx/src/module.rs`

```rust
pub struct FxModule {
    pub manifest: FxManifest,
    pub glsl_source: String,
}

impl FxModule {
    pub fn from_sources(toml_src: &str, glsl_src: &str) -> Result<Self, FxError> {
        let manifest = parse_manifest(toml_src)?;
        Ok(Self {
            manifest,
            glsl_source: String::from(glsl_src),
        })
    }
}
```

### 3.3 Tests (in `lib.rs` or `parse.rs`)

- **Happy path**: Parse `noise.fx/fx.toml` content, assert all inputs
  are present with correct types and defaults.
- **Missing meta.name**: Returns `FxError::MissingField`.
- **Bad type string**: Returns `FxError::InvalidType`.
- **Default type mismatch**: e.g. `type = "f32"` with `default = true`
  → `FxError::DefaultTypeMismatch`.
- **Choice without choices array**: Returns `FxError::ValidationError`.
- **Minimal manifest**: Only `[meta]` with name, no inputs, no
  resolution — succeeds with defaults.

## Validation
- `cargo test -p lpfx` — all tests pass.
- Error messages are clear and actionable.
