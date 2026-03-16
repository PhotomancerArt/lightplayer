//! Code generation (Module → Executable)

#[cfg(all(feature = "std", feature = "emulator"))]
pub mod builtins_linker;
#[cfg(feature = "emulator")]
pub mod emu;
pub mod jit;
