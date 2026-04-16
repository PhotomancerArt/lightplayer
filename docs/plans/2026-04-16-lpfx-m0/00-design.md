# LPFX M0 — Scaffold + First Effect — Design

## Scope of Work

Create the `lpfx/lpfx` crate with core types, TOML manifest parsing,
validation, and the first `.fx` effect module on disk (`noise.fx`).
No compilation or rendering — that's M1/M2.

Roadmap context: `docs/roadmaps/2026-04-15-lpfx/`

## File Structure

```
lpfx/
└── lpfx/
    ├── Cargo.toml                    # NEW: no_std + alloc
    └── src/
        ├── lib.rs                    # NEW: #![no_std], mod declarations, tests
        ├── manifest.rs               # NEW: FxManifest, FxMeta, FxResolution
        ├── input.rs                  # NEW: FxInputDef, FxInputType, FxPresentation, FxValue
        ├── module.rs                 # NEW: FxModule::from_sources()
        ├── parse.rs                  # NEW: RawManifest → FxManifest, TOML deser + validation
        └── error.rs                  # NEW: FxError enum

examples/
└── noise.fx/
    ├── fx.toml                       # NEW: manifest with 6 inputs
    └── main.glsl                     # NEW: adapted from rainbow.glsl with uniforms

Cargo.toml                            # UPDATE: workspace members + deps
```

## Conceptual Architecture

```
fx.toml (TOML string)    main.glsl (GLSL string)
        │                        │
        ▼                        │
   toml::from_str                │
        │                        │
        ▼                        │
   RawManifest                   │
        │                        │
        ▼                        │
   validate + convert            │
        │                        │
        ▼                        ▼
   FxManifest ──────────► FxModule
   ├── meta                ├── manifest
   ├── resolution          └── glsl_source
   └── inputs: BTreeMap
       └── FxInputDef
           ├── input_type: FxInputType
           ├── label, range, default
           ├── presentation
           └── choices, unit
```

## Main Components

### `FxModule`
Entry point. Created via `FxModule::from_sources(toml, glsl)`. Holds a
validated `FxManifest` and the raw GLSL source string. No compilation.

### `FxManifest`
Typed, validated representation of `fx.toml`. Contains `FxMeta` (name,
description, author, tags), `FxResolution` (suggested dimensions), and
a `BTreeMap<String, FxInputDef>` of inputs keyed by name.

### `parse` module
Two-phase: raw deserialization via serde (`RawManifest` with string/Value
fields), then validation + conversion to typed `FxManifest`. Gives clear
error messages on type mismatches, missing fields, etc.

### `FxValue`
Runtime value enum: `F32(f32)`, `I32(i32)`, `Bool(bool)`, `Vec3([f32; 3])`.
Used for defaults and ranges in the manifest, and later for `set_input`
in M1.

### `FxError`
Error enum covering: TOML parse failure, missing required fields, type
mismatches (e.g. default doesn't match declared type), invalid
presentation (choice without choices array), validation failures.

## Key Decisions

- **`no_std + alloc`** throughout. `toml` v0.9+ supports `no_std`.
- **No filesystem dependency.** `from_sources` takes `&str` args.
- **Raw → typed parsing.** Separates TOML deserialization from validation.
- **Effect at `examples/noise.fx/`.** Full shader with all uniforms.

# Phases

## Phase 1: Crate Scaffold + Workspace Integration
## Phase 2: Core Types
## Phase 3: TOML Parsing + Validation
## Phase 4: noise.fx Effect
## Phase 5: Cleanup + Validation
