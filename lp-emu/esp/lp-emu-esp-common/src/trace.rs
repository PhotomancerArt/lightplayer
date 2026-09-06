//! The bus log — every MMIO access, with the PC that made it.
//!
//! This is the instrument the vendor emulator does not have, and the reason
//! bring-up on it is guesswork. When a blob spins forever, the question is
//! always "which status bit is it waiting on?", and a log of
//!
//! ```text
//! cyc=41288 pc=0x42009a1c R4 TIMG0+0x068 rtccalicfg = 0x00000000
//! ```
//!
//! repeated ten thousand times answers it in one line. So the trace also
//! carries a **spin detector**: the same `(pc, address)` read `N` times in a
//! row with no intervening write emits a single `SPIN` line naming the
//! register, and the bring-up is over before the log is.
//!
//! # Line format
//!
//! ```text
//! cyc=<n> pc=0x<8 hex> <R|W><bytes> <BLOCK>+0x<off> <name> = 0x<8 hex> [flags]
//! ```
//!
//! `<name>` is omitted when the block has no register-name table covering
//! the offset; `[flags]` is omitted when there are none. Unmapped accesses
//! use the pseudo-block `UNMAPPED` and print the whole address as the
//! offset.

use alloc::string::String;
use alloc::vec::Vec;
use std::io::Write;
use std::sync::{Arc, Mutex};

use lp_emu_core::sched::Cycles;

use crate::periph::Width;

/// Read or write, for the trace line's `R`/`W`.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Access {
    Read,
    Write,
}

impl Access {
    const fn letter(self) -> char {
        match self {
            Access::Read => 'R',
            Access::Write => 'W',
        }
    }
}

/// One MMIO access, as the bus hands it to the trace.
pub struct MmioEvent<'a> {
    pub access: Access,
    pub width: Width,
    /// The peripheral instance's name (`UART0`), not its register-block type.
    pub block: &'static str,
    /// Offset within the block.
    pub off: u32,
    /// The register's name, when a table covers this offset.
    pub name: Option<&'static str>,
    pub value: u32,
    /// Extra notes: `"dropped"`, `"strict"`, `""` for none.
    pub flags: &'a str,
}

/// The default spin threshold: consecutive identical reads before a `SPIN`
/// line. Ten thousand is well past any legitimate poll loop the C6 boot path
/// runs (the longest measured one is the RTC calibration wait) and is
/// reached in milliseconds of guest time when a stub is wrong.
pub const DEFAULT_SPIN_THRESHOLD: u32 = 10_000;

/// A byte sink shared with the caller, so a test (or the CLI's in-memory
/// mode) can read back what was written.
#[derive(Clone, Debug, Default)]
pub struct SharedBuffer(Arc<Mutex<Vec<u8>>>);

impl SharedBuffer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Everything written so far, as a `String`. Trace output is ASCII.
    pub fn contents(&self) -> String {
        let guard = self.0.lock().expect("trace buffer poisoned");
        String::from_utf8_lossy(&guard).into_owned()
    }

    pub fn lines(&self) -> Vec<String> {
        self.contents().lines().map(String::from).collect()
    }
}

impl Write for SharedBuffer {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .expect("trace buffer poisoned")
            .extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Consecutive-read tracking for the spin detector.
#[derive(Debug, Default)]
struct Spin {
    key: Option<(u32, u32)>,
    count: u32,
    fired: bool,
}

/// The bus log.
///
/// Disabled by default and cheap to ask: [`is_enabled`](Trace::is_enabled)
/// is one `Option` test, and the bus checks it before building any of the
/// line's parts.
#[derive(Default)]
pub struct Trace {
    sink: Option<Box<dyn Write + Send>>,
    /// When non-empty, only these block names are logged. `SPIN` and
    /// `UNMAPPED` lines ignore the filter: they are the ones you did not
    /// know to ask for.
    blocks: Vec<String>,
    spin_threshold: u32,
    spin: Spin,
    lines: u64,
    spins: u64,
}

impl core::fmt::Debug for Trace {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Trace")
            .field("enabled", &self.sink.is_some())
            .field("blocks", &self.blocks)
            .field("spin_threshold", &self.spin_threshold)
            .field("lines", &self.lines)
            .field("spins", &self.spins)
            .finish()
    }
}

impl Trace {
    /// A trace that writes nowhere. The default for a machine run without
    /// `--trace`.
    pub fn disabled() -> Self {
        Self::default()
    }

    /// A trace writing to `sink`, with the default spin threshold.
    pub fn to_sink(sink: Box<dyn Write + Send>) -> Self {
        Self {
            sink: Some(sink),
            blocks: Vec::new(),
            spin_threshold: DEFAULT_SPIN_THRESHOLD,
            spin: Spin::default(),
            lines: 0,
            spins: 0,
        }
    }

    /// Log only these block names. An empty list means all blocks.
    pub fn with_block_filter<I, S>(mut self, blocks: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.blocks = blocks.into_iter().map(Into::into).collect();
        self
    }

