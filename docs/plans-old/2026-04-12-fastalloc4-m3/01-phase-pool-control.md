# Phase 1: Pool Control & Builder Skeleton

## Scope

Add the ability to limit register pool size for testing, and create the builder pattern skeleton.

## Implementation

### 1. RegPool::with_capacity(n)

Add constructor to `fa_alloc/pool.rs`:

```rust
impl RegPool {
    /// Create pool with limited capacity (for testing spill logic).
    pub fn with_capacity(n: usize) -> Self {
        let lru: Vec<PReg> = ALLOC_POOL.iter().copied().take(n).collect();
        Self {
            preg_vreg: [None; 32],
            lru,
        }
    }
}
```

### 2. Modify walk_linear to accept pool config

Update `walk.rs`:

```rust
pub fn walk_linear_with_pool(
    vinsts: &[VInst],
    vreg_pool: &[VReg],
    func_abi: &FuncAbi,
    pool: RegPool,  // Accept configured pool
) -> Result<AllocOutput, AllocError> {
    // Use provided pool instead of RegPool::new()
}

// Original function uses default pool
pub fn walk_linear(...) -> Result<AllocOutput, AllocError> {
    walk_linear_with_pool(..., RegPool::new())
}
```

### 3. Create test/builder.rs skeleton

Create `fa_alloc/test/builder.rs`:

```rust
//! Allocation test builder for flexible testing.

pub struct AllocTestBuilder {
    pool_size: Option<usize>,
    arg_reg_limit: Option<usize>,
    input_vinst: Option<String>,
    input_lpir: Option<String>,
}

pub fn alloc_test() -> AllocTestBuilder {
    AllocTestBuilder {
        pool_size: None,
        arg_reg_limit: None,
        input_vinst: None,
        input_lpir: None,
    }
}

impl AllocTestBuilder {
    pub fn pool_size(mut self, n: usize) -> Self {
        self.pool_size = Some(n);
        self
    }
    
    pub fn vinst(mut self, input: &str) -> Self {
        self.input_vinst = Some(input.to_string());
        self
    }
    
    // TODO(phase 3): .arg_reg_limit(), .lpir()
    
    pub fn run(self) -> AllocTestResult {
        // Parse input
        // Create pool with specified size
        // Run walk_linear_with_pool
        // Return result
    }
}

pub struct AllocTestResult {
    output: AllocOutput,
    rendered: String,
}

impl AllocTestResult {
    pub fn expect_vinst(self, expected: &str) {
        assert_eq!(self.rendered.trim(), expected.trim());
    }
}
```

### 4. Add comprehensive unit tests

In `fa_alloc/mod.rs` or `fa_alloc/test/builder.rs`:

```rust
#[test]
fn pool_size_1_everything_spills() {
    // Extreme case: only 1 register, everything must spill
    alloc_test()
        .pool_size(1)
        .vinst("
            i0 = IConst32 10
            i1 = IConst32 20
            i2 = Add32 i0, i1
            Ret i2
        ")
        .expect_vinst("
            i0 = IConst32 10
            ; write: i0 -> t0
            ; spill: t0 -> slot0
            i1 = IConst32 20
            ; write: i1 -> t0
            ; reload: slot0 -> t0
            ; spill: t0 -> slot0
            ; read: i0 <- slot0
            ; read: i1 <- t0
            i2 = Add32 i0, i1
            ; write: i2 -> t0
            ; reload: slot0 -> ?
            Ret i2
        ");
}

#[test]
fn pool_size_2_pairwise_ops() {
    // 2 registers: can do pairwise operations
    alloc_test()
        .pool_size(2)
        .vinst("
            i0 = IConst32 10
            i1 = IConst32 20
            i2 = IConst32 30
            i3 = Add32 i0, i1
            i4 = Add32 i3, i2
            Ret i4
        ")
        .expect_vinst("...");
}

#[test]
fn pool_size_4_small_but_usable() {
    // 4 registers: enough for simple chains
    alloc_test()
        .pool_size(4)
        .vinst("
            i0 = IConst32 1
            i1 = IConst32 2
            i2 = IConst32 3
            i3 = IConst32 4
            i4 = Add32 i0, i1
            i5 = Add32 i2, i3
            i6 = Add32 i4, i5
            Ret i6
        ")
        .expect_vinst("...");
}

#[test]
fn pool_size_full_default_no_spill() {
    // Default 16 registers: no spills for simple cases
    alloc_test()
        .pool_size(16)
        .vinst("
            i0 = IConst32 1
            i1 = IConst32 2
            i2 = IConst32 3
            i3 = IConst32 4
            i4 = IConst32 5
            i5 = IConst32 6
            i6 = Add32 i0, i1
            i7 = Add32 i2, i3
            i8 = Add32 i4, i5
            i9 = Add32 i6, i7
            i10 = Add32 i8, i9
            Ret i10
        ")
        .expect_vinst("
            // No spills expected with 16 regs
            i0 = IConst32 1
            ; write: i0 -> t0
            i1 = IConst32 2
            ; write: i1 -> t1
            // ... etc, no spill annotations
        ");
}
```

## Validation

```bash
cargo test -p lpvm-native fa_alloc::test::builder::
```

## Notes

- Keep existing tests passing
- Builder doesn't need full features yet, just skeleton
- Actual spill rendering will come in Phase 2
