# M2: Straight-Line Allocator - Design

## Scope

Implement backward walk allocator for Linear regions only. No calls, no control flow.
Produce per-operand allocations (`AllocOutput`) and edit list. Unit tested with
snapshot tests. Filetests pass for straight-line functions.

## File Structure

```
lp-shader/lpvm-native-fa/src/
├── fa_alloc/
│   ├── mod.rs          # UPDATE: wire up walk, snapshot tests
│   ├── walk.rs         # NEW: backward walk for Linear regions
│   ├── render.rs       # NEW: human-readable AllocOutput rendering
│   ├── pool.rs         # (from M1 - RegPool with LRU)
│   ├── spill.rs        # (from M1 - SpillAlloc)
│   └── trace.rs        # (from M1 - AllocTrace)
├── debug/
│   └── vinst.rs        # (existing - VInst text parser)
└── rv32/
    └── emit.rs         # (from M1 - EmitContext skeleton)
```

## Conceptual Architecture

```
┌─────────────────┐     ┌──────────────┐     ┌─────────────────┐
│  VInst Text     │────▶│  parse()     │────▶│  Vec<VInst>     │
│  (test input)   │     │  (existing)  │     │  + vreg_pool    │
└─────────────────┘     └──────────────┘     └─────────────────┘
                                                        │
                                                        ▼
┌─────────────────┐     ┌──────────────┐     ┌─────────────────┐
│  AllocOutput    │◀────│  walk()      │◀────│  walk_linear()  │
│  (allocs+edits) │     │  (backward)  │     │  (per-inst)     │
└─────────────────┘     └──────────────┘     └─────────────────┘
        │
        ▼
┌─────────────────┐     ┌──────────────┐
│  render()       │────▶│  String      │────▶ snapshot compare
│  (human fmt)    │     │  (expected)  │
└─────────────────┘     └──────────────┘
```

## Components

### Backward Walk (`walk.rs`)

The core allocator. Walks Linear region in reverse order:

```rust
pub fn walk_linear(
    vinsts: &[VInst],
    vreg_pool: &[VReg],
    func_abi: &FuncAbi,
    spill: &mut SpillAlloc,
) -> Result<AllocOutput, AllocError>
```

Algorithm per instruction (backward):
1. **Process defs**: Free registers, record `Alloc::Reg` or `Alloc::Stack`
2. **Process uses**: Allocate registers, evict if needed, record edits for spills/reloads
3. **Record allocations** in `allocs` table at `inst_alloc_offsets[inst_idx]`

Entry handling:
- Before walk: Seed RegPool with params at ABI registers via `alloc_fixed()`
- After walk: For each param, if `pool.home(vreg) != abi_reg`, record entry move

### Rendering (`render.rs`)

Human-readable output for snapshot tests:

```rust
pub fn render_alloc_output(
    vinsts: &[VInst],
    vreg_pool: &[VReg],
    output: &AllocOutput,
) -> String
```

Format:
```
i0 = IConst32 10
; write: i0 -> t0
; ---------------------------
; read: i0 <- t0
Ret i0
```

Rules:
- Instruction appears first
- Write (def) allocations shown after instruction (where the value was written)
- Separator before next instruction
- Read (use) allocations shown before the instruction that uses them
- Spill/reload edits shown between instructions

### Snapshot Tests (`mod.rs`)

```rust
fn expect_alloc(input: &str, expected: &str) {
    let (vinsts, symbols, pool) = debug::vinst::parse(input).unwrap();
    let output = walk_linear(...).unwrap();
    let actual = render_alloc_output(&vinsts, &pool, &output);
    assert_eq!(actual.trim(), expected.trim());
}
```

## Key Design Decisions

1. **Entry params**: Seed at ABI registers, record move only if evicted
2. **Edit recording**: Append during walk, reverse at end (matches regalloc2)
3. **Spill slots**: Assigned during walk via `SpillAlloc`
4. **Operand order**: Uses (reads) before instruction, defs (writes) after
5. **Render format**: Columnar with separators, assembly-style `;` comments

## Deliverables

- `fa_alloc/walk.rs` - backward walk for Linear regions
- `fa_alloc/render.rs` - human-readable output formatting  
- `fa_alloc/mod.rs` - wired up allocator + snapshot tests
- Straight-line filetests passing (`spill_simple.glsl` target)
