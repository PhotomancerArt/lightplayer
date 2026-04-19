//! Options threaded through LPIR → [`crate::vinst::VInst`] lowering.

use lpir::FloatMode;

/// Per-call lowering options. Threaded through [`crate::lower::lower_lpir_op`] and its
/// callees so that fast-math dispatch can read the active [`lps_q32::q32_options::Q32Options`].
#[derive(Clone, Copy)]
pub struct LowerOpts<'a> {
    pub float_mode: FloatMode,
    pub q32: &'a lps_q32::q32_options::Q32Options,
}
