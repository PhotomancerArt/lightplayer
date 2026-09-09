//! Per-instruction cost classes for cycle accounting (not a hardware perf counter).

/// Identifies the CPU whose cycle behaviour is being estimated by the
/// emulator's per-instruction cost model.
///
/// Only [`CycleModel::Esp32C6`] is implemented today; additional variants
/// can be added without touching the run loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CycleModel {
    /// Per-instruction estimate ignored; cycle count tracks instruction count 1:1.
    InstructionCount,

    /// ESP32-C6 (Andes N22-class single-issue in-order RV32IMAC core).
    ///
    /// Reference: <https://ctrlsrc.io/posts/2023/counting-cpu-cycles-on-esp32c3-esp32c6/>
    ///
    /// This is a coarse approximation: per-class fixed costs plus
    /// branch-taken vs not-taken. ICache misses, branch-predictor warm-up,
    /// variable DIV cycles, and load-use hazards are not modelled.
    #[default]
    Esp32C6,

    /// The same core, with the four class costs the `cycle-probe` payload
    /// *measured* on silicon replacing the ones [`CycleModel::Esp32C6`]
    /// carried from a blog post.
    ///
    /// This variant exists rather than a correction to `Esp32C6` because a
    /// committed transcript names its grade: moving `Esp32C6` would move
    /// every `lp-emu:esp32c6:t2` cycle count already on the record. See
    /// `docs/reports/2026-09-08-esp32c6-t3-calibration.md` §2 for the
    /// kernel behind each number and §3 for what is *not* measured here
    /// (every class this variant does not name keeps `Esp32C6`'s figure,
    /// which no kernel has checked).
    ///
    /// The classes it changes, each with the kernel that isolated it:
    ///
    /// | class | `Esp32C6` | here | kernel |
    /// |---|---:|---:|---|
    /// | `DivRem` | 32 | 10 | `muldiv/div` − `muldiv/mul` |
    /// | `Load` | 2 | 1 | `mmio_poll` (12.0 cycles/iteration = 1 + 1 + 10 APB) |
    /// | `BranchTaken` | 2 | 1 | `iram_loop` (6 instructions = 6.0002 cycles) |
    /// | `Mul` | 1 | 1 | `muldiv/mul` — unchanged, and now measured |
    ///
    /// A memory access's *address* costs nothing here; that is
    /// [`MemoryCost`]'s job, and on this part it is where the cache and the
    /// APB live.
    Esp32C6Kernels,
}

/// Extra cycles a memory access costs beyond the instruction's own class.
/// Injected by the machine; the hart knows nothing about what makes an
/// address expensive.
///
/// Every method returns the cycles to add to the hart's counter for **this
/// access alone**, on top of whatever [`CycleModel::cycles_for`] charged the
/// instruction that made it. An implementation may keep state — a cache's
/// tags are the reason for `&mut self` — but that state belongs to the chip
/// crate that implements it: this crate has no cache, no window and no
/// number of its own.
///
/// The contract that makes it usable in a validation system: for one image
/// and one input, the sequence of calls is fixed, so the sequence of answers
/// must be too. An implementation that consults the host clock, a hash seed
/// or an allocator address breaks the machine's determinism.
pub trait MemoryCost {
    /// An instruction fetch of the word at `addr`.
    fn fetch(&mut self, addr: u32) -> u32;
    /// A load of `width` bytes from `addr`.
    fn load(&mut self, addr: u32, width: u8) -> u32;
    /// A store of `width` bytes to `addr`.
    fn store(&mut self, addr: u32, width: u8) -> u32;
}

/// Every access is free — what a machine with no memory-cost model uses, and
/// what `t1` and `t2` keep using so their cycle counts do not move.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NoMemoryCost;

impl MemoryCost for NoMemoryCost {
    #[inline(always)]
    fn fetch(&mut self, _addr: u32) -> u32 {
        0
    }
    #[inline(always)]
    fn load(&mut self, _addr: u32, _width: u8) -> u32 {
        0
    }
    #[inline(always)]
    fn store(&mut self, _addr: u32, _width: u8) -> u32 {
        0
    }
}

