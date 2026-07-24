//! Remaining instruction fuel from the VMContext header.
//!
//! The VMContext pointer is passed as a zero/sign-extended `i32` word (same as LPIR [`VMCTX_VREG`]),
//! not as a native pointer type, so Cranelift import signatures match shader calls on every ISA.

use lpvm::VmContext;

#[unsafe(no_mangle)]
pub extern "C" fn __lp_vm_get_fuel_q32(vmctx_word: i32) -> u32 {
    // NOTE: never linked by lpvm-wasm modules — the wasm emitter inlines
    // `__lp_get_fuel` as a direct vmctx load (a pointer deref of the wasm
    // vmctx word 0 would be a rejected null dereference; see
    // `lpvm-wasm/src/emit/imports.rs::import_is_inline_get_fuel`).
    let ctx = vmctx_word as usize as *const VmContext;
    if ctx.is_null() {
        return 0;
    }
    unsafe { (*ctx).fuel as u32 }
}
