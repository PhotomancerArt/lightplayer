//! The recovery facade: boot-time analysis, frame entry, crash lifecycle,
//! and the global instance the panic handler and engine code reach.

use core::cell::RefCell;
use core::ptr::NonNull;

use critical_section::Mutex;

use crate::backend::RecoveryBackend;
use crate::crash_record::{CrashCause, OomStats};
use crate::frame_guard::FrameGuard;
use crate::frame_kind::FrameKind;
use crate::frame_record::fnv1a_32;
use crate::ledger::GatedInfo;
use crate::recovery_level::RecoveryLevel;
use crate::recovery_stack::{clear_stack, current_path, current_path_names, pop_frame, push_frame};
use crate::reset_cause::ResetCause;
use crate::snapshot::{CrashSnapshot, RecoverySnapshot};

/// What `Recovery::init` learned about the previous run, plus the resulting
/// policy inputs (level, safe mode) for this boot.
#[derive(Copy, Clone, Debug)]
pub struct BootAssessment {
    /// Why this boot happened.
    pub cause: ResetCause,
    /// Whether the previous run reached the boot-complete milestone.
    /// `true` when there is no valid evidence to the contrary (fresh
    /// region, power-on): absence of evidence is not a failed boot.
    pub prior_boot_complete: bool,
    /// The crash that ended the previous run, if any.
    pub prior_crash: Option<CrashSnapshot>,
    /// Device level after accounting for the previous run.
    pub level: RecoveryLevel,
    /// Whether this boot should skip crash-prone work (project auto-load):
    /// the previous boots kept dying before the boot-complete milestone.
    pub safe_mode: bool,
}

/// Why `enter` refused a frame.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum EnterDenied {
    /// The frame stack is at [`MAX_FRAME_DEPTH`](crate::MAX_FRAME_DEPTH).
    StackFull,
    /// The path (or a parent of it) is gated red after repeated crashes.
    Gated(GatedInfo),
}

impl core::fmt::Display for EnterDenied {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::StackFull => f.write_str("recovery: frame stack full"),
            Self::Gated(info) => write!(f, "recovery: {info}"),
        }
    }
}

/// Token identifying an entered frame; held by [`FrameGuard`].
#[derive(Copy, Clone, Debug)]
pub struct EnteredFrame {
    pub(crate) name_hash: u32,
    pub(crate) crash_epoch: u32,
}

/// Object-safe surface of [`Recovery`], for the global instance.
///
/// Prefer the free functions ([`enter`], [`stage_crash`], ...) which route
/// through the installed global; this trait exists so the global can be a
/// single fat pointer regardless of the backend type.
pub trait RecoveryHandle {
    fn enter_frame(&mut self, kind: FrameKind, name: &str) -> Result<EnteredFrame, EnterDenied>;
    fn leave_frame(&mut self, token: EnteredFrame);
    fn stage_crash(
        &mut self,
        cause: CrashCause,
        msg: &dyn core::fmt::Display,
        location: Option<(&str, u32)>,
        pcs: &[u32],
        oom: Option<OomStats>,
    );
    fn clear_tentative_crash(&mut self);
    /// A panic was caught in-process (layer 1): void the staged record AND
    /// feed the crash into the blame ledger so repeated in-process crashes
    /// gate their path exactly like reboot-causing ones.
    fn record_recovered_crash(&mut self);
    /// Commit the staged crash (synthesizing an `Unknown` one if nothing is
    /// staged) and ask the backend to reset. On real hardware the backend
    /// diverges; if it returns, the caller must apply its own platform
    /// reset fallback.
    fn finalize_crash_and_reset(&mut self);
    fn mark_boot_complete(&mut self);
    fn snapshot(&mut self) -> RecoverySnapshot;
}

/// The recovery system over a platform backend.
pub struct Recovery<B: RecoveryBackend> {
    backend: B,
    /// Why this boot happened (for reporting).
    boot_cause: ResetCause,
    /// Bumped on every staged crash; frames entered before a crash and
    /// dropped after it are not clean completions.
    crash_epoch: u32,
}

