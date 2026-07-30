//! Panic handler: format the message into a fixed buffer, report it via
//! `SYS_PANIC` (the host records it and terminates the run).

use core::fmt::Write;

use crate::syscall::{syscall3, SYS_PANIC};

struct BufWriter<'a> {
    buf: &'a mut [u8],
    len: usize,
}

impl Write for BufWriter<'_> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let bytes = s.as_bytes();
        let n = bytes.len().min(self.buf.len() - self.len);
        self.buf[self.len..self.len + n].copy_from_slice(&bytes[..n]);
        self.len += n;
        Ok(())
    }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    let mut buf = [0u8; 256];
    let mut w = BufWriter {
        buf: &mut buf,
        len: 0,
    };
    if let Some(loc) = info.location() {
        let _ = write!(w, "{}:{}: ", loc.file(), loc.line());
    }
    let _ = write!(w, "{}", info.message());
    let len = w.len;
    syscall3(SYS_PANIC, buf.as_ptr() as u32, len as u32, 0);
    // The host never resumes a panic; loop as the `-> !` backstop.
    #[allow(clippy::empty_loop)]
    loop {}
}
