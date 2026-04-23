# M1 — Pointer-ABI foundation + array migration · Design

Roadmap: `docs/roadmaps/2026-04-22-lp-shader-aggregates/`
Milestone: `m1-pointer-abi-foundation.md`
Notes / Q&A: `00-notes.md`

## Scope of work

Establish the unified pass-by-pointer aggregate ABI end-to-end and migrate
the existing array `in` calling convention from "flat scalars by value"
to "pointer-to-slot + Memcpy-on-entry". After M1:

- Every aggregate (today: arrays; M2: structs; M3: arrays-of-structs)
  passes through function boundaries as a pointer to a std430-laid-out
  buffer.
- Aggregate returns use a hidden first `sret` pointer arg
  (caller-allocated).
- Local aggregates are always slot-backed.
- All aggregate memory in the project — slot bytes, vmctx uniforms /
  globals, host `LpvmDataQ32` buffers, sret return buffers — uses one
  layout: `lps_shared::layout::std430`.
- All four backends (`lpvm-cranelift`, `lpvm-native`, `lpvm-emu`,
  `lpvm-wasm`) consume the new ABI driven by an explicit
  `IrFunction::sret_arg` marker.

Out of scope (later milestones): struct lowering (M2), arrays of structs
(M3), uniform-with-array-field (M4), read-only `in` optimisation (M5),
small-aggregate register-return fast path (perf follow-up).

## File structure

```
lp-shader/
├── lpir/
│   └── src/
│       ├── lpir_module.rs               # UPDATE: IrFunction.sret_arg, ImportDecl.sret, accessors
│       ├── builder.rs                   # UPDATE: FunctionBuilder.add_sret_param, sret-aware finish
│       ├── print.rs                     # UPDATE: print `sret %N` marker on `func` / `import` headers
│       ├── parse.rs                     # UPDATE: parse `sret %N` marker
│       └── validate.rs                  # UPDATE: validate sret invariants
├── lps-shared/
│   └── src/
│       └── layout.rs                    # (no change — already canonical std430)
├── lps-frontend/
│   └── src/
│       ├── lower_aggregate_layout.rs    # NEW: naga-type → (size, align, lps_type) std430 funnel
│       ├── lower_array_multidim.rs      # UPDATE: stride/size from lps_shared::layout (not Naga)
│       ├── lower_ctx.rs                 # UPDATE: ArrayInfo→AggregateInfo, ArraySlot→AggregateSlot;
│       │                                #         pointer-arg classification for aggregate `in`
│       ├── lower_array.rs               # UPDATE: drop store_array_from_flat_vregs +
│       │                                #         load_array_flat_vregs_for_call;
│       │                                #         add aggregate Memcpy-on-entry helper
│       ├── lower.rs                     # UPDATE: lower_function — sret allocation for aggregate returns
│       ├── lower_stmt.rs                # UPDATE: extract lower_user_call → lower_call.rs;
│       │                                #         aggregate Statement::Return = Memcpy + ret void
│       ├── lower_call.rs                # NEW: pulled from lower_stmt.rs;
│       │                                #      aggregate-args-by-pointer + sret call dance
│       └── naga_util.rs                 # UPDATE: array_ty_pointer_arg_ir_type;
│                                        #         func_return_ir_types_with_sret
├── lpvm/
│   └── src/
│       └── lpvm_abi.rs                  # UPDATE: aggregate arms collapse — host marshals via LpvmDataQ32
├── lpvm-cranelift/
│   └── src/
│       └── emit/
│           ├── mod.rs                   # UPDATE: signature_uses_struct_return reads sret_arg
│           └── call.rs                  # (existing sret machinery stays; trigger source changes)
├── lpvm-native/
│   └── src/
│       ├── abi/
│       │   ├── classify.rs              # UPDATE: aggregate pointer = 1 word in arg classification
│       │   └── func_abi.rs              # (no change — ReturnMethod::Sret already exists)
│       └── isa/
│           └── rv32/
│               └── abi.rs               # UPDATE: sret triggered by func.sret_arg.is_some()
├── lpvm-emu/
│   └── src/
│       └── instance.rs                  # UPDATE: host marshalling — alloc in EmuSharedArena;
│                                        #         sret size from std430; LpvmDataQ32 round-trip
├── lpvm-wasm/
│   └── src/
│       ├── rt_wasmtime/
│       │   ├── marshal.rs               # UPDATE: aggregate args/returns via shadow-stack pointers
│       │   └── instance.rs              # UPDATE: thread $sp alloc through the call paths
│       └── rt_browser/
│           ├── marshal.rs               # UPDATE: same shape as wasmtime
│           └── instance.rs              # UPDATE: same threading
└── lps-filetests/
    ├── filetests/
    │   └── function/
    │       ├── param-array.glsl         # UPDATE: CHECK lines reflect pointer-arg + entry Memcpy
    │       ├── return-array.glsl        # UPDATE / NEW: aggregate sret round-trip
    │       └── (other affected tests)   # UPDATE: bulk CHECK rewrite per Q8
    └── tests/
        └── (back-end smoke tests)       # NEW: per-backend sret round-trip
```

