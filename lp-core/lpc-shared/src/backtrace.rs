//! Backtrace capture and panic payload for panic recovery.
//!
//! Used by platform panic handlers to build a payload that survives unwinding,
//! and by the engine to format panic errors for node status.

use alloc::string::String;
use core::fmt::{self, Write as _};
use core::sync::atomic::{AtomicUsize, Ordering};

pub const MAX_FRAMES: usize = 16;
const MAX_MESSAGE_BYTES: usize = 160;
const MAX_FILE_BYTES: usize = 96;
static OOM_CONTEXT_PTR: AtomicUsize = AtomicUsize::new(0);
static OOM_CONTEXT_LEN: AtomicUsize = AtomicUsize::new(0);

/// Panic payload that survives unwinding.
///
/// Built by platform panic handlers, caught by catch_unwind in the engine.
/// Implements Send for compatibility with unwinding::panic::begin_panic.
pub struct PanicPayload {
    pub message: FixedStr<MAX_MESSAGE_BYTES>,
    pub file: Option<FixedStr<MAX_FILE_BYTES>>,
    pub line: Option<u32>,
    pub oom: Option<OomInfo>,
    pub frames: [u32; MAX_FRAMES],
    pub frame_count: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OomInfo {
    pub requested: usize,
    pub align: usize,
    pub free: usize,
    pub used: usize,
    pub context: Option<&'static str>,
}

impl PanicPayload {
    pub fn new(message: impl fmt::Display, file: Option<&str>, line: Option<u32>) -> Self {
        Self::new_inner(message, file, line, None)
    }

    pub fn new_oom(
        message: impl fmt::Display,
        file: Option<&str>,
        line: Option<u32>,
        oom: OomInfo,
    ) -> Self {
        Self::new_inner(message, file, line, Some(oom))
    }

    fn new_inner(
        message: impl fmt::Display,
        file: Option<&str>,
        line: Option<u32>,
        oom: Option<OomInfo>,
    ) -> Self {
        let mut payload = Self {
            message: FixedStr::from_display(message),
            file: file.map(FixedStr::from_str),
            line,
            oom,
            frames: [0; MAX_FRAMES],
            frame_count: 0,
        };
        payload.frame_count = capture_frames(&mut payload.frames);
        payload
    }

    /// Format as error string for NodeStatus::Error.
    ///
    /// Format: "panic: <msg> (at <file>:<line>) [0x00001234, 0x00005678, ...]; decode: just decode-backtrace 0x00001234 ..."
    pub fn format_error(&self) -> String {
        let mut s = String::new();
        if let Some(oom) = self.oom {
            push_fmt(
                &mut s,
                format_args!(
                    "oom: requested={} align={} free={} used={}; ",
                    oom.requested, oom.align, oom.free, oom.used
                ),
            );
            if let Some(context) = oom.context {
                push_fmt(&mut s, format_args!("context={context}; "));
            }
        }
        push_fmt(&mut s, format_args!("panic: {}", self.message.as_str()));
        if let Some(ref file) = self.file {
            if let Some(line) = self.line {
                push_fmt(&mut s, format_args!(" (at {}:{line})", file.as_str()));
            } else {
                push_fmt(&mut s, format_args!(" (at {})", file.as_str()));
            }
        }
        if self.frame_count > 0 {
            s.push_str(" [");
            for i in 0..self.frame_count {
                if i > 0 {
                    s.push_str(", ");
                }
                push_fmt(&mut s, format_args!("0x{:08x}", self.frames[i]));
            }
            s.push(']');
            s.push_str("; decode: just decode-backtrace");
            for i in 0..self.frame_count {
                push_fmt(&mut s, format_args!(" 0x{:08x}", self.frames[i]));
            }
        }
        s
    }
}

pub struct FixedStr<const N: usize> {
    bytes: [u8; N],
    len: usize,
}

impl<const N: usize> FixedStr<N> {
    pub fn from_str(value: &str) -> Self {
        let mut out = Self {
            bytes: [0; N],
            len: 0,
        };
        out.push_str(value);
        out
    }

    pub fn from_display(value: impl fmt::Display) -> Self {
        let mut out = Self {
            bytes: [0; N],
            len: 0,
        };
        let _ = write!(out, "{value}");
        out
    }

    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.bytes[..self.len]).unwrap_or("<invalid utf8>")
    }

    fn push_str(&mut self, value: &str) {
        if self.len >= N {
            return;
        }
        let remaining = N - self.len;
        let mut end = value.len().min(remaining);
        while !value.is_char_boundary(end) {
            end -= 1;
        }
        self.bytes[self.len..self.len + end].copy_from_slice(&value.as_bytes()[..end]);
        self.len += end;
    }
}