impl<B: RecoveryBackend> Recovery<B> {
    /// Boot-time entry point: validate the region, analyze the previous
    /// run, and open a new boot generation.
    pub fn init(mut backend: B, cause: ResetCause) -> (Self, BootAssessment) {
        let region = backend.region();

        if !region.is_valid(cause) {
            region.reinit();
            region.begin_generation();
            return (
                Self {
                    backend,
                    boot_cause: cause,
                    crash_epoch: 0,
                },
                BootAssessment {
                    cause,
                    prior_boot_complete: true,
                    prior_crash: None,
                    level: RecoveryLevel::Green,
                    safe_mode: false,
                },
            );
        }

        let prev_generation = region.generation();
        let prior_boot_complete = region.boot_complete() || prev_generation == 0;

        // A tentative record means a panic was staged but never committed:
        // either layer-1 recovery caught it (stale staging), or the system
        // hung mid-panic and the watchdog fired. Only the latter is evidence.
        if region.crash().is_tentative() {
            if cause == ResetCause::WatchdogReset {
                region.crash_mut().finalize();
            } else {
                region.crash_mut().clear_tentative();
            }
        }

        // A watchdog reset with no committed record: the leftover stack is
        // the record. Synthesize one from it.
        if cause == ResetCause::WatchdogReset && !region.crash().is_final() {
            let path = current_path(region);
            let names = current_path_names(region);
            region.crash_mut().stage(
                CrashCause::Watchdog,
                &"hang detected by hardware watchdog",
                None,
                &[],
                None,
                path,
                &names[..path.len()],
                prev_generation,
            );
            region.crash_mut().finalize();
        }

        let has_prior_crash = region.crash().is_final()
            && region.crash().generation() == prev_generation
            && cause.blames_code();

        clear_stack(region);
        region.begin_generation();

        // Snapshot after the generation bump so `boots_ago` is relative to
        // the boot that is starting (crash last run => boots_ago == 1).
        let prior_crash = has_prior_crash
            .then(|| CrashSnapshot::from_record(region.crash(), region.generation()));

        // Ledger transitions: demote reds for their one-retry-per-boot,
        // account boot-loop counting, THEN record the prior crash (so a
        // repeat offender goes straight back to red for this run).
        region.ledger_mut().on_boot(prior_boot_complete, cause);
        if let Some(crash) = &prior_crash {
            let record = *region.crash();
            region
                .ledger_mut()
                .record_crash(&crash.path, record.path_names());
        }
        let level = region.ledger().device_level();
        let safe_mode = region.ledger().safe_mode();

        (
            Self {
                backend,
                boot_cause: cause,
                crash_epoch: 0,
            },
            BootAssessment {
                cause,
                prior_boot_complete,
                prior_crash,
                level,
                safe_mode,
            },
        )
    }

    /// Hand the backend (and its region bytes) back — used by tests and
    /// harnesses to simulate a reboot.
    pub fn into_backend(self) -> B {
        self.backend
    }
}

impl<B: RecoveryBackend> RecoveryHandle for Recovery<B> {
    fn enter_frame(&mut self, kind: FrameKind, name: &str) -> Result<EnteredFrame, EnterDenied> {
        let region = self.backend.region();
        // Gate BEFORE touching the stack: a denied entry leaves no trace.
        let mut candidate = current_path(region);
        if !candidate.push(kind, fnv1a_32(name.as_bytes())) {
            return Err(EnterDenied::StackFull);
        }
        if let Some(gated) = region.ledger().check_enter(&candidate) {
            return Err(EnterDenied::Gated(gated));
        }
        let name_hash = push_frame(region, kind, name).ok_or(EnterDenied::StackFull)?;
        Ok(EnteredFrame {
            name_hash,
            crash_epoch: self.crash_epoch,
        })
    }

    fn leave_frame(&mut self, token: EnteredFrame) {
        let region = self.backend.region();
        // The completed path includes the frame being popped.
        let completed = current_path(region);
        let matched = pop_frame(region, token.name_hash);
        debug_assert!(matched, "recovery frames must drop in LIFO order");
        if matched && self.crash_epoch == token.crash_epoch {
            region.ledger_mut().record_clean_completion(&completed);
        }
    }

