//! Backtrace capture and panic payload for panic recovery.
//!
//! Used by platform panic handlers to build a payload that survives unwinding,
//! and by the engine to format panic errors for node status.

use alloc::format;
use alloc::string::String;

pub const MAX_FRAMES: usize = 16;

/// Panic payload that survives unwinding.
///
/// Built by platform panic handlers, caught by catch_unwind in the engine.
/// Implements Send for compatibility with unwinding::panic::begin_panic.
pub struct PanicPayload {
    pub message: String,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub frames: [u32; MAX_FRAMES],
    pub frame_count: usize,
}

impl PanicPayload {
    pub fn new(message: String, file: Option<String>, line: Option<u32>) -> Self {
        let mut payload = Self {
            message,
            file,
            line,
            frames: [0; MAX_FRAMES],
            frame_count: 0,
        };
        payload.frame_count = capture_frames(&mut payload.frames);
        payload
    }

    /// Format as error string for NodeStatus::Error.
    ///
    /// Format: "panic: <msg> (at <file>:<line>) [0x00001234, 0x00005678, ...]"
    pub fn format_error(&self) -> String {
        let mut s = format!("panic: {}", self.message);
        if let Some(ref file) = self.file {
            if let Some(line) = self.line {
                s.push_str(&format!(" (at {file}:{line})"));
            } else {
                s.push_str(&format!(" (at {file})"));
            }
        }
        if self.frame_count > 0 {
            s.push_str(" [");
            for i in 0..self.frame_count {
                if i > 0 {
                    s.push_str(", ");
                }
                s.push_str(&format!("0x{:08x}", self.frames[i]));
            }
            s.push(']');
        }
        s
    }
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
    const RAM_START: u32 = 0x8000_0000;

    if buf.is_empty() {
        return 0;
    }

    let fp: u32;
    unsafe { core::arch::asm!("mv {}, s0", out(reg) fp) };

    let mut fp = fp;
    if fp < RAM_START || fp % 4 != 0 {
        return 0;
    }

    let mut count = 0;
    while count < buf.len() {
        let ra = unsafe { (fp.wrapping_sub(4) as *const u32).read() };
        let prev_fp = unsafe { (fp.wrapping_sub(8) as *const u32).read() };

        if ra != 0 {
            buf[count] = ra;
            count += 1;
        }

        if prev_fp == 0 || prev_fp < RAM_START || prev_fp <= fp {
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
