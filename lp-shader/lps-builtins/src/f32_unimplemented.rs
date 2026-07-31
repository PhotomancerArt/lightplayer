//! Shared trap for the not-yet-implemented native-f32 lpfn builtins.
//!
//! The `_f32` builtins in `builtins/lpfn/**` are placeholders: their
//! signatures, `lpfn_impl` annotations and builtin-table wiring are correct,
//! but no body exists yet (f32 roadmap M5). They used to round-trip through
//! Q32 via `Q32::from_f32_wrapping`, which silently returned Q32-precision
//! results with wrapped range — the exact property native f32 is being added
//! for. They trap instead.
//!
//! Every call site funnels through this one function on purpose. Panicking
//! in place with a per-builtin message cost ~2.8 KB of `.rodata` (one message
//! plus one `core::panic::Location` per site), which overflowed `DRAM` in the
//! Xtensa builtins image. One shared message and one location is ~130 bytes;
//! the panic's own location still names the file that trapped.

/// Abort: a native-f32 lpfn builtin was called before M5 implemented it.
#[cold]
#[inline(never)]
pub fn f32_unimplemented() -> ! {
    panic!("lpfn f32 builtin unimplemented (f32 roadmap M5)")
}
