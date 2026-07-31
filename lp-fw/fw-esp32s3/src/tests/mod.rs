//! Hardware harnesses, selected by `test_*` features (see build.rs's
//! `fw_harness` cfg). Each replaces the boot path's park loop with a runner.

#[cfg(feature = "test_backtrace_oracle")]
pub mod backtrace_oracle;
#[cfg(feature = "test_xt_jit_corpus")]
pub mod xt_jit_corpus;
