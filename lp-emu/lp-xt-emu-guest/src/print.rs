//! `print!` / `println!` over the `SYS_WRITE` trap.

use core::fmt::{self, Write};

use crate::syscall::sys_write;

struct HostWriter;

impl Write for HostWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        if !s.is_empty() {
            sys_write(s.as_bytes());
        }
        Ok(())
    }
}

/// Backing function for the `print!`/`println!` macros.
pub fn _print(args: fmt::Arguments) {
    // Formatting into the host stream cannot fail (the host accepts any bytes).
    let _ = HostWriter.write_fmt(args);
}

/// Print to the host-collected output stream.
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {
        $crate::_print(core::format_args!($($arg)*));
    };
}

/// Print a line to the host-collected output stream.
#[macro_export]
macro_rules! println {
    () => {
        $crate::print!("\n");
    };
    ($($arg:tt)*) => {
        $crate::print!($($arg)*);
        $crate::print!("\n");
    };
}