    fn stage_crash(
        &mut self,
        cause: CrashCause,
        msg: &dyn core::fmt::Display,
        location: Option<(&str, u32)>,
        pcs: &[u32],
        oom: Option<OomStats>,
    ) {
        self.crash_epoch = self.crash_epoch.wrapping_add(1);
        let region = self.backend.region();
        let path = current_path(region);
        let names = current_path_names(region);
        let generation = region.generation();
        region.crash_mut().stage(
            cause,
            msg,
            location,
            pcs,
            oom,
            path,
            &names[..path.len()],
            generation,
        );
    }

    fn clear_tentative_crash(&mut self) {
        self.backend.region().crash_mut().clear_tentative();
    }

    fn record_recovered_crash(&mut self) {
        let region = self.backend.region();
        // Blame the staged record's path (snapshotted at panic time, before
        // unwinding popped the guards); fall back to the live stack.
        let (path, record) = if region.crash().is_tentative() {
            (region.crash().path(), *region.crash())
        } else {
            let mut synthetic = crate::crash_record::CrashRecord::EMPTY;
            let path = current_path(region);
            let names = current_path_names(region);
            synthetic.stage(
                CrashCause::Unknown,
                &"recovered crash (no staged record)",
                None,
                &[],
                None,
                path,
                &names[..path.len()],
                region.generation(),
            );
            (path, synthetic)
        };
        region.ledger_mut().record_crash(&path, record.path_names());
        region.crash_mut().clear_tentative();
    }

    fn finalize_crash_and_reset(&mut self) {
        let region = self.backend.region();
        if !region.crash().is_tentative() && !region.crash().is_final() {
            // Nothing staged: synthesize so the next boot still learns
            // something.
            let path = current_path(region);
            let names = current_path_names(region);
            let generation = region.generation();
            region.crash_mut().stage(
                CrashCause::Unknown,
                &"reset requested with no staged crash",
                None,
                &[],
                None,
                path,
                &names[..path.len()],
                generation,
            );
        }
        region.crash_mut().finalize();
        self.backend.request_reset();
    }

    fn mark_boot_complete(&mut self) {
        self.backend.region().set_boot_complete();
    }

    fn snapshot(&mut self) -> RecoverySnapshot {
        RecoverySnapshot::capture(self.backend.region(), self.boot_cause)
    }
}

// --- Global instance -------------------------------------------------------

/// Slot for the installed global. The raw pointer originates from a
/// `&'static mut`, so it is valid forever and uniquely owned by this slot.
struct GlobalSlot(RefCell<Option<NonNull<dyn RecoveryHandle>>>);

// SAFETY: access to the inner pointer only happens inside a critical
// section (single-core targets / std mutex on hosts), and reentrant access
// is rejected via RefCell::try_borrow_mut. The pointee is never moved.
unsafe impl Send for GlobalSlot {}

static GLOBAL: Mutex<GlobalSlot> = Mutex::new(GlobalSlot(RefCell::new(None)));

/// Install the global recovery instance.
///
/// Firmware calls this once at boot. Calling it again replaces the handle —
/// allowed so tests can install fresh instances, but never do this on a
/// live system with guards outstanding.
pub fn set_global(handle: &'static mut dyn RecoveryHandle) {
    critical_section::with(|cs| {
        *GLOBAL.borrow(cs).0.borrow_mut() = Some(NonNull::from(handle));
    });
}

/// Run `f` against the installed global, if any.
///
/// Returns `None` when no global is installed OR when called reentrantly
/// (e.g. a panic fired inside a recovery operation and the panic handler
/// called back in) — degrading to "no breadcrumb" instead of deadlocking
/// or double-borrowing.
pub(crate) fn with_global<R>(f: impl FnOnce(&mut dyn RecoveryHandle) -> R) -> Option<R> {
    critical_section::with(|cs| {
        let slot = GLOBAL.borrow(cs);
        let borrowed = slot.0.try_borrow_mut().ok()?;
        let mut ptr = (*borrowed)?;
        // SAFETY: see `GlobalSlot` — unique access guaranteed by the
        // critical section plus the RefCell borrow held for `f`'s duration.
        let handle = unsafe { ptr.as_mut() };
        Some(f(handle))
    })
}

