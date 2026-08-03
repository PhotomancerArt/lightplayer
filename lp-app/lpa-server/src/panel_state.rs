//! Panel state persistence: `/.lp/panel.json` (panel.md P10/P11).
//!
//! Latched panel writers outlive a power cycle. The defining scenario is
//! the scarf: dimmed from a phone at 4 a.m., unplugged, replugged — it
//! must come back dim, with **not one bright frame**. That requirement is
//! what fixes the restore seam at project construction (before the first
//! tick, therefore before the first render), not at some later "ready"
//! event.
//!
//! The file lives in the framework-owned `/.lp/` tier inside the project's
//! own (chrooted) filesystem — never in authored artifacts. `/.lp/` is
//! already excluded from the canonical package hash
//! (`lpc_history::is_hashed_path`) and from snapshots, so panel state can
//! never destabilize a hash or show up as a device diff.
//!
//! Keys are `scope-path / channel` — the STABLE identity from
//! [`lpc_engine::node::ScopeRef::persist_path`] (tree paths and authored
//! entry keys, never runtime ids or indices), so state survives reload,
//! reattach, and sibling reorder. An unknown scope path on load is simply
//! dropped: vendoring and renames degrade gracefully rather than failing
//! the boot.
//!
//! Posture is the `/.lp/device.json` one — lenient load (missing,
//! unparseable, or unknown-version file → clean boot, no panic, no
//! migration; alpha bump-and-refuse) and best-effort write (a full flash
//! must never fail a frame).

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

use lpc_engine::Engine;
use lpc_engine::node::ScopeRef;
use lpc_model::{AsLpPath, ChannelName, LpValue};
use lpfs::LpFs;
use serde::{Deserialize, Serialize};

/// Path of the panel-state file inside the project's own filesystem.
pub const PANEL_STATE_PATH: &str = "/.lp/panel.json";

/// Format version of the panel-state file. Bump-and-refuse: a file
/// carrying any other version is ignored wholesale (alpha posture — we
/// never migrate panel state, and a dropped file costs one re-dim).
pub const PANEL_STATE_VERSION: u32 = 1;

/// Minimum spacing between panel-state writes, for flash preservation
/// (panel.md P11): a knob wiggled for a minute writes ~6 times, not once
/// per input event.
pub const PANEL_STATE_WRITE_INTERVAL_MS: u32 = 10_000;

/// The persisted file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PanelStateFile {
    pub version: u32,
    /// Whether panel state keeps saving (panel.md P11 — on by default).
    /// Persisted with the state so the choice itself survives a reboot.
    #[serde(default = "auto_save_default")]
    pub auto_save: bool,
    /// One entry per engaged latching writer. Engagement is implied by
    /// presence; momentary writers (P14) are never written here.
    #[serde(default)]
    pub entries: Vec<PanelStateEntry>,
}

fn auto_save_default() -> bool {
    true
}

/// One persisted panel writer: `scope-path / channel → value`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PanelStateEntry {
    /// The owning scope's stable persist path.
    pub scope: String,
    pub channel: String,
    /// The RAW held value (panel.md P7 — emission clamps, storage never
    /// does, so a range that shrinks and grows back restores exactly).
    pub value: LpValue,
}

/// Project the engine's engaged latching writers into a persistable file.
///
/// Momentary writers are skipped by construction (P14: they despawn, and
/// a deadline that outlived a reboot would be meaningless). A writer whose
/// scope owner has vanished is skipped too — it has no stable key.
pub fn snapshot(engine: &Engine, auto_save: bool) -> PanelStateFile {
    let mut entries: Vec<PanelStateEntry> = engine
        .panel_writers()
        .iter()
        .filter(|(_, writer)| writer.expires_at_ms.is_none())
        .filter_map(|((scope, channel), writer)| {
            Some(PanelStateEntry {
                scope: engine.tree().scope_persist_path(*scope)?,
                channel: channel.0.clone(),
                value: writer.value.clone(),
            })
        })
        .collect();
    // Deterministic on-disk order: the file is diffed by humans and
    // compared by tests, and writer iteration order is an implementation
    // detail of the store.
    entries.sort_by(|a, b| (&a.scope, &a.channel).cmp(&(&b.scope, &b.channel)));
    PanelStateFile {
        version: PANEL_STATE_VERSION,
        auto_save,
        entries,
    }
}

/// Read the panel-state file. Missing, unparseable, or wrong-version →
/// `None` (boot clean).
pub fn read(fs: &dyn LpFs) -> Option<PanelStateFile> {
    let bytes = fs.read_file(PANEL_STATE_PATH.as_path()).ok()?;
    let file = lpc_wire::json::from_slice::<PanelStateFile>(&bytes).ok()?;
    if file.version != PANEL_STATE_VERSION {
        log::warn!(
            "panel state: ignoring /.lp/panel.json with unknown version {} (expected {})",
            file.version,
            PANEL_STATE_VERSION
        );
        return None;
    }
    Some(file)
}

/// Write the panel-state file. Best-effort: a write failure is logged and
/// swallowed — persistence must never fail a frame or a shutdown.
pub fn write(fs: &dyn LpFs, file: &PanelStateFile) {
    let json = match lpc_wire::json::to_string(file) {
        Ok(json) => json,
        Err(error) => {
            log::warn!("panel state: failed to encode /.lp/panel.json: {error:?}");
            return;
        }
    };
    if let Err(error) = fs.write_file(PANEL_STATE_PATH.as_path(), json.as_bytes()) {
        log::warn!("panel state: failed to write /.lp/panel.json: {error}");
    }
}

/// Re-materialize persisted writers into the engine's store.
///
/// Returns the restored `auto_save` preference (defaulting to on when
/// there is no usable file). Entries naming a scope this project no longer
/// has are dropped — that is the graceful-degradation rule, not an error.
pub fn restore(fs: &dyn LpFs, engine: &mut Engine) -> bool {
    let Some(file) = read(fs) else {
        return auto_save_default();
    };
    for entry in &file.entries {
        let Some(scope) = scope_by_persist_path(engine, &entry.scope) else {
            log::debug!(
                "panel state: dropping entry for unknown scope {}",
                entry.scope
            );
            continue;
        };
        engine.panel_write(
            scope,
            ChannelName(entry.channel.clone()),
            entry.value.clone(),
            None,
        );
    }
    log::info!(
        "panel state: restored {} engaged control(s) (auto_save={})",
        file.entries.len(),
        file.auto_save
    );
    file.auto_save
}

/// The scope whose stable persist path is `path`, if this project has one.
fn scope_by_persist_path(engine: &Engine, path: &str) -> Option<ScopeRef> {
    engine
        .tree()
        .scopes()
        .into_iter()
        .find(|scope| engine.tree().scope_persist_path(*scope).as_deref() == Some(path))
}
