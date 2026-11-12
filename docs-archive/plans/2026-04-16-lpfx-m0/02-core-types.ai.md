# Phase 2: Core Types

## Goal
Define the public-facing types that represent a parsed, validated effect
manifest. No parsing logic yet — just the data structures.

## Files

### 2.1 `lpfx/lpfx/src/error.rs`

```rust
#[derive(Debug)]
pub enum FxError {
    TomlParse(toml::de::Error),
    MissingField { section: &'static str, field: &'static str },
    InvalidType { input: String, found: String },
    DefaultTypeMismatch { input: String, expected: String, found: String },
    ValidationError(String),
}
```

Use `alloc::string::String` for owned messages. No `std::error::Error`
impl (no_std), but `Display` via `core::fmt`.

### 2.2 `lpfx/lpfx/src/manifest.rs`

```rust
pub struct FxManifest {
    pub meta: FxMeta,
    pub resolution: FxResolution,
    pub inputs: BTreeMap<String, FxInputDef>,
}

pub struct FxMeta {
    pub name: String,
    pub description: Option<String>,
    pub author: Option<String>,
    pub tags: Vec<String>,
}

pub struct FxResolution {
    pub width: u32,
    pub height: u32,
}
```

### 2.3 `lpfx/lpfx/src/input.rs`

```rust
pub struct FxInputDef {
    pub input_type: FxInputType,
    pub label: Option<String>,
    pub default: Option<FxValue>,
    pub min: Option<FxValue>,
    pub max: Option<FxValue>,
    pub presentation: Option<FxPresentation>,
    pub choices: Option<Vec<FxChoice>>,
    pub unit: Option<String>,
    pub role: Option<String>,
}

pub enum FxInputType {
    F32,
    I32,
    Bool,
    Vec3,
    Color,
    Palette,
}

pub enum FxPresentation {
    Slider,
    Toggle,
    Choice,
    ColorPicker,
    PalettePicker,
}

pub struct FxChoice {
    pub value: i32,
    pub label: String,
}

pub enum FxValue {
    F32(f32),
    I32(i32),
    Bool(bool),
    Vec3([f32; 3]),
}
```

### 2.4 Update `lib.rs`

```rust
#![no_std]
extern crate alloc;

pub mod error;
pub mod input;
pub mod manifest;
```

## Validation
- `cargo check -p lpfx` succeeds.
- All types are accessible from `lpfx::manifest::FxManifest`, etc.
