//! Entry glue: `emu_main!` names a `fn(u32) -> u32` as the guest program.
//!
//! No startup assembly is needed: the emulator invokes `_start` via a
//! synthesized windowed CALL8 (argument arriving in `a2` after ENTRY) with a
//! valid SP already in `a1`, and the ELF loader has materialized
//! `.data`/`.bss` — so `_start` is a plain windowed `extern "C"` function
//! that calls `main` and then takes the exit trap.

/// Define `_start` around a `fn(u32) -> u32` main.
#[macro_export]
macro_rules! emu_main {
    ($main:path) => {
        #[no_mangle]
        pub extern "C" fn _start(arg: u32) -> u32 {
            let code: u32 = $main(arg);
            $crate::exit(code)
        }
    };
}
