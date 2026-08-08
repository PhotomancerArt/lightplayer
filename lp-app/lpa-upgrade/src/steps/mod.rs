//! The ordered chain of per-version migration steps.
//!
//! Blender's `do_versions`, not Minecraft's DataFixerUpper: one plain
//! function per version bump, run in order, rewriting JSON in place. The
//! architecture is worth stealing; the framework is not.
//!
//! Adding a step is the only correct response to a format bump — see the
//! crate README and `just format-bump`.

pub(crate) mod v4_to_v5;
pub(crate) mod v5_to_v6;

use crate::project_files::ProjectFiles;
use crate::upgrade_error::UpgradeError;
use crate::upgrade_report::UpgradeReport;

pub(crate) type StepFn = fn(&mut ProjectFiles, &mut UpgradeReport) -> Result<(), UpgradeError>;

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
];
