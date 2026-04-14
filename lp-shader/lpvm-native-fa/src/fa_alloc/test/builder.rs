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
use crate::vinst::{ModuleSymbols, VInst, VReg};
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use lps_shared::{FnParam, LpsFnSig, LpsType, ParamQualifier};

/// Builder for allocation tests.
pub struct AllocTestBuilder {
    pool_size: Option<usize>,
    abi_params: usize,
    /// Same spelling as filetests: `void`, `i32`, `f32`, `vec4`, `mat4`, …
    abi_return: String,
}

/// Start building an allocation test.
pub fn alloc_test() -> AllocTestBuilder {
    AllocTestBuilder {
        pool_size: None,
        abi_params: 0,
        abi_return: String::from("void"),
    }
}

fn lps_return_type(s: &str) -> LpsType {
    match s.trim() {
        "void" => LpsType::Void,
        "i32" | "int" => LpsType::Int,
        "f32" | "float" => LpsType::Float,
        "vec4" => LpsType::Vec4,
        "mat4" => LpsType::Mat4,
        _ => LpsType::Void,
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

    /// Return type of the enclosing function for ABI (`void`, `i32`, `vec4`, …).
    pub fn abi_return(mut self, s: &str) -> Self {
        self.abi_return = s.to_string();
        self
    }

    fn build_func_abi(&self) -> FuncAbi {
        let return_type = lps_return_type(&self.abi_return);
        if self.abi_params > 0 {
            let params: Vec<FnParam> = (0..self.abi_params)
                .map(|i| FnParam {
                    name: alloc::format!("arg{}", i),
                    ty: LpsType::Int,
                    qualifier: ParamQualifier::In,
                })
                .collect();
            let total_param_slots = 1 + self.abi_params;
            abi::func_abi_rv32(
                &LpsFnSig {
                    name: String::from("test"),
                    return_type,
                    parameters: params,
                },
                total_param_slots,
            )
        } else {
            abi::func_abi_rv32(
                &LpsFnSig {
                    name: String::from("test"),
                    return_type,
                    parameters: Vec::new(),
                },
                0,
            )
        }
    }

    fn run_vinst_inner(
        self,
        vinsts: Vec<VInst>,
        vreg_pool: Vec<VReg>,
        symbols: ModuleSymbols,
    ) -> AllocTestResult {
        let func_abi = self.build_func_abi();

        let pool = match self.pool_size {
            Some(n) => RegPool::with_capacity(n),
            None => RegPool::new(),
        };

        let output = walk_linear_with_pool(&vinsts, &vreg_pool, &func_abi, pool)
            .unwrap_or_else(|e| panic!("Allocation failed: {:?}", e));

        crate::fa_alloc::verify::verify_alloc(&vinsts, &vreg_pool, &output, &func_abi);

        let rendered = render_alloc_output(&vinsts, &vreg_pool, &output, Some(&symbols));

        AllocTestResult { output, rendered }
    }

    /// Run allocation on VInst input and return test result for assertions.
    ///
    /// Panics with a clear message if parse or allocation fails (tests are the caller).
    pub fn run_vinst(self, input: &str) -> AllocTestResult {
        let input = input.trim();

        let (vinsts, symbols, vreg_pool) =
            vinst::parse(input).unwrap_or_else(|e| panic!("Failed to parse VInst input: {:?}", e));

        self.run_vinst_inner(vinsts, vreg_pool, symbols)
    }

    /// Build a [`VInst::Call`] from parts, run allocation, return result.
    ///
    /// `callee` is the symbol name interned for the call (e.g. `__lp_q32_fadd`).
    pub fn run_call(
        self,
        callee: &str,
        arg_iregs: &[u16],
        ret_iregs: &[u16],
        callee_uses_sret: bool,
    ) -> AllocTestResult {
        let args_s = arg_iregs
            .iter()
            .map(|n| format!("i{}", n))
            .collect::<Vec<_>>()
            .join(", ");
        let line = if ret_iregs.is_empty() {
            format!("Call {} ({})", callee, args_s)
        } else if ret_iregs.len() == 1 {
            format!("i{} = Call {} ({})", ret_iregs[0], callee, args_s)
        } else {
            let rets_s = ret_iregs
                .iter()
                .map(|n| format!("i{}", n))
                .collect::<Vec<_>>()
                .join(", ");
            format!("({}) = Call {} ({})", rets_s, callee, args_s)
        };

        let (mut vinsts, symbols, vreg_pool) =
            vinst::parse(&line).unwrap_or_else(|e| panic!("run_call parse: {:?}", e));
        for inst in &mut vinsts {
            if let VInst::Call {
                callee_uses_sret: flag,
                ..
            } = inst
            {
                *flag = callee_uses_sret;
            }
        }
        self.run_vinst_inner(vinsts, vreg_pool, symbols)
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

    #[test]
    fn smoke_run_call_allocates() {
        let r = alloc_test().run_call("callee", &[0, 1], &[2], false);
        assert!(
            r.rendered.contains("Call callee"),
            "expected Call in render: {}",
            r.rendered
        );
    }

    #[test]
    fn smoke_run_call_sret_flag_in_render() {
        let r = alloc_test().run_call("big", &[0], &[1, 2, 3, 4], true);
        r.expect_spill_slots(0);
        assert!(r.rendered.contains("Call big sret"));
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

    // ── Layer 2: Call-specific parametric tests ────────────────────

    /// iconst, call, use iconst after call → value must survive across call.
    /// At small pools all pool regs are caller-saved t-regs, so spill ≥ 1.
    /// At larger pools s-regs are available, no spill may be needed.
    #[rstest]
    fn call_with_live_value(#[values(1, 2, 4, 8, 16)] pool: usize) {
        let r = alloc_test().pool_size(pool).run_vinst(
            "i0 = IConst32 42
             i1 = Call callee (i0)
             i2 = Add32 i0, i1
             Ret i2",
        );
        if pool <= 6 {
            // All pool regs are t-regs (caller-saved), i0 needs save/restore
            r.expect_spill_slots_at_least(1);
        }
    }

    /// Ret of call A → arg of call B.
    /// Structural correctness: verifier checks ARG/RET constraints.
    #[rstest]
    fn call_chain(#[values(1, 2, 4, 8)] pool: usize) {
        let r = alloc_test().pool_size(pool).run_vinst(
            "i0 = IConst32 1
             i1 = Call foo (i0)
             i2 = Call bar (i1)
             Ret i2",
        );
        // Verifier checks pass (via run_vinst_inner). Just confirm it completes.
        let _ = r;
    }

    /// Call with 4 arguments — all should be placed in a0–a3.
    #[rstest]
    fn multi_arg_call(#[values(1, 2, 4)] pool: usize) {
        let r = alloc_test().pool_size(pool).run_vinst(
            "i0 = IConst32 1
             i1 = IConst32 2
             i2 = IConst32 3
             i3 = IConst32 4
             i4 = Call helper (i0, i1, i2, i3)
             Ret i4",
        );
        // Verifier checks pass (via run_vinst_inner).
        let _ = r;
    }
}