impl<const N: usize> fmt::Write for FixedStr<N> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.push_str(s);
        Ok(())
    }
}

fn push_fmt(out: &mut String, args: fmt::Arguments<'_>) {
    let _ = out.write_fmt(args);
}

pub fn set_oom_context(context: &'static str) {
    OOM_CONTEXT_PTR.store(context.as_ptr() as usize, Ordering::Relaxed);
    OOM_CONTEXT_LEN.store(context.len(), Ordering::Relaxed);
}

pub fn clear_oom_context() {
    OOM_CONTEXT_LEN.store(0, Ordering::Relaxed);
    OOM_CONTEXT_PTR.store(0, Ordering::Relaxed);
}

pub fn oom_context() -> Option<&'static str> {
    let ptr = OOM_CONTEXT_PTR.load(Ordering::Relaxed);
    let len = OOM_CONTEXT_LEN.load(Ordering::Relaxed);
    if ptr == 0 || len == 0 {
        return None;
    }

    let bytes = unsafe { core::slice::from_raw_parts(ptr as *const u8, len) };
    core::str::from_utf8(bytes).ok()
}

/// Capture stack frame return addresses into `buf`.
///
/// Returns the number of frames written. Platform-specific: uses frame pointer
/// walking on supported architectures, returns 0 on unsupported platforms.
pub fn capture_frames(buf: &mut [u32]) -> usize {
    capture_frames_arch(buf)
}

#[cfg(target_arch = "riscv32")]
fn capture_frames_arch(buf: &mut [u32]) -> usize {
    const ESP32C6_DRAM_START: u32 = 0x4080_0000;
    const ESP32C6_DRAM_END: u32 = 0x4088_0000;

    fn is_valid_esp32c6_dram(address: u32) -> bool {
        (ESP32C6_DRAM_START..ESP32C6_DRAM_END).contains(&address)
    }

    if buf.is_empty() {
        return 0;
    }

    let fp: u32;
    unsafe { core::arch::asm!("mv {}, s0", out(reg) fp) };

    let mut fp = fp;
    if !is_valid_esp32c6_dram(fp) || fp % 4 != 0 {
        return 0;
    }

    let mut count = 0;
    while count < buf.len() {
        let ra = unsafe { (fp.wrapping_sub(4) as *const u32).read() };
        let prev_fp = unsafe { (fp.wrapping_sub(8) as *const u32).read() };

        if ra != 0 {
            // Saved RISC-V return addresses point after the call instruction.
            // Report the callsite PC so addr2line lands on the useful frame.
            buf[count] = ra.saturating_sub(4);
            count += 1;
        }

        if prev_fp == 0 || !is_valid_esp32c6_dram(prev_fp) || prev_fp <= fp {
            break;
        }
        fp = prev_fp;
    }
    count
}

#[cfg(target_arch = "wasm32")]
fn capture_frames_arch(_buf: &mut [u32]) -> usize {
    0
}

#[cfg(not(any(target_arch = "riscv32", target_arch = "wasm32")))]
fn capture_frames_arch(_buf: &mut [u32]) -> usize {
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panic_payload_formats_oom_context() {
        let payload = PanicPayload::new_oom(
            "memory allocation of 81920 bytes failed",
            Some("fw.rs"),
            Some(104),
            OomInfo {
                requested: 81920,
                align: 4,
                free: 108408,
                used: 211592,
                context: Some("load project"),
            },
        );

        let error = payload.format_error();

        assert!(error.contains("oom: requested=81920 align=4 free=108408 used=211592"));
        assert!(error.contains("context=load project"));
        assert!(error.contains("panic: memory allocation of 81920 bytes failed"));
        assert!(error.contains("fw.rs:104"));
    }

    #[test]
    fn panic_payload_formats_decode_command_for_frames() {
        let mut payload = PanicPayload::new("boom", Some("fw.rs"), Some(104));
        payload.frames[0] = 0x4208c8fa;
        payload.frames[1] = 0x42097332;
        payload.frame_count = 2;

        let error = payload.format_error();

        assert!(error.contains("[0x4208c8fa, 0x42097332]"));
        assert!(error.contains("decode: just decode-backtrace 0x4208c8fa 0x42097332"));
    }

    #[test]
    fn fixed_str_truncates_at_utf8_boundary() {
        let text = FixedStr::<5>::from_str("abcdé");

        assert_eq!(text.as_str(), "abcd");
    }
}
