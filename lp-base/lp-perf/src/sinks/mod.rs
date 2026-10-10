#[cfg(all(feature = "syscall", not(feature = "log")))]
mod syscall;
#[cfg(all(feature = "syscall", not(feature = "log")))]
pub use syscall::{emit, emit_jit_map_load};

#[cfg(all(feature = "log", not(feature = "syscall")))]
mod log_sink;
#[cfg(all(feature = "log", not(feature = "syscall")))]
pub use log_sink::{emit, emit_jit_map_load};

#[cfg(all(feature = "hook", not(any(feature = "syscall", feature = "log"))))]
mod hook;
#[cfg(all(feature = "hook", not(any(feature = "syscall", feature = "log"))))]
pub use hook::{emit, emit_jit_map_load, set_hook};

#[cfg(not(any(feature = "syscall", feature = "log", feature = "hook")))]
mod noop;
#[cfg(not(any(feature = "syscall", feature = "log", feature = "hook")))]
pub use noop::{emit, emit_jit_map_load};

#[cfg(all(feature = "syscall", feature = "log"))]
compile_error!("lp-perf: enable at most one of `syscall` or `log`");
