//! The emulator: memory + CPU + the run loop and the windowed-ABI run harness.

use std::collections::VecDeque;

use lp_emu_core::{CycleModel, LogLevel};

use crate::board::BoardProfile;
use crate::cpu::Cpu;
use crate::error::{Trap, TrapKind};
use crate::memory::Memory;
use crate::trace::{TraceEvent, Tracer};

/// Where payload code is placed in the **ESP32-S3** profile's SRAM1 (D-bus
/// address). The runner picks a heap address; we pick a fixed one inside the
/// dual-mapped window. Code executes at the I-bus alias of this address.
///
/// S3-specific alias kept for existing consumers; profile-aware code reads
/// [`BoardProfile`] (`emu.profile`) instead.
pub const CODE_DBUS_BASE: u32 = 0x3FC8_8000;
/// Size of the S3 profile's code region.
pub const CODE_REGION_LEN: usize = 0x0002_0000; // 128 KiB
/// S3 profile's stack region (D-bus). Separate SRAM1-mapped region; stack
/// grows down from the top. Save areas produced by window spills live here.
pub const STACK_DBUS_BASE: u32 = 0x3FCC_0000;
pub const STACK_REGION_LEN: usize = 0x0002_0000; // 128 KiB
/// S3 profile's initial stack pointer (top of the stack region, 16-aligned).
pub const INITIAL_SP: u32 = STACK_DBUS_BASE + STACK_REGION_LEN as u32 - 16;

/// Sentinel return address: when the top-level windowed function returns here,
/// the run stops. Chosen unmapped (on both board profiles) and in the code
/// region's high bits — every profile executes code in the `0x4xxx_xxxx`
/// quadrant — so the RETW address-unmangle reproduces it exactly (see
/// `finish_call`).
pub const SENTINEL_PC: u32 = 0x4000_0000;

/// Default instruction budget before a run is declared a [`TrapKind::Timeout`]
/// (models the device watchdog catching an infinite loop). Far above any
/// payload the corpus runs; the hang case is the only one that reaches it.
pub const DEFAULT_STEP_BUDGET: u64 = 2_000_000;

/// Control-flow outcome of executing one instruction.
pub(crate) enum Flow {
    /// Advance to `pc + len`.
    Next,
    /// Jump to an absolute address (branch taken, call, return, jump).
    Jump(u32),
    /// A `SYSCALL` was executed: hand control to the run loop's
    /// [`SyscallHandler`] before advancing.
    Syscall,
}

/// What [`Emulator::step`] observed (beyond a trap).
enum Step {
    /// A normal instruction retired; `pc` already advanced.
    Normal,
    /// A `SYSCALL` retired; `pc` still points at it. `next_pc` is the
    /// instruction after it (where a resumed guest continues).
    Syscall { next_pc: u32 },
}

/// How a [`SyscallHandler`] tells the run loop to proceed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SyscallOutcome {
    /// Write this value to the guest's `a2` and continue after the `SYSCALL`.
    Resume(u32),
    /// Stop the run; the value becomes [`RunOutcome::Ok`]. (Guest `exit` — the
    /// handler records any abnormal detail, e.g. a panic message, itself.)
    Exit(u32),
}

/// Host hook invoked when the guest executes a `SYSCALL` instruction.
///
/// The guest ABI (which registers carry the syscall number/arguments) is the
/// handler's business, not the emulator's — the handler gets the full CPU and
/// memory. `lp-xt-elf` defines the ABI used by the fixture corpus.
pub trait SyscallHandler {
    fn syscall(&mut self, cpu: &mut Cpu, mem: &mut Memory) -> SyscallOutcome;
}

/// A completed run.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RunOutcome {
    /// The top-level function returned; the value is its result register.
    Ok(u32),
    /// Execution trapped (exception or timeout).
    Trap(Trap),
}

/// One retired (or trapping) instruction in the debug ring log. Stored raw and
/// formatted lazily via [`lp_xt_inst::disasm`] — the Xtensa mirror of
/// `lp-riscv-emu`'s `InstLog` (which is an rv32-shaped enum; full parity is
/// deliberately deferred — the consumers only read the formatted dump).
#[derive(Clone, Copy, Debug)]
struct InstRecord {
    cycle: u64,
    pc: u32,
    bytes: [u8; 3],
    len: u8,
}