/// Whether a global recovery instance is installed.
pub fn is_initialized() -> bool {
    critical_section::with(|cs| {
        GLOBAL
            .borrow(cs)
            .0
            .try_borrow()
            .map(|slot| slot.is_some())
            .unwrap_or(true) // borrowed == installed and busy
    })
}

/// Enter a recovery frame via the global instance.
///
/// With no global installed this returns an inert guard: instrumented code
/// runs identically on targets without recovery.
pub fn enter(kind: FrameKind, name: &str) -> Result<FrameGuard, EnterDenied> {
    match with_global(|r| r.enter_frame(kind, name)) {
        None => Ok(FrameGuard::inert()),
        Some(Ok(token)) => Ok(FrameGuard::active(token)),
        Some(Err(denied)) => Err(denied),
    }
}

/// Stage a crash record (tentative) from panic/OOM context. Zero-alloc.
/// Returns `false` if no global is installed or it was busy (reentrancy).
pub fn stage_crash(
    cause: CrashCause,
    msg: &dyn core::fmt::Display,
    location: Option<(&str, u32)>,
    pcs: &[u32],
    oom: Option<OomStats>,
) -> bool {
    with_global(|r| r.stage_crash(cause, msg, location, pcs, oom)).is_some()
}

/// Void a staged (tentative) crash after layer-1 recovery caught the panic.
pub fn clear_tentative_crash() {
    with_global(|r| r.clear_tentative_crash());
}

/// Record a caught-in-process panic into the blame ledger (and void the
/// staged record). This is what makes repeatedly-crashing nodes go
/// yellow → red without any reboot involved.
pub fn record_recovered_crash() {
    with_global(|r| r.record_recovered_crash());
}

/// Commit the staged crash and request a system reset. Returns `false` if
/// no global is installed (or busy) — the caller must then reset directly.
pub fn finalize_crash_and_reset() -> bool {
    with_global(|r| r.finalize_crash_and_reset()).is_some()
}

/// Mark the boot-complete milestone for this run.
pub fn mark_boot_complete() {
    with_global(|r| r.mark_boot_complete());
}

/// Snapshot recovery state for reporting, if a global is installed.
pub fn snapshot() -> Option<RecoverySnapshot> {
    with_global(|r| r.snapshot())
}

#[cfg(test)]
mod tests {
    extern crate std;
    use std::boxed::Box;
    use std::string::ToString;

    use super::*;
    use crate::in_memory_backend::InMemoryBackend;

    fn boot_fresh() -> (Recovery<InMemoryBackend>, BootAssessment) {
        Recovery::init(InMemoryBackend::new(), ResetCause::PowerOn)
    }

    #[test]
    fn fresh_power_on_boot_is_clean() {
        let (_recovery, assessment) = boot_fresh();
        assert_eq!(assessment.cause, ResetCause::PowerOn);
        assert!(assessment.prior_boot_complete);
        assert!(assessment.prior_crash.is_none());
    }

    #[test]
    fn staged_then_finalized_crash_is_reported_next_boot() {
        let (mut recovery, _) = boot_fresh();
        recovery.mark_boot_complete();
        let _f1 = recovery.enter_frame(FrameKind::Boot, "boot").unwrap();
        let _f2 = recovery
            .enter_frame(FrameKind::NodeRender, "nodes/fire")
            .unwrap();
        recovery.stage_crash(
            CrashCause::Panic,
            &"boom",
            Some(("node.rs", 10)),
            &[0x42],
            None,
        );
        recovery.finalize_crash_and_reset();

        let (_recovery, assessment) = InMemoryBackend::reboot(recovery, ResetCause::SoftwareReset);

        let crash = assessment.prior_crash.expect("crash reported");
        assert_eq!(crash.cause, CrashCause::Panic);
        assert_eq!(crash.msg.as_str(), "boom (at node.rs:10)");
        assert_eq!(crash.boots_ago, 1);
        assert_eq!(crash.path_display().to_string(), "boot/node:nodes/fire");
        assert!(assessment.prior_boot_complete);
    }

