# Scope Of Phase

Remove the `lpvm-cranelift` RV32 `Load16U` word-load decomposition and restore
the ordinary Cranelift `uload16` lowering path.

Out of scope:

- Do not modify the external `/Users/yona/dev/photomancer/lp-cranelift` fork in
  this phase.
- Do not add unaligned `Load16U` support.
- Do not change `lpvm-native`; it already emits `lhu`.
- Do not implement emulator ISA-profile gating.

# Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

# Sub-Agent Reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within "Scope Of Phase".
- Do not suppress warnings or `#[allow(...)]` problems away.
- Do not disable, skip, or weaken existing tests to make the build pass.
- If something blocks completion, stop and report back rather than improvising.
- Report back: what changed, what was validated, and any deviations.

# Implementation Details

Update these files:

- `lp-shader/lpvm-cranelift/src/emit/memory.rs`
- `lp-shader/lpvm-cranelift/src/emit/mod.rs`
- `lp-shader/lpvm-cranelift/src/module_lower.rs`

Remove:

- `LpirEmitCtx::riscv_decompose_load16u`
- The `module_lower.rs` logic that enables it for all RV32 targets.
- The manual `Load16U` lowering sequence that builds two aligned word loads and
  extracts bytes with shifts/selects.

The final `Load16U` lowering should look like the other narrow loads:

```rust
let ptr = operand_as_ptr(builder, vars, ctx, *base);
let val = builder.ins().uload16(
    types::I32,
    MemFlags::new(),
    ptr,
    i32::try_from(*offset)
        .map_err(|_| CompileError::unsupported("load16u offset does not fit in i32"))?,
);
def_v(builder, vars, *dst, val);
```

Clean up imports made unused by the removed sequence, such as `IntCC` if it is
no longer used in `memory.rs`.

If plain `uload16` fails validation, stop and report the exact failure. Do not
reintroduce a workaround without main-agent review.

# Validate

Run:

```bash
cargo test -p lpvm-cranelift
TEST_FILE=textures cargo test -p lps-filetests --test filetests filetests -- --ignored --nocapture
```

The texture filetests must include `rv32c.q32` coverage. If they fail, report
the failing target and emulator/codegen error.
