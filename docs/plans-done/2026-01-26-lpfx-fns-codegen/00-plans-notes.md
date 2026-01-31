# Questions

## Q1: Attribute syntax for GLSL signature

**Context**: We need to annotate LPFX functions with their GLSL signatures so the codegen can extract them. The `#[]` things are called "attributes" in Rust.

**Decision**: Use `#[lpfx_impl("float lpfx_snoise3(vec3 p, u32 seed)")]` - indicating this is an implementation, and we can use the GLSL parser to parse the declaration string.

**Rationale**:
- Uses existing GLSL parsing infrastructure
- Simple string-based attribute
- Clear that this is marking an implementation
- Can leverage `glsl::syntax` crate for parsing

## Q2: Attribute syntax for all variants

**Context**: We need to handle non-decimal functions (hash) and decimal functions (simplex with f32/q32 variants).

**Decision**: Use consistent syntax with variant type for decimal functions:

**Syntax**:
- Non-decimal: `#[lpfx_impl("uint lpfx_hash1(uint x, uint seed)")]` - just the signature string (note: GLSL uses 'uint' not 'u32')
- Decimal f32: `#[lpfx_impl(f32, "float lpfx_snoise3(vec3 p, uint seed)")]` - variant type + signature
- Decimal q32: `#[lpfx_impl(q32, "float lpfx_snoise3(vec3 p, uint seed)")]` - variant type + signature

**Note**: GLSL syntax uses `uint` for unsigned integers, not `u32`. The attributes should use standard GLSL syntax.

**Rationale**:
- Duplicating the signature is fine - explicit and clear
- Can see everything right there in the attribute
- Consistent syntax across all variants
- Codegen can parse: if first token is a variant (f32/q32), it's decimal; otherwise non-decimal
- For decimal functions, match f32 and q32 by comparing parsed function names from signatures

## Q3: Correlating f32 and q32 implementations

**Context**: We need to correlate `__lpfx_snoise3_f32` with `__lpfx_snoise3_q32` to generate the `LpfxFnImpl::Decimal { float_impl, q32_impl }` structure.

**Decision**: Both f32 and q32 have the same GLSL signature in their attributes. Codegen will:
1. Find all functions with `#[lpfx_impl(...)]` attribute
2. Parse the attribute to determine variant (none = non-decimal, f32/q32 = decimal)
3. Parse the GLSL signature string from the attribute
4. For decimal functions, match f32 and q32 by comparing the parsed function names from their GLSL signatures
5. Validate that pairs exist for all decimal functions

**Rationale**:
- Both signatures are present, making matching explicit
- No need for separate reference mechanism
- Can validate that signatures match between f32 and q32 variants

## Q4: Non-decimal functions

**Context**: Hash functions (`lpfx_hash1`, `lpfx_hash2`, `lpfx_hash3`) don't have decimal variants - they're `LpfxFnImpl::NonDecimal`.

**Decision**: Yes, non-decimal functions use the same attribute format but without a variant specifier: `#[lpfx_impl("u32 lpfx_hash1(u32 x, u32 seed)")]`

**Rationale**:
- Consistent syntax across all LPFX functions
- Codegen can detect non-decimal by absence of variant specifier
- All functions have explicit GLSL signatures

## Q5: Attribute parsing and validation

**Context**: The codegen needs to parse the GLSL signature string and convert it to the `FunctionSignature` type.

**Decision**: Use the GLSL parser (`glsl::parser::Parse`) to parse the function declaration string, similar to how `parse_type_specifier_str` works in `type_resolver.rs`. Then convert the parsed GLSL AST to `FunctionSignature` using existing type conversion utilities.

**Rationale**:
- Leverages existing GLSL parsing infrastructure
- Consistent with how the compiler handles GLSL parsing elsewhere
- Can reuse `parse_type_specifier` and similar utilities
- Handles all GLSL syntax correctly (vectors, types, etc.)

## Q6: Codegen output format

**Context**: The codegen will generate `lpfx_fns.rs` with the `init_functions()` function.

**Question**: Should the generated code maintain the same structure as the current manual implementation?

**Suggested approach**: Yes, maintain the same structure:
- Keep the `lpfx_fns()` function with the same caching logic
- Keep `init_functions()` that returns the array
- Generate the same `LpfxFn` structures
- This minimizes changes to code that uses `lpfx_fns()`

## Q7: Error handling

**Context**: The codegen might encounter errors (missing attributes, invalid signatures, missing pairs, etc.).

**Decision**: Fail fast with clear error messages. Inconsistencies between declarations of functions with the same name are bugs and should be caught.

**Architecture**:
- Put error handling code in its own functions
- Test error handling separately
- Clean, well-separated architecture

**Error cases to handle**:
- Missing `#[lpfx_impl(...)]` attribute on LPFX function
- Invalid GLSL signature syntax
- Decimal function missing f32 or q32 variant
- f32 and q32 signatures don't match (same function name but different signatures)
- Invalid BuiltinId references
- Other validation errors
