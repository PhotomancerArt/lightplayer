//! Panel state persistence (panel.md P10/P11, roadmap P10).
//!
//! The requirement these tests exist for, verbatim from the design: *4
//! a.m., Burning Man, LED scarf dimmed from a phone; unplug, replug — it
//! must come back dim, with not one bright frame.* So the interesting
//! assertion is never "the value came back eventually" but "the value was
//! already there before the first tick".
//!
//! Per settled D-B, persistence is device-first and both sim tiers run on
//! `LpFsMemory`; these unit tests ARE the correctness story, and the
//! device walk (G4) is the confirmation.

extern crate alloc;

use alloc::boxed::Box;
use alloc::rc::Rc;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use core::cell::RefCell;

use lp_gfx_lpvm::TargetLpvmGraphics;
use lpa_server::panel_state::{PANEL_STATE_PATH, PANEL_STATE_WRITE_INTERVAL_MS, PanelStateFile};
use lpa_server::{LpGraphics, LpServer, Project};
use lpc_model::{AsLpPath, LpPath, LpPathBuf, LpValue};
use lpc_shared::output::MemoryOutputProvider;
use lpc_wire::{
    BindingGraphProbeRequest, BindingGraphProbeResult, WireBindingGraph, WirePanelClearRequest,
    WirePanelWriteRequest, WireProjectHandle, WireScopeRef,
};
use lpfs::{LpFs, LpFsMemory, LpFsView};

/// One tick shorter than the throttle window, so a test can step right up
/// to the boundary without crossing it.
const ALMOST_THE_WINDOW: u32 = PANEL_STATE_WRITE_INTERVAL_MS - 16;

#[test]
fn a_latched_value_survives_a_reboot_and_is_there_before_the_first_frame() {
    let mut harness = Harness::new("panel-persist-round-trip");
    harness.load();
    harness.write_panel("time", 42.5);
    harness.advance(PANEL_STATE_WRITE_INTERVAL_MS);

    let saved = harness.state_file().expect("panel state written");
    assert_eq!(saved.entries.len(), 1, "one engaged control persisted");
    assert_eq!(saved.entries[0].channel, "time");
    assert_eq!(saved.entries[0].value, LpValue::F32(42.5));

    // Reboot: a brand-new server over the SAME filesystem, exactly as a
    // replug rebuilds everything from flash.
    let mut rebooted = harness.reboot();
    // No tick yet — this is the pre-first-frame moment the scarf cares
    // about. The value must already be resolving.
    assert_eq!(
        rebooted.channel_value("time"),
        Some(LpValue::F32(42.5)),
        "the held value resolves BEFORE the first frame, not after it"
    );
}

#[test]
fn reload_restores_identically() {
    // `reload()` rebuilds the Engine (and with it an empty writer store),
    // so it needs the same restore as construction.
    let mut harness = Harness::new("panel-persist-reload");
    harness.load();
    harness.write_panel("time", 7.25);
    harness.advance(PANEL_STATE_WRITE_INTERVAL_MS);

    harness.project().reload().expect("reload");
    assert_eq!(
        harness.channel_value("time"),
        Some(LpValue::F32(7.25)),
        "reload restores the held value"
    );
}

#[test]
fn an_ordinary_edit_does_not_disturb_engaged_writers() {
    // `apply_project_changes` does NOT rebuild the Engine, so the live
    // store survives editing without a file round-trip. This is the
    // reason panel writers are a side store rather than bindings.
    let mut harness = Harness::new("panel-persist-edit");
    harness.load();
    harness.write_panel("time", 3.5);

    harness.touch_artifact();
    harness.advance(16);

    assert_eq!(harness.channel_value("time"), Some(LpValue::F32(3.5)));
}

