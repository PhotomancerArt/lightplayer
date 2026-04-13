# Phase 3: Snapshot Tests

## Scope

Update `fa_alloc/mod.rs` to wire up walk.rs, add `expect_alloc()` helper,
and create comprehensive snapshot tests.

## Implementation

### File Updates: `fa_alloc/mod.rs`

Add module declaration:
```rust
pub mod walk;
pub mod render;
```

Update `allocate()` function to use walk:
```rust
pub fn allocate(lowered: &LoweredFunction, func_abi: &FuncAbi) -> Result<AllocResult, AllocError> {
    // For now, only handle Linear regions (no control flow)
    if lowered.region_tree.root == REGION_ID_NONE {
        return Ok(AllocResult {
            trace: AllocTrace::new(),
            spill_slots: 0,
        });
    }
    
    // Check for supported region types (Linear only for M2)
    let root_region = &lowered.region_tree.nodes[lowered.region_tree.root as usize];
    match root_region {
        Region::Linear { start, end } => {
            let vinst_slice = &lowered.vinsts[*start as usize..*end as usize];
            let _output = walk::walk_linear(
                vinst_slice,
                &lowered.vreg_pool,
                func_abi,
            )?;
            
            // TODO: Convert AllocOutput to AllocResult for M2
            // For now, just return success
            Ok(AllocResult {
                trace: AllocTrace::new(),
                spill_slots: 0,
            })
        }
        _ => Err(AllocError::UnsupportedControlFlow),
    }
}
```

### Snapshot Test Helper

Add to `mod tests`:
```rust
fn expect_alloc(input: &str, expected: &str) {
    use crate::debug::vinst;
    use crate::rv32::abi;
    use lps_shared::{LpsFnSig, LpsType};
    
    // Parse VInst text
    let (vinsts, _symbols, pool) = vinst::parse(input).unwrap();
    
    // Build minimal ABI (no params for simple tests)
    let func_abi = abi::func_abi_rv32(
        &LpsFnSig {
            name: alloc::string::String::from("test"),
            return_type: LpsType::Void,
            parameters: vec![],
        },
        0,
    );
    
    // Run allocator
    let output = walk::walk_linear(&vinsts, &pool, &func_abi).unwrap();
    
    // Render
    let actual = render::render_alloc_output(&vinsts, &pool, &output);
    
    // Compare
    assert_eq!(actual.trim(), expected.trim(), 
        "\nActual output:\n{}\n\nExpected:\n{}", actual, expected);
}
```

### Test Cases

1. **Simple: iconst + ret**
```rust
#[test]
fn simple_iconst_ret() {
    expect_alloc("
        i0 = IConst32 10
        Ret i0
    ","
        i0 = IConst32 10
        ; write: i0 -> t0
        ; ---------------------------
        ; read: i0 <- t0
        Ret i0
    ");
}
```

2. **Binary: iconst + iconst + add + ret**
```rust
#[test]
fn binary_add() {
    expect_alloc("
        i0 = IConst32 10
        i1 = IConst32 20
        i2 = Add32 i0, i1
        Ret i2
    ","
        i0 = IConst32 10
        ; write: i0 -> t0
        ; ---------------------------
        i1 = IConst32 20
        ; write: i1 -> t1
        ; ---------------------------
        ; read: i0 <- t0
        ; read: i1 <- t1
        i2 = Add32 i0, i1
        ; write: i2 -> t0
        ; ---------------------------
        ; read: i2 <- t0
        Ret i2
    ");
}
```

3. **Spill: more vregs than registers**
4. **Reuse: value used twice**
5. **Dead value: def with no use**
6. **Params: entry parameter handling**

## Steps

1. Add `mod walk;` and `mod render;` declarations
2. Update `allocate()` to call `walk_linear`
3. Add `expect_alloc()` helper
4. Write test cases from simple to complex
5. Run tests, bless expected output as needed

## Validate

```bash
cargo test -p lpvm-native-fa fa_alloc::tests
```

All tests should pass with correct expected output.
