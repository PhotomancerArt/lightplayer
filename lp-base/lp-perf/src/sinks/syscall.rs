use crate::PerfEventKind;

#[cfg(target_arch = "riscv32")]
#[inline(always)]
pub fn emit(name: &'static str, kind: PerfEventKind) {
    use lp_riscv_emu_shared::SYSCALL_PERF_EVENT;
    let ptr = name.as_ptr() as i32;
    let len = name.len() as i32;
    let kind_u = kind as i32;
    unsafe {
        core::arch::asm!(
            "ecall",
            in("x17") SYSCALL_PERF_EVENT,
            in("x10") ptr,
            in("x11") len,
            in("x12") kind_u,
            // x13 reserved for future arg payload
            options(nostack, preserves_flags),
        );
    }
}

/// Host / non-RV32 targets: syscall sink is selected for RV32 firmware builds;
/// this keeps `cargo check --features syscall` valid on the host.
#[cfg(not(target_arch = "riscv32"))]
#[inline(always)]
pub fn emit(_name: &'static str, _kind: PerfEventKind) {}
