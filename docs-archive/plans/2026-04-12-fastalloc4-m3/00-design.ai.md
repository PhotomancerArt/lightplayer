# M3.1: Advanced Straight-Line - Design

## Scope

Extend the allocator to robustly handle advanced straight-line scenarios:
1. **Spill pressure testing** - verify eviction logic with limited registers
2. **Entry param moves** - record moves when params get evicted from ABI regs
3. **Stack-passed args** - handle arguments beyond ABI register limit
4. **Comprehensive validation** - filetests for each scenario

## Out of Scope (M3.2)

- Function calls (clobbers, call/ret ABI)
- Sret handling
- Control flow (If/Loop/Seq)

## File Structure

```
lp-shader/lpvm-native/src/
├── fa_alloc/
│   ├── mod.rs              # allocate(), AllocResult, AllocTestBuilder
│   ├── walk.rs             # walk_linear() - backward walk allocator
│   ├── render.rs           # render_alloc_output() - annotated VInst output
│   ├── pool.rs             # RegPool, RegPool::with_capacity(n) [NEW]
│   ├── spill.rs            # SpillAlloc
│   ├── liveness.rs         # analyze_liveness()
│   └── test/
│       └── builder.rs      # AllocTestBuilder, AllocTestRunner [NEW]
├── abi/
│   └── func.rs             # FuncAbi::with_arg_reg_limit(n) [NEW]
└── rv32/
    └── emit.rs             # Forward emitter (updates for entry moves)

lp-shader/lps-filetests/filetests/
└── advanced/
    ├── spill_pressure_3regs.glsl    # [NEW] forces spilling
    └── param_eviction.glsl           # [NEW] entry param moves
```

## Conceptual Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    AllocTestBuilder                           │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐        │
│  │ .pool_size() │  │ .arg_reg_limit()│ │ .vinst()    │        │
│  │   (4)        │  │   (1 or 0)   │  │   or .lpir()│        │
│  └──────────────┘  └──────────────┘  └──────────────┘        │
└────────────────────────┬──────────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────────────┐
│                    walk_linear()                            │
│                                                             │
│   ┌─────────────────┐      ┌──────────────────┐            │
│   │ Entry: Seed     │      │ Loop: Backward   │            │
│   │ params at ABI   │──────│ walk, allocate   │            │
│   │ regs (limited   │      │ uses, free defs │            │
│   │ by arg_reg_limit)│     └────────┬─────────┘            │
│   └─────────────────┘                │                      │
│                                     │                      │
│                                     ▼                      │
│                         ┌──────────────────┐              │
│                         │ Exit: Record     │              │
│                         │ entry moves if   │              │
│                         │ params evicted   │              │
│                         └────────┬─────────┘              │
│                                  │                        │
└──────────────────────────────────┼────────────────────────┘
                                   │
                                   ▼
                         ┌──────────────────┐
                         │ AllocOutput      │
                         │ - allocs[]       │
                         │ - edits[]        │
                         │ - spill_slots    │
                         └────────┬─────────┘
                                  │
                                  ▼
                    ┌─────────────────────────┐
                    │ render_alloc_output()   │
                    │ adds annotations:       │
                    │ ; move: a0 -> t1        │
                    │ ; spill: t1 -> slot0    │
                    │ ; reload: slot0 -> t0   │
                    └────────┬────────────────┘
                               │
                               ▼
                    ┌─────────────────────────┐
                    │ AllocTestResult         │
                    │ .expect_vinst() -       │
                    │ compares to expected    │
                    └─────────────────────────┘
```

## Key Design Decisions

### 1. RegPool::with_capacity(n)

For testing spill logic without needing 17+ vregs:
- Constructor takes `capacity: usize`
- Only first `n` registers from `ALLOC_POOL` are available
- Eviction happens when pool exhausted, just like normal

### 2. FuncAbi::with_arg_reg_limit(n)

For testing stack-passed arguments:
- Limits argument registers (normally a0-a7 = 8)
- Setting to 1 means only a0 for args, rest on stack
- Setting to 0 means all args on stack

### 3. Entry Move Recording

Current behavior: params seeded at ABI regs, no entry moves recorded.

New behavior:
1. After backward walk, check each param's final location
2. If different from ABI reg: generate `Edit::Move(abi → final)`
3. Insert at `EditPoint::Before(0)`
4. Render shows: `; move: param_i0: a0 -> t1`

### 4. Builder Pattern

```rust
// Example: test spill with limited pool
alloc_test()
    .pool_size(3)        // Only 3 regs available
    .lpir("fn test() -> i32 { let a = 1; let b = 2; let c = 3; let d = 4; return a + b + c + d; }")
    .expect_vinst("
        i0 = IConst32 1
        ; write: i0 -> t0
        i1 = IConst32 2
        ; write: i1 -> t1
        i2 = IConst32 3
        ; write: i2 -> t2
        ; spill: t0 -> slot0
        i3 = IConst32 4
        ; write: i3 -> t0
        ; reload: slot0 -> t1
        ...
    ");
```

## Component Details

### AllocTestBuilder

```rust
pub fn alloc_test() -> AllocTestBuilder;

impl AllocTestBuilder {
    pub fn pool_size(self, n: usize) -> Self;
    pub fn arg_reg_limit(self, n: usize) -> Self;
    pub fn vinst(self, input: &str) -> Self;
    pub fn lpir(self, input: &str) -> Self;
    pub fn run(self) -> AllocTestResult;
}
```

### AllocTestResult

```rust
impl AllocTestResult {
    pub fn expect_vinst(self, expected: &str);
    pub fn expect_spill_slots(self, count: u32);
    pub fn expect_moves(self, count: usize);
}
```

## Success Criteria

- All 23 existing M2 filetests pass
- New filetest `spill_pressure_3regs.glsl` passes
- New filetest `param_eviction.glsl` passes
- Unit tests with builder pattern for each scenario
- No regressions in validation commands
