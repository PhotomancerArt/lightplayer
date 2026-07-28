//! Load-time bus scope assignment and writer-shadowing resolution
//! (scoped-buses ADR, `docs/adr/2026-07-28-scoped-buses.md`).
//!
//! Scopes are computed from the projected spine each time the binding phase
//! runs: every project node introduces a named scope around its children,
//! and every playlist-entry-owned child gets an anonymous scope of its own.
//! Pass 1 collects the writer set per `(scope, channel)`; the registration
//! pass then resolves consumed endpoints to the nearest enclosing scope with
//! a writer (else the root scope) and produced endpoints to the producer's
//! own scope.

use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use lp_collection::{VecMap, VecSet};

use lpc_model::{ChannelName, NodeDef, NodeId, NodeKind, SlotDirection};
use lpc_registry::ProjectRegistry;

use crate::dataflow::bus::{ScopeId, ScopedChannel};

use super::project_loader::{
    ProjectedNode, ProjectedNodeOwnership, binding_target, declared_default_binds,
};

/// Scope assignment over one projected spine, plus the pass-1 writer set.
pub(super) struct BusScopes {
    /// The scope each projected node's own bus endpoints resolve against.
    scope_of: VecMap<NodeId, ScopeId>,
    /// Enclosing scope per scope; the root scope has no entry.
    parent: VecMap<ScopeId, ScopeId>,
    root: ScopeId,
    /// `(scope, channel)` pairs with at least one writer.
    writers: VecSet<ScopedChannel>,
}

impl BusScopes {
    /// Compute scope assignment (structure only) and the writer set
    /// (pass 1). `projected_nodes` is parent-before-child ordered — the
    /// spine build sorts it that way.
    pub(super) fn compute(
        registry: &ProjectRegistry,
        projected_nodes: &[ProjectedNode],
        root_node: NodeId,
    ) -> Self {
        let root = ScopeId::Project(root_node);
        let mut scopes = Self {
            scope_of: VecMap::new(),
            parent: VecMap::new(),
            root,
            writers: VecSet::new(),
        };

        for node in projected_nodes {
            let containing = match node.ownership {
                ProjectedNodeOwnership::Root => root,
                ProjectedNodeOwnership::PlaylistEntry { .. } => {
                    // Anonymous per-child scope; its parent is the scope
                    // the owning playlist lives in.
                    let anon = ScopeId::Entry(node.id);
                    let playlist_scope = node
                        .parent
                        .and_then(|parent| scopes.scope_of.get(&parent).copied())
                        .unwrap_or(root);
                    scopes.parent.insert(anon, playlist_scope);
                    anon
                }
                ProjectedNodeOwnership::ProjectChild => node
                    .parent
                    .map(ScopeId::Project)
                    .filter(|scope| scopes.parent.contains_key(scope) || *scope == root)
                    .unwrap_or(root),
            };
            scopes.scope_of.insert(node.id, containing);

            // A project node (root included) introduces a named scope
            // around its children. Error-state defs project with the
            // `Project` fallback kind and no children — introducing a
            // scope for them is harmless and keeps this structural.
            if node.kind == NodeKind::Project {
                let introduced = ScopeId::Project(node.id);
                if introduced != root {
                    scopes.parent.insert(introduced, containing);
                }
            }
        }

        scopes.collect_writers(registry, projected_nodes);
        scopes
    }

    /// Rule 4: a produced `bus:` endpoint always writes the producer's own
    /// nearest scope.
    pub(super) fn write_channel(&self, producer: NodeId, channel: &ChannelName) -> ScopedChannel {
        ScopedChannel::new(self.scope_containing(producer), channel.clone())
    }

    /// Rule 3: a consumed `bus:` endpoint resolves to the nearest enclosing
    /// scope with a writer for the channel, else the root scope (unfilled
    /// channels surface where a host can later fill them).
    pub(super) fn read_channel(&self, consumer: NodeId, channel: &ChannelName) -> ScopedChannel {
        let mut scope = self.scope_containing(consumer);
        loop {
            let candidate = ScopedChannel::new(scope, channel.clone());
            if self.writers.contains(&candidate) {
                return candidate;
            }
            match self.parent.get(&scope) {
                Some(parent) => scope = *parent,
                None => return ScopedChannel::new(self.root, channel.clone()),
            }
        }
    }

