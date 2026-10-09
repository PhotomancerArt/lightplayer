//! When to cut power and how the in-flight operation tears.

use crate::calibrated_tear::EraseShape;

/// How the operation in flight when power goes is left.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TearModel {
    /// The in-flight operation does nothing at all.
    Clean,
    /// A program: the first *n* bytes of the in-flight page land, byte *n*
    /// gets a random subset of its intended 1→0 clears, the rest nothing.
    BytePrefix,
    /// A program: a random subset of the page's intended 1→0 clears lands,
    /// anywhere in the page.
    RandomBits,
    /// Tears shaped and weighted the way a real part tore (CX1, 200 cuts):
    /// programs stop on a 32-byte command or a 4-byte word; erases leave a
    /// `0x00` run from the front, all `0x00`, a zero residue with weak bits,
    /// or a sector reading `0xFF` (see [`crate::calibrated_tear`]). Not in
    /// [`TearModel::ALL`]: name it to run it.
    Calibrated,
    /// [`TearModel::Calibrated`] with every torn erase forced to one shape
    /// (programs keep the calibrated mix): a sweep in which every erase cut
    /// meets that state, rather than about one in five.
    CalibratedErase(EraseShape),
}

impl TearModel {
    /// The three original (guessed) models: every driver's default list.
    pub const ALL: [TearModel; 3] = [
        TearModel::Clean,
        TearModel::BytePrefix,
        TearModel::RandomBits,
    ];

    /// Every model a name can select: [`TearModel::ALL`] and the calibrated
    /// one.
    pub const NAMED: [TearModel; 9] = [
        TearModel::Clean,
        TearModel::BytePrefix,
        TearModel::RandomBits,
        TearModel::Calibrated,
        TearModel::CalibratedErase(EraseShape::Zeroing),
        TearModel::CalibratedErase(EraseShape::AllZero),
        TearModel::CalibratedErase(EraseShape::Erasing),
        TearModel::CalibratedErase(EraseShape::ReadsFfWeak),
        TearModel::CalibratedErase(EraseShape::ReadsFf),
    ];

    pub fn name(&self) -> &'static str {
        match self {
            TearModel::Clean => "clean",
            TearModel::BytePrefix => "byte_prefix",
            TearModel::RandomBits => "random_bits",
            TearModel::Calibrated => "calibrated",
            TearModel::CalibratedErase(EraseShape::Zeroing) => "calibrated_zeroing",
            TearModel::CalibratedErase(EraseShape::AllZero) => "calibrated_all_zero",
            TearModel::CalibratedErase(EraseShape::Erasing) => "calibrated_erasing",
            TearModel::CalibratedErase(EraseShape::ReadsFfWeak) => "calibrated_reads_ff_weak",
            TearModel::CalibratedErase(EraseShape::ReadsFf) => "calibrated_reads_ff",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::NAMED.into_iter().find(|t| t.name() == name)
    }
}

/// A power-cut plan: cut after `cut_after` completed operations (counted from
/// the moment the plan is installed), tearing the next one per `tear`.
///
/// Every program page and every sector erase is one operation; reads are not.
/// Erases are torn under every model except [`TearModel::Clean`] (a torn erase
/// leaves a mix of old bytes, `0xFF`, and *weak* bits; under
/// [`TearModel::Calibrated`], the shapes a real part left; see the README).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FaultPlan {
    pub cut_after: Option<u64>,
    pub tear: TearModel,
    pub seed: u64,
}

impl FaultPlan {
    /// No cut.
    pub const fn none() -> Self {
        Self {
            cut_after: None,
            tear: TearModel::Clean,
            seed: 0,
        }
    }

    pub const fn cut(cut_after: u64, tear: TearModel, seed: u64) -> Self {
        Self {
            cut_after: Some(cut_after),
            tear,
            seed,
        }
    }
}

impl Default for FaultPlan {
    fn default() -> Self {
        Self::none()
    }
}
