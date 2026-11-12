# FA3 Performance Roadmap — Overview

## Motivation / rationale

The rv32fa backend (lpvm-native) produces significantly more instructions
than rv32 (lpvm-native / cranelift + regalloc2) for the same LPIR input. On the
`caller-save-pressure.glsl` benchmark suite: 140 instructions vs 85 (1.65x).
Per-function ratios range from 1.58x (4 values live across a call) to 4.50x
(trivial identity function).

Since Q32 fixed-point math lowers every float operation to a function call,
call overhead dominates real shader code. Closing the gap here directly
improves code density on the ESP32-C6 where flash and RAM are constrained.

Both backends receive identical LPIR. The gap comes from two layers:

1. **Register allocation** — the backward-walk allocator uses more callee-saved
   registers than necessary and has suboptimal call clobber handling.
2. **Emit** — the emitter unconditionally generates frame pointer setup and
   redundant branches, adding fixed overhead to every function.

## Architecture / design

The FA pipeline:

```
LPIR ─→ lower_ops ─→ VInsts ─→ fa_alloc (backward walk) ─→ emit ─→ machine code
         lower.rs      vinst.rs   fa_alloc/walk.rs           emit.rs
                                  fa_alloc/pool.rs
```

This roadmap targets the regalloc and emit stages. Lowering improvements
(constant folding, immediate fusion) are captured as later milestones.

Key files:

- `lp-shader/lpvm-native/src/fa_alloc/pool.rs` — LRU register pool
- `lp-shader/lpvm-native/src/fa_alloc/walk.rs` — backward walk + call handling
- `lp-shader/lpvm-native/src/emit.rs` — prologue/epilogue + instruction emission
- `lp-shader/lpvm-native/src/rv32/emit.rs` — RV32 encoding + frame layout
- `lp-shader/lpvm-native/src/lower.rs` — LPIR → VInst lowering

## Milestones

```
Milestone 1:  Pool LRU register reuse (regalloc)
Milestone 2:  Evict-then-reload call clobber (regalloc)
Milestone 3:  Empty-function overhead (emit)
Milestone 4:  Constant/immediate folding (lowering)
Milestone 5:  Call arg/ret register shortcuts (regalloc)
```

M1-M2 are small, high-ROI regalloc fixes. M3 is a self-contained emit
improvement. M4-M5 are larger efforts captured for context — they may not be
implemented in this cycle.

## Alternatives considered

- **Rewrite the allocator as a forward walk**: Would match regalloc2 more
  closely but would be a much larger change. The backward walk works correctly;
  the issues are specific and fixable.

- **Port regalloc2 directly**: The FA backend exists specifically because
  regalloc2 carries too much weight for the embedded target. Fixing the few
  areas where we diverge from regalloc2's behavior is more practical.

- **Constant folding in LPIR (before lowering)**: Would benefit both backends
  but is a larger compiler infrastructure change. Doing it at the VInst level
  in the FA lowering is simpler and more targeted.

## Risks

- The pool LRU change (M1) may shift register assignments in ways that affect
  existing filetests. Snapshot tests will need updating but correctness tests
  (execution results) should be stable.

- Evict-then-reload (M2) changes the invariant about when spill stores happen.
  Needs careful verification against the existing call filetests
  (`filetests/call/`).

- Frame pointer omission (M3) may interact with debug tooling that assumes fp
  is always available. Need to check if any debug/trace infrastructure depends
  on the frame pointer.

## Validation

- **Macro**: `scripts/filetests.sh -t rv32,rv32fa` on the perf suite.
  Baseline: `caller-save-pressure.glsl` — rv32 85 inst, rv32fa 140 inst (1.65x).
- **Micro**: `shader-debug` CLI for per-function instruction count comparison.
- **Correctness**: Existing filetests (`filetests/call/`, unit tests in
  `fa_alloc/`) must continue passing.
