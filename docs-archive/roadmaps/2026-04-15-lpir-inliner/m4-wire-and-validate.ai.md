# M4 — Wire Inliner + Full Validation

Connect the inlining pass to the native compilation pipeline, tag filetests
with disable annotations where needed, and run the full suite.

## Wire into `lpvm-native`

### `compile.rs` changes

The inlining pass runs on the **module** before per-function compilation
(unlike const_fold and imm_fold which run per-function). Add it to
`compile_module`:

```rust
pub fn compile_module(
    ir: &LpirModule,
    sig: &lps_shared::LpsModuleSig,
    float_mode: FloatMode,
    options: NativeCompileOptions,
) -> Result<CompiledModule, NativeError> {
    let mut ir_opt = ir.clone();
    let inline_result = lpir::inline::inline_module(
        &mut ir_opt,
        &options.config.inline,
    );
    if inline_result.call_sites_replaced > 0 {
        log::debug!(
            "[native-fa] inline: {} calls replaced across {} functions",
            inline_result.call_sites_replaced,
            inline_result.functions_inlined,
        );
    }

    let module_abi = ModuleAbi::from_ir_and_sig(&ir_opt, sig);
    let mut session = CompileSession::new(module_abi, float_mode, options);

    // ... compile each function in ir_opt.functions ...
}
```

### Signature handling

When functions are deleted from the module, the `LpsModuleSig` still has
entries for them. Two options:

A. Filter `sig.functions` to only include functions still present in the
   inlined module. Match by name.
B. Have `inline_module` return a list of deleted function names so the
   caller can filter.

Option A is simplest and sufficient.

### Per-function passes

After inlining, each function's body may be larger (inlined code). The
existing per-function passes (const_fold, imm_fold) run on the inlined
bodies — this is desirable since inlining exposes new constant folding
opportunities (e.g. `paletteHeatmap(0.0)` — the constant `0.0` flows
into the inlined body).

Pipeline order:
1. `inline_module` (module-level)
2. For each function:
   a. `const_fold` (LPIR)
   b. `lower_ops` (LPIR → VInst)
   c. `fold_immediates` (VInst)
   d. `emit` (VInst → machine code)

## Filetest annotations

### Files to tag with `// @config(inline.mode, never)`

These tests exist specifically to validate call/return mechanics:

```
filetests/function/call-simple.glsl
filetests/function/call-multiple.glsl
filetests/function/call-order.glsl
filetests/function/call-return-value.glsl
```

Review all files under `filetests/function/` and tag any that test call
semantics specifically. Files that test parameter passing (param-in,
param-out, param-inout) should also keep real calls since inlining would
eliminate the parameter passing path being tested.

### Files to tag with `// @config(inline.mode, always)`

New inliner correctness tests added in this milestone. Forces inlining
regardless of heuristic, so tests don't break when thresholds change.

### No annotation needed

Most filetests (arithmetic, control flow, builtins, etc.) should work
identically with or without inlining. The inliner only affects files that
define helper functions, and even then the results should be numerically
identical.

## Validation plan

### Step 1: Correctness

```bash
# Full filetest suite — all targets
cargo test -p lps-filetests -- --test-threads=4
```

Every test must pass. Any failure indicates a bug in the inliner (vreg
remap, control flow offset, slot remap, etc.).

### Step 2: Firmware builds

```bash
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf \
    --profile release-esp32 --features esp32c6,server
cargo check -p fw-emu --target riscv32imac-unknown-none-elf \
    --profile release-emu
```

### Step 3: Performance comparison

Run filetests with instruction counting and compare before/after:

```bash
# Before (disable inlining via @disable or env flag)
# After (default — inlining on)
```

Key files to measure:
- `debug/rainbow.glsl` — many helper calls, significant call overhead.
- `function/call-*` (with `// @disable(inline)`) — baseline for call cost.
- Any test with deep call chains.

Expected: measurable instruction count reduction for files with helper
functions. No change for files without calls (arithmetic, control flow).

### Step 4: Host still works

```bash
cargo check -p lpa-server
cargo test -p lpa-server --no-run
```

## Rollback

If the inliner introduces correctness issues:
- Set `InlineConfig { mode: Never, .. }` globally in `NativeCompileOptions`.
- Individual tests can use `// @config(inline.mode, never)`.
- No structural changes to the pipeline — removing the `inline_module`
  call restores the previous behavior exactly.

## Note on dead function elimination

The inliner does NOT delete functions. After inlining, helper functions
still exist and get compiled (they just have zero local call sites).
This is intentional — filetests need all functions to remain callable.

Dead function elimination is a separate pass (M5) that runs in production
with a known root set. It is not part of this milestone.

## Success criteria

1. All filetests pass (4400+ pass, 0 fail).
2. Firmware builds succeed.
3. `debug/rainbow.glsl` shows measurable instruction reduction on `rv32n.q32`.
4. Compile time may increase slightly (inlined functions are larger, and
   originals are still compiled). DeadFuncElim (M5) addresses this for
   production.