    /// The `(scope, channel)` writer set — exposed for the loader's
    /// scan-vs-registered consistency test.
    #[cfg(test)]
    pub(super) fn writers(&self) -> &VecSet<ScopedChannel> {
        &self.writers
    }

    fn scope_containing(&self, node: NodeId) -> ScopeId {
        self.scope_of.get(&node).copied().unwrap_or(self.root)
    }

    /// Pass 1: collect `(scope, channel)` writers — authored bus targets on
    /// the slots the registration arms register, plus produce-direction
    /// declared defaults not overridden by an authored target on the same
    /// slot. Mirrors `register_node_bindings`; the loader test
    /// `example_projects_resolve_bus_endpoints_to_written_or_root_scopes`
    /// pins the rule-3 postcondition over the shipped examples, and the
    /// nested-project loader tests pin nearest-writer resolution.
    fn collect_writers(&mut self, registry: &ProjectRegistry, projected_nodes: &[ProjectedNode]) {
        for node in projected_nodes {
            if node.kind == NodeKind::Project {
                // Real project defs register no bindings; error-state defs
                // project with the fallback kind and register nothing.
                continue;
            }
            let Some(def) = loaded_node_def(registry, node) else {
                continue;
            };
            let Some((bindings, target_slots)) = authored_target_surface(def) else {
                continue;
            };
            for slot in target_slots {
                if let Some(lpc_model::BindingRef::Bus(bus)) = binding_target(bindings, &slot) {
                    let scoped = self.write_channel(node.id, bus.channel());
                    self.writers.insert(scoped);
                }
            }
            for (name, direction, endpoint) in declared_default_binds(node.kind) {
                if direction != SlotDirection::Produced || binding_target(bindings, &name).is_some()
                {
                    continue;
                }
                if let Ok(lpc_model::BindingRef::Bus(bus)) = lpc_model::BindingRef::parse(&endpoint)
                {
                    let scoped = self.write_channel(node.id, bus.channel());
                    self.writers.insert(scoped);
                }
            }
        }
    }
}

/// The def's authored bindings plus the slots whose `target` refs
/// `register_node_bindings` actually registers, per kind. An authored target
/// on any other slot never becomes a binding, so it must not count as a
/// writer either.
fn authored_target_surface(def: &NodeDef) -> Option<(&lpc_model::BindingDefs, Vec<String>)> {
    let (bindings, names): (&lpc_model::BindingDefs, Vec<&str>) = match def {
        NodeDef::Clock(config) => (&config.bindings, vec!["seconds", "delta_seconds"]),
        NodeDef::Button(config) => (&config.bindings, vec!["down", "held", "up"]),
        NodeDef::ControlRadio(config) => (&config.bindings, vec!["output"]),
        NodeDef::Shader(config) => (&config.bindings, vec!["output"]),
        NodeDef::ComputeShader(config) => {
            let produced = config
                .produced_slots
                .entries
                .keys()
                .map(|name| name.to_string())
                .collect();
            return Some((&config.bindings, produced));
        }
        NodeDef::Fluid(config) => (&config.bindings, vec!["output"]),
        NodeDef::Playlist(config) => (&config.bindings, vec!["output"]),
        NodeDef::Fixture(config) => (&config.bindings, vec!["output"]),
        NodeDef::Output(config) => (&config.bindings, vec![]),
        NodeDef::Project(_) | NodeDef::Texture(_) => return None,
    };
    Some((
        bindings,
        names.into_iter().map(ToString::to_string).collect(),
    ))
}

/// The node's loaded def, or `None` for error-state/missing entries (which
/// register no bindings).
fn loaded_node_def<'a>(registry: &'a ProjectRegistry, node: &ProjectedNode) -> Option<&'a NodeDef> {
    match &registry.def(&node.def_location)?.state {
        lpc_model::NodeDefState::Loaded(def) => Some(def),
        _ => None,
    }
}
