use core::fmt::{self, Write};

use crate::syscall::{SYSCALL_ARGS, syscall};

/// Syscall number for write
const SYSCALL_WRITE: i32 = 2;

/// Writer that sends output to the host via syscall
///
/// Syscall 2: Write string to host
/// - args[0] = pointer to string (as i32)
/// - args[1] = length of string
pub struct BuiltinsWriter;

impl Write for BuiltinsWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        // Skip empty strings (formatting artifacts)
        if s.is_empty() {
            return Ok(());
        }

        // Syscall 2: Write string to host
        // args[0] = pointer to string (as i32)
        // args[1] = length of string
        let ptr = s.as_ptr() as usize as i32;
        let len = s.len() as i32;

        let mut args = [0i32; SYSCALL_ARGS];
        args[0] = ptr;
        args[1] = len;
        let _ = syscall(SYSCALL_WRITE, &args);
        Ok(())
    }
}

/// Global writer instance
static mut WRITER: BuiltinsWriter = BuiltinsWriter;

/// Print function used by print!/println! macros
///
/// This function is called by the print! and println! macros
/// when used in a no_std environment.
#[unsafe(no_mangle)]
#[allow(static_mut_refs)] // Safe: WRITER is only accessed from this single-threaded function
pub fn _print(args: fmt::Arguments) {
    unsafe {
        // Use addr_of_mut! to safely get a pointer to the mutable static
        // This avoids creating a mutable reference directly, which is unsafe in Rust 2024
        match (*core::ptr::addr_of_mut!(WRITER)).write_fmt(args) {
            Ok(()) => {}
            Err(_) => {
                // If formatting fails, we can't do much in no_std
                // But at least we tried
            }
        }
    }
}

/// Print macro for no_std environments
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {
        $crate::_print(core::format_args!($($arg)*));
    };
}

/// Println macro for no_std environments
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