## Conceptual architecture

### One ABI for all aggregates

```
                ┌─ scalar / vec / mat ────────────┐
                │  flat VRegs (unchanged)         │
caller ─────────┤                                 ├────► callee
                │  aggregates (array/struct/AoS)  │
                │  pointer to caller's slot       │
                └──────────────┬──────────────────┘
                               │
                               ▼
                       Memcpy on entry
                       (callee owns its copy)
```

For aggregate returns:

```
                        sret hidden first arg
caller's dest slot ─────────────────────────────► callee
       ▲                                            │
       │            store result components        │
       └────────────────────────────────────────────┘
                        (no LPIR `return` values)
```

### Per-component responsibilities

**LPIR.** Add `IrFunction::sret_arg: Option<VReg>` and `ImportDecl::sret:
bool`. Both signal "this function returns its aggregate via the first
hidden pointer arg". When set:

- `return_types` is empty.
- `vreg_types[sret_arg.0 as usize] == IrType::Pointer`.
- VReg numbering: `%0` = vmctx, `%1` = sret (if present), user params start
  at `%(1 + sret_offset)` where `sret_offset = sret_arg.is_some() as u32`.
- `total_param_slots() = 1 + sret_offset + param_count`.

**Layout authority.** `lps_shared::layout::std430` is the single source of
truth for aggregate byte layout. The frontend's `lower_aggregate_layout`
module is the funnel: given a Naga `Handle<Type>`, return `(size_bytes,
align_bytes, lps_type)`. All slot allocation, sret-arg sizing, host
marshalling, and entry-Memcpy size go through it.

**Frontend, callee side.** For each aggregate `in` parameter:

1. Add a single `IrType::Pointer` param at the LPIR level.
2. Allocate a slot of size `aggregate_size_and_align(arg_ty).0`.
3. Emit `Memcpy { dst_addr: slot_addr, src_addr: param_addr, size }` at
   entry — callee owns its copy.
4. Use the slot as the local variable's storage.

For aggregate returns:

1. Allocate the sret VReg via `FunctionBuilder::add_sret_param`. It lives
   at `%1` (immediately after `vmctx_vreg`).
2. Mark `IrFunction::sret_arg = Some(sret_vreg)`.
3. `Statement::Return(value)` lowers to:
   - Compute the value into a local slot (or use an existing slot).
   - `Memcpy { dst_addr: sret_arg, src_addr: value_slot_addr, size }`.
   - `push_return(&[])`.

**Frontend, caller side.** For each aggregate argument: push the slot
address (via `SlotAddr`) instead of flattened scalars. For aggregate
results: allocate a dest slot, push its address as the first arg
(when `callee.sret`), do not allocate result VRegs from `push_call`.
Subsequent reads from the call result happen via `Load { base: slot_addr,
offset }`.

**Host ABI marshalling (`lpvm_abi.rs`).** Aggregate arms of
`flatten_q32_arg` and `decode_q32_return` collapse. Per-backend
marshallers handle aggregates directly using `LpvmDataQ32::from_value`
(args) and `LpvmDataQ32::to_value` (returns), with backend-specific buffer
placement.

**Backends.**

