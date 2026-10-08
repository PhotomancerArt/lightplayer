//! When to cut power and how the in-flight operation tears.

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
}

impl TearModel {
    pub const ALL: [TearModel; 3] = [
        TearModel::Clean,
        TearModel::BytePrefix,
        TearModel::RandomBits,
    ];

    pub fn name(&self) -> &'static str {
        match self {
            TearModel::Clean => "clean",
            TearModel::BytePrefix => "byte_prefix",
            TearModel::RandomBits => "random_bits",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.name() == name)
    }
}

/// A power-cut plan: cut after `cut_after` completed operations (counted from
/// the moment the plan is installed), tearing the next one per `tear`.
///
/// Every program page and every sector erase is one operation; reads are not.
/// Erases are torn under every model except [`TearModel::Clean`] (a torn erase
/// leaves a mix of old bytes, `0xFF`, and *weak* bits; see the README).
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