/// Cost bucket for [`CycleModel::cycles_for`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstClass {
    Alu,
    Mul,
    DivRem,
    Load,
    Store,
    BranchTaken,
    BranchNotTaken,
    JalCall,
    JalTail,
    JalrCall,
    JalrReturn,
    JalrIndirect,
    Lui,
    Auipc,
    System,
    Fence,
    Atomic,

    // --- floating point ---
    //
    // Added by M6 for the Xtensa FP coprocessor, and shaped so RV32F (M9) lands
    // in the same buckets. **None of them carries a measured cost.** There is no
    // measured Xtensa cycle model at all (`lp-xt-emu` defaults to
    // `CycleModel::InstructionCount`), and the one model that *is* measured —
    // `Esp32C6` — is for a core with no FPU, so it can never see these. They
    // exist so a future measured model has somewhere to land, and so an FP
    // instruction is not silently miscounted as an `Alu`. Do not read the
    // numbers in `cycles_for` as a claim about silicon.
    /// `add.s` / `sub.s` / `mul.s` and their RV32F equivalents.
    FloatArith,
    /// `madd.s` / `msub.s` — a distinct bucket because fused multiply-add is
    /// the one FP op whose cost is routinely *not* the sum of its parts.
    FloatMulAdd,
    /// `float.s` / `trunc.s` / `round.s` / `floor.s` / `ceil.s` and friends.
    FloatConvert,
    /// The FP compare predicates, which write a boolean register on Xtensa.
    FloatCompare,
    /// `recip0.s` / `rsqrt0.s` / `sqrt0.s` / `div0.s` — table lookups, and the
    /// per-step instructions of a divide or square-root sequence.
    FloatEstimate,
}

impl CycleModel {
    pub fn cycles_for(self, class: InstClass) -> u8 {
        match self {
            CycleModel::InstructionCount => 1,
            CycleModel::Esp32C6 => match class {
                InstClass::Alu | InstClass::Mul | InstClass::Lui | InstClass::Auipc => 1,
                InstClass::DivRem => 32,
                InstClass::Load => 2,
                InstClass::Store => 1,
                InstClass::BranchNotTaken => 1,
                InstClass::BranchTaken => 2,
                InstClass::JalCall | InstClass::JalTail => 2,
                InstClass::JalrCall | InstClass::JalrReturn | InstClass::JalrIndirect => 3,
                InstClass::System => 4,
                InstClass::Fence => 4,
                InstClass::Atomic => 4,
                // Unreachable: the C6 is RV32IMAC and decodes no FP
                // instruction. 1 rather than a plausible-looking FPU latency,
                // so nothing here can be mistaken for a measurement.
                InstClass::FloatArith
                | InstClass::FloatMulAdd
                | InstClass::FloatConvert
                | InstClass::FloatCompare
                | InstClass::FloatEstimate => 1,
            },
            CycleModel::Esp32C6Kernels => match class {
                // `iram_loop`: six instructions an iteration, 6.0002 cycles
                // an iteration on silicon — one cycle each, the taken branch
                // included. `muldiv/mul`: 1.0 cycles.
                InstClass::Alu
                | InstClass::Mul
                | InstClass::Lui
                | InstClass::Auipc
                | InstClass::Load
                | InstClass::Store
                | InstClass::BranchNotTaken
                | InstClass::BranchTaken => 1,
                // `muldiv/div` − `muldiv/mul`: 14.0 cycles an iteration less
                // the four non-dividing instructions at 1.0 each.
                InstClass::DivRem => 10,
                // Not measured by any kernel: `Esp32C6`'s figures, carried.
                InstClass::JalCall | InstClass::JalTail => 2,
                InstClass::JalrCall | InstClass::JalrReturn | InstClass::JalrIndirect => 3,
                InstClass::System => 4,
                InstClass::Fence => 4,
                InstClass::Atomic => 4,
                // Unreachable, as above.
                InstClass::FloatArith
                | InstClass::FloatMulAdd
                | InstClass::FloatConvert
                | InstClass::FloatCompare
                | InstClass::FloatEstimate => 1,
            },
        }
    }
}
