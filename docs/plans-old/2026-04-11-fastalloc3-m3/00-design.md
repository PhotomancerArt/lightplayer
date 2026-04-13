# fastalloc3-m3 Design: Control Flow and Calls

## Scope

Extend `fa_alloc` backward-walk allocator to handle all VInst types and all
Region types. After this milestone, all existing filetests pass under `rv32fa`.

## File Structure

```
lp-shader/lpvm-native-fa/src/
├── fa_alloc/
│   ├── mod.rs              # UPDATE: wire func_abi, sret buffer, param precoloring
│   ├── walk.rs             # UPDATE: IfThenElse, Loop, Call, Select32, BrIf, Br, Label
│   ├── liveness.rs         # UPDATE: IfThenElse and Loop liveness (for debug/trace)
│   └── spill.rs            # (no changes expected)
├── rv32/
│   ├── inst.rs             # UPDATE: add PInst::Label
│   ├── rv32_emit.rs        # UPDATE: label tracking, branch fixups, PInst::Label emission
│   └── abi.rs              # (no changes expected)
├── abi/
│   └── func_abi.rs         # UPDATE: add max_callee_sret_bytes to FuncAbi
├── compile.rs              # UPDATE: pass sret info to allocate()
└── emit.rs                 # UPDATE: pass sret info to allocate()
```

## Architecture

```
                    allocate(lowered, func_abi)
                             │
                    ┌────────┴────────┐
                    │  walk_region()  │  ← recursive on RegionTree
                    └────────┬────────┘
                             │
              ┌──────────────┼──────────────┐
              │              │              │
         Linear          IfThenElse       Loop
       (existing)          (new)          (new)
              │              │              │
              ▼              ▼              ▼
        process_inst    save pool       walk body
        for each inst   walk else       walk header
        in reverse      restore pool    fixup back-edge
                        walk then       moves
                        reconcile
                        walk head
              │
              ├── BrIf → PInst::Beq/Bne + label fixup
              ├── Br   → PInst::J + label fixup
              ├── Label → PInst::Label (emits nothing, records offset)
              ├── Call  → clobber spill + arg moves + PInst::Call + reloads
              ├── Select32 → PInst sequence (sub + and + add)
              └── (existing arithmetic/cmp/mov/load/store/ret)
                        │
                        ▼
              pinsts.reverse() + trace.reverse()
                        │
                        ▼
              Rv32Emitter::emit_all(pinsts)
                        │
                   label fixup pass
                        │
                        ▼
                   machine code bytes
```

### Call handling (backward walk order)

```
    ┌─── Backward stream (reversed later) ───┐
    │                                         │
    │  1. Free return-value regs (def logic)  │
    │  2. Emit Lw reloads for clobbered vregs │  ← post-call in execution
    │  3. Emit PInst::Call { target }         │
    │  4. Emit Mv args → ARG_REGS            │  ← pre-call in execution
    │  5. Emit Sw spills for clobbered vregs  │  ← pre-call in execution
    │                                         │
    └─────────────────────────────────────────┘

    After reverse: spills → arg moves → call → reloads
```

### IfThenElse handling

```
    merge-point state (from prior instructions after the if/else)
         │
         ├── snapshot pool state
         │
         ├── walk else_body → state_else
         │   └── if differs from state_then: spill disagreeing vregs
         │
         ├── restore snapshot
         │
         ├── walk then_body → state_then (canonical)
         │
         └── walk head (BrIf) → use state_then as head state
```

### Loop handling

```
    post-loop state (from instructions after the loop)
         │
         ├── walk body backward → body-entry state
         │
         ├── walk header backward → header state
         │
         └── compare back-edge state with header state
             └── if differs: emit Mv fixups at back-edge
```

### Label fixup in Rv32Emitter

```
    Pass 1: emit all PInsts, recording label offsets and branch locations
    Pass 2: patch branch instruction bytes with real offsets
```

## Phases

1. Branch infra + Select32
2. Direct calls (clobber, spill, reload, arg/ret)
3. IfThenElse walk
4. Loop walk
5. Sret calls + param precoloring + frame plumbing
6. Filetest validation + cleanup