/// The emulator.
pub struct Emulator {
    pub cpu: Cpu,
    pub mem: Memory,
    /// Instruction budget for [`RunOutcome::Trap`] timeout detection.
    pub step_budget: u64,
    /// The board memory map this emulator was built on.
    pub profile: BoardProfile,
    /// Instruction-log verbosity ([`LogLevel::Instructions`] fills the ring
    /// log read back through [`format_debug_info`](Self::format_debug_info)).
    log_level: LogLevel,
    /// Per-instruction cost model for [`get_cycle_count`](Self::get_cycle_count).
    cycle_model: CycleModel,
    /// Instructions retired since the last run was staged.
    instruction_count: u64,
    /// Estimated cycles per `cycle_model` since the last run was staged.
    cycle_count: u64,
    /// Ring buffer of the most recent instructions (log_level = Instructions).
    inst_log: VecDeque<InstRecord>,
}

impl Emulator {
    /// Build an emulator with the standard **ESP32-S3** layout — the default
    /// board, so existing S3 code and tests are unchanged. Other boards go
    /// through [`with_profile`](Self::with_profile).
    pub fn new() -> Emulator {
        Emulator::with_profile(BoardProfile::esp32s3())
    }

    /// Build an emulator on an arbitrary board memory map.
    pub fn with_profile(profile: BoardProfile) -> Emulator {
        let mut mem = Memory::new();
        profile.install(&mut mem);
        Emulator {
            cpu: Cpu::new(),
            mem,
            step_budget: DEFAULT_STEP_BUDGET,
            profile,
            log_level: LogLevel::None,
            // No measured Xtensa cycle model yet — instruction counting is the
            // honest default (rv32 defaults to its measured Esp32C6 model).
            cycle_model: CycleModel::InstructionCount,
            instruction_count: 0,
            cycle_count: 0,
            inst_log: VecDeque::new(),
        }
    }

    /// Set the instruction-log verbosity (builder form).
    #[must_use]
    pub fn with_log_level(mut self, level: LogLevel) -> Self {
        self.log_level = level;
        self
    }

    /// Set the instruction-log verbosity.
    pub fn set_log_level(&mut self, level: LogLevel) {
        self.log_level = level;
    }

    /// Set the cycle model (builder form).
    #[must_use]
    pub fn with_cycle_model(mut self, model: CycleModel) -> Self {
        self.cycle_model = model;
        self
    }

    /// Set the cycle model used for [`get_cycle_count`](Self::get_cycle_count).
    pub fn set_cycle_model(&mut self, model: CycleModel) {
        self.cycle_model = model;
    }

    /// The current cycle model.
    pub fn cycle_model(&self) -> CycleModel {
        self.cycle_model
    }

    /// Instructions retired since the last run was staged.
    pub fn get_instruction_count(&self) -> u64 {
        self.instruction_count
    }

    /// Estimated cycles (per the configured [`CycleModel`]) since the last run
    /// was staged.
    pub fn get_cycle_count(&self) -> u64 {
        self.cycle_count
    }

    /// Run `code` as `fn(arg) -> u32`, entered at `entry_offset` within the
    /// blob, exactly as the device runner does: the code is written into the
    /// profile's code region so its executable image is I-bus-contiguous, and
    /// the entry is invoked via a synthesized windowed CALL8 (arg staged in
    /// `a10`, arriving in the callee's `a2` after its ENTRY). Uses a no-op
    /// tracer.
    pub fn run(&mut self, code: &[u8], entry_offset: u32, arg: u32) -> RunOutcome {
        let mut t = crate::trace::NoopTracer;
        self.run_traced(code, entry_offset, arg, &mut t)
    }

    /// As [`run`](Self::run), emitting [`TraceEvent`]s to `tracer`.
    pub fn run_traced(
        &mut self,
        code: &[u8],
        entry_offset: u32,
        arg: u32,
        tracer: &mut dyn Tracer,
    ) -> RunOutcome {
        // Load code at the I-bus base of the profile's code region — byte i of
        // the blob lands at ibus_base + i. Under the S3's offset alias this is
        // the same bytes as a D-bus write at CODE_DBUS_BASE; under classic's
        // word-mirrored alias the backing D-bus image walks downward word by
        // word, exactly as the device writer lays it out (FINDINGS C2b).
        let ibus_base = self.profile.code_ibus_base();
        self.mem.load_bytes(ibus_base, code);
        let entry = ibus_base.wrapping_add(entry_offset);
        self.stage_windowed_entry(entry, arg);
        self.run_loop(tracer, None)
    }

