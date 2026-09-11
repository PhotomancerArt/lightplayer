//! The external-register file: what `rer` and `wer` read and write.
//!
//! Xtensa's External Registers option puts a second, byte-addressed register
//! space beside the special registers, reached only by `rer at, as` (read
//! `ExternalReg[AR[s]]` into `AR[t]`) and `wer at, as` (write `AR[t]` there).
//! On the ESP32 family the space is the **ERI** window onto the Xtensa Debug
//! Module — TRAX at `0x10_0000`, the performance monitors at `0x10_1000`, the
//! On-Chip-Debug block at `0x10_2000` — plus whatever else the SoC hangs off
//! it. None of that is core state, which is why this file is a *store* and
//! not a model.
//!
//! # The rule
//!
//! An address nobody has written reads **0**; a `wer` is remembered and the
//! matching `rer` reads it back. That is the whole behaviour. Two
//! consequences worth stating rather than discovering:
//!
//! - **No debugger is attached to this emulator, and 0 says so.** The one
//!   external register the firmware this hart runs actually touches is
//!   `XDM_OCD_DCR_SET` ([`XDM_OCD_DCR_SET`]): `xtensa_lx::is_debugger_attached`
//!   reads it and tests bit 0, `DCR_ENABLEOCD` (xtensa-lx-0.13.0
//!   `src/lib.rs:17-18, 98-104`), and esp-hal's `debugger_connected()`
//!   (`third_party/esp-hal/src/debugger.rs:4-18`) is that function on Xtensa.
//!   The IDF second-stage bootloader asks the same question through
//!   `esp_cpu_dbgr_is_attached()`. A read of 0 is therefore not a placeholder
//!   standing in for an answer we do not have — it **is** the answer, and it
//!   is the same answer the C6 machine already gives on the RISC-V side by
//!   forcing `ASSIST_DEBUG.cpu(0).debug_mode.debug_module_active` to 0
//!   (`lp-emu-esp32c6/src/periph/accept.rs`).
//!
//! - **The DCR set/clear pair is deliberately not modelled.** On silicon
//!   `XDM_OCD_DCR_SET` and [`XDM_OCD_DCR_CLR`] are two write ports onto one
//!   Debug Control register, so a write to the SET port ORs bits in rather
//!   than replacing the register. Modelling that would be inventing
//!   behaviour nothing exercises: across the classic ROM, the IDF bootloader
//!   and the shipped app image the M0 inventory found **three `rer` sites and
//!   no `wer` at all** (`docs/reports/2026-09-10-xtensa-firmware-isa-inventory.md`).
//!   A plain per-address store is the smaller claim, and a guest that does
//!   start driving the debug module will show up in the trace (every access
//!   emits [`crate::TraceEvent::ExtRegAccess`]) rather than silently getting a
//!   wrong answer.
//!
//! Addresses are the Xtensa Debug Module's published ERI addresses (Xtensa
//! Debug Guide; the same three constants appear in Tensilica's `xdm-regs.h`
//! and, for `DCR_SET`, in Espressif's `xtensa/config/extreg.h` as `DSRSET`
//! and in xtensa-lx as `XDM_OCD_DCR_SET`). Only the three this hart has a
//! reason to name are named; the rest of the space needs no table, because
//! the store treats every address alike.

use std::collections::BTreeMap;

/// `XDM_OCD_DCR_CLR` — the Debug Control register's clear port.
pub const XDM_OCD_DCR_CLR: u32 = 0x0010_2008;

/// `XDM_OCD_DCR_SET` — the Debug Control register's set port, and the one
/// external register the firmware reads: bit 0 (`DCR_ENABLEOCD`) is "a
/// debugger is attached". It reads 0 here, and 0 means **not attached**.
pub const XDM_OCD_DCR_SET: u32 = 0x0010_200C;

/// `XDM_OCD_DSR` — the Debug Status register. Reads 0: nothing has halted,
/// stopped or trapped into a debug module that is not there.
pub const XDM_OCD_DSR: u32 = 0x0010_2010;

/// `DCR_ENABLEOCD`, bit 0 of the Debug Control register — the bit
/// `xtensa_lx::is_debugger_attached` tests after its `rer`.
pub const DCR_ENABLEOCD: u32 = 0x1;

/// The external-register space: written words, and 0 everywhere else.
///
/// Sparse on purpose. The space is 32 bits wide and the guest touches a
/// handful of addresses in it, so the map holds exactly what has been
/// written — which also makes "what has this guest driven?" a question a
/// snapshot can answer ([`ExternalRegs::iter`]).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExternalRegs {
    written: BTreeMap<u32, u32>,
}

impl ExternalRegs {
    /// An untouched space: every address reads 0.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            written: BTreeMap::new(),
        }
    }

    /// `ExternalReg[addr]`. Unwritten reads 0 — see the module docs for why
    /// that is an answer and not a stub.
    #[must_use]
    pub fn read(&self, addr: u32) -> u32 {
        self.written.get(&addr).copied().unwrap_or(0)
    }

    /// `ExternalReg[addr] <- value`, remembered verbatim.
    pub fn write(&mut self, addr: u32, value: u32) {
        self.written.insert(addr, value);
    }

    /// Every address this guest has written, in address order.
    pub fn iter(&self) -> impl Iterator<Item = (u32, u32)> + '_ {
        self.written.iter().map(|(&a, &v)| (a, v))
    }

    /// How many distinct addresses have been written.
    #[must_use]
    pub fn len(&self) -> usize {
        self.written.len()
    }

    /// True while nothing has been written — the state every run starts in.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.written.is_empty()
    }
}
