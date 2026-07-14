use core::time::Duration;

/// Sync-engine scheduling class for a controller operation.
///
/// `ActionClass` folds an op's *preemption* behavior and its *timeout* budget
/// into a single value declared beside the op (see `ControllerOp::action_class`).
/// The client sync-engine actor reads it to decide whether an incoming action
/// preempts an in-flight pull and to build the pull loop's deadline.
///
/// The `deadline` carried by [`ActionClass::Foreground`] and
/// [`ActionClass::Passive`] is a **quiet-gap budget**, not a wall-clock cap: it
/// is the maximum time the pull loop tolerates with *no* streamed frame before
/// it gives up. Every received frame resets it, so a slow-but-progressing
/// multi-frame read never trips it. This makes the seeded values far less
/// punishing than the old wall-clock watchdogs they replace, which timed the
/// whole request regardless of progress.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActionClass {
    /// Recovery / device flows: preempt an in-flight passive pull **and** any
    /// foreground op, and carry no deadline (they own the connection until they
    /// finish). Seeded from the web policy's preemption set:
    /// every `DeviceOp` variant plus `ServerOp::DisconnectServer`.
    Recovery,
    /// Normal foreground ops: preempt a passive pull but not another foreground
    /// op, timed by a quiet-gap `deadline`.
    Foreground {
        /// Quiet-gap budget fed to the pull loop's progress deadline.
        deadline: Duration,
    },
    /// Passive refresh ticks: never preempt anything; preemptable and
    /// coalescable, timed by a quiet-gap `deadline`.
    Passive {
        /// Quiet-gap budget fed to the pull loop's progress deadline.
        deadline: Duration,
    },
}

impl ActionClass {
    /// Whether this class preempts an in-flight passive refresh pull.
    ///
    /// Per the class semantics: [`ActionClass::Recovery`] preempts everything
    /// and [`ActionClass::Foreground`] preempts passive pulls (a user action
    /// should not wait behind a background refresh). [`ActionClass::Passive`]
    /// never preempts.
    ///
    /// This is a slight generalization of the retired web-crate
    /// `action_preempts_passive_refresh`, which special-cased `ProjectOp` to
    /// *not* preempt; the go-forward class model lets any foreground op jump a
    /// passive tick (a background refresh is always safe to drop and re-run).
    pub fn preempts_passive_refresh(self) -> bool {
        matches!(self, ActionClass::Recovery | ActionClass::Foreground { .. })
    }

    /// Whether this class preempts an in-flight foreground action.
    ///
    /// Only [`ActionClass::Recovery`] preempts a foreground action, mirroring
    /// the retired web-crate `action_preempts_foreground_action` (whose
    /// preemption set was exactly the recovery/device ops).
    pub fn preempts_foreground_action(self) -> bool {
        matches!(self, ActionClass::Recovery)
    }

    /// The quiet-gap deadline budget for this class, if any.
    ///
    /// [`ActionClass::Recovery`] returns `None` (it owns the connection and is
    /// not deadline-bounded).
    pub fn deadline(self) -> Option<Duration> {
        match self {
            ActionClass::Recovery => None,
            ActionClass::Foreground { deadline } | ActionClass::Passive { deadline } => {
                Some(deadline)
            }
        }
    }
}

/// Quiet-gap deadline for connect / project-attach / refresh foreground ops.
///
/// Seeded from the web policy's `PROJECT_ACTION_TIMEOUT_MS` (8 s), which covered
/// `ConnectRunningProject`, `ConnectLoadedProject`, and `RefreshProject`.
pub const PROJECT_ACTION_DEADLINE: Duration = Duration::from_secs(8);

/// Quiet-gap deadline for the demo-project load foreground op.
///
/// Seeded from the web policy's `PROJECT_LOAD_TIMEOUT_MS` (20 s).
pub const PROJECT_LOAD_DEADLINE: Duration = Duration::from_secs(20);

/// Quiet-gap deadline for project-editor foreground ops (e.g. `Focus`).
///
/// Seeded from the web policy's `PROJECT_EDITOR_ACTION_TIMEOUT_MS` (6 s).
pub const PROJECT_EDITOR_ACTION_DEADLINE: Duration = Duration::from_secs(6);

/// Quiet-gap deadline for passive refresh ticks.
///
/// The old web policy split this by transport: `SIMULATOR_PASSIVE_REFRESH_TIMEOUT_MS`
/// (4 s) vs `DEVICE_PASSIVE_REFRESH_TIMEOUT_MS` (12 s), both wall-clock. The two
/// collapse to a single quiet-gap budget: the device value (12 s) is the safe
/// default. Because this is now a quiet-gap (per-frame) budget rather than a
/// wall-clock cap, the larger value is far less punishing — a healthy sim read
/// still resets it on every frame, so it does not slow the fast path.
pub const PASSIVE_REFRESH_DEADLINE: Duration = Duration::from_secs(12);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_preempts_everything_and_has_no_deadline() {
        let class = ActionClass::Recovery;

        assert!(class.preempts_passive_refresh());
        assert!(class.preempts_foreground_action());
        assert_eq!(class.deadline(), None);
    }

    #[test]
    fn foreground_preempts_only_passive_and_carries_its_deadline() {
        let class = ActionClass::Foreground {
            deadline: PROJECT_ACTION_DEADLINE,
        };

        assert!(class.preempts_passive_refresh());
        assert!(!class.preempts_foreground_action());
        assert_eq!(class.deadline(), Some(PROJECT_ACTION_DEADLINE));
    }

    #[test]
    fn passive_preempts_nothing_and_carries_its_deadline() {
        let class = ActionClass::Passive {
            deadline: PASSIVE_REFRESH_DEADLINE,
        };

        assert!(!class.preempts_passive_refresh());
        assert!(!class.preempts_foreground_action());
        assert_eq!(class.deadline(), Some(PASSIVE_REFRESH_DEADLINE));
    }

    #[test]
    fn seeded_deadlines_match_retired_web_policy_constants() {
        assert_eq!(PROJECT_ACTION_DEADLINE, Duration::from_millis(8_000));
        assert_eq!(PROJECT_LOAD_DEADLINE, Duration::from_millis(20_000));
        assert_eq!(PROJECT_EDITOR_ACTION_DEADLINE, Duration::from_millis(6_000));
        assert_eq!(PASSIVE_REFRESH_DEADLINE, Duration::from_millis(12_000));
    }
}
