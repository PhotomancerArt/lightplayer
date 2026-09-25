//! The pre-tick residency step: a parent's request to load or unload one of
//! its own entry-keyed children, applied at a safe point.
//!
//! Loading children is the parent's job (multi-pattern vision D2, D4). A node
//! asks through [`NodeRuntime::residency_request`]; the tick owner calls
//! [`Engine::apply_residency`] BEFORE [`Engine::tick`], because the tick has
//! neither the filesystem nor a mutable registry (plan PD2). The top of a
//! tick is a safe point: no render borrow is live and no node is
//! `Executing`, the same point the compile window opens at
//! (`docs/adr/2026-08-03-memory-pressure-at-compile-safe-points.md`).
//!
//! Per request, in this order:
//!
//! 1. **Unload first** (abort-tier firmware: never hold two entries at
//!    once). The registry drops the entry (re-derive), then the runtime
//!    subtree goes. A refused unload (edits a commit would write) refuses the
//!    whole request and changes nothing.
//! 2. **Then load.** The registry adds the entry, the spine projects it, its
//!    nodes attach, and the whole projection is re-wired (asset consumers,
//!    every binding, the resolver). A failure anywhere — the registry, the
//!    attach, a node that attached `Failed`, the re-wire — rolls the entry
//!    back out, so nothing is half-attached, and is reported to the owner.
//!    It never fails the tick.
//! 3. **Tell the owner**, through the `entry_*` hooks, after the tree is
//!    consistent again.
//!
//! Knob memory (vision D12): writers in the entry's own sink scope are keyed
//! by the playlist's id, which does not change, so they simply stay. Writers
//! in scopes OWNED by a node inside the entry (a pattern module's scope, a
//! nested playlist's sinks) are keyed by an id a reload replaces; they are
//! parked by persist path on unload and re-engaged on load.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use lp_collection::VecSet;

use lpc_model::{NodeId, NodeUseLocation, Revision, SlotPath};
use lpc_registry::{ParseCtx, ProjectRegistry};
use lpfs::LpFs;

use crate::node::{NodeEntryState, NodeRuntime, ResidencyRequest, ScopeRef};

use super::{Engine, EngineError, EntryResidencyEvent, ProjectLoader, ResidencyApplied};