    /// Run already-loaded code (e.g. ELF segments written into `self.mem` by a
    /// loader) starting at the I-bus address `entry`, invoked via the same
    /// synthesized windowed CALL8 as [`run`](Self::run). `SYSCALL` instructions
    /// are dispatched to `handler`.
    pub fn run_loaded(
        &mut self,
        entry: u32,
        arg: u32,
        tracer: &mut dyn Tracer,
        handler: &mut dyn SyscallHandler,
    ) -> RunOutcome {
        self.stage_windowed_entry(entry, arg);
        self.run_loop(tracer, Some(handler))
    }

    /// Reset the CPU and stage the synthesized windowed CALL8 into `entry`.
    fn stage_windowed_entry(&mut self, entry: u32, arg: u32) {
        // Synthesize the caller frame (the runner's context) at base 0 and the
        // CALL8 that jumps into `entry`. A real CALL8 writes the (mangled)
        // return address into the caller's a8 and stages args in a10..; the
        // callee's ENTRY then rotates WindowBase by PS.CALLINC (=2), so a8→a0
        // and a10→a2.
        let initial_sp = self.profile.initial_sp();
        self.instruction_count = 0;
        self.cycle_count = 0;
        self.inst_log.clear();
        self.cpu = Cpu::new();
        self.cpu.window_base = 0;
        self.cpu.window_start = 1; // frame 0 resident
        self.cpu.call_stack.push(crate::cpu::FrameRec {
            base: 0,
            sp: initial_sp,
            inc: 2,
            resident: true,
        });
        self.cpu.set_a(1, initial_sp); // caller SP
        // Mangled sentinel return address in a8: callinc=2 in top bits, sentinel
        // low bits. RETW unmangles to SENTINEL_PC (see finish_call).
        self.cpu
            .set_a(8, (2u32 << 30) | (SENTINEL_PC & 0x3FFF_FFFF));
        self.cpu.set_a(10, arg); // first argument
        self.cpu.ps_callinc = 2;
        self.cpu.pc = entry;
    }

    fn run_loop(
        &mut self,
        tracer: &mut dyn Tracer,
        mut handler: Option<&mut dyn SyscallHandler>,
    ) -> RunOutcome {
        let mut steps = 0u64;
        loop {
            if self.cpu.pc == SENTINEL_PC {
                // Top-level RETW landed on the sentinel: the result is in the
                // caller's a10 (== the callee's a2 before the return rotation).
                return RunOutcome::Ok(self.cpu.a(10));
            }
            if steps >= self.step_budget {
                return RunOutcome::Trap(Trap {
                    kind: TrapKind::Timeout,
                    cause: 0,
                    pc: self.cpu.pc,
                    vaddr: 0,
                });
            }
            steps += 1;
            match self.step(tracer) {
                Ok(Step::Normal) => {}
                Ok(Step::Syscall { next_pc }) => match handler.as_mut() {
                    // No handler: model unhandled hardware behavior (a
                    // SyscallCause exception at the SYSCALL's pc).
                    None => {
                        return RunOutcome::Trap(Trap {
                            kind: TrapKind::Exception,
                            cause: crate::error::EXC_SYSCALL,
                            pc: self.cpu.pc,
                            vaddr: 0,
                        });
                    }
                    Some(h) => match h.syscall(&mut self.cpu, &mut self.mem) {
                        SyscallOutcome::Resume(v) => {
                            self.cpu.set_a(2, v);
                            self.cpu.pc = next_pc;
                        }
                        SyscallOutcome::Exit(code) => return RunOutcome::Ok(code),
                    },
                },
                Err(mut trap) => {
                    if trap.pc == 0 {
                        trap.pc = self.cpu.pc;
                    }
                    return RunOutcome::Trap(trap);
                }
            }
        }
    }

