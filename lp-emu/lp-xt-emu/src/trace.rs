//! Trace hook: a [`Tracer`] trait with a no-op default and a basic text tracer.
//!
//! This is deliberately a *hook*, not a full instruction-log implementation.
//! Full `InstLog`-parity (matching the lp2025 filetest consumers) is backport
//! work; here we expose the events an emulator can emit — per-instruction
//! retirement, register writes (naming the *physical* AR touched), memory
//! writes, and window events (rotate / spill / reload) — so the eventual parity
//! layer plugs into the same seam.

use lp_xt_inst::Inst;

/// One thing worth recording as the emulator runs. Borrowed so a no-op tracer
/// costs nothing and the text tracer formats lazily.
#[derive(Clone, Copy, Debug)]
pub enum TraceEvent<'a> {
    /// An instruction was fetched and decoded at `pc` (`len` bytes).
    Inst { pc: u32, len: usize, inst: &'a Inst },
    /// A windowed register `a{index}` (physical `AR[phys]`) was written.
    RegWrite { index: u8, phys: u8, value: u32 },
    /// A float register `f{index}` was written. Carries **raw bits**; the text
    /// tracer prints them alongside the `f32` reading because a payload and a
    /// value are different questions when bisecting a numeric divergence.
    FRegWrite { index: u8, bits: u32 },
    /// A boolean register `b{index}` was written — the only place an FP compare
    /// result is visible, so without this event a compare is invisible in a
    /// trace.
    BRegWrite { index: u8, value: bool },
    /// `nbytes` were written to data memory at `addr`.
    MemWrite { addr: u32, value: u32, nbytes: u8 },
    /// The register window rotated (ENTRY / RETW / CALL).
    WindowRotate {
        what: &'static str,
        old_base: u8,
        new_base: u8,
        window_start: u16,
    },
    /// A frame's registers were spilled to its stack save area (overflow).
    WindowSpill { base: u8, sp: u32, nregs: u8 },
    /// A frame's registers were reloaded from its stack save area (underflow).
    WindowReload { base: u8, sp: u32, nregs: u8 },
}

/// Sink for [`TraceEvent`]s. The default impl ignores everything, so an emulator
/// generic over `&mut dyn Tracer` pays nothing when handed [`NoopTracer`].
pub trait Tracer {
    fn event(&mut self, _event: TraceEvent<'_>) {}
}

/// A tracer that discards every event.
pub struct NoopTracer;

impl Tracer for NoopTracer {}

/// A tracer that appends a readable line per event to an in-memory log.
#[derive(Default)]
pub struct TextTracer {
    /// Accumulated lines; join with `\n` for a full trace.
    pub lines: Vec<String>,
}

impl TextTracer {
    pub fn new() -> TextTracer {
        TextTracer::default()
    }

    /// The whole trace as one newline-separated string.
    pub fn dump(&self) -> String {
        self.lines.join("\n")
    }
}

impl Tracer for TextTracer {
    fn event(&mut self, event: TraceEvent<'_>) {
        let line = match event {
            TraceEvent::Inst { pc, len, inst } => {
                format!("{pc:#010x}  [{len}]  {inst:?}")
            }
            TraceEvent::RegWrite { index, phys, value } => {
                format!("             a{index} (AR[{phys}]) <- {value:#010x}")
            }
            TraceEvent::FRegWrite { index, bits } => {
                format!(
                    "             f{index} <- {bits:#010x} ({})",
                    f32::from_bits(bits)
                )
            }
            TraceEvent::BRegWrite { index, value } => {
                format!("             b{index} <- {}", u8::from(value))
            }
            TraceEvent::MemWrite {
                addr,
                value,
                nbytes,
            } => {
                format!("             mem[{addr:#010x}] <- {value:#010x} ({nbytes}B)")
            }
            TraceEvent::WindowRotate {
                what,
                old_base,
                new_base,
                window_start,
            } => {
                format!(
                    "             window {what}: base {old_base} -> {new_base}  \
                     WindowStart={window_start:#06x}"
                )
            }
            TraceEvent::WindowSpill { base, sp, nregs } => {
                format!("             spill frame@base{base} ({nregs} regs) to sp={sp:#010x}")
            }
            TraceEvent::WindowReload { base, sp, nregs } => {
                format!("             reload frame@base{base} ({nregs} regs) from sp={sp:#010x}")
            }
        };
        self.lines.push(line);
    }
}