#[test]
fn writes_are_throttled_and_an_idle_project_writes_nothing() {
    let mut harness = Harness::new("panel-persist-throttle");
    harness.load();

    // A drag: many writes well inside one window.
    for step in 0..20 {
        harness.write_panel("time", step as f32);
        harness.advance(16);
    }
    assert!(
        harness.state_file().is_none(),
        "no write before the throttle window elapses"
    );

    harness.advance(PANEL_STATE_WRITE_INTERVAL_MS);
    let first = harness.state_file().expect("one write after the window");
    assert_eq!(first.entries[0].value, LpValue::F32(19.0), "latest value");

    // Idle: the window elapses repeatedly with nothing engaged or moved.
    // Flash must not be touched at all.
    harness.delete_state_file();
    for _ in 0..5 {
        harness.advance(PANEL_STATE_WRITE_INTERVAL_MS);
    }
    assert!(
        harness.state_file().is_none(),
        "an idle project never rewrites panel state"
    );
}

#[test]
fn a_clean_shutdown_flushes_inside_the_window() {
    let mut harness = Harness::new("panel-persist-flush");
    harness.load();
    harness.write_panel("time", 5.5);
    harness.advance(ALMOST_THE_WINDOW);
    assert!(
        harness.state_file().is_none(),
        "still inside the throttle window"
    );

    harness
        .server
        .project_manager_mut()
        .unload_all_projects()
        .expect("unload");

    let flushed = harness.state_file().expect("shutdown flushes");
    assert_eq!(flushed.entries[0].value, LpValue::F32(5.5));
}

#[test]
fn auto_save_off_stops_writing_and_the_choice_itself_persists() {
    let mut harness = Harness::new("panel-persist-auto-save");
    harness.load();
    harness.project().set_panel_auto_save(false);
    let recorded = harness
        .state_file()
        .expect("turning it off records the choice");
    assert!(!recorded.auto_save);

    // With auto-save off, engaging a control changes nothing on disk —
    // the file keeps the snapshot taken when it was switched off.
    harness.write_panel("time", 9.0);
    harness.advance(PANEL_STATE_WRITE_INTERVAL_MS * 3);
    assert_eq!(
        harness.state_file().expect("the file still exists"),
        recorded,
        "auto-save off writes nothing, however long it runs"
    );

    // And the preference outlives a reboot — otherwise "off" would silently
    // turn itself back on overnight.
    let rebooted = harness.reboot();
    assert!(!rebooted.project_ref().panel_auto_save());
}

#[test]
fn clearing_a_control_removes_its_persisted_entry() {
    let mut harness = Harness::new("panel-persist-clear");
    harness.load();
    let scope = harness.scope();
    harness.write_panel("time", 4.0);
    harness.advance(PANEL_STATE_WRITE_INTERVAL_MS);
    assert_eq!(harness.state_file().expect("written").entries.len(), 1);

    harness
        .project()
        .panel_clear(&WirePanelClearRequest::Channel {
            scope,
            channel: "time".to_string(),
        });
    harness.advance(PANEL_STATE_WRITE_INTERVAL_MS);

    assert!(
        harness.state_file().expect("rewritten").entries.is_empty(),
        "a cleared control leaves no persisted entry behind"
    );
}

#[test]
fn momentary_writers_are_never_persisted() {
    // P14: a gesture has no held value, and a deadline that outlived a
    // reboot would be meaningless.
    let mut harness = Harness::new("panel-persist-momentary");
    harness.load();
    let scope = harness.scope();
    harness.project().panel_write(&WirePanelWriteRequest {
        scope,
        channel: "time".to_string(),
        value: LpValue::F32(1.0),
        ttl_ms: Some(PANEL_STATE_WRITE_INTERVAL_MS * 10),
    });
    harness.advance(PANEL_STATE_WRITE_INTERVAL_MS);

    let saved = harness.state_file().expect("a write happened");
    assert!(
        saved.entries.is_empty(),
        "the momentary writer is engaged but not persisted: {saved:?}"
    );
}

