# Design: LPIR inliner M2 — Block / ExitBlock

## Scope of work

Add a structured forward-only region construct to LPIR (`Block`, `ExitBlock`) so multi-return inlined callees (M3) can merge at one point without misusing `LoopStart` + `Break`. The closing marker is the existing **`End`** op (same convention as `IfStart` / `LoopStart` / `SwitchStart`) — no new closer is added. Wire the construct end-to-end:

- **lpir**: two new op variants (`Block { end_offset }`, `ExitBlock`); `FunctionBuilder` helpers (`push_block`, `end_block`); print/parse; validation; interpreter; `const_fold` conservative arm.
- **lpvm-native**: new `Region::Block { body, exit_label }` variant; build it in `lower.rs`; thread it through `regalloc/walk.rs`, `regalloc/liveness.rs`, and `rv32/debug/region.rs`.
- **lpvm-wasm**: map `Block` → `block`, the closing `End` → `end`, `ExitBlock` → `br <depth>` in `emit/ops.rs` (`CtrlEntry::Block` tracks the outer depth).
- **lpvm-cranelift**: extend `emit/control.rs` with a `CtrlFrame::Block` variant and merge-block jump; `End` closes a `Block` frame the same way it closes any other.
- **Tests**: unit tests at every layer (LPIR has no direct `.lpir` filetests today, so unit tests in each crate are the test surface). Cover the four roadmap scenarios:
  1. Block with no `ExitBlock` (fall-through).
  2. Block with one `ExitBlock` (skip tail).
  3. Block with `ExitBlock` inside an `IfStart` (conditional skip).
  4. Nested `Block`s with `ExitBlock` targeting the inner block.

`BrIfNot` semantics are unchanged in M2 — it remains loop-only. Conditional exits from a `Block` use `IfStart { cond } / ExitBlock / End`.

Validation commands:

- `cargo test -p lpir`
- `cargo test -p lpvm-native`
- `cargo test -p lpvm-wasm`
- `cargo test -p lpvm-cranelift`
- `cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server`

## File structure

```
lp-shader/
├── lpir/
│   └── src/
│       ├── lpir_op.rs              # UPDATE: add Block / ExitBlock variants; def_vreg() returns None
│       ├── builder.rs              # UPDATE: BlockEntry::Block; push_block / end_block; close_brace_for_text dispatches Block
│       ├── print.rs                # UPDATE: emit `block {`, `exit_block`; reuse `}` for close
│       ├── parse.rs                # UPDATE: parse `block {`, `exit_block`; close `}` already dispatches via stack state
│       ├── validate.rs             # UPDATE: StackEntry::Block; ExitBlock requires enclosing Block (skip If/Loop/Switch arms); End pops Block too
│       ├── interp.rs               # UPDATE: Ctrl::Block { exit }; ExitBlock unwinds to it; End arm gains Some(Ctrl::Block) → pop, pc += 1
│       ├── const_fold.rs           # UPDATE: add Block / ExitBlock to conservative-clear arm
│       └── tests/
│           ├── mod.rs              # UPDATE: pub mod block_ops;
│           └── block_ops.rs        # NEW: 4 scenarios × validate + interp + print/parse roundtrip
│
├── lpvm-native/
│   └── src/
│       ├── region.rs               # UPDATE: Region::Block { body, exit_label }
│       ├── lower.rs                # UPDATE: lower_range builds Region::Block; BlockFrame stack mirrors LoopFrame; Br to exit_label for ExitBlock; Else | End arm already covers Block close
│       ├── regalloc/
│       │   ├── walk.rs             # UPDATE: dispatch Region::Block (first-vinst + walk)
│       │   └── liveness.rs         # UPDATE: forward-only live-in/out for Region::Block
│       ├── rv32/debug/region.rs    # UPDATE: dump `block { … }`
│       └── tests/                  # UPDATE: add block-ops smoke (lower + regalloc) — location follows existing convention
│
├── lpvm-wasm/
│   └── src/emit/
│       ├── control.rs              # UPDATE: CtrlEntry::Block { outer_open_depth }; helper `innermost_block_exit_depth`
│       └── ops.rs                  # UPDATE: Block → sink.block + push CtrlEntry::Block; End closes Block via existing pop arm; ExitBlock → sink.br(depth)
│                                   # UPDATE: tests in same file or sibling module per local convention
│
└── lpvm-cranelift/
    └── src/emit/
        ├── mod.rs                  # UPDATE: CtrlFrame::Block { merge_block }
        └── control.rs              # UPDATE: Block creates merge cl::Block; ExitBlock = jump merge + switch_to_unreachable_tail; End arm closes Block (jump + switch_to_block)
                                    # UPDATE: tests added per local convention
```

