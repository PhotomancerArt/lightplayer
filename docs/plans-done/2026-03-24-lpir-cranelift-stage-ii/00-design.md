# Stage II: LPIR → CLIF Emitter Core — Design

## Scope of work

Complete the `lpir-cranelift` crate's core translation: structured control flow
(if/else, loops, switch), integer comparisons, local function calls, memory ops
(stack slots, load/store, memcpy), and multi-return. All using native f32 — no
Q32 mode, no import resolution, no builtins. Testable with `jit_from_ir` on
hand-built LPIR.

## File structure

```
lp-glsl/lpir-cranelift/
└── src/
    ├── lib.rs                  # UPDATE: re-exports, expanded tests
    ├── jit_module.rs           # UPDATE: EmitCtx setup, FuncRef wiring, remove guards
    ├── error.rs                # unchanged
    └── emit/
        ├── mod.rs              # NEW: translate_function entry, EmitCtx, CtrlFrame,
        │                       #   main op match dispatch, shared helpers
        │                       #   (ir_type, var, use_v, def_v, signature_for_ir_func)
        ├── scalar.rs           # NEW: 1:1 arithmetic, comparisons, constants, casts,
        │                       #   select, copy (moved from old emit.rs)
        ├── control.rs          # NEW: if/else/end, loop/break/continue/brifnot,
        │                       #   switch/case/default, block stack push/pop
        ├── memory.rs           # NEW: stack slot allocation, SlotAddr, Load, Store, Memcpy
        └── call.rs             # NEW: Call (local func refs), Return
```

## Conceptual architecture

```
               jit_from_ir(ir: &IrModule)
               ══════════════════════════
                        │
    ┌───────────────────┴───────────────────┐
    ▼                                       ▼
  Host ISA + JITModule                Per-function loop:
  (cranelift_native)                    │
                                        ├─ declare_func_in_func → FuncRef per callee
                                        ├─ create_sized_stack_slot per SlotDecl
                                        ├─ create EmitCtx { func_refs, slots, ir }
                                        ├─ translate_function(func, builder, ctx)
                                        │    │
                                        │    ├─ declare vars (VReg → Variable)
                                        │    ├─ bind params from entry block
                                        │    ├─ walk ops → dispatch to submodules:
                                        │    │   scalar.rs  → CLIF inst (1:1)
                                        │    │   control.rs → blocks + branches
                                        │    │   memory.rs  → stack_addr / load / store
                                        │    │   call.rs    → call via FuncRef / return
                                        │    └─ seal_all_blocks()
                                        └─ define_function

               CtrlFrame enum (control.rs)
               ═══════════════════════════
               If     { else_block, merge_block }
               Loop   { header_block, exit_block }
               Switch { selector: Value, merge_block, next_case: Block }
               Case   { merge_block, next_case: Block }
               Default{ merge_block }

               EmitCtx (mod.rs)
               ════════════════
               func_refs: &[FuncRef]        local function references
               slots:     &[StackSlot]      stack slots for this function
               ir:        &IrModule         module for callee resolution
```

## Main components and interactions

### `emit/mod.rs` — entry point and dispatch

- `EmitCtx<'a>` struct: holds `func_refs`, `slots`, `ir` reference
- `translate_function(func, builder, ctx)`: declares variables, binds params,
  walks `func.body` with a single match dispatching to submodule functions,
  calls `builder.seal_all_blocks()` at the end
- Shared helpers: `ir_type`, `var`, `use_v`, `def_v`, `signature_for_ir_func`
- `CtrlFrame` enum and block stack (`Vec<CtrlFrame>`)

### `emit/scalar.rs` — mechanical 1:1 translations

Moved from current `emit.rs`. All float arithmetic, integer arithmetic,
immediate ops, float comparisons, integer comparisons (new), constants, casts,
select, copy. Each op is 3-5 lines: use_v args → CLIF instruction → def_v
result. Integer comparisons follow the same pattern as float comparisons using
`IntCC`.

### `emit/control.rs` — structured control flow

**If/Else/End:**
- `IfStart { cond }` → create else_block + merge_block, `brif cond` to
  current (fall-through then) vs else_block, push `CtrlFrame::If`
- `Else` → jump to merge, switch to else_block
- `End` on If → jump to merge, switch to merge_block

**Loop:**
- `LoopStart` → create header_block + exit_block, jump to header, switch to
  header, push `CtrlFrame::Loop`
- `Break` → jump to exit_block (walk stack for innermost loop)
- `Continue` → jump to header_block (walk stack for innermost loop)
- `BrIfNot { cond }` → `brif (not cond)` to exit_block
- `End` on Loop → jump to header (back-edge), switch to exit_block

**Switch:**
- `SwitchStart { selector }` → create merge_block + first next_case block,
  push `CtrlFrame::Switch`
- `CaseStart { value }` → in next_case block: icmp selector == value, create
  new next_case block, brif to case_body vs new next_case, push `CtrlFrame::Case`
- `DefaultStart` → switch to next_case block (fall-through), push
  `CtrlFrame::Default`
- `End` on Case → jump to merge, switch to next_case
- `End` on Default → jump to merge
- `End` on Switch → switch to merge_block

### `emit/memory.rs` — stack slots

Stack slots are allocated before `translate_function` (in `jit_module.rs`)
via `builder.create_sized_stack_slot()` and passed in `EmitCtx`.

- `SlotAddr { dst, slot }` → `stack_addr` instruction
- `Load { dst, base, offset }` → `load` with type from `vreg_types[dst]`
- `Store { base, offset, value }` → `store` with type from `vreg_types[value]`
- `Memcpy { dst_addr, src_addr, size }` → emit a `memory_copy` or
  small-constant unrolled load/store sequence

### `emit/call.rs` — function calls and return

- `Call { callee, args, results }` → resolve `callee` to `FuncRef` from
  `EmitCtx`, `use_v` each arg from pool_slice, emit `call`, `def_v` each
  result from call return values
- `Return { values }` → `use_v` each value from pool_slice, emit `return_`

### `jit_module.rs` — updated setup

- Remove `imports.is_empty()` guard (still reject imports, but with a per-call
  error in call.rs rather than up-front rejection — forward-compatible for
  Stage III)
- Remove entry block `seal_block` (translate_function handles sealing)
- Before each `translate_function` call:
  - Create `FuncRef` for each local function via `declare_func_in_func`
  - Create `StackSlot` for each `SlotDecl` via `create_sized_stack_slot`
  - Bundle into `EmitCtx`
