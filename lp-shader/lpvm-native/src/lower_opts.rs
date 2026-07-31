//! Options threaded through LPIR → [`crate::vinst::VInst`] lowering.

use lpir::FloatMode;

/// Per-call lowering options. Threaded through [`crate::lower::lower_lpir_op`]
/// and its callees.
#[derive(Clone, Copy)]
pub struct LowerOpts {
    pub float_mode: FloatMode,
    /// Insert fuel checks ([`crate::vinst::VInst::FuelCheck`]) at function
    /// entry and loop back-edges (see
    /// [`crate::native_options::NativeCompileOptions::fuel`]).
    pub fuel: bool,
}
