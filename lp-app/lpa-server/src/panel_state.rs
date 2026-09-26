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

use alloc::format;
use alloc::string::ToString;

use lpc_engine::Engine;
use lpc_engine::node::ScopeRef;
use lpc_model::{AsLpPath, ChannelName, LpValue, NodeId};
use lpc_registry::ProjectRegistry;
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
///
/// Writers of a dormant playlist entry are kept (multi-pattern vision
/// D12): a sink-scope writer's owner is the playlist, which stays, and a
/// writer whose scope left with the entry is parked by its persist path,
/// which is exactly the key this file uses.
pub fn snapshot(engine: &Engine, auto_save: bool) -> PanelStateFile {
    let live = engine
        .panel_writers()
        .iter()
        .filter(|(_, writer)| writer.expires_at_ms.is_none())
        .filter_map(|((scope, channel), writer)| {
            Some(PanelStateEntry {
                scope: engine.tree().scope_persist_path(*scope)?,
                channel: channel.0.clone(),
                value: writer.value.clone(),
            })
        });
    let parked = engine
        .panel_writers()
        .parked()
        .map(|((path, channel), value)| PanelStateEntry {
            scope: path.clone(),
            channel: channel.0.clone(),
            value: value.clone(),
        });
    let mut entries: Vec<PanelStateEntry> = live.chain(parked).collect();
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
///
/// A playlist entry that is dormant at boot is not in the tree, but its
/// knobs are still the project's (multi-pattern vision D12). So restore
/// also accepts the sink scope of every AUTHORED entry of each live
/// playlist, read from the playlist's def, and parks — by persist path —
/// entries for scopes owned by nodes inside a dormant entry; those engage
/// when the entry loads ([`Engine::apply_residency`]).
pub fn restore(fs: &dyn LpFs, engine: &mut Engine, registry: &ProjectRegistry) -> bool {
    let Some(file) = read(fs) else {
        return auto_save_default();
    };
    let playlists = authored_playlist_entries(engine, registry);
    let mut restored = 0usize;
    for entry in &file.entries {
        let channel = ChannelName(entry.channel.clone());
        if let Some(scope) = scope_by_persist_path(engine, &playlists, &entry.scope) {
            engine.panel_write(scope, channel, entry.value.clone(), None);
        } else if is_under_dormant_entry(&playlists, &entry.scope) {
            engine.panel_park(entry.scope.clone(), channel, entry.value.clone());
        } else {
            log::debug!(
                "panel state: dropping entry for unknown scope {}",
                entry.scope
            );
            continue;
        }
        restored += 1;
    }
    log::info!(
        "panel state: restored {} of {} engaged control(s) (auto_save={})",
        restored,
        file.entries.len(),
        file.auto_save
    );
    file.auto_save
}

/// One live playlist and the entries its def authors: `(playlist node,
/// its tree path, [(entry key, entry child name, resident)])`.
struct AuthoredPlaylist {
    owner: NodeId,
    path: String,
    entries: Vec<(u32, String, bool)>,
}

/// Every playlist in the tree with the entries its def authors — dormant
/// ones included, which is the point.
fn authored_playlist_entries(engine: &Engine, registry: &ProjectRegistry) -> Vec<AuthoredPlaylist> {
    let mut playlists = Vec::new();
    for node in engine.tree().entries() {
        let Some(def) = node
            .def_location
            .as_ref()
            .and_then(|location| registry.def(location))
            .and_then(|entry| entry.state.loaded_def())
            .and_then(|def| def.as_playlist())
        else {
            continue;
        };
        let entries = def
            .entries
            .entries
            .iter()
            .map(|(key, entry)| {
                // The loader names an entry's child after the entry, or
                // `entry_<k>` (`projected_node_name_and_ownership`).
                let name = entry
                    .name
                    .data
                    .as_ref()
                    .map(|name| name.value().clone())
                    .unwrap_or_else(|| format!("entry_{key}"));
                let resident = node.children.value().iter().any(|child| {
                    engine.tree().get(*child).is_some_and(|child| {
                        child.scope
                            == Some(ScopeRef::Sink {
                                owner: node.id,
                                entry: *key,
                            })
                    })
                });
                (*key, name, resident)
            })
            .collect();
        playlists.push(AuthoredPlaylist {
            owner: node.id,
            path: node.path.to_string(),
            entries,
        });
    }
    playlists
}

/// The scope whose stable persist path is `path`, if this project has one:
/// a scope in the tree, or the sink of an authored entry of a live playlist.
fn scope_by_persist_path(
    engine: &Engine,
    playlists: &[AuthoredPlaylist],
    path: &str,
) -> Option<ScopeRef> {
    if let Some(scope) = engine
        .tree()
        .scopes()
        .into_iter()
        .find(|scope| engine.tree().scope_persist_path(*scope).as_deref() == Some(path))
    {
        return Some(scope);
    }
    playlists.iter().find_map(|playlist| {
        playlist.entries.iter().find_map(|(key, _, _)| {
            let scope = ScopeRef::Sink {
                owner: playlist.owner,
                entry: *key,
            };
            (engine.tree().scope_persist_path(scope).as_deref() == Some(path)).then_some(scope)
        })
    })
}

/// Whether `path` names a scope inside a dormant authored entry: under the
/// path its child will have (`<playlist path>/<child name>.<kind>…`).
fn is_under_dormant_entry(playlists: &[AuthoredPlaylist], path: &str) -> bool {
    playlists.iter().any(|playlist| {
        let Some(rest) = path
            .strip_prefix(playlist.path.as_str())
            .and_then(|rest| rest.strip_prefix('/'))
        else {
            return false;
        };
        playlist
            .entries
            .iter()
            .filter(|(_, _, resident)| !resident)
            .any(|(_, name, _)| {
                rest.strip_prefix(name.as_str())
                    .is_some_and(|after| after.starts_with('.'))
            })
    })
}
