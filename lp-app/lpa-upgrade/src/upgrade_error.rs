//! Why an upgrade did not happen.

use crate::format_class::FormatClass;

/// An upgrade that did not run, or ran into a shape it will not guess at.
///
/// There is deliberately no "partially upgraded" outcome: a step either
/// transforms every file it recognizes or the whole run fails, so a caller
/// never has to reason about a half-migrated package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpgradeError {
    /// The project's format is not in the upgradable range — including
    /// [`FormatClass::Current`], which needs no upgrade. Callers classify
    /// first; this is the guard, not the report.
    NotUpgradable(FormatClass),
    /// A file references something the step understands the *meaning* of but
    /// not this *spelling* of, so it refuses rather than guessing. Loud by
    /// design: a wrong guess here silently changes what a project looks like.
    Refused { file: String, reason: String },
    /// A file that must be JSON is not.
    Malformed { file: String, detail: String },
}

impl std::fmt::Display for UpgradeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotUpgradable(class) => write!(f, "cannot upgrade: {}", class.describe()),
            Self::Refused { file, reason } => write!(
                f,
                "{file}: this project uses a shape the upgrader will not change on its own \
                 ({reason}). Fix it by hand, then re-open the project."
            ),
            Self::Malformed { file, detail } => write!(f, "{file}: not readable JSON ({detail})"),
        }
    }
}

impl std::error::Error for UpgradeError {}
