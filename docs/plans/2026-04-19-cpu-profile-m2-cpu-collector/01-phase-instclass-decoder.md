# Phase 1 — `InstClass` extension + decoder updates

Refactor the existing `InstClass::Jal` and `InstClass::Jalr` variants
into five call/return-aware variants. Decoder sites in `executor/jump.rs`
and `executor/compressed.rs` classify each jump based on `rd`/`rs1` to
emit the correct variant. `CycleModel::Esp32C6` cost map mirrors the
old per-mnemonic costs across the new variants.

This phase is **fully independent of m1** — it only touches
`lp-riscv-emu`'s instruction classification and cycle accounting,
both of which exist on `main` today. Can ship in parallel with m1.

**Sub-agent suitable**: yes (mechanical refactor + decoder logic +
unit tests).

## Dependencies

- None (no upstream phase, no m1 dependency).

## Files

### `lp-riscv-emu/src/emu/cycle_model.rs`

Replace the `Jal` and `Jalr` variants of `InstClass`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstClass {
    Alu, Mul, DivRem, Load, Store,
    BranchTaken, BranchNotTaken,
    JalCall,         // JAL rd, _   with rd != x0
    JalTail,         // JAL x0, _
    JalrCall,        // JALR rd, _, _   with rd != x0
    JalrReturn,      // JALR x0, x1, 0   (canonical `ret`)
    JalrIndirect,    // any other JALR x0, _, _
    Lui, Auipc, System, Fence, Atomic,
}
```

Update `CycleModel::Esp32C6::cycles_for`:

```rust
InstClass::JalCall  | InstClass::JalTail                                => 2,
InstClass::JalrCall | InstClass::JalrReturn | InstClass::JalrIndirect   => 3,
```

`CycleModel::InstructionCount` keeps its uniform 1.

### `lp-riscv-emu/src/emu/executor/mod.rs`

Add `inst_size: u8` to `ExecutionResult`:

```rust
pub struct ExecutionResult {
    pub new_pc: Option<u32>,
    pub should_halt: bool,
    pub syscall: bool,
    pub class: InstClass,
    pub inst_size: u8,           // 2 for compressed, 4 for full
    pub log: Option<InstLog>,
}
```

Every constructor of `ExecutionResult` in the executor modules sets
`inst_size: 4` (or `2` for compressed). The compressed dispatcher in
`compressed::decode_execute_compressed` sets `inst_size: 2` everywhere.

(P2 consumes `inst_size` in the run-loop helper. Including the field
in P1 keeps P1 self-contained — the field is harmless dead data
until P2 lands.)

### `lp-riscv-emu/src/emu/executor/jump.rs`

In `decode_execute_jal`:

```rust
let class = if rd == 0 {
    InstClass::JalTail
} else {
    InstClass::JalCall
};
```

In `decode_execute_jalr`:

```rust
let class = if rd != 0 {
    InstClass::JalrCall
} else if rs1 == 1 /* x1 = ra */ && imm == 0 {
    InstClass::JalrReturn
} else {
    InstClass::JalrIndirect
};
```

`rd`, `rs1`, `imm` are already decoded locally in both functions —
no plumbing changes needed.

### `lp-riscv-emu/src/emu/executor/compressed.rs`

Mapping table for the four compressed jumps:

| Mnemonic     | rd  | New class                                      |
| ------------ | --- | ---------------------------------------------- |
| `c.j`        | x0  | `JalTail`                                      |
| `c.jal`      | x1  | `JalCall`                                      |
| `c.jr rs1`   | x0  | `JalrReturn` if `rs1 == 1` else `JalrIndirect` |
| `c.jalr rs1` | x1  | `JalrCall`                                     |

Update each of `execute_c_j`, `execute_c_jal`, `execute_c_jr`,
`execute_c_jalr` to set the right variant.

## Tests

### `lp-riscv-emu/src/emu/executor/jump.rs#tests` — new

Add five test cases (or extend existing JAL/JALR tests with
classification assertions):

