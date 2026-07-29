//! The host side of the guest syscall ABI ([`crate::abi`]): collects guest
//! output, records exit / panic, and a one-call ELF runner for tests.

use crate::abi;
use crate::loader::{ElfError, XtensaElf};
use lp_xt_emu::cpu::Cpu;
use lp_xt_emu::memory::Memory;
use lp_xt_emu::{Emulator, NoopTracer, RunOutcome, SyscallHandler, SyscallOutcome, Tracer};

/// Upper bound on a single guest write/panic-message length — anything larger
/// than the modeled RAM is a corrupt length, not a real request.
const MAX_XFER: u32 = 1 << 20;

/// [`SyscallHandler`] implementing [`crate::abi`]: print, exit, panic.
#[derive(Default)]
pub struct GuestHost {
    /// Bytes the guest wrote via `SYS_WRITE`, in order.
    pub output: Vec<u8>,
    /// Exit code from `SYS_EXIT` (or the synthesized panic exit).
    pub exit_code: Option<u32>,
    /// Message from `SYS_PANIC`, if the guest panicked.
    pub panic: Option<String>,
}

impl GuestHost {
    fn read_guest(mem: &Memory, ptr: u32, len: u32) -> Option<Vec<u8>> {
        if len > MAX_XFER {
            return None;
        }
        let mut buf = Vec::with_capacity(len as usize);
        for i in 0..len {
            buf.push(mem.read_u8(ptr.wrapping_add(i)).ok()?);
        }
        Some(buf)
    }
}

impl SyscallHandler for GuestHost {
    fn syscall(&mut self, cpu: &mut Cpu, mem: &mut Memory) -> SyscallOutcome {
        let nr = cpu.a(2);
        let (a3, a4) = (cpu.a(3), cpu.a(4));
        match nr {
            abi::SYS_EXIT => {
                self.exit_code = Some(a3);
                SyscallOutcome::Exit(a3)
            }
            abi::SYS_WRITE => match Self::read_guest(mem, a3, a4) {
                Some(bytes) => {
                    self.output.extend_from_slice(&bytes);
                    SyscallOutcome::Resume(a4)
                }
                None => SyscallOutcome::Resume(abi::ERR),
            },
            abi::SYS_PANIC => {
                let msg = Self::read_guest(mem, a3, a4)
                    .map(|b| String::from_utf8_lossy(&b).into_owned())
                    .unwrap_or_else(|| "<unreadable panic message>".to_string());
                self.panic = Some(msg);
                self.exit_code = Some(abi::PANIC_EXIT_CODE);
                SyscallOutcome::Exit(abi::PANIC_EXIT_CODE)
            }
            _ => SyscallOutcome::Resume(abi::ERR),
        }
    }
}

/// Everything a completed guest run produced.
#[derive(Debug)]
pub struct GuestRun {
    /// How the emulator run ended (exit value or trap).
    pub outcome: RunOutcome,
    /// Collected `SYS_WRITE` output.
    pub output: Vec<u8>,
    /// Exit code, if the guest exited through the ABI.
    pub exit_code: Option<u32>,
    /// Panic message, if the guest panicked.
    pub panic: Option<String>,
}

impl GuestRun {
    /// Collected output as UTF-8 (lossy).
    pub fn output_str(&self) -> String {
        String::from_utf8_lossy(&self.output).into_owned()
    }
}

/// Load a linked Xtensa ELF into a fresh [`Emulator`] and run it to
/// completion with the guest syscall ABI hosted, passing `arg` as the entry
/// function's argument.
pub fn run_elf(elf: &[u8], arg: u32) -> Result<GuestRun, ElfError> {
    run_elf_traced(elf, arg, &mut NoopTracer)
}

/// As [`run_elf`], emitting trace events to `tracer`.
pub fn run_elf_traced(elf: &[u8], arg: u32, tracer: &mut dyn Tracer) -> Result<GuestRun, ElfError> {
    let parsed = XtensaElf::parse(elf)?;
    let mut emu = Emulator::new();
    // Compiled fixture programs (deep recursion, formatting) run far more
    // instructions than the raw-blob corpus the default budget was sized for;
    // still fractions of a second on the host, and hangs are still caught.
    emu.step_budget = 50_000_000;
    parsed.load_into(&mut emu)?;
    let mut host = GuestHost::default();
    let outcome = emu.run_loaded(parsed.entry(), arg, tracer, &mut host);
    Ok(GuestRun {
        outcome,
        output: host.output,
        exit_code: host.exit_code,
        panic: host.panic,
    })
}
