# Phase 4 — `CpuCollector` + unit tests

Add `lp-riscv-emu/src/profile/cpu.rs` containing `CpuCollector`,
`Frame`, `FuncStats`, `CallEdge`, and the `Collector` trait impl.
Re-export `CpuCollector` from `lp-riscv-emu::profile`. Add eight unit
tests for the attribution state machine (no instruction-decoder
dependency in tests — tests drive `on_instruction` directly with
hand-built event sequences).

`CpuCollector` starts with `active: false` (R-INIT). Capture begins
only when `on_gate_action(GateAction::Enable)` fires. Tests for the
gate→active wiring use a synthetic gate-action sequence; tests for
the actual gate impls live in P5.

**Sub-agent suitable**: yes (self-contained data structure + eight
unit tests against a clearly-specified state machine).

## Dependencies

- **P3** — needs `Collector` trait widening (`on_gate_action`,
  widened `on_instruction` signature) and `InstClass` re-export.

## Files

### `lp-riscv-emu/src/profile/cpu.rs` — new

Full implementation per `00-design.md`. Two implementation notes:

1. **`HashMap` capacity**: leave at default. R6 resolved this — low
   tens of thousands of entries in worst case for a 4-frame
   steady-render capture. No FxHash, no capacity hint.
2. **`shadow_stack` capacity**: `Vec::with_capacity(64)` — typical
   call depths in shader/engine code are well under 32; 64 avoids
   one growth.

### `lp-riscv-emu/src/profile/mod.rs`

Add the module:
```rust
#[cfg(feature = "std")]
pub mod cpu;
#[cfg(feature = "std")]
pub use cpu::CpuCollector;
```

### `lp-riscv-emu/src/lib.rs`

Re-export at the crate root for CLI convenience:
```rust
#[cfg(feature = "std")]
pub use profile::CpuCollector;
```

## Tests

All in `lp-riscv-emu/src/profile/cpu.rs#tests` (or sibling
`cpu_tests.rs`).

### Test 1 — `gate_disabled_no_attribution`

```rust
let mut cpu = CpuCollector::new("esp32c6");
// active starts false; no Enable yet
cpu.on_instruction(0x1000, 0x1004, InstClass::Alu, 1);
cpu.on_instruction(0x1004, 0x1008, InstClass::Alu, 1);
assert_eq!(cpu.total_cycles_attributed, 0);
assert!(cpu.func_stats.is_empty());
```

### Test 2 — `simple_call_return`

```rust
let mut cpu = CpuCollector::new("esp32c6");
cpu.on_gate_action(GateAction::Enable);

// Pre-call: 5 alu cycles in <root>
for _ in 0..5 { cpu.on_instruction(0x1000, 0x1004, InstClass::Alu, 1); }
// Call: 0x1014 (callee)
cpu.on_instruction(0x1010, 0x1014, InstClass::JalCall, 2);
// In callee: 10 alu cycles
for _ in 0..10 { cpu.on_instruction(0x1014, 0x1018, InstClass::Alu, 1); }
// Return
cpu.on_instruction(0x1024, 0x1010, InstClass::JalrReturn, 3);

assert_eq!(cpu.total_cycles_attributed, 5 + 2 + 10 + 3);

// <root> self_cycles: 5 (pre-call) + 3 (return inst attributes to caller? No—
//   return inst executes IN callee, so its self_cycles credit goes to callee).
// Confirm against impl: the order in on_instruction is "bump self for current_pc()
// FIRST, then mutate stack." So return-inst self_cycles credits callee.
assert_eq!(cpu.func_stats[&0].self_cycles, 5);          // <root>
assert_eq!(cpu.func_stats[&0x1014].self_cycles, 10 + 3); // callee
assert_eq!(cpu.func_stats[&0x1014].calls_in, 1);
assert_eq!(cpu.func_stats[&0x1014].inclusive_cycles, 10 + 3);
assert_eq!(cpu.call_edges[&(0, 0x1014)].count, 1);
```

(Note on bump-then-mutate ordering: this is a load-bearing detail of
the `on_instruction` impl. Document it at the top of `on_instruction`
in `cpu.rs`. The JAL/JALR's own cycle cost gets credited to the
caller's self-cycles (for JalCall) and to the callee's self-cycles
(for JalrReturn) because that's the function executing the
instruction. This matches callgrind semantics.)

### Test 3 — `nested_three_deep`

```rust
let mut cpu = CpuCollector::new("esp32c6");
cpu.on_gate_action(GateAction::Enable);

// A → B → C → return → return → return
cpu.on_instruction(0x1000, 0x2000, InstClass::JalCall, 2);  // <root> → A=0x2000
for _ in 0..3 { cpu.on_instruction(0x2000, 0x2004, InstClass::Alu, 1); } // 3 in A
cpu.on_instruction(0x2010, 0x3000, InstClass::JalCall, 2);  // A → B=0x3000
for _ in 0..5 { cpu.on_instruction(0x3000, 0x3004, InstClass::Alu, 1); } // 5 in B
cpu.on_instruction(0x3010, 0x4000, InstClass::JalCall, 2);  // B → C=0x4000
for _ in 0..7 { cpu.on_instruction(0x4000, 0x4004, InstClass::Alu, 1); } // 7 in C
cpu.on_instruction(0x4010, 0x3014, InstClass::JalrReturn, 3); // C → B
cpu.on_instruction(0x3014, 0x2014, InstClass::JalrReturn, 3); // B → A
cpu.on_instruction(0x2014, 0x1004, InstClass::JalrReturn, 3); // A → <root>

// Inclusive cycles bubble:
// C.inclusive = 7 + 3 (return) = 10
// B.inclusive = 5 + 2 (call to C) + C.inclusive + 3 (return) = 5 + 2 + 10 + 3 = 20
// A.inclusive = 3 + 2 (call to B) + B.inclusive + 3 (return) = 3 + 2 + 20 + 3 = 28
assert_eq!(cpu.func_stats[&0x4000].inclusive_cycles, 10);
assert_eq!(cpu.func_stats[&0x3000].inclusive_cycles, 20);
assert_eq!(cpu.func_stats[&0x2000].inclusive_cycles, 28);
```

