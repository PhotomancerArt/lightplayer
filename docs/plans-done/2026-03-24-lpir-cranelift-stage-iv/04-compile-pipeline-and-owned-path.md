# Phase 4: `compile.rs` — `jit()`, `jit_from_ir_owned()`, memory-conscious drain

## Scope

- Add dependencies: **`naga`**, **`lp-glsl-naga`** (with features aligned to
  **`lpir-cranelift`** **`std`** build).
- **`compile.rs`**:
  - **`pub fn jit(source: &str, options: CompileOptions) -> Result<JitModule, CompilerError>`**
  - **`lp_glsl_naga::compile(source)`** → **`lower(&naga)?`** →
    **`jit_from_ir_owned(ir, meta, options)?`**
- Implement **`jit_from_ir_owned`**:
  - Input: **`IrModule`**, **`GlslModuleMeta`**, **`CompileOptions`**
  - Sort **`functions`** by **`body.len()`** (or total vreg count) descending
  - **`declare_function`** for all locals first (unchanged dependency order), **or**
    keep current declare-all-then-define order from Stage III — **do not break
    callee refs**
  - When defining each function, **`translate_function`**, **`define_function`**,
    then **`take`** / **`swap_remove`** / replace with empty placeholder to drop
    **`IrFunction`** body (if we cannot remove from vec without breaking indices,
    use **`Vec<Option<IrFunction>>`** or **`mem::take`** on **`body`** + **`slots`**
    to shrink memory — document chosen strategy)
- **`CompilerError::Parse`**: from **`lp_glsl_naga::CompileError`** (string).

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### Callee order caveat

**`FuncId`** / **`FuncRef`** order must stay consistent with **`CalleeRef`** indices.
Declaring all functions before defining any is required. **Draining** should only
**drop IR data** after **all** functions are **defined**, **or** drain in define
order if declarations are independent — **simplest safe approach for Stage IV:**
define all functions without removing from **`Vec`** until after full pass; **optional:**
second pass clears **`IrFunction`** bodies to reduce peak memory **after**
finalize — or defer true drain to a follow-up if risky.

**Pragmatic Stage IV deliverable:** **`jit_from_ir_owned`** takes ownership and
**sorts** define order by size, but **keep** **`Vec<IrFunction>`** alive until all
**`define_function`** complete; then **`drop(ir)`** whole module. That still
frees Naga earlier in **`jit()`**. Document; tighten drain later.

### Re-export

**`pub use compile::jit;`** from **`lib.rs`**.

## Validate

```
cargo check -p lpir-cranelift
cargo test -p lpir-cranelift
```

Add **`jit()`** smoke test: small GLSL string **`float add(float a,float b){return a+b;}`**
→ **`JitModule`** → raw call via pointer (until **`call()`** exists in Phase 5).
