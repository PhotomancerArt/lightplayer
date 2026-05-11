# Phase 6: Calls + Import Resolution

## Scope

Implement `imports.rs` — resolve `@std.math` and `@lpfn` imports to
`builtins` WASM imports. Implement `Call` op emission. After this phase,
user function calls, math builtins, and LPFX calls all work.

## Implementation

### `emit/imports.rs` — import resolution

```rust
pub(crate) struct ResolvedImports {
    /// WASM import entries to emit (module, name, type_index).
    pub wasm_imports: Vec<WasmImportEntry>,
    /// LPIR CalleeRef → WASM function index.
    pub callee_map: BTreeMap<u32, u32>,
    /// Whether env.memory is needed (true if any builtins import exists).
    pub needs_memory: bool,
}
```

**Resolution logic:**

For each `ImportDecl` in `ir.imports`:

1. **`@std.math::*`** — map to Q32 builtins:

   | LPIR import | BuiltinId | WASM name |
      |-------------|-----------|-----------|
   | `@std.math::sin` | `LpQ32Sin` | `__lp_q32_sin` |
   | `@std.math::cos` | `LpQ32Cos` | `__lp_q32_cos` |
   | `@std.math::pow` | `LpQ32Pow` | `__lp_q32_pow` |
   | etc. | | |

   Name mapping: strip `std.math::` prefix, look up
   `BuiltinId::from_std_math_name(name)` (new helper or match table).

   WASM signature: all params and returns are `i32` (Q32 values).
   Use `BuiltinId::name()` for the WASM import name under the `builtins`
   module.

2. **`@lpfn::*`** — map to Q32 LPFX builtins:

   | LPIR import | BuiltinId | WASM name |
      |-------------|-----------|-----------|
   | `@lpfn::lpfn_hash1` | `LpfnHash1` | `__lpfn_hash_1` |
   | `@lpfn::lpfn_snoise2` | `LpfnSnoise2Q32` | `__lpfn_snoise2_q32` |
   | etc. | | |

   Name mapping: use `lps_builtin_ids` resolution functions.

   WASM signature: look up via `q32_lpfn_wasm_signature(bid)` from the
   existing `lpfn.rs` logic (moved to `imports.rs` or referenced from
   `lps-builtin-ids`).

3. Any unresolved import → `Err("unsupported import @module::name")`.

**Callee index mapping:**

WASM function indices: imports come first (0..N), then defined functions
(N..N+M). This matches LPIR's `CalleeRef` encoding:

- LPIR `CalleeRef(i)` where `i < imports.len()` → WASM import index `i`
- LPIR `CalleeRef(i)` where `i >= imports.len()` → WASM function index
  `import_count + (i - imports.len())`

Wait — WASM function indices are: imports 0..N, then defined 0..M with
absolute index N..N+M. So:

- LPIR import `i` → WASM func index `i` (if imports are 1:1)
- LPIR function `j` → WASM func index `N + j`

But the WASM imports might have env.memory inserted, which is a memory
import, not a function import. Memory imports don't take function indices.
Only function imports count toward the function index space.

So: WASM function import count = number of builtins imports (not memory).
LPIR import count = number of LPIR imports.

The mapping needs to handle the case where some LPIR imports map to the
same builtin (dedup) — actually no, LPIR imports should already be
deduplicated by the lowering pass.

### `Call` op emission (`emit/ops.rs`)

**`Call { callee, args, results }`**:

1. Push arguments: for each arg VReg, `local.get vreg`.
2. Determine WASM function index from `callee_map[callee.0]`.
3. Emit `call <wasm_func_index>`.
4. Pop results: for each result VReg, `local.set vreg` (in reverse order
   since WASM stack is LIFO).

For multi-result calls: WASM multi-value returns push multiple values.
Pop in reverse order to match VReg ordering.

### Tier 1 math ops that use builtins

Some LPIR ops from Phase 3 were marked as builtin calls:

- `Fsqrt` → `builtins::__lp_q32_sqrt`
- `Fnearest` → `builtins::__lp_q32_roundeven`

These need to be emitted as `call <builtin_index>` rather than inline
WASM. The import resolution must include these builtins even if no
`@std.math::sqrt` import exists in the LPIR.

Approach: during op emission, if an op needs a builtin call, look up the
builtin in the resolved imports. If not present, add it lazily — or
better, scan all function bodies during import collection to find which
builtins are needed (Fsqrt, Fnearest → their BuiltinIds).

### WASM type section

Each unique function signature gets a type index. Build a dedup map:
`(params, results) → type_index`. In Q32, most signatures are
`(i32, ...) → i32` or `(i32, ...) → ()`.

## Validate

```
cargo check -p lps-wasm
cargo test -p lps-wasm
```

At this point, the full op set is covered. Programs with calls, math
builtins, and LPFX should emit valid WASM.