## Conceptual architecture

### Op semantics

```
Block { end_offset }   ── enter forward region; closed by End at end_offset-1
ExitBlock              ── unconditional forward jump to nearest enclosing Block's End+1 (= end_offset)
End                    ── existing polymorphic closer; pops the innermost open frame
                          (If / Else / Loop / Switch / SwitchArm / Block)
```

`ExitBlock` searches outward, skipping `IfStart`/`Else`/`LoopStart`/`SwitchStart` frames, binding to the nearest `Block` frame (mirror of how `Break` finds the nearest `Loop`).

**Design note on closer:** LPIR uses one polymorphic `End` for every structured op (see `IfStart`, `LoopStart`, `SwitchStart`, switch arms). The validator pops the stack on `End`; the interpreter dispatches on `ctrl.last()`; print emits `}`; parse uses `close_brace_for_text` to dispatch on stack state. We keep that convention for `Block` rather than introducing an `EndBlock` op. (The roadmap proposed `EndBlock`; we intentionally diverge — see `00-notes.md` Q0.)

### Layered flow

```
                ┌──────────────────────────────────────────────┐
   producer ──▶ │  M3 inliner (later)                          │
                │  emits: Block { e } … ExitBlock … End        │
                └──────────────────────────────────────────────┘
                                   │
                                   ▼
                ┌──────────────────────────────────────────────┐
                │  lpir                                         │
                │  • LpirOp variants (Block, ExitBlock)         │
                │  • FunctionBuilder helpers (offset patching)  │
                │  • print / parse round-trip (`}` closes Block)│
                │  • validate (StackEntry::Block, pairing)      │
                │  • interp (Ctrl::Block { exit })              │
                │  • const_fold conservative arm                │
                └──────────────────────────────────────────────┘
                                   │
        ┌──────────────────────────┼──────────────────────────┐
        ▼                          ▼                          ▼
 ┌─────────────┐          ┌─────────────┐           ┌─────────────────┐
 │ lpvm-native │          │  lpvm-wasm  │           │ lpvm-cranelift  │
 │             │          │             │           │                 │
 │ Region::Bl. │          │ sink.block  │           │ cl::Block       │
 │  { body,    │          │ + sink.end  │           │ + jump(merge)   │
 │    exit }   │          │ + sink.br N │           │ + switch_to(    │
 │             │          │             │           │     merge)      │
 │ regalloc:   │          │ N = depth   │           │                 │
 │  forward-   │          │ to nearest  │           │ ctrl_stack adds │
 │  only       │          │ Block       │           │ Block frame     │
 └─────────────┘          └─────────────┘           └─────────────────┘
```

### Native region tree

```
Before (today):                 After (M2):
  Region                          Region
  ├── Linear                      ├── Linear
  ├── IfThenElse                  ├── IfThenElse
  ├── Loop                        ├── Loop
  └── Seq                         ├── Seq
                                  └── Block { body, exit_label }   ◀── NEW
```

`Region::Block` differs from `Region::Loop`:

- **No header label / back-edge.** Entry is the start of `body`.
- **Single forward exit.** Liveness flows forward only, no fixpoint.

### Frame / stack additions

| Layer                  | Existing                                | Added                                              |
|------------------------|-----------------------------------------|----------------------------------------------------|
| `lpir` builder         | `BlockEntry::{If,Else,Loop,Switch}`     | `BlockEntry::Block`                                |
| `lpir` validate        | `StackEntry::{If,Else,Loop,Switch,Arm}` | `StackEntry::Block`                                |
| `lpir` interp          | `Ctrl::{If,Loop,SwitchArm}`             | `Ctrl::Block { exit }`                             |
| `lpvm-native` lower    | `loop_stack: Vec<LoopFrame>`            | `block_stack: Vec<BlockFrame { exit: LabelId }>`   |
| `lpvm-wasm` ctrl       | `CtrlEntry::{If,Else,Loop,Switch,…}`    | `CtrlEntry::Block { outer_open_depth }`            |
| `lpvm-cranelift` lower | `CtrlFrame::{If,Else,Loop,Switch,…}`    | `CtrlFrame::Block { merge_block }`                 |
