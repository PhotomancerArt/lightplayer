//! The recovery record's half of `old_startup_project.rs`: a newer engine's
//! startup load that refuses an older project ends its load record, so the
//! next boot does not take the refusal for a load that reset the board (and
//! skip the startup project, `lp-recovery`'s `InterruptedLoad`).
//!
//! Its own test binary with one test: the recovery global is process-wide,
//! and another test's load would write the same record.

extern crate alloc;

use alloc::boxed::Box;
use alloc::rc::Rc;
use alloc::sync::Arc;
use core::cell::RefCell;
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use lp_gfx_lpvm::TargetLpvmGraphics;
use lp_recovery::{
    CrashCause, EnterDenied, EnteredFrame, FrameKind, InMemoryBackend, OomStats, Recovery,
    RecoveryHandle, RecoverySnapshot, ResetCause,
};
use lpa_server::{LpGraphics, LpServer};
use lpc_model::AsLpPath;
use lpc_shared::output::MemoryOutputProvider;
use lpfs::{LpFs, LpFsMemory};

/// The recovery the global points at, so the test can take it back and
/// reboot it.
static RECOVERY: Mutex<Option<Recovery<InMemoryBackend>>> = Mutex::new(None);

/// Loads begun through the global, so the test knows the server wrote one.
static BEGUN: AtomicUsize = AtomicUsize::new(0);

#[test]
fn a_refused_old_startup_project_ends_its_load_record() {
    // The control: a load that began and never ended is what the next
    // boot (after a watchdog) reports, so the check below can see one.
    let (mut recovery, _) = Recovery::init(InMemoryBackend::new(), ResetCause::PowerOn);
    recovery.begin_load("eaves", "", true);
    let (_, assessment) = InMemoryBackend::reboot(recovery, ResetCause::WatchdogReset);
    assert!(
        assessment.interrupted_load.is_some(),
        "the control: an unfinished load is reported"
    );

    let (recovery, _) = Recovery::init(InMemoryBackend::new(), ResetCause::PowerOn);
    *RECOVERY.lock().unwrap() = Some(recovery);
    lp_recovery::set_global(Box::leak(Box::new(Global)));

    let fs = LpFsMemory::new();
    let corpus = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../lpa-upgrade/tests/corpus/v11/eaves-wired/project.json");
    fs.write_file(
        "/projects/eaves/project.json".as_path(),
        &std::fs::read(corpus).unwrap(),
    )
    .unwrap();
    let mut server = LpServer::new(
        Rc::new(RefCell::new(MemoryOutputProvider::new())),
        Box::new(fs),
        "projects".as_path(),
        None,
        None,
        Arc::new(TargetLpvmGraphics::new(lpa_server::DEVICE_SHADER_FRONTEND))
            as Arc<dyn LpGraphics>,
    );
    let error = server
        .load_startup_project("/projects/eaves".as_path())
        .expect_err("format 11 is refused");
    assert!(
        error.to_string().contains("unsupported project format 11"),
        "{error}"
    );

    assert_eq!(
        BEGUN.load(Ordering::SeqCst),
        1,
        "the startup load wrote its record through the global"
    );

    // Whatever happens next — even a watchdog — the next boot finds no
    // load left open: the startup project is tried again, by a host's
    // upgrade or a reboot after it.
    let recovery = RECOVERY.lock().unwrap().take().expect("installed");
    let (_, assessment) = InMemoryBackend::reboot(recovery, ResetCause::WatchdogReset);
    assert_eq!(assessment.interrupted_load, None);
}

/// A [`RecoveryHandle`] over [`RECOVERY`].
struct Global;

fn with<R>(f: impl FnOnce(&mut Recovery<InMemoryBackend>) -> R) -> R {
    f(RECOVERY
        .lock()
        .unwrap()
        .as_mut()
        .expect("the recovery is installed while the server runs"))
}

impl RecoveryHandle for Global {
    fn enter_frame(&mut self, kind: FrameKind, name: &str) -> Result<EnteredFrame, EnterDenied> {
        with(|r| r.enter_frame(kind, name))
    }
    fn leave_frame(&mut self, token: EnteredFrame) {
        with(|r| r.leave_frame(token));
    }
    fn stage_crash(
        &mut self,
        cause: CrashCause,
        msg: &dyn core::fmt::Display,
        location: Option<(&str, u32)>,
        pcs: &[u32],
        oom: Option<OomStats>,
    ) {
        with(|r| r.stage_crash(cause, msg, location, pcs, oom));
    }
    fn clear_tentative_crash(&mut self) {
        with(|r| r.clear_tentative_crash());
    }
    fn commit_staged_crash(&mut self) {
        with(|r| r.commit_staged_crash());
    }
    fn record_recovered_crash(&mut self) {
        with(|r| r.record_recovered_crash());
    }
    fn finalize_crash_and_reset(&mut self) {
        with(|r| r.finalize_crash_and_reset());
    }
    fn mark_boot_complete(&mut self) {
        with(|r| r.mark_boot_complete());
    }
    fn clear_ledger(&mut self) {
        with(|r| r.clear_ledger());
    }
    fn snapshot(&mut self) -> RecoverySnapshot {
        with(|r| r.snapshot())
    }
    fn begin_load(&mut self, target: &str, previous: &str, at_startup: bool) {
        BEGUN.fetch_add(1, Ordering::SeqCst);
        with(|r| r.begin_load(target, previous, at_startup));
    }
    fn end_load(&mut self) {
        with(|r| r.end_load());
    }
}
