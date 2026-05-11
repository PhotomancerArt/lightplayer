# Stage III: Builtins, Imports, and Q32 Emission — Design

## Scope of work

Add import resolution, builtin declaration, Q32 float mode, and inline Q32
ops to `lpvm-cranelift`. After this stage, the emitter can compile hand-built
LPIR in Q32 mode — float ops become builtin calls or inline integer ops,
imports resolve to `BuiltinId` → Cranelift func refs, and JIT symbol lookup
provides `lps-builtins` function pointers. GLSL source compilation and
the typed call interface are Stage IV.

## File structure

```
lp-shader/lpir/src/
└── types.rs                        # UPDATE: add FloatMode { Q32, F32 }

lp-shader/lps-frontend/src/
└── lib.rs                          # UPDATE: remove FloatMode, re-export lpir::FloatMode

lp-shader/lps-wasm/src/
├── emit/imports.rs                 # UPDATE: use lpir::FloatMode
└── options.rs                      # UPDATE: use lpir::FloatMode

lp-shader/legacy/lpvm-cranelift/
├── Cargo.toml                      # UPDATE: add lps-builtin-ids, lps-builtins deps
└── src/
    ├── lib.rs                      # UPDATE: re-exports, FloatMode in public API
    ├── jit_module.rs               # UPDATE: FloatMode param, symbol_lookup_fn, import FuncRefs
    ├── error.rs                    # unchanged
    ├── q32.rs                      # NEW: q32_encode, inline Q32 ops, Q32 casts
    ├── builtins.rs                 # NEW: declare_builtins, resolve_import, get_function_pointer
    └── emit/
        ├── mod.rs                  # UPDATE: EmitCtx gets FloatMode, import func_refs
        ├── scalar.rs               # UPDATE: Q32 dispatch for float ops
        ├── control.rs              # unchanged from Stage II
        ├── memory.rs               # unchanged from Stage II
        └── call.rs                 # UPDATE: import calls via resolved BuiltinId FuncRefs
```

## Conceptual architecture

```
        jit_from_ir(ir: &IrModule, mode: FloatMode)
        ════════════════════════════════════════════
                        │
      ┌─────────────────┴─────────────────┐
      ▼                                   ▼
    builtins.rs                     jit_module.rs
    ┌──────────────────────┐        ┌──────────────────────────────┐
    │ resolve_import(decl) │        │ 1. symbol_lookup_fn:         │
    │   module + name      │        │    BuiltinId::all() → name   │
    │   → BuiltinId        │        │    → get_function_pointer    │
    │   via glsl_q32_math_ │        │                              │
    │   builtin_id / lpir_ │        │ 2. declare_builtins:         │
    │   q32_builtin_id /   │        │    BuiltinId::all()          │
    │   glsl_lpfn_q32_     │        │    filter by FloatMode       │
    │   builtin_id         │        │    → Linkage::Import         │
    │                      │        │                              │
    │ get_function_pointer │        │ 3. Per-function:             │
    │   BuiltinId          │        │    import FuncRefs (builtins)│
    │   → fn as *const u8  │        │    + local FuncRefs          │
    │   (lps-builtins) │        │    → EmitCtx                 │
    └──────────────────────┘        └──────────────────────────────┘

        emit/scalar.rs — Q32 dispatch
        ══════════════════════════════

        ┌─────────────────────────────────────────────────┐
        │ FloatMode::F32                                  │
        │   fadd → builder.ins().fadd()     (existing)    │
        │   fcmp → builder.ins().fcmp()     (existing)    │
        │   fconst → builder.ins().f32const (existing)    │
        │   ftoi → fcvt_to_sint_sat         (existing)    │
        ├─────────────────────────────────────────────────┤
        │ FloatMode::Q32                                  │
        │   fadd/fsub/fmul/fdiv/sqrt/nearest              │
        │     → call __lp_lpir_*_q32 via FuncRef          │
        │   fneg → ineg                                   │
        │   fabs → select(icmp(sge,v,0), v, ineg(v))      │
        │   fmin → select(icmp(sle,a,b), a, b)            │
        │   fmax → select(icmp(sge,a,b), a, b)            │
        │   ffloor/fceil/ftrunc → bit mask ops            │
        │   fcmp → icmp (signed)                          │
        │   fconst → iconst(q32_encode(val))              │
        │   ftoi → Q32 shift+clamp (from old crate)       │
        │   itof → clamp+ishl 16   (from old crate)       │
        └─────────────────────────────────────────────────┘

        q32.rs — Q32 helpers
        ═════════════════════
        q32_encode(f32) → i32           constant encoding
        emit_q32_fneg(builder, v)       inline: ineg
        emit_q32_fabs(builder, v)       inline: icmp+select+ineg
        emit_q32_fmin(builder, a, b)    inline: icmp+select
        emit_q32_fmax(builder, a, b)    inline: icmp+select
        emit_q32_ffloor(builder, v)     inline: bit mask
        emit_q32_fceil(builder, v)      inline: bit mask + add
        emit_q32_ftrunc(builder, v)     inline: bit mask (sign-aware)
        emit_q32_to_sint(builder, v)    inline: bias+sshr 16
        emit_q32_to_uint(builder, v)    inline: to_sint+clamp
        emit_q32_from_sint(builder, v)  inline: clamp+ishl 16
        emit_q32_from_uint(builder, v)  inline: extend+ishl 16

        emit/call.rs — import calls
        ════════════════════════════
        CalleeRef < import_count
          → EmitCtx.import_func_refs[callee.0]
          → builder.ins().call(func_ref, args)

        Import resolution flow:
        ImportDecl { module: "glsl", func: "sin" }
          → builtins::resolve_import(decl, FloatMode::Q32)
          → BuiltinId::LpGlslSinQ32
          → jit_module declares as Linkage::Import
          → symbol_lookup_fn resolves "__lps_sin_q32"
          → get_function_pointer → __lps_sin_q32 as *const u8
```

