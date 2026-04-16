# Phase 3: Text Format Parser + Round-trip Tests

## Scope

Implement a `nom`-based parser for the LPIR text format (`String → IrModule`),
and add round-trip tests that lock the printer and parser against each other.
The parser must handle the full grammar from `docs/lpir/07-text-format.md` and
produce good error messages with line/column information via `nom_locate`.

## Code organization reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment.

## Implementation details

### 1. Add dependencies

Update `lp-shader/lpir/Cargo.toml`:

```toml
[dependencies]
nom = { version = "8", default-features = false, features = ["alloc"] }
nom_locate = { version = "5", default-features = false, features = ["alloc"] }
```

Both crates support `no_std + alloc`. Verify the exact version numbers at
implementation time — use the latest stable versions.

### 2. src/parse.rs — Parser structure

The parser uses `nom_locate::LocatedSpan` as the input type for automatic
span tracking:

```rust
use nom_locate::LocatedSpan;

type Span<'a> = LocatedSpan<&'a str>;

pub fn parse_module(input: &str) -> Result<IrModule, ParseError> {
    // ...
}
```

`ParseError` should include line, column, and a description of what was
expected:

```rust
pub struct ParseError {
    pub line: u32,
    pub column: u32,
    pub message: String,
}
```

#### Parser organization

The parser is recursive-descent via nom combinators. Key parsing functions
(each returns `IResult<Span, T>`):

**Top level:**

- `module` — `many0(module_item)`, where `module_item` is
  `import_decl | func_decl | entry_func_decl`
- `import_decl` — `"import" import_name "(" type_list ")" ["->" return_type]`
- `func_decl` — `"func" local_func_name "(" param_list ")" ["->" return_type] "{" func_body "}"`
- `entry_func_decl` — `"entry" func_decl` (sets `is_entry = true`)

**Function body:**

- `func_body` — `many0(slot_line) many0(inner_line)`
- `slot_line` — `"slot" slot_name "," uint_literal`
- `inner_line` — one of: `assign_stmt`, `void_stmt`, `if_stmt`, `loop_stmt`, `switch_stmt`,
  `return_stmt`, `"break"`, `"continue"`, `br_if_not_stmt`

**Statements:**

- `assign_stmt` — `vreg_list "=" rhs` (where rhs is `op | call_expr`)
- `void_stmt` — `store_stmt | memcpy_stmt | void_call_stmt`
- Control flow: `if_stmt`, `loop_stmt`, `switch_stmt` — recursive, parse nested bodies

**Ops:**

- `op` — dispatch on keyword: `"fadd"` → binary float, `"iconst.i32"` → const, etc.

**Tokens:**

- `vreg` — `"v" digits` → `VReg(n)`
- `vreg_def` — `vreg [":" type]` → VReg + optional type
- `slot_name` — `"ss" digits` → `SlotId(n)`
- `local_func_name` — `"@" identifier`
- `import_name` — `"@" identifier "::" identifier`
- `type_` — `"f32" | "i32"`
- `integer_literal` — optional `-`, digits or `0x` hex
- `uint_literal` — digits or hex
- `float_literal` — decimal float, `inf`, `-inf`, `nan`

#### Building IR during parsing

The parser needs to construct `IrFunction` with the flat encoding. This means
it must use the builder internally or replicate builder logic:

**Recommended approach**: use `FunctionBuilder` inside the parser. As the parser
encounters ops, it calls builder methods. Control flow keywords (`if`, `else`,
`loop`, etc.) map to `push_if`, `push_else`, `end_if`, etc. The closing `}`
maps to the appropriate `end_*` call.

The parser needs a resolution step for callee names:

- Maintain a name → `CalleeRef` map, populated from imports and function
  declarations.
- Since functions may be called before they're defined (forward references),
  parse all function signatures first (two-pass), or defer resolution until
  all declarations are parsed.

**Simpler approach for v1**: require imports before functions, and functions
before their first call site (no forward references). This matches typical
usage and avoids two-pass complexity. If forward references are needed later,
the parser can be extended.

Actually, the simpler approach is: parse everything, collect function/import
names, then resolve CalleeRef in a post-parse pass. Store callee as a string
during parsing, resolve to CalleeRef after all declarations are known.

#### VReg type tracking during parsing

When parsing `v2:f32 = fadd v0, v1`, the parser sees the type annotation on
`v2` and records it. On subsequent uses of `v2` without annotation, it reuses
the known type. The parser should maintain a `HashMap<u32, IrType>` or
`Vec<Option<IrType>>` for VReg type tracking.

Parameters get their types from the function signature.

#### Whitespace and comments

- `nom` combinators for whitespace: skip spaces, tabs, and `;`-comments.
- Between tokens, consume optional whitespace.
- Between statements, consume whitespace including newlines.

### 3. Tests

#### Round-trip tests

The core pattern: build IR → print → parse → print → assert the two printed
strings are identical.

```rust
#[test]
fn round_trip_simple_add() {
    let input = "\
func @add(v0:f32, v1:f32) -> f32 {
  v2:f32 = fadd v0, v1
  return v2
}
";
    let module = parse_module(input).unwrap();
    let output = print_module(&module);
    assert_eq!(input, output);
}
```

#### Round-trip tests for all spec examples

One test per spec example from `docs/lpir/04-control-flow.md`:

- `round_trip_abs` — simple conditional
- `round_trip_max` — if/else
- `round_trip_sum_to_n` — loop with br_if_not
- `round_trip_nested_loops` — nested loops
- `round_trip_dispatch` — switch with cases and default
- `round_trip_early_return` — early return in if

From `docs/lpir/03-memory.md`:

- `round_trip_noise_sample` — LPFX out-pointer ABI
- `round_trip_fill_vec3` — out parameter
- `round_trip_arr_dyn` — dynamic indexing
- `round_trip_use_ctx` — context pointer
- `round_trip_copy_mat4` — memcpy

Additional:

- `round_trip_multi_return` — `-> (f32, f32)` with `return v0, v1`
- `round_trip_entry_func` — `entry func @main(...)`
- `round_trip_imports` — module with imports + calls
- `round_trip_all_ops` — a function exercising every op variant (arithmetic,
  comparison, logic, immediates, casts, select, copy)
- `round_trip_constants` — float literals including `0.0`, `-0.0`, `inf`,
  `-inf`, `nan`, hex integers

#### Parse error tests

```rust
#[test]
fn parse_error_unexpected_token() {
    let input = "func @test() { xyz }";
    let err = parse_module(input).unwrap_err();
    assert!(err.message.contains("expected"));
}

#[test]
fn parse_error_unclosed_brace() {
    let input = "func @test() {";
    let err = parse_module(input).unwrap_err();
    // Should report the error location
    assert!(err.line > 0);
}
```

## Validate

```
cargo check -p lpir
cargo test -p lpir
cargo +nightly fmt -- --check
```