    /// Consecutive identical reads before a `SPIN` line. `0` disables the
    /// detector.
    pub fn with_spin_threshold(mut self, n: u32) -> Self {
        self.spin_threshold = n;
        self
    }

    #[inline]
    pub fn is_enabled(&self) -> bool {
        self.sink.is_some()
    }

    /// Lines written so far.
    pub fn lines_written(&self) -> u64 {
        self.lines
    }

    /// `SPIN` lines emitted so far.
    pub fn spins_reported(&self) -> u64 {
        self.spins
    }

    fn block_passes(&self, block: &str) -> bool {
        self.blocks.is_empty() || self.blocks.iter().any(|b| b == block)
    }

    /// Record one MMIO access.
    pub fn mmio(&mut self, now: Cycles, pc: u32, ev: &MmioEvent<'_>) {
        if self.sink.is_none() {
            return;
        }
        if self.block_passes(ev.block) {
            let line = format_line(
                now, pc, ev.access, ev.width, ev.block, ev.off, ev.name, ev.value, ev.flags,
            );
            self.emit(&line);
        }
        self.note_for_spin(now, pc, ev);
    }

    /// Record an access to an address no region and no peripheral claims.
    ///
    /// Always logged, filter or not: an unmapped access is the log line you
    /// did not know to ask for.
    pub fn unmapped(
        &mut self,
        now: Cycles,
        pc: u32,
        access: Access,
        width: Width,
        address: u32,
        value: u32,
    ) {
        if self.sink.is_none() {
            return;
        }
        let flags = match access {
            Access::Read => "unmapped",
            Access::Write => "unmapped,dropped",
        };
        let line = format_line(
            now, pc, access, width, "UNMAPPED", address, None, value, flags,
        );
        self.emit(&line);
    }

    /// Reset the spin detector. The bus calls this on any write, because a
    /// write is exactly the thing that could have unstuck the loop.
    pub fn note_write_anywhere(&mut self) {
        self.spin = Spin::default();
    }

    fn note_for_spin(&mut self, now: Cycles, pc: u32, ev: &MmioEvent<'_>) {
        if self.spin_threshold == 0 {
            return;
        }
        if ev.access == Access::Write {
            self.spin = Spin::default();
            return;
        }
        let key = (pc, ev.off);
        if self.spin.key == Some(key) {
            self.spin.count = self.spin.count.saturating_add(1);
        } else {
            self.spin = Spin {
                key: Some(key),
                count: 1,
                fired: false,
            };
        }
        if !self.spin.fired && self.spin.count >= self.spin_threshold {
            self.spin.fired = true;
            self.spins += 1;
            let name = ev.name.map(|n| alloc::format!(" {n}")).unwrap_or_default();
            let line = alloc::format!(
                "cyc={now} pc=0x{pc:08x} SPIN {}+0x{:03x}{name} = 0x{:08x} x{}",
                ev.block,
                ev.off,
                ev.value,
                self.spin.count
            );
            self.emit(&line);
        }
    }

    fn emit(&mut self, line: &str) {
        if let Some(sink) = self.sink.as_mut() {
            if let Err(e) = writeln!(sink, "{line}") {
                log::warn!("bus trace sink write failed, disabling the trace: {e}");
                self.sink = None;
                return;
            }
            self.lines += 1;
        }
    }
}