## Main components and interactions

### `builtins.rs` — builtin declaration and resolution

- `resolve_import(decl: &ImportDecl, mode: FloatMode) → Result<BuiltinId>`:
  dispatches on `decl.module_name` ("glsl"/"lpir"/"lpfn"), calls into
  `lps-builtin-ids` mapping functions. Same logic as WASM emitter's
  `resolve_builtin_id`.
- `declare_builtins(module: &mut JITModule, mode: FloatMode)`: iterates
  `BuiltinId::all()`, filters by mode, declares each as `Linkage::Import`.
  Derives Cranelift signature from the LPIR `ImportDecl` param/return types.
- `get_function_pointer(id: BuiltinId) → *const u8`: big match mapping each
  `BuiltinId` to the corresponding `lps-builtins` function pointer.
- `symbol_lookup_fn(mode: FloatMode) → Box<dyn Fn(&str) → Option<*const u8>>`:
  closure for `JITBuilder` that resolves symbol names via `BuiltinId::all()`
  and `get_function_pointer`.

### `q32.rs` — Q32 encoding and inline ops

- `q32_encode(value: f32) → i32`: `((value as f64) * 65536.0).round() as i32`
  with saturation.
- `emit_q32_*` functions: each takes `&mut FunctionBuilder` and operand
  `Value`s, emits inline CLIF integer ops, returns result `Value`. Ported
  from old crate's `Q32Strategy` methods in `numeric.rs`.

### `emit/mod.rs` — updated EmitCtx

```
EmitCtx {
    func_refs: &[FuncRef],          // local function refs (Stage II)
    import_func_refs: &[FuncRef],   // import/builtin func refs (Stage III)
    slots: &[StackSlot],            // stack slots (Stage II)
    ir: &IrModule,                  // module for callee resolution
    pointer_type: Type,             // from ISA
    float_mode: FloatMode,          // Q32 or F32
}
```

### `emit/scalar.rs` — Q32-aware dispatch

Each float op checks `ctx.float_mode`:

- `F32` → existing native CLIF instruction
- `Q32` → either call builtin FuncRef (for the 6 with builtins) or call
  inline `q32::emit_q32_*` helper

Float comparisons in Q32 use `icmp` with signed `IntCC` instead of `fcmp`.
Float constants in Q32 use `iconst(q32_encode(val))` instead of `f32const`.

### `emit/call.rs` — import calls

Extends the existing Call handler. If `callee.0 < import_count`, index into
`ctx.import_func_refs`. If `callee.0 >= import_count`, index into
`ctx.func_refs` (existing local call path).

### `jit_module.rs` — updated setup

`jit_from_ir` gains a `FloatMode` parameter. When Q32:

1. Set `symbol_lookup_fn` on `JITBuilder` before `JITModule::new`
2. Call `declare_builtins` after module creation
3. For each function, resolve import `FuncRef`s via
   `declare_func_in_func` for each declared builtin
4. Pass `import_func_refs` and `float_mode` in `EmitCtx`

### `lpir/types.rs` — FloatMode migration

Add `FloatMode { Q32, F32 }` to the `lpir` crate. Update `lps-frontend` to
re-export or alias it. Update `lps-wasm` imports.
