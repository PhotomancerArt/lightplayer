//! Remaining instruction fuel from the VMContext header.
//!
//! The VMContext pointer is passed as a zero/sign-extended `i32` word (same as LPIR [`VMCTX_VREG`]),
//! not as a native pointer type, so Cranelift import signatures match shader calls on every ISA.

use lpvm::VmContext;

#[unsafe(no_mangle)]
pub extern "C" fn __lp_vm_get_fuel_q32(vmctx_word: i32) -> u32 {
    let ctx = vmctx_word as usize as *const VmContext;
    if ctx.is_null() {
        return 0;
    }
    unsafe { (*ctx).fuel as u32 }
}
