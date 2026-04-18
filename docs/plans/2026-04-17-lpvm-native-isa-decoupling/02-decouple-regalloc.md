# Phase 2: Decouple Regalloc

## Goal

Cut every `crate::isa::rv32::*` import out of `lp-shader/lpvm-native/src/regalloc/`.
After this phase, regalloc speaks only in terms of `crate::abi::*`,
`crate::vinst`, `FuncAbi`, and `IsaTarget` — never directly to an ISA leaf.

This phase relies on Phase 1 having added `IsaTarget`, `FuncAbi::isa()`,
`FuncAbi::arg_regs()`, `FuncAbi::is_caller_saved_pool()`, and the
`IsaTarget` per-target methods (`allocatable_pool_order`,
`is_in_allocatable_pool`, `reg_name`, `sret_uses_buffer_for`).

## Inventory of leakage to remove

(Line numbers from current `00-notes.md` snapshot; verify with `rg`.)

| File                  | Current import                                            | Replacement                                  |
| --------------------- | --------------------------------------------------------- | -------------------------------------------- |
| `regalloc/mod.rs:32`  | `Alloc::Reg(crate::isa::rv32::gpr::PReg)`                 | `Alloc::Reg(u8)`                             |
| `regalloc/mod.rs:48`  | `Alloc::reg() -> Option<crate::isa::rv32::gpr::PReg>`     | `Alloc::reg() -> Option<u8>`                 |
| `regalloc/mod.rs:202` | `gpr::is_callee_saved_pool_gpr(r); abi::PReg::int(r)`     | `func_abi.is_caller_saved_pool(p)` (negated) |
| `regalloc/pool.rs:3`  | `use crate::isa::rv32::gpr::{ALLOC_POOL, PReg}`           | `IsaTarget::allocatable_pool_order()`        |
| `regalloc/walk.rs:15` | `use crate::isa::rv32::gpr::{self, PReg}`                 | (delete)                                     |
| `regalloc/walk.rs:681`| `crate::isa::rv32::abi::SRET_SCALAR_THRESHOLD`            | `func_abi.isa().sret_uses_buffer_for(n)`     |
| `regalloc/verify.rs:8`| `gpr::ALLOC_POOL`                                         | `IsaTarget::is_in_allocatable_pool(p)`       |
| `regalloc/render.rs:9`| `gpr::reg_name`                                           | `IsaTarget::reg_name(p)`                     |

## Steps

### 2.1 Change `Alloc::Reg` payload to `u8`

In `lp-shader/lpvm-native/src/regalloc/mod.rs`:

```rust
pub enum Alloc {
    Reg(u8),         // raw hw encoding; semantics from FuncAbi::isa()
    Spill(SpillSlot),
    None,
}

impl Alloc {
    pub fn reg(self) -> Option<u8> {
        match self { Alloc::Reg(p) => Some(p), _ => None }
    }
}
```

This is a no-op layout change: `crate::isa::rv32::gpr::PReg` is already
`u8`. All call sites that pattern-match on `Alloc::Reg(p)` keep working;
type checkers will flag any place that was treating `p` as
`crate::isa::rv32::gpr::PReg` for trait dispatch.

### 2.2 Update `regalloc/mod.rs::used_callee_saved_from_output`

The current implementation reaches into `gpr::is_callee_saved_pool_gpr` and
constructs `abi::PReg::int(r)` to hand off. Replace with a `FuncAbi`
accessor:

```rust
pub fn used_callee_saved_from_output(
    func_abi: &FuncAbi,
    output: &AllocOutput,
) -> Vec<PReg> {
    output.allocs
        .iter()
        .filter_map(|a| a.reg())
        .filter(|&p| !func_abi.is_caller_saved_pool(p))
        .map(|p| /* convert u8 → crate::abi::PReg via existing helper */)
        .collect()
}
```

If a `u8 → crate::abi::PReg` adapter doesn't yet exist for the boundary,
add a small one in `crate::abi`. Caller-saved is the existing
`call_clobbers` set on `FuncAbi`; "not caller-saved" within the allocatable
pool ≡ callee-saved-and-used.