    #[test]
    fn finalize_asks_the_backend_to_reset() {
        let (mut recovery, _) = boot_fresh();
        recovery.stage_crash(CrashCause::Panic, &"x", None, &[], None);
        recovery.finalize_crash_and_reset();
        assert!(recovery.into_backend().reset_requested());
    }

    #[test]
    fn watchdog_reset_synthesizes_crash_from_leftover_stack() {
        let (mut recovery, _) = boot_fresh();
        recovery.mark_boot_complete();
        let f1 = recovery.enter_frame(FrameKind::Boot, "boot").unwrap();
        let _f2 = recovery
            .enter_frame(FrameKind::ShaderCompile, "shaders/fire.glsl")
            .unwrap();
        // Hang: no stage, no finalize, no pops. Watchdog fires.
        let _ = f1;
        let (_recovery, assessment) = InMemoryBackend::reboot(recovery, ResetCause::WatchdogReset);

        let crash = assessment.prior_crash.expect("wdt crash attributed");
        assert_eq!(crash.cause, CrashCause::Watchdog);
        assert_eq!(crash.path.len(), 2);
        assert_eq!(
            crash.path_display().to_string(),
            "boot/shader-compile:shaders/fire.g"
        );
    }

    #[test]
    fn tentative_crash_plus_watchdog_is_finalized_as_the_record() {
        let (mut recovery, _) = boot_fresh();
        let _f = recovery.enter_frame(FrameKind::NodeRender, "n").unwrap();
        recovery.stage_crash(CrashCause::Panic, &"panicked then hung", None, &[], None);
        // No finalize — hang during unwinding; WDT fires.
        let (_recovery, assessment) = InMemoryBackend::reboot(recovery, ResetCause::WatchdogReset);
        let crash = assessment.prior_crash.expect("crash reported");
        assert_eq!(crash.cause, CrashCause::Panic);
        assert_eq!(crash.msg.as_str(), "panicked then hung");
    }

    #[test]
    fn stale_tentative_crash_is_cleared_on_normal_boot() {
        let (mut recovery, _) = boot_fresh();
        recovery.mark_boot_complete();
        recovery.stage_crash(CrashCause::Panic, &"recovered later", None, &[], None);
        // Layer-1 caught it but (bug) never cleared; device later resets
        // for an unrelated software reason without committing... the
        // tentative record must not resurrect as blame even though a
        // software reset otherwise blames code.
        let (mut recovery, assessment) =
            InMemoryBackend::reboot(recovery, ResetCause::SoftwareReset);
        assert!(assessment.prior_crash.is_none());
        assert!(recovery.snapshot().last_crash.is_none());
    }

    #[test]
    fn user_reset_and_brownout_do_not_blame() {
        for cause in [ResetCause::UserReset, ResetCause::Brownout] {
            let (mut recovery, _) = boot_fresh();
            recovery.stage_crash(CrashCause::Panic, &"x", None, &[], None);
            recovery.finalize_crash_and_reset();
            let (_r, assessment) = InMemoryBackend::reboot(recovery, cause);
            assert!(
                assessment.prior_crash.is_none(),
                "{cause:?} must not blame code"
            );
        }
    }

    #[test]
    fn incomplete_boot_is_visible_next_boot() {
        let (recovery, _) = boot_fresh();
        // No mark_boot_complete: "crashed during boot".
        let (mut recovery, assessment) =
            InMemoryBackend::reboot(recovery, ResetCause::SoftwareReset);
        assert!(!assessment.prior_boot_complete);

        recovery.mark_boot_complete();
        let (_r, assessment) = InMemoryBackend::reboot(recovery, ResetCause::SoftwareReset);
        assert!(assessment.prior_boot_complete);
    }