- `lpvm-cranelift` (host JIT + RV32 cross): `signature_uses_struct_return`
  becomes `func.sret_arg.is_some()`. Existing sret machinery in
  `emit/call.rs` (caller pushes stack-slot address as `ArgumentPurpose::
  StructReturn`, callee loads from it) stays.
- `lpvm-native` (from-scratch RV32): `func_abi_rv32` reads `func.sret_arg`
  to choose `ReturnMethod::Sret`. Aggregate pointer args are 1 word each.
- `lpvm-emu`: callee codegen comes free from `lpvm-cranelift`. Host
  marshalling: aggregate args allocate from `EmuSharedArena`, write
  `LpvmDataQ32` bytes, pass guest base as i32. sret returns use
  `call_function_with_struct_return(entry, args, sig, std430_size)`,
  read the returned bytes back into `LpvmDataQ32::from_bytes`.
- `lpvm-wasm`: callee codegen unchanged (pointer params lower naturally
  to wasm i32 + memory ops). Host marshalling: aggregate args bump-allocate
  on the shadow stack via the exported `$sp` global, `mem.write` the
  bytes, pass `Val::I32(ptr)`. sret returns alloc on shadow stack, pass
  as first arg, after the call `mem.read` and decode via `LpvmDataQ32`.
  Same shape mirrored in `rt_browser/`.

### Main components and their interactions

```
                  ┌──────────────────────────────────────┐
                  │ lps_shared::layout::std430 (canonical)│
                  └─────────────┬────────────────────────┘
                                │ used by
        ┌───────────────────────┴────────────┬────────────────────────┐
        ▼                                    ▼                        ▼
  lower_aggregate_layout                LpvmDataQ32             vmctx packer
   (frontend funnel)                  (host buffer)             (uniforms/globals)
        │
        │ slot size, stride, total size
        ▼
  lower_ctx::AggregateInfo ──► lower_array (slot ops, Memcpy, indexed load/store)
        │                               │
        │ aggregate `in` → Pointer arg  │ aggregate return → sret slot
        ▼                               ▼
  FunctionBuilder.add_param   FunctionBuilder.add_sret_param
        │                               │
        ▼                               ▼
                 IrFunction { sret_arg: Option<VReg>, ... }
                                │
            ┌───────────────────┼───────────────────┐
            ▼                   ▼                   ▼
   lpvm-cranelift          lpvm-native       lpvm-wasm     lpvm-emu (via cranelift)
        │                       │                 │              │
        ▼                       ▼                 ▼              ▼
   sret_arg drives         sret_arg drives   no codegen      host marshal:
   StructReturn purpose    ReturnMethod      change; host    EmuSharedArena
   in cl signature         ::Sret in         marshals via    + call_function_
                           rv32 abi          $sp shadow      with_struct_return
                                             stack
                                ▲ (all backends)
                                │
                                │ aggregates always pass as a single Pointer arg
                                │ (1 wasm i32 / 1 RV32 word / 1 cranelift i32 param)
```

## Phase plan

```
P1.  LPIR sret marker                        [sub-agent: yes,  parallel: P2]
P2.  Layout authority migration (std430)     [sub-agent: yes,  parallel: P1]
P3.  Frontend aggregate + pointer ABI        [sub-agent: yes,  parallel: -]
P4.  lpvm-cranelift sret driven by marker    [sub-agent: yes,  parallel: P5, P7]
P5.  lpvm-native sret driven by marker       [sub-agent: yes,  parallel: P4, P7]
P6.  lpvm-emu host marshalling               [sub-agent: yes,  parallel: P7]
P7.  lpvm-wasm host marshalling              [sub-agent: yes,  parallel: P4, P5, P6]
P8.  lpvm_abi aggregate-arm cleanup          [sub-agent: yes,  parallel: P9]
P9.  Filetest CHECK rewrites + sret tests    [sub-agent: yes,  parallel: P8]
P10. Cleanup & validation                    [sub-agent: supervised]
```

Validation command (per phase, run from repo root):

```
just check && just test
```

Final-phase sweep adds: `just filetests` (or whatever the project's
filetest runner is called — confirm in P9/P10).
