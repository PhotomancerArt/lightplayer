//! The emulator: memory + CPU + the run loop and the windowed-ABI run harness.

use std::collections::VecDeque;

use lp_emu_core::{CycleModel, LogLevel};

use crate::board::BoardProfile;
use crate::cpu::Cpu;
use crate::error::{Trap, TrapKind};
use crate::fp_policy::FpPolicy;
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

/// Number of windowed call-argument registers: the caller stages `a10..a15`
/// (`isa/xt`'s `OUT_ARG_REGS`), which the callee's ENTRY rotates into
/// `a2..a7`. Arguments past these go to the caller's outgoing stack area.
pub const OUT_ARG_REG_COUNT: usize = 6;

/// Default instruction budget before a run is declared a [`TrapKind::Timeout`]
/// (models the device watchdog catching an infinite loop). Far above any
/// payload the corpus runs; the hang case is the only one that reaches it.
pub const DEFAULT_STEP_BUDGET: u64 = 2_000_000;

/// Control-flow outcome of executing one instruction.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
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

/// Outcome of a host-initiated call ([`Emulator::run_loaded_with_args`]).
///
/// Distinct from [`RunOutcome`] because a call can return **two** words: the
/// windowed ABI's result bank is `a10`/`a11` in the caller's view
/// (`isa/xt`'s `CALL_RET_REGS`), and a two-scalar return uses both. Returning
/// them explicitly is deliberate — the alternative, letting the caller reach
/// into `emu.cpu` for the second half of its own result, is an implicit
/// contract that breaks the first time the run loop changes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CallOutcome {
    /// The call returned. `lo`/`hi` are the caller-view `a10`/`a11`; `hi` is
    /// meaningful only for a two-word return.
    Ok { lo: u32, hi: u32 },
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
    /// The behaviors of the FPU that IEEE-754 does not fix. Mostly
    /// [`crate::fp_policy::Unknown`] until the M6 P6 campaign measures them;
    /// reading an unresolved field panics rather than defaulting.
    pub fp_policy: FpPolicy,
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
            fp_policy: FpPolicy::m6(),
            log_level: LogLevel::None,
            // No measured Xtensa cycle model yet — instruction counting is the
            // honest default (rv32 defaults to its measured Esp32C6 model).
            cycle_model: CycleModel::InstructionCount,
            instruction_count: 0,
            cycle_count: 0,
            inst_log: VecDeque::new(),
        }
    }

    /// Install a floating-point policy (builder form). P6 uses this to hand the
    /// emulator the constants it measured on silicon; nothing else should need
    /// it, and nothing may use it to paper over an unresolved field.
    #[must_use]
    pub fn with_fp_policy(mut self, policy: FpPolicy) -> Self {
        self.fp_policy = policy;
        self
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

    /// As [`run`](Self::run) with up to six register arguments: `args[i]` is
    /// staged in the caller's `a{10+i}`, arriving in the callee's `a{2+i}`
    /// after its ENTRY — the CALL8 convention's full register-argument bank.
    /// (Stack-passed arguments beyond six are the caller's outgoing-area
    /// business and not synthesized here.) Added at monorepo landing for the
    /// isa/xt emitter pipeline tests, which call multi-parameter functions.
    ///
    /// # Panics
    /// If `args.len() > 6`.
    pub fn run_with_args(&mut self, code: &[u8], entry_offset: u32, args: &[u32]) -> RunOutcome {
        assert!(
            args.len() <= OUT_ARG_REG_COUNT,
            "run_with_args stages register args only (max {OUT_ARG_REG_COUNT}, got {}) — \
             use run_loaded_with_args for stack arguments",
            args.len()
        );
        let ibus_base = self.profile.code_ibus_base();
        self.mem.load_bytes(ibus_base, code);
        let entry = ibus_base.wrapping_add(entry_offset);
        self.stage_windowed_entry(entry, args.first().copied().unwrap_or(0));
        for (i, &a) in args.iter().enumerate().skip(1) {
            self.cpu.set_a(10 + i as u8, a);
        }
        let mut t = crate::trace::NoopTracer;
        self.run_loop(&mut t, None)
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

    /// Call already-loaded code with a full argument list and read back **both**
    /// result words — the entry point a host emulation engine uses to invoke a
    /// compiled shader function.
    ///
    /// What this adds over [`run_loaded`](Self::run_loaded) (one argument) and
    /// [`run_with_args`](Self::run_with_args) (six, and it *loads a code blob*,
    /// which is wrong once a loader has placed an ELF image):
    ///
    /// - **Register arguments**: `args[0..6]` are staged in the caller's
    ///   `a10..a15` (`isa/xt`'s `OUT_ARG_REGS`), arriving as the callee's
    ///   `a2..a7` after its ENTRY rotation.
    /// - **Stack arguments**: `args[6..]` are written to the caller's outgoing
    ///   argument area at `[caller SP + 4*i]`, matching what `isa/xt`'s
    ///   `classify_params` computes (`ArgLoc::Stack { offset }`, from 0 by 4)
    ///   and what its emitter stores (`[SP + (i - cap) * 4]`, where the callee's
    ///   SP plus its ENTRY frame *is* the caller's SP).
    /// - **Outgoing-area headroom**: the caller SP is lowered by the (16-aligned)
    ///   size of that area first. `BoardProfile::initial_sp` sits 16 bytes below
    ///   the top of the stack region, which would leave room for only four stack
    ///   words before running off the end.
    /// - **Two-word results**: see [`CallOutcome`].
    ///
    /// `self.step_budget` bounds the run. A host engine that arms in-guest fuel
    /// should raise it well above the fuel tank so fuel traps fire first and the
    /// budget stays a backstop for fuel-off compiles (rv32's `rt_emu` raises its
    /// equivalent to 64M for exactly this reason); [`DEFAULT_STEP_BUDGET`] is
    /// sized for the fixture corpus, not for real shaders.
    pub fn run_loaded_with_args(
        &mut self,
        entry: u32,
        args: &[u32],
        tracer: &mut dyn Tracer,
        handler: Option<&mut dyn SyscallHandler>,
    ) -> CallOutcome {
        let n_reg = args.len().min(OUT_ARG_REG_COUNT);
        let stack_bytes = ((args.len() - n_reg) * 4) as u32;
        // Keep SP 16-aligned (the ABI's frame invariant): initial_sp already is,
        // and rounding the outgoing area up to 16 preserves it.
        let sp = self
            .profile
            .initial_sp()
            .wrapping_sub(stack_bytes.next_multiple_of(16));

        self.stage_windowed_entry_at(entry, args.first().copied().unwrap_or(0), sp);
        for (i, &a) in args.iter().enumerate().take(n_reg).skip(1) {
            self.cpu.set_a(10 + i as u8, a);
        }
        for (i, &a) in args.iter().enumerate().skip(n_reg) {
            let at = sp.wrapping_add(((i - n_reg) * 4) as u32);
            if let Err(trap) = self.mem.write_u32(at, a) {
                return CallOutcome::Trap(trap);
            }
        }

        match self.run_loop(tracer, handler) {
            RunOutcome::Ok(lo) => CallOutcome::Ok {
                lo,
                hi: self.cpu.a(11),
            },
            RunOutcome::Trap(t) => CallOutcome::Trap(t),
        }
    }

    /// Reset the CPU and stage the synthesized windowed CALL8 into `entry`,
    /// with the caller frame's SP at [`BoardProfile::initial_sp`].
    fn stage_windowed_entry(&mut self, entry: u32, arg: u32) {
        self.stage_windowed_entry_at(entry, arg, self.profile.initial_sp());
    }

    /// As [`stage_windowed_entry`](Self::stage_windowed_entry) with an explicit
    /// caller SP, so a caller that needs an outgoing-argument area can reserve
    /// it below the top of the stack region.
    fn stage_windowed_entry_at(&mut self, entry: u32, arg: u32, initial_sp: u32) {
        // Synthesize the caller frame (the runner's context) at base 0 and the
        // CALL8 that jumps into `entry`. A real CALL8 writes the (mangled)
        // return address into the caller's a8 and stages args in a10..; the
        // callee's ENTRY then rotates WindowBase by PS.CALLINC (=2), so a8→a0
        // and a10→a2.
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

    /// Execute one already-decoded instruction against the current state,
    /// advancing `pc` as the run loop would.
    ///
    /// The entry point for single-instruction harnesses — specifically
    /// `tests/fp_conformance.rs`, which replays tens of thousands of FP vectors
    /// and has no code image to fetch from. Real runs go through
    /// [`run`](Self::run) and its siblings; this deliberately skips the fetch,
    /// the decode, and the instruction log, so it is not a substitute for them.
    ///
    /// Instruction and cycle counters advance, so a harness can still read them.
    pub fn exec_one(&mut self, inst: &lp_xt_inst::Inst) -> Result<(), Trap> {
        let mut t = crate::trace::NoopTracer;
        self.exec_one_traced(inst, &mut t)
    }

    /// As [`exec_one`](Self::exec_one), emitting [`TraceEvent`]s.
    pub fn exec_one_traced(
        &mut self,
        inst: &lp_xt_inst::Inst,
        tracer: &mut dyn Tracer,
    ) -> Result<(), Trap> {
        let pc = self.cpu.pc;
        let flow = self.execute(inst, pc, tracer).map_err(|mut trap| {
            if trap.pc == 0 {
                trap.pc = pc;
            }
            trap
        })?;
        self.instruction_count += 1;
        self.cycle_count += u64::from(
            self.cycle_model
                .cycles_for(crate::executor::inst_class(inst, &flow)),
        );
        match flow {
            // Width is not known without the encoding; 3 is the wide form and
            // the only thing a straight-line harness needs it for is progress.
            Flow::Next | Flow::Syscall => self.cpu.pc = pc.wrapping_add(3),
            Flow::Jump(addr) => self.cpu.pc = addr,
        }
        Ok(())
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

    /// Write float register `f{i}` (raw bits) and emit a trace event.
    ///
    /// Executors go through here rather than touching `cpu.fr` so that every FP
    /// write is on one traced path — P6 bisects numeric divergences off this
    /// trace, and an intermediate you cannot see is a bad day.
    pub(crate) fn wfreg(&mut self, i: u8, bits: u32, tracer: &mut dyn Tracer) {
        self.cpu.set_f(i, bits);
        tracer.event(TraceEvent::FRegWrite { index: i, bits });
    }

    /// Read float register `f{i}` (raw bits).
    #[inline]
    pub(crate) fn rfreg(&self, i: u8) -> u32 {
        self.cpu.f(i)
    }

    /// Write boolean register `b{i}` and emit a trace event.
    pub(crate) fn wbreg(&mut self, i: u8, v: bool, tracer: &mut dyn Tracer) {
        self.cpu.set_b(i, v);
        tracer.event(TraceEvent::BRegWrite { index: i, value: v });
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
        // The FP block. `f{i}` is printed as bits *and* as an f32 reading: a raw
        // NaN payload and a numeric value answer different questions, and the
        // payload is exactly what M6 measures.
        let _ = writeln!(
            s,
            "cpenable={:#x} ({})  fcr={:#010x}  fsr={:#010x}  br={:#06x}",
            self.cpu.cpenable,
            if self.cpu.fpu_enabled() {
                "FPU armed"
            } else {
                "FPU DISABLED"
            },
            self.cpu.fcr,
            self.cpu.fsr,
            self.cpu.br
        );
        for row in 0..4u8 {
            let base = row * 4;
            let _ = writeln!(
                s,
                "f{:<2}..f{:<2}  {}",
                base,
                base + 3,
                (0..4)
                    .map(|k| {
                        let bits = self.cpu.f(base + k);
                        format!("{bits:#010x}={:e}", f32::from_bits(bits))
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
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
