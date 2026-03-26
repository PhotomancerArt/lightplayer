# Phase 4: LowMemory strategy

## Scope

Implement the `LowMemory` branch in `module_lower` — strip CLIF metadata
after `define_function` and sort functions by size (biggest first). Add a
test that exercises the path.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 1. Sort by size in `lower_lpir_into_module`

Currently `LpirFuncEmitOrder::Source` iterates in source order. When
`memory_strategy == LowMemory`, sort by descending body length (largest
function first) regardless of the `order` parameter. This matches the old
compiler's `memory_optimized` behavior — biggest function compiles first when
the module has accumulated the least defined code.

```rust
let indices: Vec<usize> = match (order, options.memory_strategy) {
    (_, MemoryStrategy::LowMemory) => {
        let mut v: Vec<usize> = (0..ir.functions.len()).collect();
        v.sort_by(|a, b| ir.functions[*b].body.len().cmp(&ir.functions[*a].body.len()));
        v
    }
    (LpirFuncEmitOrder::Source, _) => (0..ir.functions.len()).collect(),
    (LpirFuncEmitOrder::Name, _) => {
        let mut v: Vec<usize> = (0..ir.functions.len()).collect();
        v.sort_by(|a, b| ir.functions[*a].name.cmp(&ir.functions[*b].name));
        v
    }
};
```

### 2. CLIF metadata strip after `define_function`

After `module.define_function(fid, &mut ctx)`, the `ctx` holds compiled CLIF
that Cranelift no longer needs (it's been handed to the module). Currently
`ctx.clear()` is called implicitly when the loop continues (the `ctx` is
reused). Investigate:

- Does `cranelift_module::Module::clear_context(&mut ctx)` free more than
  `ctx.clear()`? Check the Cranelift source — `Module::clear_context` may
  release module-level metadata associated with the context.
- If `clear_context` exists and does more, use it in `LowMemory` mode.
- If not, `ctx.clear()` is sufficient — document in the design.

In `LowMemory` mode, explicitly call the most aggressive clear available:

```rust
module.define_function(fid, &mut ctx).map_err(|e| ...)?;

if options.memory_strategy == MemoryStrategy::LowMemory {
    // Aggressively free CLIF metadata after define
    module.clear_context(&mut ctx);
} else {
    ctx.clear();
}
```

Or if `clear_context` doesn't exist / doesn't do more, just use `ctx.clear()`
in both paths and document the finding.

### 3. Pass `options` through to `lower_lpir_into_module`

`lower_lpir_into_module` already receives `options: CompileOptions`. The
`memory_strategy` field is now available for branching.

### 4. Tests

```rust
#[test]
fn low_memory_strategy_compiles() {
    let ir = lpir::parse_module(
        r"func @big(v0:f32, v1:f32) -> f32 {
  v2:f32 = fadd v0, v1
  v3:f32 = fadd v2, v0
  v4:f32 = fadd v3, v1
  return v4
}

func @small(v0:f32) -> f32 {
  return v0
}
",
    )
    .expect("parse");

    let m = jit_from_ir(
        &ir,
        &CompileOptions {
            memory_strategy: MemoryStrategy::LowMemory,
            float_mode: FloatMode::F32,
            ..Default::default()
        },
    )
    .expect("jit with LowMemory");

    // Verify both functions are callable
    let big_ptr = m.finalized_ptr("big").expect("big");
    let small_ptr = m.finalized_ptr("small").expect("small");
    assert!(!big_ptr.is_null());
    assert!(!small_ptr.is_null());
}
```

## Validate

```bash
cargo test -p lpir-cranelift
cargo test -p lpir-cranelift --features riscv32-emu
cargo check --target riscv32imac-unknown-none-elf -p lpir-cranelift --no-default-features
```