    /// Fetch, decode, and execute one instruction, updating `pc`.
    fn step(&mut self, tracer: &mut dyn Tracer) -> Result<Step, Trap> {
        let pc = self.cpu.pc;
        let mut bytes = [0u8; 3];
        let got = self.mem.fetch(pc, &mut bytes)?;
        let (inst, len) = lp_xt_inst::decode(&bytes[..got]).map_err(|_| Trap {
            kind: TrapKind::Exception,
            cause: crate::error::EXC_ILLEGAL_INSTRUCTION,
            pc,
            vaddr: 0,
        })?;
        // Log before executing so a trapping instruction is the log's last line.
        if self.log_level == LogLevel::Instructions {
            if self.inst_log.len() >= lp_emu_core::config::INSTRUCTION_LOG_BUFFER_SIZE {
                self.inst_log.pop_front();
            }
            self.inst_log.push_back(InstRecord {
                cycle: self.cycle_count,
                pc,
                bytes,
                len: len as u8,
            });
        }
        tracer.event(TraceEvent::Inst {
            pc,
            len,
            inst: &inst,
        });
        let flow = self.execute(&inst, pc, tracer)?;
        self.instruction_count += 1;
        self.cycle_count += u64::from(
            self.cycle_model
                .cycles_for(crate::executor::inst_class(&inst, &flow)),
        );
        match flow {
            Flow::Next => self.cpu.pc = pc.wrapping_add(len as u32),
            Flow::Jump(addr) => self.cpu.pc = addr,
            // Leave pc at the SYSCALL; the run loop advances after dispatch.
            Flow::Syscall => {
                return Ok(Step::Syscall {
                    next_pc: pc.wrapping_add(len as u32),
                });
            }
        }
        Ok(Step::Normal)
    }

    // --- small shared helpers used by the executor modules ---

    /// Write windowed register `a{i}` and emit a trace event.
    pub(crate) fn wreg(&mut self, i: u8, v: u32, tracer: &mut dyn Tracer) {
        let phys = self.cpu.set_a(i, v);
        tracer.event(TraceEvent::RegWrite {
            index: i,
            phys,
            value: v,
        });
    }

    /// Read windowed register `a{i}`.
    #[inline]
    pub(crate) fn rreg(&self, i: u8) -> u32 {
        self.cpu.a(i)
    }

    // --- debug dumps (the consumer-facing shape of lp-riscv-emu's debug.rs) ---

    /// A one-screen dump of the architectural state: pc, window registers,
    /// window bookkeeping, and call depth.
    pub fn dump_state(&self) -> String {
        use std::fmt::Write as _;
        let mut s = String::new();
        let _ = writeln!(
            s,
            "pc={:#010x}  window_base={}  window_start={:#06x}  sar={}  callinc={}  depth={}",
            self.cpu.pc,
            self.cpu.window_base,
            self.cpu.window_start,
            self.cpu.sar,
            self.cpu.ps_callinc,
            self.cpu.call_stack.len()
        );
        for row in 0..4u8 {
            let base = row * 4;
            let _ = writeln!(
                s,
                "a{:<2}..a{:<2}  {:#010x} {:#010x} {:#010x} {:#010x}",
                base,
                base + 3,
                self.cpu.a(base),
                self.cpu.a(base + 1),
                self.cpu.a(base + 2),
                self.cpu.a(base + 3)
            );
        }
        let _ = writeln!(
            s,
            "instructions={}  cycles={} ({:?})",
            self.instruction_count, self.cycle_count, self.cycle_model
        );
        s
    }

    /// Format the tail of the instruction ring log (up to `log_count` lines),
    /// disassembled via [`lp_xt_inst`], optionally marking `highlight_pc`.
    /// Empty unless the log level was [`LogLevel::Instructions`] during the run.
    pub fn format_debug_info(&self, highlight_pc: Option<u32>, log_count: usize) -> String {
        use std::fmt::Write as _;
        let mut s = String::new();
        if self.inst_log.is_empty() {
            let _ = writeln!(
                s,
                "(instruction log empty — run with LogLevel::Instructions to fill it)"
            );
            return s;
        }
        let skip = self.inst_log.len().saturating_sub(log_count);
        let _ = writeln!(
            s,
            "--- last {} instructions ---",
            self.inst_log.len() - skip
        );
        for rec in self.inst_log.iter().skip(skip) {
            let text =
                lp_xt_inst::disasm::format_instruction(&rec.bytes[..rec.len as usize], rec.pc);
            let mark = if highlight_pc == Some(rec.pc) {
                ">"
            } else {
                " "
            };
            let _ = writeln!(s, "{mark} [{:>8}] {:#010x}  {text}", rec.cycle, rec.pc);
        }
        s
    }
}

impl Default for Emulator {
    fn default() -> Self {
        Emulator::new()
    }
}
