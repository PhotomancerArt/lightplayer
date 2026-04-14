//! Allocation test builder for flexible testing.
//!
//! ```rust
//! alloc_test()
//!     .pool_size(4)
//!     .run_vinst(
//!         "i0 = IConst32 10
//!          Ret i0",
//!     )
//!     .expect_spill_slots(0)
//!     .expect(
//!         "i0 = IConst32 10
//!          ; write: i0 -> t0
//!          ; ---------------------------
//!          ; read: i0 <- t0
//!          Ret i0",
//!     );
//! ```

use crate::abi::FuncAbi;
use crate::debug::vinst;
use crate::fa_alloc::AllocOutput;
use crate::fa_alloc::pool::RegPool;
use crate::fa_alloc::render::render_alloc_output;
use crate::fa_alloc::walk::walk_linear_with_pool;
use crate::rv32::abi;
use alloc::string::String;
use alloc::vec::Vec;
use lps_shared::{FnParam, LpsFnSig, LpsType, ParamQualifier};

/// Builder for allocation tests.
pub struct AllocTestBuilder {
    pool_size: Option<usize>,
    abi_params: usize,
}

/// Start building an allocation test.
pub fn alloc_test() -> AllocTestBuilder {
    AllocTestBuilder {
        pool_size: None,
        abi_params: 0,
    }
}

impl AllocTestBuilder {
    /// Set the register pool size (for testing spill logic).
    pub fn pool_size(mut self, n: usize) -> Self {
        self.pool_size = Some(n);
        self
    }

    /// Set the number of ABI parameters (for entry move testing).
    pub fn abi_params(mut self, n: usize) -> Self {
        self.abi_params = n;
        self
    }

    /// Run allocation on VInst input and return test result for assertions.
    ///
    /// Panics with a clear message if parse or allocation fails (tests are the caller).
    pub fn run_vinst(self, input: &str) -> AllocTestResult {
        let input = input.trim();

        let (vinsts, _symbols, vreg_pool) =
            vinst::parse(input).unwrap_or_else(|e| panic!("Failed to parse VInst input: {:?}", e));

        let func_abi = if self.abi_params > 0 {
            make_abi_with_params(self.abi_params)
        } else {
            make_test_abi()
        };

        let pool = match self.pool_size {
            Some(n) => RegPool::with_capacity(n),
            None => RegPool::new(),
        };

        let output = walk_linear_with_pool(&vinsts, &vreg_pool, &func_abi, pool)
            .unwrap_or_else(|e| panic!("Allocation failed: {:?}", e));

        // Structural invariants checked on every allocation
        crate::fa_alloc::verify::verify_alloc(&vinsts, &vreg_pool, &output, &func_abi);

        let rendered = render_alloc_output(&vinsts, &vreg_pool, &output);

        AllocTestResult { output, rendered }
    }
}

/// Result of an allocation test — use chained `.expect_*` methods to assert.
pub struct AllocTestResult {
    pub output: AllocOutput,
    pub rendered: String,
}

impl AllocTestResult {
    /// Assert that rendered output matches expected annotated VInst (full alloc plan).
    pub fn expect(&self, expected: &str) -> &Self {
        let expected_normalized = expected.trim().replace("\r\n", "\n");
        let actual_normalized = self.rendered.trim().replace("\r\n", "\n");

        assert_eq!(
            actual_normalized, expected_normalized,
            "Allocation output mismatch\n\nActual:\n{}\n\nExpected:\n{}",
            actual_normalized, expected_normalized
        );
        self
    }

    /// Assert spill slot count (use together with [`Self::expect`] to see the full plan).
    pub fn expect_spill_slots(&self, count: u32) -> &Self {
        assert_eq!(
            self.output.num_spill_slots, count,
            "Expected {} spill slots, got {}.\n\nOutput:\n{}",
            count, self.output.num_spill_slots, self.rendered
        );
        self
    }

    /// At least this many spill slots (when exact count is less important than “some spill”).
    pub fn expect_spill_slots_at_least(&self, min: u32) -> &Self {
        assert!(
            self.output.num_spill_slots >= min,
            "Expected at least {} spill slots, got {}.\n\nOutput:\n{}",
            min,
            self.output.num_spill_slots,
            self.rendered
        );
        self
    }
}

fn make_test_abi() -> FuncAbi {
    abi::func_abi_rv32(
        &LpsFnSig {
            name: String::from("test"),
            return_type: LpsType::Void,
            parameters: Vec::new(),
        },
        0,
    )
}