impl Engine {
    /// Apply every pending [`NodeRuntime::residency_request`] (see the module
    /// docs). Call it before [`Engine::tick`], on every edge.
    ///
    /// Allocates nothing when no node asks. Returns what it did; each event
    /// was also delivered to its owner. An `Err` means the engine could not
    /// restore a consistent tree (a removal hit an `Executing` node, or a
    /// re-wire failed with the failed entry already rolled back) — a load
    /// that fails is an event, never an `Err`.
    pub fn apply_residency(
        &mut self,
        fs: &dyn LpFs,
        registry: &mut ProjectRegistry,
    ) -> Result<ResidencyApplied, EngineError> {
        let mut requests: Vec<(NodeId, ResidencyRequest)> = Vec::new();
        for entry in self.tree_mut().entries_mut() {
            let id = entry.id;
            if let NodeEntryState::Alive(node) = entry.state.get_mut()
                && let Some(request) = node.residency_request()
                && !request.is_empty()
            {
                requests.push((id, request));
            }
        }
        let mut applied = ResidencyApplied::default();
        if requests.is_empty() {
            return Ok(applied);
        }

        let frame = lpc_model::current_revision();
        let shapes = self.slot_shapes().clone();
        let ctx = ParseCtx { shapes: &shapes };
        for (owner, request) in requests {
            self.apply_one_request(fs, registry, owner, request, frame, &ctx, &mut applied)?;
        }
        for event in &applied.events {
            self.notify_owner(event);
        }
        Ok(applied)
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "one request's whole context; splitting it into a struct would only rename the list"
    )]
    fn apply_one_request(
        &mut self,
        fs: &dyn LpFs,
        registry: &mut ProjectRegistry,
        owner: NodeId,
        request: ResidencyRequest,
        frame: Revision,
        ctx: &ParseCtx<'_>,
        applied: &mut ResidencyApplied,
    ) -> Result<(), EngineError> {
        let Some(playlist) = self.project_runtime_index().use_location(owner).cloned() else {
            applied.events.push(EntryResidencyEvent::Refused {
                owner,
                request,
                reason: String::from("the requesting node is not a projected node"),
            });
            return Ok(());
        };

        let mut rewire = false;
        if let Some(entry) = request.unload {
            lp_perf::emit_begin!(lp_perf::EVENT_ENTRY_UNLOAD);
            let unloaded = self.unload_entry(fs, registry, owner, &playlist, entry, frame, ctx);
            lp_perf::emit_end!(lp_perf::EVENT_ENTRY_UNLOAD);
            match unloaded? {
                Ok(()) => {
                    rewire = true;
                    applied
                        .events
                        .push(EntryResidencyEvent::Unloaded { owner, entry });
                }
                Err(reason) => {
                    applied.events.push(EntryResidencyEvent::Refused {
                        owner,
                        request,
                        reason,
                    });
                    return Ok(());
                }
            }
        }

        if let Some(entry) = request.load {
            lp_perf::emit_begin!(lp_perf::EVENT_ENTRY_LOAD);
            let loaded = self.load_entry(fs, registry, owner, &playlist, entry, frame, ctx);
            lp_perf::emit_end!(lp_perf::EVENT_ENTRY_LOAD);
            match loaded? {
                Ok(child) => {
                    // `load_entry` re-wired as its last check.
                    rewire = false;
                    applied.events.push(EntryResidencyEvent::Loaded {
                        owner,
                        entry,
                        child,
                    });
                }
                Err(reason) => {
                    // The rollback re-wired too.
                    rewire = false;
                    log::warn!("entry residency: entry {entry} did not load: {reason}");
                    applied.events.push(EntryResidencyEvent::LoadFailed {
                        owner,
                        entry,
                        reason,
                    });
                }
            }
        }

        if rewire {
            self.rewire_projection(registry, frame)
                .map_err(|e| residency_error(owner, "re-wire after unload", e))?;
        }
        Ok(())
    }

    /// Drop `entry` from the registry, then its runtime subtree. The inner
    /// `Err` is a refusal that changed nothing.
    #[allow(
        clippy::too_many_arguments,
        reason = "one request's whole context; splitting it into a struct would only rename the list"
    )]
    fn unload_entry(
        &mut self,
        fs: &dyn LpFs,
        registry: &mut ProjectRegistry,
        owner: NodeId,
        playlist: &NodeUseLocation,
        entry: u32,
        frame: Revision,
        ctx: &ParseCtx<'_>,
    ) -> Result<Result<(), String>, EngineError> {
        if let Err(error) = registry.set_entry_resident(fs, playlist, entry, false, frame, ctx) {
            return Ok(Err(error.to_string()));
        }
        if let Some(child) = self.entry_child(owner, entry) {
            self.park_owned_panel_writers(child)?;
            self.remove_runtime_subtree(child, frame)?;
        }
        Ok(Ok(()))
    }

    /// Add `entry` to the registry, project and attach its subtree, and
    /// re-wire. The inner `Err` is a load failure, already rolled back.
    #[allow(
        clippy::too_many_arguments,
        reason = "one request's whole context; splitting it into a struct would only rename the list"
    )]
    fn load_entry(
        &mut self,
        fs: &dyn LpFs,
        registry: &mut ProjectRegistry,
        owner: NodeId,
        playlist: &NodeUseLocation,
        entry: u32,
        frame: Revision,
        ctx: &ParseCtx<'_>,
    ) -> Result<Result<NodeId, String>, EngineError> {
        let changes = match registry.set_entry_resident(fs, playlist, entry, true, frame, ctx) {
            Ok(changes) => changes,
            Err(error) => return Ok(Err(error.to_string())),
        };
        let targets: VecSet<NodeUseLocation> = changes.uses.added.iter().cloned().collect();

        let failure = match self.attach_entry_subtree(fs, registry, &targets, frame) {
            Err(reason) => Some(reason),
            Ok(()) => match self.entry_child(owner, entry) {
                None => Some(String::from("the entry projected no child node")),
                Some(child) => self
                    .first_failed_in_subtree(child)
                    .or_else(|| {
                        self.rewire_projection(registry, frame)
                            .err()
                            .map(|e| format!("bind: {e}"))
                    })
                    .map(|reason| reason.to_string()),
            },
        };
        if let Some(reason) = failure {
            self.roll_back_entry(fs, registry, owner, playlist, entry, frame, ctx)?;
            return Ok(Err(reason));
        }

        let child = self
            .entry_child(owner, entry)
            .expect("checked above: the entry has a child");
        self.unpark_owned_panel_writers(child, frame);
        Ok(Ok(child))
    }

    fn attach_entry_subtree(
        &mut self,
        fs: &dyn LpFs,
        registry: &mut ProjectRegistry,
        targets: &VecSet<NodeUseLocation>,
        frame: Revision,
    ) -> Result<(), String> {
        let projected = ProjectLoader::ensure_runtime_spine(registry, self, frame)
            .map_err(|e| format!("project: {e}"))?;
        ProjectLoader::attach_selected_projected_nodes(
            fs, registry, self, &projected, targets, frame,
        )
        .map_err(|e| format!("attach: {e}"))
    }

    /// Take a failed load back out: runtime subtree, then registry, then
    /// re-wire, so the tree is exactly what it was before the load.
    ///
    /// If the registry refuses to drop the entry again (it cannot today: a
    /// dormant entry's files take no edits, so a just-loaded one has none to
    /// strand), the failed nodes stay, matching the registry, and render
    /// nothing — never a runtime tree that disagrees with its registry.
    #[allow(
        clippy::too_many_arguments,
        reason = "one request's whole context; splitting it into a struct would only rename the list"
    )]
    fn roll_back_entry(
        &mut self,
        fs: &dyn LpFs,
        registry: &mut ProjectRegistry,
        owner: NodeId,
        playlist: &NodeUseLocation,
        entry: u32,
        frame: Revision,
        ctx: &ParseCtx<'_>,
    ) -> Result<(), EngineError> {
        match registry.set_entry_resident(fs, playlist, entry, false, frame, ctx) {
            Ok(_) => {
                if let Some(child) = self.entry_child(owner, entry) {
                    self.remove_runtime_subtree(child, frame)?;
                }
            }
            Err(error) => {
                log::warn!(
                    "entry residency: could not roll entry {entry} back out ({error}); \
                     its failed nodes stay attached"
                );
            }
        }
        self.rewire_projection(registry, frame)
            .map_err(|e| residency_error(owner, "re-wire after a failed load", e))
    }

    /// The node an entry plays: the owner's child that inhabits the entry's
    /// sink scope.
    fn entry_child(&self, owner: NodeId, entry: u32) -> Option<NodeId> {
        let scope = ScopeRef::Sink { owner, entry };
        let owner_entry = self.tree().get(owner)?;
        owner_entry.children.value().iter().copied().find(|child| {
            self.tree()
                .get(*child)
                .is_some_and(|child| child.scope == Some(scope))
        })
    }

    /// The first `Failed` node in `root`'s subtree, as a reason.
    fn first_failed_in_subtree(&self, root: NodeId) -> Option<String> {
        let ids = self.tree().subtree_ids_depth_first(root).ok()?;
        ids.iter().find_map(|id| {
            let entry = self.tree().get(*id)?;
            match entry.state.value() {
                NodeEntryState::Failed { reason } => Some(format!("{}: {reason}", entry.path)),
                _ => None,
            }
        })
    }

    /// Park every latched writer whose scope is owned by a node in `root`'s
    /// subtree, by persist path (see the module docs).
    fn park_owned_panel_writers(&mut self, root: NodeId) -> Result<(), EngineError> {
        if self.panel_writers().is_empty() {
            return Ok(());
        }
        let ids = self.tree().subtree_ids_depth_first(root)?;
        let mut owned: Vec<(ScopeRef, String)> = Vec::new();
        for ((scope, _), _) in self.panel_writers().iter() {
            if ids.contains(&scope.owner()) && !owned.iter().any(|(known, _)| known == scope) {
                if let Some(path) = self.tree().scope_persist_path(*scope) {
                    owned.push((*scope, path));
                }
            }
        }
        for (scope, path) in owned {
            self.panel_writers_mut().park_scope(scope, &path);
        }
        Ok(())
    }

    /// Re-engage writers parked for scopes owned by nodes in `root`'s
    /// subtree, which just loaded under new ids.
    fn unpark_owned_panel_writers(&mut self, root: NodeId, frame: Revision) {
        if self.panel_writers().parked().next().is_none() {
            return;
        }
        let Ok(ids) = self.tree().subtree_ids_depth_first(root) else {
            return;
        };
        let mut homes: Vec<(String, ScopeRef)> = Vec::new();
        for id in ids {
            let Some(entry) = self.tree().get(id) else {
                continue;
            };
            let node_path = entry.path.to_string();
            if entry.introduces_scope {
                homes.push((node_path.clone(), ScopeRef::Module { owner: id }));
            }
            let sink_prefix = format!("{node_path}/entries[");
            for ((path, _), _) in self.panel_writers().parked() {
                let Some(key) = path
                    .strip_prefix(sink_prefix.as_str())
                    .and_then(|rest| rest.strip_suffix(']'))
                    .and_then(|key| key.parse::<u32>().ok())
                else {
                    continue;
                };
                let scope = ScopeRef::Sink {
                    owner: id,
                    entry: key,
                };
                if !homes.iter().any(|(_, known)| *known == scope) {
                    homes.push((path.clone(), scope));
                }
            }
        }
        let mut engaged = 0;
        for (path, scope) in homes {
            engaged += self.panel_writers_mut().unpark_into(&path, scope, frame);
        }
        if engaged > 0 {
            self.resolver_mut().invalidate_structure();
        }
    }

    fn notify_owner(&mut self, event: &EntryResidencyEvent) {
        let owner = match event {
            EntryResidencyEvent::Unloaded { owner, .. }
            | EntryResidencyEvent::Loaded { owner, .. }
            | EntryResidencyEvent::LoadFailed { owner, .. }
            | EntryResidencyEvent::Refused { owner, .. } => *owner,
        };
        let Some(entry) = self.tree_mut().get_mut(owner) else {
            return;
        };
        let NodeEntryState::Alive(node) = entry.state.get_mut() else {
            return;
        };
        deliver(node.as_mut(), event);
    }
}

fn deliver(node: &mut dyn NodeRuntime, event: &EntryResidencyEvent) {
    match event {
        EntryResidencyEvent::Unloaded { entry, .. } => node.entry_unloaded(*entry),
        EntryResidencyEvent::Loaded { entry, child, .. } => {
            node.entry_loaded(*entry, *child, &entry_output_slot());
        }
        EntryResidencyEvent::LoadFailed { entry, reason, .. } => {
            node.entry_load_failed(*entry, reason);
        }
        EntryResidencyEvent::Refused {
            request, reason, ..
        } => node.residency_refused(*request, reason),
    }
}

/// The slot an entry's child publishes its visual on — the same path the
/// loader gives every playlist entry (`playlist_runtime_entries`).
fn entry_output_slot() -> SlotPath {
    SlotPath::parse("output").expect("playlist child output path")
}

fn residency_error(owner: NodeId, what: &str, error: impl core::fmt::Display) -> EngineError {
    EngineError::Node {
        node: owner,
        message: format!("entry residency: {what}: {error}"),
    }
}
