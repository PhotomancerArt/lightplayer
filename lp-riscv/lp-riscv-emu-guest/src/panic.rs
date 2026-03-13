use core::{arch::asm, fmt::Write, ptr::null};

use crate::syscall::{SYSCALL_PANIC, syscall};

/// Exit the interpreter
#[inline(always)]
pub fn ebreak() -> ! {
    unsafe { asm!("ebreak", options(nostack, noreturn)) }
}

/// Report a panic to the host VM
///
/// This should be called from the panic handler before ebreak.
/// args[0] = panic message pointer (as i32)
/// args[1] = panic message length
/// args[2] = file pointer (as i32, 0 if unavailable)
/// args[3] = file length
/// args[4] = line number (0 if unavailable)
fn panic_syscall(
    msg_ptr: *const u8,
    msg_len: usize,
    file_ptr: *const u8,
    file_len: usize,
    line: u32,
) -> ! {
    let args = [
        msg_ptr as i32,
        msg_len as i32,
        file_ptr as i32,
        file_len as i32,
        line as i32,
        0,
        0,
    ];
    let _ = syscall(SYSCALL_PANIC, &args);
    ebreak()
}

/// Format panic info and report to host via syscall. Used by both panic handlers.
fn report_panic_to_host(info: &core::panic::PanicInfo) -> ! {
    let mut panic_msg_buf = [0u8; 256];
    let mut cursor = 0;

    struct BufWriter<'a> {
        buf: &'a mut [u8],
        cursor: &'a mut usize,
    }

    impl Write for BufWriter<'_> {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            let bytes = s.as_bytes();
            let remaining = self.buf.len() - *self.cursor;
            let to_write = bytes.len().min(remaining);
            if to_write > 0 {
                self.buf[*self.cursor..*self.cursor + to_write].copy_from_slice(&bytes[..to_write]);
                *self.cursor += to_write;
            }
            Ok(())
        }
    }

    let mut writer = BufWriter {
        buf: &mut panic_msg_buf,
        cursor: &mut cursor,
    };

    let _ = write!(writer, "{}", info.message());

    if cursor == 0 {
        let default_msg = b"panic occurred (no message)";
        let to_copy = default_msg.len().min(panic_msg_buf.len());
        panic_msg_buf[..to_copy].copy_from_slice(&default_msg[..to_copy]);
        cursor = to_copy;
    }

    let (file_ptr, file_len, line) = if let Some(loc) = info.location() {
        let file = loc.file();
        let file_bytes = file.as_bytes();
        (file_bytes.as_ptr(), file_bytes.len(), loc.line())
    } else {
        (null(), 0, 0)
    };

    panic_syscall(panic_msg_buf.as_ptr(), cursor, file_ptr, file_len, line);
}

/// Panic handler: build PanicPayload, attempt unwinding, fall back to host report.
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    extern crate alloc;
    use core::fmt::Write;

    let message = {
        let mut buf = alloc::string::String::new();
        let _ = write!(buf, "{}", info.message());
        if buf.is_empty() {
            alloc::string::String::from("panic occurred (no message)")
        } else {
            buf
        }
    };
    let (file, line) = if let Some(loc) = info.location() {
        (
            Some(alloc::string::String::from(loc.file())),
            Some(loc.line()),
        )
    } else {
        (None, None)
    };

    let payload = lp_shared::backtrace::PanicPayload::new(message, file, line);
    let _code = unwinding::panic::begin_panic(alloc::boxed::Box::new(payload));

    // begin_panic returned — no catch_unwind on stack. Fall back to host report.
    report_panic_to_host(info);
}
