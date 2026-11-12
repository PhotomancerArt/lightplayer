# Phase 5: Unit Tests

## Scope

Comprehensive unit tests covering the allocator's behavior: simple allocation,
spill/reload under register pressure, param precoloring, IConst32
rematerialization, trace output correctness. Also update existing stub tests
that may conflict.

## Code Organization Reminders

- Tests at the top of test modules
- Test helpers at the bottom
- Each test should test one thing clearly
- Use builder helpers for constructing LoweredFunction/FuncAbi test data

## Implementation Details

### Test categories

#### Basic allocation

```rust
#[test]
fn alloc_iconst_ret() {
    // v0 = IConst32 42; Ret v0
    // Verify: Li + Ret, value ends up in a0
}

#[test]
fn alloc_two_iconsts_add_ret() {
    // v0 = IConst32 1; v1 = IConst32 2; v2 = Add32 v0, v1; Ret v2
    // Verify: two Li, one Add, Ret. Result in a0.
}

#[test]
fn alloc_chain_of_ops() {
    // v0 = IConst32 1; v1 = IConst32 2; v2 = Add v0,v1; v3 = Mul v2,v0; Ret v3
    // Verify: regs are reused after last use
}
```

#### Params and precoloring

```rust
#[test]
fn alloc_param_passthrough() {
    // param v1 (a0); Ret v1
    // Verify: no Mv needed, just Ret
}

#[test]
fn alloc_two_params_add() {
    // params v1(a0), v2(a1); v3 = Add v1, v2; Ret v3
    // Verify: Add uses a0, a1; result moved to a0 for return
}

#[test]
fn alloc_param_used_twice() {
    // param v1(a0); v2 = Add v1, v1; Ret v2
    // Verify: a0 used for both sources of Add
}
```

#### Spill and reload

```rust
#[test]
fn alloc_spill_under_pressure() {
    // Create ALLOC_POOL.len() + 1 live values simultaneously
    // Verify: at least one Sw (spill) and one Lw (reload) in output
    // Verify: spill_slots > 0
}

#[test]
fn alloc_spill_correct_offset() {
    // Force a spill, verify the Sw/Lw offsets are frame-pointer relative
    // First spill: offset = -(1 * 4) = -4
    // Second spill: offset = -(2 * 4) = -8
}
```

#### Trace output

```rust
#[test]
fn trace_records_real_decisions() {
    // Simple function
    // Verify: trace entries have non-"STUB" decisions
    // Verify: trace entries mention actual register names
}

#[test]
fn trace_shows_spill() {
    // Force a spill
    // Verify: trace decision mentions "evict" or "spill"
}
```

#### Error cases

```rust
#[test]
fn rejects_if_then_else_region() {
    // LoweredFunction with IfThenElse in region tree
    // Expected: AllocError::UnsupportedControlFlow
}

#[test]
fn rejects_call_instruction() {
    // VInst::Call in a Linear region
    // Expected: AllocError::UnsupportedCall
}
```

### Test helpers (bottom of test module)

```rust
fn build_lowered(vinsts: Vec<VInst>, vreg_pool: Vec<VReg>, tree: RegionTree) -> LoweredFunction {
    LoweredFunction {
        vinsts,
        vreg_pool,
        region_tree: tree,
        symbols: ModuleSymbols::default(),
        loop_regions: Vec::new(),
    }
}

fn build_linear_tree(n_vinsts: u16) -> RegionTree {
    let mut tree = RegionTree::new();
    let root = tree.push(Region::Linear { start: 0, end: n_vinsts });
    tree.root = root;
    tree
}

fn build_void_abi() -> FuncAbi { /* no params, void return */ }
fn build_int_ret_abi() -> FuncAbi { /* no params, i32 return */ }
fn build_two_int_params_abi() -> FuncAbi { /* two i32 params, i32 return */ }
```

## Validate

```bash
cargo test -p lpvm-native --lib -- fa_alloc
```

All new tests pass. No existing tests broken (update stub tests if signatures
changed).