#[allow(clippy::too_many_arguments, reason = "one line's fields, spelled out")]
fn format_line(
    now: Cycles,
    pc: u32,
    access: Access,
    width: Width,
    block: &str,
    off: u32,
    name: Option<&str>,
    value: u32,
    flags: &str,
) -> String {
    let name = name.map(|n| alloc::format!(" {n}")).unwrap_or_default();
    let flags = if flags.is_empty() {
        String::new()
    } else {
        alloc::format!(" [{flags}]")
    };
    alloc::format!(
        "cyc={now} pc=0x{pc:08x} {}{} {block}+0x{off:03x}{name} = 0x{value:08x}{flags}",
        access.letter(),
        width.bytes()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trace_to(buf: &SharedBuffer) -> Trace {
        Trace::to_sink(Box::new(buf.clone()))
    }

    fn ev<'a>(access: Access, off: u32, name: Option<&'static str>, value: u32) -> MmioEvent<'a> {
        MmioEvent {
            access,
            width: Width::Word,
            block: "UART0",
            off,
            name,
            value,
            flags: "",
        }
    }

    #[test]
    fn a_disabled_trace_writes_nothing_and_counts_nothing() {
        let mut t = Trace::disabled();
        assert!(!t.is_enabled());
        t.mmio(1, 2, &ev(Access::Read, 0, Some("fifo"), 3));
        t.unmapped(1, 2, Access::Read, Width::Word, 0x5000_0000, 0);
        assert_eq!(t.lines_written(), 0);
    }

    #[test]
    fn the_line_format_is_the_documented_one() {
        let buf = SharedBuffer::new();
        let mut t = trace_to(&buf);
        t.mmio(
            41288,
            0x4200_9a1c,
            &MmioEvent {
                access: Access::Read,
                width: Width::Word,
                block: "TIMG0",
                off: 0x68,
                name: Some("rtccalicfg"),
                value: 0,
                flags: "",
            },
        );
        t.mmio(
            41290,
            0x4000_0058,
            &MmioEvent {
                access: Access::Write,
                width: Width::Byte,
                block: "UART0",
                off: 0x000,
                name: Some("fifo"),
                value: 0x48,
                flags: "",
            },
        );
        // No name table for this block, and a flag.
        t.mmio(
            41291,
            0x4000_0058,
            &MmioEvent {
                access: Access::Write,
                width: Width::Half,
                block: "MODEM_LPCON",
                off: 0x24,
                name: None,
                value: 0xbeef,
                flags: "read-only",
            },
        );
        assert_eq!(
            buf.lines(),
            [
                "cyc=41288 pc=0x42009a1c R4 TIMG0+0x068 rtccalicfg = 0x00000000",
                "cyc=41290 pc=0x40000058 W1 UART0+0x000 fifo = 0x00000048",
                "cyc=41291 pc=0x40000058 W2 MODEM_LPCON+0x024 = 0x0000beef [read-only]",
            ]
        );
        assert_eq!(t.lines_written(), 3);
    }

    #[test]
    fn unmapped_lines_print_the_whole_address_and_say_dropped_on_writes() {
        let buf = SharedBuffer::new();
        let mut t = trace_to(&buf);
        t.unmapped(7, 0x4200_0000, Access::Read, Width::Word, 0x5000_0000, 0);
        t.unmapped(
            8,
            0x4200_0004,
            Access::Write,
            Width::Byte,
            0x5000_0001,
            0xab,
        );
        assert_eq!(
            buf.lines(),
            [
                "cyc=7 pc=0x42000000 R4 UNMAPPED+0x50000000 = 0x00000000 [unmapped]",
                "cyc=8 pc=0x42000004 W1 UNMAPPED+0x50000001 = 0x000000ab [unmapped,dropped]",
            ]
        );
    }

    #[test]
    fn the_block_filter_keeps_only_the_named_blocks() {
        let buf = SharedBuffer::new();
        let mut t = trace_to(&buf).with_block_filter(["TIMG0"]);
        t.mmio(1, 0, &ev(Access::Read, 0, Some("fifo"), 0));
        t.mmio(
            2,
            0,
            &MmioEvent {
                block: "TIMG0",
                ..ev(Access::Read, 0x68, Some("rtccalicfg"), 0)
            },
        );
        assert_eq!(
            buf.lines(),
            ["cyc=2 pc=0x00000000 R4 TIMG0+0x068 rtccalicfg = 0x00000000"]
        );
    }

    #[test]
    fn the_spin_detector_fires_once_at_the_threshold() {
        let buf = SharedBuffer::new();
        let mut t = trace_to(&buf)
            .with_block_filter(["NOTHING"]) // silence the ordinary lines
            .with_spin_threshold(10);
        for cyc in 0..25 {
            t.mmio(cyc, 0x4200_9a1c, &ev(Access::Read, 0x1c, Some("status"), 0));
        }
        assert_eq!(
            buf.lines(),
            ["cyc=9 pc=0x42009a1c SPIN UART0+0x01c status = 0x00000000 x10"]
        );
        assert_eq!(t.spins_reported(), 1);
    }

    #[test]
    fn a_write_anywhere_resets_the_spin_run() {
        let buf = SharedBuffer::new();
        let mut t = trace_to(&buf)
            .with_block_filter(["NOTHING"])
            .with_spin_threshold(10);
        for cyc in 0..9 {
            t.mmio(cyc, 0x1000, &ev(Access::Read, 0x1c, Some("status"), 0));
        }
        t.note_write_anywhere();
        for cyc in 9..17 {
            t.mmio(cyc, 0x1000, &ev(Access::Read, 0x1c, Some("status"), 0));
        }
        assert_eq!(t.spins_reported(), 0);
        assert!(buf.lines().is_empty());
    }

    #[test]
    fn a_different_pc_or_register_restarts_the_run() {
        let buf = SharedBuffer::new();
        let mut t = trace_to(&buf)
            .with_block_filter(["NOTHING"])
            .with_spin_threshold(5);
        for _ in 0..4 {
            t.mmio(0, 0x1000, &ev(Access::Read, 0x1c, Some("status"), 0));
        }
        // Same PC, different register.
        t.mmio(0, 0x1000, &ev(Access::Read, 0x20, Some("conf0"), 0));
        for _ in 0..4 {
            t.mmio(0, 0x1000, &ev(Access::Read, 0x1c, Some("status"), 0));
        }
        assert_eq!(t.spins_reported(), 0);
    }

    #[test]
    fn a_zero_threshold_disables_the_detector() {
        let buf = SharedBuffer::new();
        let mut t = trace_to(&buf)
            .with_block_filter(["NOTHING"])
            .with_spin_threshold(0);
        for _ in 0..100_000 {
            t.mmio(0, 0x1000, &ev(Access::Read, 0x1c, Some("status"), 0));
        }
        assert_eq!(t.spins_reported(), 0);
    }
}
