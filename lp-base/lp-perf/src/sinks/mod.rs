#[cfg(all(feature = "syscall", not(feature = "log")))]
mod syscall;
#[cfg(all(feature = "syscall", not(feature = "log")))]
pub use syscall::emit;

#[cfg(all(feature = "log", not(feature = "syscall")))]
mod log_sink;
#[cfg(all(feature = "log", not(feature = "syscall")))]
pub use log_sink::emit;

#[cfg(not(any(feature = "syscall", feature = "log")))]
mod noop;
#[cfg(not(any(feature = "syscall", feature = "log")))]
pub use noop::emit;

#[cfg(all(feature = "syscall", feature = "log"))]
compile_error!("lp-perf: enable at most one of `syscall` or `log`");
