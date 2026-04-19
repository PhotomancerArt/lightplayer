# Implementation notes

Cross-cutting context for the LPIR inliner work that doesn't belong in any
single milestone doc.

## Unified `lps-shader` crate (parallel branch)

A separate in-flight branch introduces a new top-level **`lps-shader`** crate
that consolidates the LPIR-side compile pipeline. Today, three backends each
have their own entry point with their own options struct and their own copy
of the "lower GLSL → optimize LPIR → emit" wiring:

```
lps_frontend::compile  +  lps_frontend::lower
    ↓
LpirModule
    ↓
lpvm-cranelift  (CraneliftEngine::compile,  CompileOptions)
lpvm-native     (NativeFaEngine::compile,   NativeCompileOptions)
lpvm-wasm       (WasmLpvmEngine::compile,   WasmOptions)
```

The unified crate will own the LPIR-side pipeline once and let each backend
plug in only its target-specific bits:

```
lps_shader::compile(source, target, options)
    ↓
lps_frontend → LpirModule
    ↓  ←  shared mid-end (inline, const_fold, future passes)
LpirModule (post-mid-end)
    ↓  →  one of: cranelift / native / wasm backend
```

That branch is **waiting on this one** (the inliner). Once both land:

- The inliner call site moves from three places (one per backend's
  `compile_module` / equivalent) to a single place in `lps-shader`.
- `CompilerConfig` lives at the `lps-shader` API boundary; backend
  `CompileOptions` / `NativeCompileOptions` / `WasmOptions` lose the
  `config: CompilerConfig` field they all carry today.
- The filetest harness's `CompiledShader::compile_glsl` (which currently
  dispatches per backend and threads `compiler_config` into each options
  struct) collapses into a single call.

### Implications for M4

We're wiring `inline_module` into all three backends in M4 (per the
"all backends for consistency" decision). That means M4 lands three call
sites — one in each backend's compile entry — that the unified-crate
branch will later consolidate into one.

This is intentional. The alternatives were worse:

- Wait for the unified crate before wiring inlining → blocks the
  unified-crate branch on the inliner *and* delays the rv32n perf win.
- Native-only in M4 → leaves cranelift/wasm divergent from native, which
  defeats the "preview matches device, reference matches optimization
  semantics" rationale that motivated the all-backends decision.

The duplication is mechanical and cheap to remove. Each call site is one
function-call's worth of code. The unified-crate PR can rip them out as
part of its consolidation step with no behavior change.

### Guidance for the unified-crate agent

When consolidating:

1. The inliner is **mid-end**, not backend-specific. It runs once per
   compile, on a clone of `LpirModule`, before per-function passes
   (`const_fold` then backend-specific lowering).
2. `inline_module` is mutative; clone the module before passing it in
   (the backends do `let mut ir_opt = ir.clone();` today).
3. The current per-function pipeline order on each backend is:
   `inline_module` (module) → `const_fold` (per function) → backend
   lower / emit. Preserve this order in the unified crate.
4. `CompilerConfig` is `Clone`, `no_std`-compatible, and lives in
   `lpir`. It already carries everything every backend needs at the
   mid-end layer (`inline: InlineConfig`; future passes will add
   sibling fields).
5. The three filetest annotations that already exist
   (`compile-opt(inline.mode, never)` and `compile-opt(inline.mode, always)`
   sprinkled across `filetests/function/`, `filetests/lpvm/native/`,
   and the new `filetests/inline/` dir) are file-scoped and apply to
   every backend invocation for that test. The unified `lps-shader`
   entry will see them through the same `CompilerConfig` channel.

If the unified-crate branch lands first for any reason, the M4 work
slots in trivially: one call to `inline_module` at the top of the
shared `compile` function, and the per-backend wiring this milestone
adds becomes a no-op delete.
