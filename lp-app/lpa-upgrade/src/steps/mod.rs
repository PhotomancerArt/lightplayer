//! The ordered chain of per-version migration steps.
//!
//! Blender's `do_versions`, not Minecraft's DataFixerUpper: one plain
//! function per version bump, run in order, rewriting JSON in place. The
//! architecture is worth stealing; the framework is not.
//!
//! Adding a step is the only correct response to a format bump — see the
//! crate README and `just format-bump`.

pub(crate) mod v10_to_v11;
pub(crate) mod v11_to_v12;
pub(crate) mod v4_to_v5;
pub(crate) mod v5_to_v6;
pub(crate) mod v6_to_v7;
pub(crate) mod v7_to_v8;
pub(crate) mod v8_to_v9;
pub(crate) mod v9_to_v10;

use crate::project_files::ProjectFiles;
use crate::upgrade_error::UpgradeError;
use crate::upgrade_report::UpgradeReport;

/// A step's body: rewrite `files` in place from the step's `from` format to
/// its `to`, recording every changed, added and removed file in the report.
/// The chain runner works on a copy and drops it on a refusal, so a step may
/// return early without undoing anything.
pub type StepFn = fn(&mut ProjectFiles, &mut UpgradeReport) -> Result<(), UpgradeError>;

/// One `from → to` migration.
#[derive(Clone, Copy)]
pub struct UpgradeStep {
    /// The format this step reads.
    pub from: u32,
    /// The format it writes. Always `from + 1`: the chain is dense so every
    /// bump is forced to declare what it changed.
    pub to: u32,
    pub(crate) apply: StepFn,
}

impl UpgradeStep {
    /// A step outside this crate's own chain. The chain runner
    /// ([`crate::upgrade_with_chain`]) knows nothing about what any step
    /// does, so a caller's chain (this crate's [`crate::upgrade_steps`] with
    /// a later step after it, say) upgrades through exactly the path the
    /// shipped chain does. Tests use it to prove that path general.
    pub const fn new(from: u32, to: u32, apply: StepFn) -> Self {
        Self { from, to, apply }
    }
}

impl std::fmt::Debug for UpgradeStep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "UpgradeStep(v{} → v{})", self.from, self.to)
    }
}

pub(crate) const STEPS: &[UpgradeStep] = &[
    UpgradeStep {
        from: 4,
        to: 5,
        apply: v4_to_v5::apply,
    },
    UpgradeStep {
        from: 5,
        to: 6,
        apply: v5_to_v6::apply,
    },
    UpgradeStep {
        from: 6,
        to: 7,
        apply: v6_to_v7::apply,
    },
    UpgradeStep {
        from: 7,
        to: 8,
        apply: v7_to_v8::apply,
    },
    UpgradeStep {
        from: 8,
        to: 9,
        apply: v8_to_v9::apply,
    },
    UpgradeStep {
        from: 9,
        to: 10,
        apply: v9_to_v10::apply,
    },
    UpgradeStep {
        from: 10,
        to: 11,
        apply: v10_to_v11::apply,
    },
    UpgradeStep {
        from: 11,
        to: 12,
        apply: v11_to_v12::apply,
    },
];