    #[test]
    fn power_on_wipes_all_prior_state() {
        let (mut recovery, _) = boot_fresh();
        recovery.stage_crash(CrashCause::Panic, &"x", None, &[], None);
        recovery.finalize_crash_and_reset();
        let (mut recovery, assessment) = InMemoryBackend::reboot(recovery, ResetCause::PowerOn);
        assert!(assessment.prior_crash.is_none());
        assert!(recovery.snapshot().last_crash.is_none());
        assert_eq!(recovery.snapshot().boot_count, 1);
    }

    #[test]
    fn finalize_without_stage_synthesizes_unknown() {
        let (mut recovery, _) = boot_fresh();
        let _f = recovery
            .enter_frame(FrameKind::ProjectLoad, "demo")
            .unwrap();
        recovery.finalize_crash_and_reset();
        let (_r, assessment) = InMemoryBackend::reboot(recovery, ResetCause::SoftwareReset);
        let crash = assessment.prior_crash.expect("synthesized crash");
        assert_eq!(crash.cause, CrashCause::Unknown);
        assert_eq!(crash.path_display().to_string(), "project:demo");
    }

    #[test]
    fn snapshot_reflects_live_stack_and_counters() {
        let (mut recovery, _) = boot_fresh();
        recovery.mark_boot_complete();
        let _f = recovery.enter_frame(FrameKind::Boot, "boot").unwrap();
        let snap = recovery.snapshot();
        assert_eq!(snap.stack_depth, 1);
        assert_eq!(snap.generation, 1);
        assert!(snap.boot_complete);
        assert_eq!(snap.stack[0].name(), "boot");
    }

    // --- P2 blame-ledger scenarios through the full boot/reboot flow ------

    /// Crash on a path in one run → yellow next boot; crash there again →
    /// red at the following boot (gated), then demoted for one retry.
    #[test]
    fn repeat_crash_across_reboots_gates_then_retries() {
        let (mut recovery, _) = boot_fresh();
        recovery.mark_boot_complete();
        let _b = recovery.enter_frame(FrameKind::Boot, "boot").unwrap();
        let _n = recovery
            .enter_frame(FrameKind::NodeRender, "nodes/fire")
            .unwrap();
        recovery.stage_crash(CrashCause::Panic, &"boom", None, &[], None);
        recovery.finalize_crash_and_reset();

        // Boot 2: yellow, path runnable.
        let (mut recovery, assessment) =
            InMemoryBackend::reboot(recovery, ResetCause::SoftwareReset);
        assert_eq!(assessment.level, RecoveryLevel::Yellow);
        recovery.mark_boot_complete();
        let _b = recovery.enter_frame(FrameKind::Boot, "boot").unwrap();
        let _n = recovery
            .enter_frame(FrameKind::NodeRender, "nodes/fire")
            .expect("yellow path may retry");
        recovery.stage_crash(CrashCause::Panic, &"boom again", None, &[], None);
        recovery.finalize_crash_and_reset();

        // Boot 3: red — the path is gated for this whole run.
        let (mut recovery, assessment) =
            InMemoryBackend::reboot(recovery, ResetCause::SoftwareReset);
        assert_eq!(assessment.level, RecoveryLevel::Red);
        let _b = recovery.enter_frame(FrameKind::Boot, "boot").unwrap();
        let denied = recovery
            .enter_frame(FrameKind::NodeRender, "nodes/fire")
            .unwrap_err();
        let EnterDenied::Gated(info) = denied else {
            panic!("expected Gated, got {denied:?}");
        };
        assert_eq!(info.crash_count, 2);
        assert_eq!(info.name(), "nodes/fire");
        // Siblings unaffected.
        assert!(
            recovery
                .enter_frame(FrameKind::NodeRender, "nodes/calm")
                .is_ok()
        );

        // Boot 4 (no new crash): demoted to yellow — one retry allowed.
        recovery.mark_boot_complete();
        let (mut recovery, assessment) =
            InMemoryBackend::reboot(recovery, ResetCause::SoftwareReset);
        assert_eq!(assessment.level, RecoveryLevel::Yellow);
        let _b = recovery.enter_frame(FrameKind::Boot, "boot").unwrap();
        assert!(
            recovery
                .enter_frame(FrameKind::NodeRender, "nodes/fire")
                .is_ok()
        );
    }

