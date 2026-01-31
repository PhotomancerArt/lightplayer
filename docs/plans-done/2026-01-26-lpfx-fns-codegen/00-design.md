# Design: Auto-generate lpfx_fns.rs

## Overview

Extend `lp-glsl-builtin-gen-app` to automatically generate `lpfx_fns.rs` by discovering LPFX
functions annotated with `#[lpfx_impl(...)]` attributes, parsing their GLSL signatures, and
generating the registry code.

## Architecture

The codegen will be organized into clean, well-separated modules:

```
lp-glsl-builtin-gen-app/src/
├── main.rs                    # UPDATE: Add lpfx_fns generation
├── discovery.rs               # NEW: Discover LPFX functions with attributes
├── lpfx/
│   ├── mod.rs                # NEW: LPFX codegen module
│   ├── parse.rs              # NEW: Parse #[lpfx_impl] attributes
│   ├── validate.rs           # NEW: Validate consistency and pairs
│   ├── generate.rs           # NEW: Generate lpfx_fns.rs code
│   └── errors.rs             # NEW: Error types and messages
```

## Types and Functions

### Discovery Module (`discovery.rs`)

```
discover_lpfx_functions(dir: &Path) -> Result<Vec<LpfxFunctionInfo>, Error>
  # NEW: Walk directory tree, find all functions with #[lpfx_impl] attribute

LpfxFunctionInfo - # NEW: Information about a discovered LPFX function
├── rust_fn_name: String      # NEW: Rust function name (e.g., "__lpfx_snoise3_f32")
├── builtin_id: BuiltinId     # NEW: Corresponding BuiltinId enum variant
├── attribute: LpfxAttribute  # NEW: Parsed attribute information
└── file_path: PathBuf        # NEW: Source file path
```

### Parse Module (`lpfx/parse.rs`)

```
parse_lpfx_attribute(attr: &Attribute) -> Result<LpfxAttribute, Error>
  # NEW: Parse #[lpfx_impl(...)] attribute

LpfxAttribute - # NEW: Parsed attribute information
├── variant: Option<Variant>  # NEW: None = non-decimal, Some(f32/q32) = decimal
└── glsl_signature: String    # NEW: GLSL signature string

parse_glsl_signature(sig_str: &str) -> Result<FunctionSignature, Error>
  # NEW: Parse GLSL signature string using glsl parser

Variant - # NEW: Decimal format variant
├── F32
└── Q32
```

### Validate Module (`lpfx/validate.rs`)

```
validate_lpfx_functions(functions: &[LpfxFunctionInfo]) -> Result<(), Error>
  # NEW: Validate all discovered functions

validate_decimal_pairs(functions: &[LpfxFunctionInfo]) -> Result<(), Error>
  # NEW: Ensure all decimal functions have matching f32/q32 pairs

validate_signature_consistency(functions: &[LpfxFunctionInfo]) -> Result<(), Error>
  # NEW: Ensure f32 and q32 variants have matching signatures

Error - # NEW: Validation error with clear messages
```

### Generate Module (`lpfx/generate.rs`)

```
generate_lpfx_fns(functions: &[LpfxFunctionInfo]) -> String
  # NEW: Generate lpfx_fns.rs source code

group_functions_by_name(functions: &[LpfxFunctionInfo]) -> HashMap<String, Vec<&LpfxFunctionInfo>>
  # NEW: Group functions by GLSL name for pairing
```

### Error Module (`lpfx/errors.rs`)

```
LpfxCodegenError - # NEW: Error type for codegen
├── MissingAttribute(function_name: String)
├── InvalidSignature(function_name: String, error: String)
├── MissingPair(function_name: String, missing_variant: Variant)
├── SignatureMismatch(function_name: String, f32_sig: String, q32_sig: String)
└── InvalidBuiltinId(function_name: String, builtin_id: String)
```

## Data Flow

1. **Discovery**: Walk `lp-glsl-builtins/src/builtins/lpfx` directory, find all functions with
   `#[lpfx_impl]` attribute
2. **Parsing**: Parse attributes to extract variant and GLSL signature, parse GLSL signatures to
   `FunctionSignature`
3. **Validation**:
    - Ensure all LPFX functions have attributes
    - Ensure decimal functions have both f32 and q32 variants
    - Ensure f32 and q32 signatures match
    - Validate BuiltinId references
4. **Generation**: Generate `lpfx_fns.rs` with `init_functions()` containing all `LpfxFn` structures
5. **Formatting**: Run `cargo fmt` on generated file

## Implementation Details

### Attribute Parsing

The `#[lpfx_impl(...)]` attribute can have two forms:

- `#[lpfx_impl("signature")]` - Non-decimal function
- `#[lpfx_impl(variant, "signature")]` - Decimal function (variant is `f32` or `q32`)

Parse using `syn::Attribute::parse_args()` to extract:

- Optional variant identifier (`f32` or `q32`)
- GLSL signature string literal

### GLSL Signature Parsing

Use `glsl::parser::Parse` to parse the signature string as a function prototype:

```rust
let wrapper = format!("void wrapper() {{ {}(); }}", sig_str);
let shader = glsl::parser::Parse::parse(&wrapper)?;
// Extract FunctionPrototype from shader
// Convert to FunctionSignature using existing utilities
```

### Function Pairing

For decimal functions:

1. Group all functions by parsed GLSL function name
2. For each GLSL function name, find f32 and q32 variants
3. Validate signatures match
4. Create `LpfxFnImpl::Decimal { float_impl, q32_impl }`

For non-decimal functions:

1. Create `LpfxFnImpl::NonDecimal(builtin_id)`

### Code Generation

Generate Rust code as a string, maintaining the same structure as current `lpfx_fns.rs`:

- `lpfx_fns()` function with caching logic
- `init_functions()` that returns array of `LpfxFn`
- Each `LpfxFn` with `glsl_sig` and `impls` fields

## Error Handling

All error handling functions should:

- Be in separate, testable functions
- Provide clear, actionable error messages
- Include context (function name, file path, etc.)
- Fail fast on first error

Error messages should include:

- What went wrong
- Which function/file
- What was expected
- How to fix it

## Testing

Create tests for:

- Attribute parsing (valid and invalid syntax)
- GLSL signature parsing (various types, vectors, etc.)
- Validation (missing pairs, mismatched signatures, etc.)
- Code generation (output format, correctness)

Tests should be in `lp-glsl-builtin-gen-app/tests/` or inline test modules.