### Test 4 — `tail_call_swaps_top`

```rust
let mut cpu = CpuCollector::new("esp32c6");
cpu.on_gate_action(GateAction::Enable);

// <root> → A (call), A → B (tail), B → C (tail), C → <root> (return)
cpu.on_instruction(0x1000, 0x2000, InstClass::JalCall, 2);
for _ in 0..3 { cpu.on_instruction(0x2000, 0x2004, InstClass::Alu, 1); }
cpu.on_instruction(0x2010, 0x3000, InstClass::JalTail, 2);   // A → B (tail)
for _ in 0..5 { cpu.on_instruction(0x3000, 0x3004, InstClass::Alu, 1); }
cpu.on_instruction(0x3010, 0x4000, InstClass::JalTail, 2);   // B → C (tail)
for _ in 0..7 { cpu.on_instruction(0x4000, 0x4004, InstClass::Alu, 1); }
cpu.on_instruction(0x4010, 0x1004, InstClass::JalrReturn, 3); // C → <root>

// Stack should never have been deeper than 1 frame (each tail pop+push)
// All three (A, B, C) appear in func_stats with their self-cycles.
assert!(cpu.func_stats.contains_key(&0x2000));
assert!(cpu.func_stats.contains_key(&0x3000));
assert!(cpu.func_stats.contains_key(&0x4000));

// C is the one that returned to root, so call_edge (B, C) gets the
// final inclusive credit. A's frame never closed cleanly (replaced by tail);
// its inclusive_cycles is recorded at the moment of the JalTail pop.
```

### Test 5 — `orphaned_return_at_root`

```rust
let mut cpu = CpuCollector::new("esp32c6");
cpu.on_gate_action(GateAction::Enable);

// Return from root — should be a no-op, no panic.
cpu.on_instruction(0x1000, 0x0, InstClass::JalrReturn, 3);
assert_eq!(cpu.total_cycles_attributed, 3);
// Self-cycles credited to <root> (current_pc when return executed).
assert_eq!(cpu.func_stats[&0].self_cycles, 3);
```

### Test 6 — `root_self_cycles`

```rust
let mut cpu = CpuCollector::new("esp32c6");
cpu.on_gate_action(GateAction::Enable);

for _ in 0..100 { cpu.on_instruction(0x1000, 0x1004, InstClass::Alu, 1); }
assert_eq!(cpu.func_stats[&0].self_cycles, 100);
```

### Test 7 — `enable_disable_toggle`

```rust
let mut cpu = CpuCollector::new("esp32c6");
cpu.on_gate_action(GateAction::Enable);
for _ in 0..10 { cpu.on_instruction(0x1000, 0x1004, InstClass::Alu, 1); }
cpu.on_gate_action(GateAction::Disable);
for _ in 0..50 { cpu.on_instruction(0x1000, 0x1004, InstClass::Alu, 1); }  // ignored
cpu.on_gate_action(GateAction::Enable);
for _ in 0..20 { cpu.on_instruction(0x1000, 0x1004, InstClass::Alu, 1); }

assert_eq!(cpu.total_cycles_attributed, 10 + 20);
```

### Test 8 — `call_edge_aggregation`

```rust
let mut cpu = CpuCollector::new("esp32c6");
cpu.on_gate_action(GateAction::Enable);

// Same caller→callee edge three times.
for _ in 0..3 {
    cpu.on_instruction(0x1000, 0x2000, InstClass::JalCall, 2);
    for _ in 0..5 { cpu.on_instruction(0x2000, 0x2004, InstClass::Alu, 1); }
    cpu.on_instruction(0x2010, 0x1004, InstClass::JalrReturn, 3);
}

assert_eq!(cpu.call_edges[&(0, 0x1000)].count, 3);     // <root> → 0x1000 caller? No—
// The caller_pc on the JalCall is 0x1000 (the JAL's PC). So edge is (0x1000, 0x2000).
assert_eq!(cpu.call_edges[&(0x1000, 0x2000)].count, 3);
assert_eq!(cpu.func_stats[&0x2000].calls_in, 3);
```

(The "caller_pc" stored in Frame is the PC of the JAL instruction
itself, not the function containing it. That's what callgrind does
and it's what the symbolizer can resolve via interval lookup. Add
this clarification to the doc comment on `Frame`.)

## Risk + rollout

- **Risk**: subtle off-by-one in cycle attribution. The eight tests
  above pin every transition. Run them under
  `cargo test -- --nocapture` and visually inspect `total_cycles_attributed`
  in each.
- **Risk**: `Frame.cycles_at_entry` snapshots `total_cycles_attributed`
  *before* the call's own cost is added. Re-confirm: `on_instruction`
  bumps self-cycles BEFORE matching on class, so by the time
  `push_frame` runs, the call instruction's cost is already in
  `total_cycles_attributed`. The pop's `inclusive = current -
  cycles_at_entry` therefore *includes* the call's cost as part of
  the callee's inclusive. That matches callgrind. Document at the
  top of `on_instruction`.
- **Rollback**: trivial; entire phase is a new file plus two
  re-export lines.

## Acceptance

- All eight new unit tests pass.
- `cargo test -p lp-riscv-emu` passes.
- `cargo doc -p lp-riscv-emu` shows `CpuCollector` exported at the
  crate root.