    /// Two in-process (caught) crashes gate the path with no reboot at all.
    #[test]
    fn recovered_crashes_gate_in_run() {
        let (mut recovery, _) = boot_fresh();
        recovery.mark_boot_complete();
        let b = recovery.enter_frame(FrameKind::Boot, "boot").unwrap();

        for _ in 0..2 {
            let n = recovery
                .enter_frame(FrameKind::NodeRender, "nodes/bad")
                .unwrap();
            recovery.stage_crash(CrashCause::Panic, &"caught", None, &[], None);
            // Unwind drops the guard (epoch mismatch => not clean)...
            recovery.leave_frame(n);
            // ...and layer-1 reports the recovered crash.
            recovery.record_recovered_crash();
        }

        assert_eq!(recovery.snapshot().level, RecoveryLevel::Red);
        let denied = recovery
            .enter_frame(FrameKind::NodeRender, "nodes/bad")
            .unwrap_err();
        assert!(matches!(denied, EnterDenied::Gated(_)));
        let ok = recovery
            .enter_frame(FrameKind::NodeRender, "nodes/ok")
            .expect("sibling unaffected");
        recovery.leave_frame(ok);
        recovery.leave_frame(b);
    }

    /// a→b→c then a→b→f crashing (across reboots) gates b itself.
    #[test]
    fn escalation_across_reboots_gates_the_parent() {
        let (mut recovery, _) = boot_fresh();
        recovery.mark_boot_complete();
        let _b = recovery.enter_frame(FrameKind::Boot, "boot").unwrap();
        let _p = recovery
            .enter_frame(FrameKind::ProjectLoad, "proj")
            .unwrap();
        let _n = recovery.enter_frame(FrameKind::NodeRender, "c").unwrap();
        recovery.stage_crash(CrashCause::Panic, &"crash c", None, &[], None);
        recovery.finalize_crash_and_reset();

        let (mut recovery, _) = InMemoryBackend::reboot(recovery, ResetCause::SoftwareReset);
        recovery.mark_boot_complete();
        let _b = recovery.enter_frame(FrameKind::Boot, "boot").unwrap();
        let _p = recovery
            .enter_frame(FrameKind::ProjectLoad, "proj")
            .unwrap();
        let _n = recovery.enter_frame(FrameKind::NodeRender, "f").unwrap();
        recovery.stage_crash(CrashCause::Panic, &"crash f", None, &[], None);
        recovery.finalize_crash_and_reset();

        let (mut recovery, assessment) =
            InMemoryBackend::reboot(recovery, ResetCause::SoftwareReset);
        assert_eq!(assessment.level, RecoveryLevel::Red);
        let _b = recovery.enter_frame(FrameKind::Boot, "boot").unwrap();
        let denied = recovery
            .enter_frame(FrameKind::ProjectLoad, "proj")
            .unwrap_err();
        let EnterDenied::Gated(info) = denied else {
            panic!("expected parent gate, got {denied:?}");
        };
        assert_eq!(info.kind, Some(FrameKind::ProjectLoad));
        assert_eq!(info.name(), "proj");
        // A different project is untouched.
        assert!(
            recovery
                .enter_frame(FrameKind::ProjectLoad, "other")
                .is_ok()
        );
    }

    /// Watchdog-attributed crashes feed the ledger like any other.
    #[test]
    fn watchdog_crash_feeds_the_ledger() {
        let (mut recovery, _) = boot_fresh();
        recovery.mark_boot_complete();
        let _b = recovery.enter_frame(FrameKind::Boot, "boot").unwrap();
        let _s = recovery
            .enter_frame(FrameKind::ShaderCompile, "loop.glsl")
            .unwrap();
        // Hang — watchdog fires with the stack live.
        let (mut recovery, assessment) =
            InMemoryBackend::reboot(recovery, ResetCause::WatchdogReset);
        assert_eq!(assessment.level, RecoveryLevel::Yellow);
        let snap = recovery.snapshot();
        assert!(
            snap.path_entries
                .iter()
                .any(|e| !e.is_empty() && e.name() == "loop.glsl"),
            "hung shader path is under watch"
        );
    }

