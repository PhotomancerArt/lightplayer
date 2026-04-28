# Scope Of Phase

Remove the `lpvm-cranelift` RV32 `Load16U` decomposition workaround and return
to Cranelift's ordinary `uload16 -> lhu` lowering.

In scope:

- Remove `riscv_decompose_load16u` from `lpvm-cranelift` lowering context.
- Remove the special `Load16U` word-load/bit-extraction sequence in
  `lp-shader/lpvm-cranelift/src/emit/memory.rs`.
- Ensure `Load16U` uses `builder.ins().uload16(...)` just like `Load16S` uses
  `sload16(...)`.
- Remove now-unused imports such as `IntCC` if they become unnecessary.

Out of scope:

- Modifying the `lp-cranelift` fork.
- Implementing unaligned `Load16U`.
- Changing `lpvm-native`, which already emits `lhu`.
- Implementing emulator ISA-profile gating.

# Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

# Sub-agent Reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within "Scope Of Phase".
- Do not suppress warnings or `#[allow(...)]` problems away; fix them.
- Do not disable, skip, or weaken existing tests to make the build pass.
- If something blocks completion, stop and report back rather than improvising.
- Report back: what changed, what was validated, and any deviations from the
  phase plan.

# Implementation Details

Files to update:

- `lp-shader/lpvm-cranelift/src/emit/mod.rs`
- `lp-shader/lpvm-cranelift/src/module_lower.rs`
- `lp-shader/lpvm-cranelift/src/emit/memory.rs`

Expected `Load16U` shape in `memory.rs`:

```rust
LpirOp::Load16U { dst, base, offset } => {
    let ptr = operand_as_ptr(builder, vars, ctx, *base);
    let val = builder.ins().uload16(
        types::I32,
        MemFlags::new(),
        ptr,
        i32::try_from(*offset)
            .map_err(|_| CompileError::unsupported("load16u offset does not fit in i32"))?,
    );
    def_v(builder, vars, *dst, val);
}
```

Remove the context flag entirely rather than leaving a dead false path.

After removal, inspect generated warnings carefully. Do not silence unused
warnings with attributes; remove the unused code.

# Validate

Run:

```bash
cargo test -p lpvm-cranelift
```

Then run texture filetests after phase 4 adds/updates backend coverage.
