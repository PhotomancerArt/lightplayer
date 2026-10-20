//! What the RAM and perf walks read off an emulated board (plan
//! `lp2025/2026-10-08-2339-tree-store-firmware-and-emulator`, P8): the
//! `[heap] peak=… live=… max_usage=… largest=…` line `heap_peak_diag` prints
//! after every heartbeat, the `[perf] fs mount <us> us` boot line, and the
//! flash commands an operation sent (the in-range op census and the SPI1
//! read count). Evidence, never a gate: nothing here asserts a threshold.

#![allow(dead_code, reason = "each test file uses part of this")]

use lp_cli::commands::emu::link_host::{C6Board, EmuLinkHost};

/// One `[heap]` line.
#[derive(Clone, Copy, Debug, Default)]
pub struct HeapLine {
    /// The most requested bytes live since the line before.
    pub peak: u64,
    /// Requested bytes live now.
    pub live: u64,
    /// esp-alloc's lifetime maximum (with its overhead).
    pub max_usage: u64,
    /// The largest free block at the line.
    pub largest: u64,
}

impl HeapLine {
    pub fn parse(line: &str) -> Option<Self> {
        let at = line.find("[heap] peak=")?;
        let field = |name: &str| -> Option<u64> {
            let rest = &line[at..];
            let i = rest.find(&format!("{name}="))? + name.len() + 1;
            let digits: String = rest[i..].chars().take_while(char::is_ascii_digit).collect();
            digits.parse().ok()
        };
        Some(Self {
            peak: field("peak")?,
            live: field("live")?,
            max_usage: field("max_usage")?,
            largest: field("largest")?,
        })
    }
}

/// The `[heap]` lines on the console from `from` on.
pub fn heap_lines(host: &EmuLinkHost<C6Board>, from: usize) -> Vec<HeapLine> {
    host.console()[from.min(host.console().len())..]
        .iter()
        .filter_map(|l| HeapLine::parse(l))
        .collect()
}

/// Step to the next `[heap]` line after the console's current end (a
/// heartbeat is every 5 emulated seconds), and return it with the console
/// index past it.
pub fn next_heap_line(host: &mut EmuLinkHost<C6Board>) -> (HeapLine, usize) {
    let from = host.console().len();
    let until = host.board.machine.micros() + 12_000_000;
    while host.board.machine.micros() < until {
        host.step().expect("the board runs");
        if let Some((i, line)) = host.console()[from..]
            .iter()
            .enumerate()
            .find_map(|(i, l)| HeapLine::parse(l).map(|h| (i, h)))
        {
            return (line, from + i + 1);
        }
    }
    panic!("no [heap] line in 12 emulated seconds (built without heap_peak_diag?)");
}

/// The `[perf] fs mount <us> us` line's microseconds.
pub fn mount_us(host: &EmuLinkHost<C6Board>) -> Option<u64> {
    host.console().iter().find_map(|l| {
        let at = l.find("[perf] fs mount ")? + "[perf] fs mount ".len();
        l[at..].split(' ').next()?.parse().ok()
    })
}

/// Flash commands so far: in-range programs, program bytes, erases (the
/// census `watch` started), and SPI1 reads of the whole chip.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FlashCount {
    pub programs: u64,
    pub program_bytes: u64,
    pub erases: u64,
    pub reads: u64,
}

impl FlashCount {
    pub fn since(self, before: FlashCount) -> FlashCount {
        FlashCount {
            programs: self.programs - before.programs,
            program_bytes: self.program_bytes - before.program_bytes,
            erases: self.erases - before.erases,
            reads: self.reads - before.reads,
        }
    }
}

/// Start counting `lpfs` commands (call once, before the first
/// [`flash_count`]): the chip's own `lpfs` row — the tree table's, or a
/// fielded littlefs board's (`0x350000`).
pub fn watch(host: &EmuLinkHost<C6Board>) {
    let flash = host.board.machine.flash();
    let mut flash = flash.lock().unwrap();
    let range = lp_emu_esp32c6::flash_cut_spec::lpfs_range(flash.bytes());
    flash.watch_flash_ops(range, false);
}

pub fn flash_count(host: &EmuLinkHost<C6Board>) -> FlashCount {
    let flash = host.board.machine.flash().lock().unwrap();
    let census = flash.flash_op_census().cloned();
    let reads = flash.command_census().reads;
    drop(flash);
    let census = census.expect("watch() first");
    FlashCount {
        programs: census.programs,
        program_bytes: census.program_bytes,
        erases: census.sector_erases + census.block_erases,
        reads,
    }
}

/// A silicon ESTIMATE for `n` flash commands (never measured here; the
/// emulator's commands take no time): an erase ≈ 21 ms (M4's desk figure and
/// the datasheet's typical sector erase), a program ≈ 0.5 ms per started
/// 256-byte page of what it carried (datasheet typical page program).
pub fn silicon_estimate_ms(n: FlashCount) -> f64 {
    n.erases as f64 * 21.0 + (n.program_bytes as f64 / 256.0).ceil() * 0.5
}