    /// Two boots that die before the milestone put the third in safe mode.
    #[test]
    fn boot_loop_enters_safe_mode() {
        let (recovery, _) = boot_fresh();
        // Boot 1 dies before mark_boot_complete.
        let (recovery, a2) = InMemoryBackend::reboot(recovery, ResetCause::SoftwareReset);
        assert!(!a2.safe_mode);
        // Boot 2 dies too.
        let (recovery, a3) = InMemoryBackend::reboot(recovery, ResetCause::WatchdogReset);
        assert!(a3.safe_mode, "two incomplete boots => safe mode");
        assert!(!a3.prior_boot_complete);

        // Recovering: this boot completes; next boot is normal again.
        let mut recovery = recovery;
        recovery.mark_boot_complete();
        let (_r, a4) = InMemoryBackend::reboot(recovery, ResetCause::SoftwareReset);
        assert!(!a4.safe_mode);
    }

    /// Yellow clears to green after enough clean completions via the
    /// enter/leave flow.
    #[test]
    fn clean_runs_return_to_green() {
        let (mut recovery, _) = boot_fresh();
        recovery.mark_boot_complete();
        let n = recovery.enter_frame(FrameKind::NodeRender, "n").unwrap();
        recovery.stage_crash(CrashCause::Panic, &"x", None, &[], None);
        recovery.leave_frame(n);
        recovery.record_recovered_crash();
        assert_eq!(recovery.snapshot().level, RecoveryLevel::Yellow);

        for _ in 0..crate::tuning::CLEAN_COMPLETIONS_TO_GREEN {
            let n = recovery.enter_frame(FrameKind::NodeRender, "n").unwrap();
            recovery.leave_frame(n);
        }
        assert_eq!(recovery.snapshot().level, RecoveryLevel::Green);
    }

    /// Global-instance behavior lives in ONE test: the global is process-
    /// wide and cargo test runs threads in parallel; a single test avoids
    /// cross-test interference by construction.
    #[test]
    fn global_instance_end_to_end() {
        // Not installed: inert behavior.
        assert!(snapshot().is_none());
        let guard = enter(FrameKind::Boot, "boot").unwrap();
        assert!(!guard.is_active());
        drop(guard);
        assert!(!stage_crash(CrashCause::Panic, &"x", None, &[], None));
        assert!(!finalize_crash_and_reset());

        // Installed: full flow.
        let (recovery, _) = Recovery::init(InMemoryBackend::new(), ResetCause::PowerOn);
        set_global(Box::leak(Box::new(recovery)));
        assert!(is_initialized());
        mark_boot_complete();

        let g1 = enter(FrameKind::Boot, "boot").unwrap();
        assert!(g1.is_active());
        let g2 = enter(FrameKind::NodeRender, "nodes/a").unwrap();
        assert_eq!(snapshot().unwrap().stack_depth, 2);

        assert!(stage_crash(CrashCause::Panic, &"caught", None, &[], None));
        clear_tentative_crash();

        drop(g2);
        drop(g1);
        let snap = snapshot().unwrap();
        assert_eq!(snap.stack_depth, 0);
        assert!(
            snap.last_crash.is_none(),
            "cleared tentative leaves no committed crash"
        );

        // Stack-full denial via the global path.
        let mut guards = std::vec::Vec::new();
        for _ in 0..crate::MAX_FRAME_DEPTH {
            guards.push(enter(FrameKind::NodeRender, "deep").unwrap());
        }
        assert_eq!(
            enter(FrameKind::NodeRender, "too-deep").unwrap_err(),
            EnterDenied::StackFull
        );
        drop(guards);
    }
}
