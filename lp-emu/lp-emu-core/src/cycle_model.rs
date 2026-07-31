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
        }
    }
}