fn make_abi_with_params(n: usize) -> FuncAbi {
    let params: Vec<FnParam> = (0..n)
        .map(|i| FnParam {
            name: alloc::format!("arg{}", i),
            ty: LpsType::Int,
            qualifier: ParamQualifier::In,
        })
        .collect();

    abi::func_abi_rv32(
        &LpsFnSig {
            name: String::from("test"),
            return_type: LpsType::Int,
            parameters: params,
        },
        n,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    // ── Layer 1: Parameterized structural tests ──────────────────
    //
    // Same program shape, swept across pool sizes. Structural invariants
    // (verify_alloc) are checked automatically by run_vinst; these tests
    // add spill-count bounds that must hold at every pool size.

    /// Binary add: 2 live values at the Add32.
    /// pool >= 2 → no spill. pool == 1 → must spill.
    #[rstest]
    fn binary_add(#[values(1, 2, 3, 4, 8, 16)] pool: usize) {
        let r = alloc_test().pool_size(pool).run_vinst(
            "i0 = IConst32 10
                 i1 = IConst32 20
                 i2 = Add32 i0, i1
                 Ret i2",
        );
        if pool >= 2 {
            r.expect_spill_slots(0);
        } else {
            r.expect_spill_slots_at_least(1);
        }
    }

    /// Chain of 4 independent values consumed pairwise.
    /// Max live = 4 (i0..i3 all live when first Add happens in backward walk).
    #[rstest]
    fn pairwise_chain(#[values(1, 2, 3, 4, 8, 16)] pool: usize) {
        let r = alloc_test().pool_size(pool).run_vinst(
            "i0 = IConst32 1
                 i1 = IConst32 2
                 i2 = IConst32 3
                 i3 = IConst32 4
                 i4 = Add32 i0, i1
                 i5 = Add32 i2, i3
                 i6 = Add32 i4, i5
                 Ret i6",
        );
        if pool >= 4 {
            r.expect_spill_slots(0);
        } else {
            r.expect_spill_slots_at_least(1);
        }
    }

    /// Value reused twice: i0 appears in both Add and Sub.
    /// Max live = 2: i1 dies at Add, so at Sub only i0 and i2 are live.
    #[rstest]
    fn value_reused_twice(#[values(1, 2, 3, 4, 16)] pool: usize) {
        let r = alloc_test().pool_size(pool).run_vinst(
            "i0 = IConst32 10
                 i1 = IConst32 20
                 i2 = Add32 i0, i1
                 i3 = Sub32 i0, i2
                 Ret i3",
        );
        if pool >= 2 {
            r.expect_spill_slots(0);
        } else {
            r.expect_spill_slots_at_least(1);
        }
    }

    /// Long chain: 6 values produced then consumed in tree reduction.
    /// Tests deeper liveness under pressure.
    #[rstest]
    fn tree_reduction(#[values(1, 2, 3, 4, 6, 8, 16)] pool: usize) {
        let r = alloc_test().pool_size(pool).run_vinst(
            "i0 = IConst32 1
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
                 Ret i10",
        );
        // Tree reduces in pairs, so max live is 6 at the start
        if pool >= 6 {
            r.expect_spill_slots(0);
        } else {
            r.expect_spill_slots_at_least(1);
        }
    }

    /// Dead value: i1 is defined but never used. Should not be allocated.
    #[rstest]
    fn dead_value_not_allocated(#[values(1, 2, 4, 16)] pool: usize) {
        let r = alloc_test().pool_size(pool).run_vinst(
            "i0 = IConst32 10
                 i1 = IConst32 99
                 Ret i0",
        );
        // i1 is dead, so only 1 value is live → never spills
        r.expect_spill_slots(0);
    }

    // ── Layer 2: Snapshot tests ──────────────────────────────────
    //
    // Pinned alloc plans for specific configurations. Catch regressions
    // in register *choice* and edit ordering, not just structural correctness.

    #[test]
    fn snapshot_iconst_ret() {
        alloc_test()
            .run_vinst("i0 = IConst32 10\nRet i0")
            .expect_spill_slots(0)
            .expect(
                "i0 = IConst32 10
; write: i0 -> t0
; ---------------------------
; read: i0 <- t0
Ret i0",
            );
    }

    #[test]
    fn snapshot_binary_add_pool2() {
        alloc_test()
            .pool_size(2)
            .run_vinst(
                "i0 = IConst32 10
                 i1 = IConst32 20
                 i2 = Add32 i0, i1
                 Ret i2",
            )
            .expect_spill_slots(0)
            .expect(
                "i0 = IConst32 10
; write: i0 -> t1
; ---------------------------
i1 = IConst32 20
; write: i1 -> t0
; ---------------------------
; read: i0 <- t1
; read: i1 <- t0
i2 = Add32 i0, i1
; write: i2 -> t0
; ---------------------------
; read: i2 <- t0
Ret i2",
            );
    }

    #[test]
    fn snapshot_binary_add_pool1() {
        alloc_test()
            .pool_size(1)
            .run_vinst(
                "i0 = IConst32 10
                 i1 = IConst32 20
                 i2 = Add32 i0, i1
                 Ret i2",
            )
            .expect_spill_slots(1)
            .expect(
                "i0 = IConst32 10
; write: i0 -> slot0
; ---------------------------
i1 = IConst32 20
; write: i1 -> t0
; ---------------------------
; move: t0 -> slot0
; read: i0 <- t0
; read: i1 <- t0
i2 = Add32 i0, i1
; write: i2 -> t0
; ---------------------------
; read: i2 <- t0
Ret i2",
            );
    }
}