```rust
#[test]
fn classifies_jal_call_when_rd_nonzero() {
    // JAL x1, +8  →  encoding: 0x008000ef  (rd=x1, imm=8)
    let result = decode_execute::<LoggingDisabled>(0x008000ef, 0x1000, ...);
    assert_eq!(result.unwrap().class, InstClass::JalCall);
}

#[test]
fn classifies_jal_tail_when_rd_zero() {
    // JAL x0, +8  →  encoding: 0x0080006f
    let result = decode_execute::<LoggingDisabled>(0x0080006f, 0x1000, ...);
    assert_eq!(result.unwrap().class, InstClass::JalTail);
}

#[test]
fn classifies_jalr_call_when_rd_nonzero() {
    // JALR x5, x6, 0  →  encoding: 0x000302e7
    ...
    assert_eq!(class, InstClass::JalrCall);
}

#[test]
fn classifies_jalr_return_for_canonical_ret() {
    // JALR x0, x1, 0  →  encoding: 0x00008067   ("ret")
    ...
    assert_eq!(class, InstClass::JalrReturn);
}

#[test]
fn classifies_jalr_indirect_otherwise() {
    // JALR x0, x5, 0  →  rd=0, rs1=x5≠x1 → indirect
    ...
    assert_eq!(class, InstClass::JalrIndirect);
}

#[test]
fn classifies_jalr_indirect_when_rs1_is_ra_but_imm_nonzero() {
    // JALR x0, x1, 4  →  not canonical ret because imm≠0
    ...
    assert_eq!(class, InstClass::JalrIndirect);
}
```

Encoding byte values can be cross-checked against
`https://luplab.gitlab.io/rvcodecjs/`.

### `lp-riscv-emu/src/emu/executor/compressed.rs#tests` — new

Eight test cases covering the table above (one per mnemonic, plus
the `c.jr` and `c.jalr` rs1 variations):

```rust
#[test]
fn classifies_c_j_as_jal_tail() {
    // c.j +8  →  encoding: 0xa011
    ...
    assert_eq!(class, InstClass::JalTail);
    assert_eq!(inst_size, 2);
}

#[test]
fn classifies_c_jal_as_jal_call() { ... assert_eq!(class, InstClass::JalCall); }

#[test]
fn classifies_c_jr_ra_as_jalr_return() {
    // c.jr ra  →  encoding: 0x8082
    assert_eq!(class, InstClass::JalrReturn);
}

#[test]
fn classifies_c_jr_other_as_jalr_indirect() {
    // c.jr x5  →  rs1=x5
    assert_eq!(class, InstClass::JalrIndirect);
}

#[test]
fn classifies_c_jalr_as_jalr_call() { ... }
```

### Existing tests

All existing executor tests must continue to pass. The only change is
the `class` field's variant; existing tests that don't assert on
`class` are unaffected. Tests that do assert on `class` (search for
`InstClass::Jal` and `InstClass::Jalr` in test code) need their
expected variant updated according to the table above.

### Cycle-model tests

If `cycle_model.rs` has unit tests covering `Esp32C6::cycles_for`,
add cases for each new variant asserting the same costs as the old
`Jal`/`Jalr` variants.

## Risk + rollout

- **Risk**: any executor or cycle-model test that pattern-matches
  on `InstClass::Jal`/`Jalr` will fail to compile until updated.
  Search `lp-riscv-emu` for those identifiers and update each
  match site to the new variant set.
- **Rollback**: trivial revert; no dependencies on this phase from
  upstream code yet (P2 is the consumer).
- **Hidden coupling**: cranelift IR translation in
  `lp-riscv-emu/src/cl_*` may also reference `InstClass`. Confirm
  via `rg "InstClass::J" lp-riscv-emu/` before submitting.

## Acceptance

- All `cargo test -p lp-riscv-emu` passes.
- New unit tests added per the test plan above.
- `rg "InstClass::Jal\b|InstClass::Jalr\b" lp-riscv/` returns no
  matches (variants fully renamed away).