#[test]
fn a_corrupt_or_wrong_version_file_boots_clean() {
    for (label, body) in [
        ("corrupt", &b"{not json at all"[..]),
        ("wrong version", br#"{"version":999,"auto_save":true,"entries":[]}"#),
        ("empty", b""),
        (
            "unknown scope",
            br#"{"version":1,"auto_save":true,"entries":[{"scope":"/gone.show/nope","channel":"time","value":{"F32":1.0}}]}"#,
        ),
    ] {
        let mut harness = Harness::new(&format!("panel-persist-lenient-{}", label.replace(' ', "-")));
        harness.write_state_file(body);
        harness.load();
        // Booted, ticking, and showing authored behavior — no panic, and
        // no half-applied state.
        harness.advance(16);
        assert!(
            harness.channel_value("time").is_some(),
            "{label}: the clock still drives bus:time"
        );
        assert_ne!(
            harness.channel_value("time"),
            Some(LpValue::F32(1.0)),
            "{label}: nothing was restored from an unusable file"
        );
    }
}

#[test]
fn writing_panel_state_does_not_rebuild_the_project() {
    // THE prerequisite (roadmap P10): a write inside the project fs fires
    // an FsEvent straight back at the refresh path. Without the framework
    // -tier exclusion, every ~10 s save would clear and re-register the
    // whole binding graph — and the rebuild would schedule the next save.
    let mut harness = Harness::new("panel-persist-no-rebuild");
    harness.load();
    harness.advance(16);
    let before = harness.project_ref().applied_refresh_count();

    harness.write_panel("time", 2.0);
    harness.advance(PANEL_STATE_WRITE_INTERVAL_MS);
    assert!(harness.state_file().is_some(), "the state file was written");
    // Two more frames for the fs event to be picked up and routed.
    harness.advance(16);
    harness.advance(16);

    assert_eq!(
        harness.project_ref().applied_refresh_count(),
        before,
        "a /.lp/ write must never reach apply_project_changes"
    );

    // ...while an authored write still does, so the exclusion is not just
    // a broken refresh path.
    harness.touch_artifact();
    harness.advance(16);
    harness.advance(16);
    assert!(
        harness.project_ref().applied_refresh_count() > before,
        "an authored artifact change still rebuilds"
    );
}

#[test]
fn panel_state_inherits_the_framework_tier_exclusions() {
    // `/.lp/` is already outside the canonical package hash and outside
    // snapshots; panel.json must inherit both, or a dimmed scarf would
    // read as a modified project and show up as a device diff.
    assert!(!lpc_history::hash::is_hashed_path(LpPath::new(
        PANEL_STATE_PATH
    )));
    assert!(PANEL_STATE_PATH.starts_with("/.lp/"));
}

// ---------------------------------------------------------------------------

struct Harness {
    server: LpServer,
    project_path: LpPathBuf,
    /// The one filesystem every server in a test shares — so a "reboot"
    /// can be a brand-new server over the same bytes.
    base_fs: Rc<RefCell<dyn LpFs>>,
    handle: Option<WireProjectHandle>,
    name: String,
}

impl Harness {
    fn new(name: &str) -> Self {
        let base_fs: Rc<RefCell<dyn LpFs>> = Rc::new(RefCell::new(LpFsMemory::new()));
        let server = build_server(base_fs.clone());
        let project_path = LpPathBuf::from("/projects").join(name);
        let harness = Self {
            server,
            project_path,
            base_fs,
            handle: None,
            name: name.to_string(),
        };
        harness.write_project_files();
        harness
    }

    /// A fresh server over the same filesystem — a power cycle.
    fn reboot(&self) -> Harness {
        let mut rebooted = Harness {
            server: build_server(self.base_fs.clone()),
            project_path: self.project_path.clone(),
            base_fs: self.base_fs.clone(),
            handle: None,
            name: self.name.clone(),
        };
        rebooted.load();
        rebooted
    }

    fn load(&mut self) {
        let handle = self
            .server
            .load_project(self.project_path.as_path())
            .expect("load");
        self.handle = Some(handle);
    }

    fn advance(&mut self, delta_ms: u32) {
        self.server.advance_frame(delta_ms).expect("tick");
    }

    fn project(&mut self) -> &mut Project {
        let handle = self.handle.expect("loaded");
        self.server
            .project_manager_mut()
            .get_project_mut(handle)
            .expect("loaded project")
    }

    fn project_ref(&self) -> &Project {
        let handle = self.handle.expect("loaded");
        self.server
            .project_manager()
            .get_project(handle)
            .expect("loaded project")
    }

    fn scope(&mut self) -> WireScopeRef {
        let graph = self.probe();
        graph
            .channels
            .iter()
            .find(|channel| channel.name == "time")
            .expect("clock publishes bus:time")
            .scope
            .expect("channels are scoped")
    }

    fn write_panel(&mut self, channel: &str, value: f32) {
        let scope = self.scope();
        self.project().panel_write(&WirePanelWriteRequest {
            scope,
            channel: channel.to_string(),
            value: LpValue::F32(value),
            ttl_ms: None,
        });
    }

    fn probe(&mut self) -> WireBindingGraph {
        let (engine, registry) = self.project().runtime_read_parts();
        let result = engine.read_project_binding_graph_probe(
            registry,
            BindingGraphProbeRequest {
                include_values: true,
            },
        );
        let BindingGraphProbeResult::Graph(graph) = result else {
            panic!("expected graph result");
        };
        graph
    }

    fn channel_value(&mut self, channel: &str) -> Option<LpValue> {
        self.probe()
            .channels
            .iter()
            .find(|c| c.name == channel)
            .and_then(|c| c.value.as_ref())
            .and_then(|value| value.value.clone())
    }

    fn state_path(&self) -> LpPathBuf {
        self.project_path.join(".lp").join("panel.json")
    }

    fn state_file(&self) -> Option<PanelStateFile> {
        let bytes = self
            .base_fs
            .borrow()
            .read_file(self.state_path().as_path())
            .ok()?;
        Some(lpc_wire::json::from_slice::<PanelStateFile>(&bytes).expect("state file parses"))
    }

    fn write_state_file(&self, body: &[u8]) {
        self.base_fs
            .borrow()
            .write_file(self.state_path().as_path(), body)
            .expect("write state file");
    }

    fn delete_state_file(&self) {
        let _ = self
            .base_fs
            .borrow()
            .delete_file(self.state_path().as_path());
    }

    /// Touch an authored artifact — a real project change, for contrast
    /// with a framework-tier write.
    fn touch_artifact(&self) {
        self.base_fs
            .borrow()
            .write_file(
                self.project_path.join("clock.json").as_path(),
                br#"{"kind":"Clock","transport":{"rate":2.0}}"#,
            )
            .expect("write clock");
    }

    fn write_project_files(&self) {
        let fs = self.base_fs.borrow();
        fs.write_file(
            self.project_path.join("project.json").as_path(),
            b"{\n  \"format\": 9\n}\n",
        )
        .expect("write container manifest");
        fs.write_file(
            self.project_path.join("module.json").as_path(),
            br#"{"kind":"Module","nodes":{"clock":{"ref":"./clock.json"}}}"#,
        )
        .expect("write module");
        fs.write_file(
            self.project_path.join("clock.json").as_path(),
            br#"{"kind":"Clock","transport":{"rate":1.0}}"#,
        )
        .expect("write clock");
    }
}

fn build_server(base_fs: Rc<RefCell<dyn LpFs>>) -> LpServer {
    let output_provider = Rc::new(RefCell::new(MemoryOutputProvider::new()));
    let graphics: Arc<dyn LpGraphics> =
        Arc::new(TargetLpvmGraphics::new(lpa_server::DEVICE_SHADER_FRONTEND));
    // The server owns its filesystem, but these tests need to read the
    // same bytes afterwards and hand them to a second server — so each
    // one gets a transparent shared VIEW rather than its own storage.
    LpServer::new(
        output_provider,
        Box::new(LpFsView::new(base_fs, LpPath::new("/"))),
        "projects".as_path(),
        None,
        None,
        graphics,
    )
}