### 2.3 Update `regalloc/pool.rs::RegPool::new`

```rust
impl RegPool {
    pub fn new(isa: IsaTarget) -> Self {
        let order = isa.allocatable_pool_order();
        // ... initialize LRU from `order` ...
    }
}
```

All `RegPool::new()` call sites must pass `IsaTarget` — get it from the
`FuncAbi` already in scope. Audit:

```
rg 'RegPool::new\b' lp-shader/lpvm-native/src
```

### 2.4 Update `regalloc/walk.rs`

- Delete the `use crate::isa::rv32::gpr::{self, PReg}` import.
- Replace any `PReg` usage with `u8` (or `crate::abi::PReg` at boundaries).
- At line 681, replace the `matches!` arm using
  `crate::isa::rv32::abi::SRET_SCALAR_THRESHOLD` with
  `func_abi.isa().sret_uses_buffer_for(scalar_count)`. The exact predicate
  shape may need a small refactor to make `n` available where the `matches!`
  lives; pull the SRET check out into a `let` binding above the match.

### 2.5 Update `regalloc/verify.rs`

```rust
fn verify_allocs_within_pool(
    output: &AllocOutput,
    func_abi: &FuncAbi,
) -> Result<(), VerifyError> {
    let isa = func_abi.isa();
    for alloc in &output.allocs {
        if let Some(p) = alloc.reg() {
            if !isa.is_in_allocatable_pool(p) {
                return Err(VerifyError::OutOfPool(p));
            }
        }
    }
    Ok(())
}
```

Update all `verify_*` callers in regalloc tests to pass `&FuncAbi`.

### 2.6 Update `regalloc/render.rs`

```rust
pub fn render_alloc(alloc: Alloc, isa: IsaTarget) -> String {
    match alloc {
        Alloc::Reg(p) => format!("Reg({})", isa.reg_name(p)),
        Alloc::Spill(s) => format!("Spill({})", s),
        Alloc::None => "None".into(),
    }
}
```

Callers pass `IsaTarget` (typically `func_abi.isa()`). If callers don't
have a `FuncAbi` in scope (some debug-only call sites), they pass
`IsaTarget` directly — fine since the renderer is debug code.

### 2.7 Sanity-check tests

`alloc_trace` snapshot tests may print `Reg(N)` today and `Reg(a0)` (or
similar named form) after — register names from `reg_name` are friendlier
output. Update the golden text once and confirm all rendered forms still
read sensibly.

### 2.8 Verify

```
rg 'use crate::isa::rv32' lp-shader/lpvm-native/src/regalloc
# Should produce ZERO matches.

cargo check -p lpvm-native
cargo test -p lpvm-native
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
```

## Validation

- `rg 'use crate::isa::rv32' lp-shader/lpvm-native/src/regalloc` → 0 matches
- `rg 'crate::isa::rv32' lp-shader/lpvm-native/src/regalloc` → 0 matches
  (catches inline-path usage like `crate::isa::rv32::abi::SRET_SCALAR_THRESHOLD`)
- `cargo check -p lpvm-native` clean
- `cargo test -p lpvm-native` all green
- ESP32 target check clean
- Allocation traces still readable (golden updates expected)
- No memory regression: `Alloc` is still `2 bytes` (1-byte discriminant
  + 1-byte u8 payload) per `core::mem::size_of::<Alloc>()`. Add a static
  assert if not present.

## Notes

- `Alloc::Reg(u8)` is the deliberate choice from `00-notes.md` Q5; do not
  widen to `crate::abi::PReg` here. The hot-path memory cost matters.
- `crate::abi::PReg` stays the canonical boundary type for emitter, debug,
  and link consumers. Conversion is one-way at the boundary.
- If you find another `crate::isa::rv32::*` reference in regalloc not
  listed in the inventory above, replace it the same way (FuncAbi or
  IsaTarget accessor) — do not leave any.
