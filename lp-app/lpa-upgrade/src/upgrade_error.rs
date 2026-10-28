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
    /// Asked to upgrade to a format past the last step this build knows
    /// (`tip`): a board whose engine reads a newer format than this host.
    /// The remedy is a newer host, never a guess.
    TargetBeyondChain { target: u32, tip: u32 },
    /// The project states a format newer than the target: saved by a newer
    /// LightPlayer than the one that is to read it (a board put back on an
    /// older release, say). There is no step backwards.
    NewerThanTarget { found: u32, target: u32 },
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
            Self::TargetBeyondChain { target, tip } => write!(
                f,
                "This LightPlayer upgrades projects up to format {tip}, and the board reads \
                 format {target}. Update lp-cli or reload Studio to finish this board."
            ),
            Self::NewerThanTarget { found, target } => write!(
                f,
                "This project was saved by a newer LightPlayer (project format {found}) than \
                 the board runs (format {target}). Update the board's firmware to open it."
            ),
        }
    }
}

impl std::error::Error for UpgradeError {}
