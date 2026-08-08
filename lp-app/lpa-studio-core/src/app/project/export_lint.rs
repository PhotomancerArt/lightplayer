//! Export lint, graph half: does an exported module depend on scaffolding
//! it will not ship with?
//!
//! The static half ([`lpc_model::project::export_check`]) proves an export
//! folder is closed over its *files*. This half asks the question files
//! cannot answer: an exported module reads `bus:noise.field`, and the only
//! thing writing that channel is a sibling node that lives outside every
//! exported folder. Vendored into another project, the module gets a channel
//! nobody feeds — it will look broken for a reason invisible in its own
//! folder. That is the sibling-feed warning (module authoring vision D5).
//!
//! ## What is and is not a finding
//!
//! - **No writer anywhere is NOT a finding.** An unwritten channel with an
//!   authored default is R6's *invitation* — the module is saying "feed me
//!   this if you have it, otherwise here is a sensible value". Warning about
//!   it would punish exactly the shape the format encourages.
//! - **Panel and Default writers do not count as scaffolding.** A panel
//!   writer is lazy runtime state, never authored, and travels with nothing;
//!   a Default-origin binding is policy the runtime re-materializes wherever
//!   the module lands. Only `Authored` writers are load-bearing project data
//!   that either does or does not travel with the export.
//! - A finding needs at least one Authored writer, **all** of them on nodes
//!   outside **every** exported folder. One authored writer inside exported
//!   material means the feed ships.
//!
//! ## Resolution
//!
//! Writer lookup follows R5: from the consuming binding's endpoint scope,
//! walk outward through enclosing module scopes and stop at the first scope
//! whose same-named channel has providers. That mirrors
//! `ProjectController::ui_bus_view_for_scope`'s `scope_has_writer` /
//! `descendant_module_scopes` pair, from the other direction.
//!
//! ## Shape
//!
//! Deliberately a **free function over borrowed inputs** — a
//! [`lpc_wire::WireBindingGraph`], the manifest's export list, and a caller-built
//! [`ExportGraphContext`] describing node identity/placement. Nothing here
//! touches `ProjectController`, the engine, or a filesystem, so T3 can lift
//! the whole module into a shared crate (beside the static half) for
//! `lp-cli` and pack CI without untangling it from Studio first. The
//! `ExportGraphContext` indirection exists exactly for that: the controller
//! knows about `NodeController` trees and `def_artifacts`; this function only
//! needs the four facts those yield.

use std::collections::{BTreeMap, BTreeSet};

use lpc_model::{ExportFinding, NodeId};
use lpc_wire::{
    WireBindingDirection, WireBindingEndpoint, WireBindingGraph, WireBindingOrigin, WireScopeRef,
};

/// One project node, as the graph lint needs to see it.
#[derive(Clone, Debug)]
pub struct ExportGraphNode {
    /// Runtime node id — the identity the binding graph speaks.
    pub id: NodeId,
    /// Display label, for the finding's message.
    pub label: String,
    /// Project-relative path of the artifact defining this node
    /// (`"/chase/module.json"`; a leading `/` is optional). `None` when the
    /// controller has no def artifact for the node, which reads as "outside
    /// every export" — the conservative answer, since a node we cannot place
    /// certainly cannot be proven to travel with an export.
    pub def_path: Option<String>,
    /// Enclosing module-scope owners, **outermost first**, excluding this
    /// node itself. Drives the outward R5 walk.
    pub enclosing_scopes: Vec<NodeId>,
}

/// Node identity/placement for the whole project.
#[derive(Clone, Debug, Default)]
pub struct ExportGraphContext {
    pub nodes: Vec<ExportGraphNode>,
}

impl ExportGraphContext {
    pub fn new(nodes: Vec<ExportGraphNode>) -> Self {
        Self { nodes }
    }

    fn index(&self) -> BTreeMap<NodeId, &ExportGraphNode> {
        self.nodes.iter().map(|node| (node.id, node)).collect()
    }
}

/// Sibling-feed warnings for every exported module in `exports`.
///
/// Pure: same inputs, same findings, no I/O. Returns an empty vec for a
/// project with no exports, and for one whose exports are all fed from
/// inside themselves.
pub fn check_export_graph(
    graph: &WireBindingGraph,
    exports: &[String],
    context: &ExportGraphContext,
) -> Vec<ExportFinding> {
    if exports.is_empty() {
        return Vec::new();
    }
    let index = context.index();
    let mut findings = Vec::new();
    // One finding per (export, channel): a module reading the same starved
    // channel from three slots has one problem, not three.
    let mut seen: BTreeSet<(String, String)> = BTreeSet::new();

    for binding in &graph.bindings {
        if binding.direction != WireBindingDirection::Consumes {
            continue;
        }
        let WireBindingEndpoint::Bus { scope, channel } = &binding.endpoint else {
            continue;
        };
        let Some(consumer) = index.get(&binding.node) else {
            continue;
        };
        let Some(export) = export_of(consumer, exports) else {
            continue;
        };

        let writers = resolve_writers(graph, *scope, channel, &index);
        // Only authored writers are project data that either travels or does
        // not; panel/default writers are runtime policy (see module doc).
        let authored: Vec<&lpc_wire::WireEffectiveBinding> = writers
            .into_iter()
            .filter(|writer| writer.origin == WireBindingOrigin::Authored)
            .collect();
        if authored.is_empty() {
            // R6: no writer at all (or only runtime-materialized ones) is an
            // invitation, not a hazard. Never a finding.
            continue;
        }
        if authored.iter().any(|writer| {
            index
                .get(&writer.node)
                .and_then(|node| export_of(node, exports))
                .is_some()
        }) {
            // Fed from inside exported material — the feed ships.
            continue;
        }

        if !seen.insert((export.clone(), channel.clone())) {
            continue;
        }
        let outsiders: Vec<String> = {
            let mut labels: Vec<String> = authored
                .iter()
                .map(|writer| {
                    index
                        .get(&writer.node)
                        .map(|node| node.label.clone())
                        .unwrap_or_else(|| format!("node {}", writer.node.0))
                })
                .collect();
            labels.sort();
            labels.dedup();
            labels
        };
        findings.push(ExportFinding::warning(
            &export,
            format!(
                "`{export}` reads `{channel}`, but the only thing writing it is {} \
                 — outside the exported folders. A project that imports `{export}` \
                 gets nothing on `{channel}`: publish it from inside `{export}`, or \
                 give the consuming slot an authored default so the module stands \
                 on its own.",
                join_labels(&outsiders)
            ),
            consumer.def_path.clone(),
        ));
    }

    findings
}

/// Writers a consuming endpoint resolves to, per R5: outward from the
/// endpoint's own scope, stopping at the first scope that has any provider.
///
/// An endpoint with no scope (scope-less engines / test fakes) matches the
/// scope-less channel entry.
fn resolve_writers<'a>(
    graph: &'a WireBindingGraph,
    scope: Option<WireScopeRef>,
    channel: &str,
    index: &BTreeMap<NodeId, &ExportGraphNode>,
) -> Vec<&'a lpc_wire::WireEffectiveBinding> {
    let chain: Vec<Option<WireScopeRef>> = match scope {
        None => vec![None],
        Some(scope) => outward_chain(scope, index).into_iter().map(Some).collect(),
    };
    for step in chain {
        let Some(entry) = graph
            .channels
            .iter()
            .find(|candidate| candidate.scope == step && candidate.name == channel)
        else {
            continue;
        };
        if entry.providers.is_empty() {
            continue;
        }
        return entry
            .providers
            .iter()
            .filter_map(|provider| graph.bindings.get(*provider as usize))
            .collect();
    }
    Vec::new()
}

/// `scope` itself, then each enclosing module scope, innermost first.
///
/// A sink scope (one playlist entry) is a scope like any other for this
/// walk: values resolve outward from it (R5), so it just contributes its
/// owner's enclosing modules after itself.
fn outward_chain(
    scope: WireScopeRef,
    index: &BTreeMap<NodeId, &ExportGraphNode>,
) -> Vec<WireScopeRef> {
    let mut chain = vec![scope];
    if let Some(node) = index.get(&scope.owner()) {
        for owner in node.enclosing_scopes.iter().rev() {
            chain.push(WireScopeRef::Module { owner: *owner });
        }
    }
    chain
}

/// The export folder a node's defining artifact lives in, if any.
fn export_of(node: &ExportGraphNode, exports: &[String]) -> Option<String> {
    let path = node.def_path.as_deref()?;
    let path = if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    };
    exports
        .iter()
        .find(|export| path.starts_with(&format!("/{export}/")))
        .cloned()
}

fn join_labels(labels: &[String]) -> String {
    match labels {
        [] => String::from("nothing"),
        [one] => format!("`{one}`"),
        [rest @ .., last] => format!(
            "{} and `{last}`",
            rest.iter()
                .map(|label| format!("`{label}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_model::{Kind, Revision, SlotPath};
    use lpc_wire::{WireBusChannel, WireEffectiveBinding};

    const ROOT: NodeId = NodeId(0);
    const FIRE: NodeId = NodeId(1);
    const FIRE_SHADER: NodeId = NodeId(2);
    const COMMON: NodeId = NodeId(3);
    const NOISE_IN_FIRE: NodeId = NodeId(4);

    fn exports() -> Vec<String> {
        vec![String::from("fire")]
    }

    fn node(
        id: NodeId,
        label: &str,
        def_path: Option<&str>,
        enclosing: &[NodeId],
    ) -> ExportGraphNode {
        ExportGraphNode {
            id,
            label: label.to_string(),
            def_path: def_path.map(str::to_string),
            enclosing_scopes: enclosing.to_vec(),
        }
    }

    /// Root module, an exported `fire` module with a shader inside it, and a
    /// non-exported `common` node sitting beside `fire` at the root.
    fn context() -> ExportGraphContext {
        ExportGraphContext::new(vec![
            node(ROOT, "project", Some("/module.json"), &[]),
            node(FIRE, "fire", Some("/fire/module.json"), &[ROOT]),
            node(
                FIRE_SHADER,
                "flame",
                Some("/fire/shader.json"),
                &[ROOT, FIRE],
            ),
            node(COMMON, "common", Some("/common.json"), &[ROOT]),
            node(
                NOISE_IN_FIRE,
                "noise",
                Some("/fire/noise.json"),
                &[ROOT, FIRE],
            ),
        ])
    }

    fn consumer(node: NodeId, scope: WireScopeRef, channel: &str) -> WireEffectiveBinding {
        binding(
            node,
            WireBindingDirection::Consumes,
            scope,
            channel,
            WireBindingOrigin::Authored,
        )
    }

    fn binding(
        node: NodeId,
        direction: WireBindingDirection,
        scope: WireScopeRef,
        channel: &str,
        origin: WireBindingOrigin,
    ) -> WireEffectiveBinding {
        WireEffectiveBinding {
            owner: node,
            node,
            slot: Some(SlotPath::parse("field").unwrap()),
            direction,
            endpoint: WireBindingEndpoint::Bus {
                scope: Some(scope),
                channel: channel.to_string(),
            },
            origin,
            priority: 0,
            kind: Kind::Ratio,
            panel_show: false,
        }
    }

    fn channel(
        scope: WireScopeRef,
        name: &str,
        providers: Vec<u32>,
        consumers: Vec<u32>,
    ) -> WireBusChannel {
        WireBusChannel {
            scope: Some(scope),
            name: name.to_string(),
            kind: Some(Kind::Ratio),
            providers,
            consumers,
            value: None,
            primary_visual: false,
        }
    }

    fn graph(
        bindings: Vec<WireEffectiveBinding>,
        channels: Vec<WireBusChannel>,
    ) -> WireBindingGraph {
        WireBindingGraph {
            revision: Revision::new(1),
            bindings,
            channels,
        }
    }

    fn root_scope() -> WireScopeRef {
        WireScopeRef::Module { owner: ROOT }
    }

    fn fire_scope() -> WireScopeRef {
        WireScopeRef::Module { owner: FIRE }
    }

    /// The headline case: `fire` consumes `noise.field`; the only writer is
    /// `common`, a sibling outside every export. Warning.
    #[test]
    fn export_lint_warns_when_only_writer_is_a_non_exported_sibling() {
        let graph = graph(
            vec![
                consumer(FIRE_SHADER, fire_scope(), "noise.field"),
                binding(
                    COMMON,
                    WireBindingDirection::Publishes,
                    root_scope(),
                    "noise.field",
                    WireBindingOrigin::Authored,
                ),
            ],
            vec![
                channel(fire_scope(), "noise.field", vec![], vec![0]),
                channel(root_scope(), "noise.field", vec![1], vec![]),
            ],
        );
        let findings = check_export_graph(&graph, &exports(), &context());
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].export, "fire");
        assert_eq!(findings[0].severity, lpc_model::ExportSeverity::Warning);
        assert!(findings[0].message.contains("noise.field"), "{findings:?}");
        assert!(findings[0].message.contains("common"), "{findings:?}");
        assert_eq!(findings[0].path.as_deref(), Some("/fire/shader.json"));
    }

    /// Writer inside the exported folder — the feed ships with the module.
    /// Clean.
    #[test]
    fn export_lint_is_clean_when_the_writer_is_inside_the_export() {
        let graph = graph(
            vec![
                consumer(FIRE_SHADER, fire_scope(), "noise.field"),
                binding(
                    NOISE_IN_FIRE,
                    WireBindingDirection::Publishes,
                    fire_scope(),
                    "noise.field",
                    WireBindingOrigin::Authored,
                ),
            ],
            vec![channel(fire_scope(), "noise.field", vec![1], vec![0])],
        );
        assert!(check_export_graph(&graph, &exports(), &context()).is_empty());
    }

    /// R6: no writer anywhere is an authored-default invitation, never a
    /// finding. This is the semantic the phase file guards explicitly.
    #[test]
    fn export_lint_is_clean_when_no_one_writes_the_channel() {
        let graph = graph(
            vec![consumer(FIRE_SHADER, fire_scope(), "noise.field")],
            vec![
                channel(fire_scope(), "noise.field", vec![], vec![0]),
                channel(root_scope(), "noise.field", vec![], vec![]),
            ],
        );
        assert!(check_export_graph(&graph, &exports(), &context()).is_empty());
    }

    /// A panel writer is lazy runtime state that never travels and is never
    /// authored — it is not the scaffolding this warning is about.
    #[test]
    fn export_lint_ignores_panel_and_default_writers() {
        for origin in [WireBindingOrigin::Panel, WireBindingOrigin::Default] {
            let graph = graph(
                vec![
                    consumer(FIRE_SHADER, fire_scope(), "speed"),
                    binding(
                        COMMON,
                        WireBindingDirection::Publishes,
                        root_scope(),
                        "speed",
                        origin,
                    ),
                ],
                vec![
                    channel(fire_scope(), "speed", vec![], vec![0]),
                    channel(root_scope(), "speed", vec![1], vec![]),
                ],
            );
            assert!(
                check_export_graph(&graph, &exports(), &context()).is_empty(),
                "{origin:?} must not read as scaffolding"
            );
        }
    }

    /// One authored writer inside the export is enough, even alongside an
    /// outside one.
    #[test]
    fn export_lint_is_clean_when_any_authored_writer_is_exported() {
        let graph = graph(
            vec![
                consumer(FIRE_SHADER, fire_scope(), "noise.field"),
                binding(
                    NOISE_IN_FIRE,
                    WireBindingDirection::Publishes,
                    fire_scope(),
                    "noise.field",
                    WireBindingOrigin::Authored,
                ),
                binding(
                    COMMON,
                    WireBindingDirection::Publishes,
                    fire_scope(),
                    "noise.field",
                    WireBindingOrigin::Authored,
                ),
            ],
            vec![channel(fire_scope(), "noise.field", vec![1, 2], vec![0])],
        );
        assert!(check_export_graph(&graph, &exports(), &context()).is_empty());
    }

    /// R5 shadowing: an inner scope writer wins, so the outer sibling never
    /// enters the picture.
    #[test]
    fn export_lint_stops_at_the_nearest_writing_scope() {
        let graph = graph(
            vec![
                consumer(FIRE_SHADER, fire_scope(), "noise.field"),
                binding(
                    NOISE_IN_FIRE,
                    WireBindingDirection::Publishes,
                    fire_scope(),
                    "noise.field",
                    WireBindingOrigin::Authored,
                ),
                binding(
                    COMMON,
                    WireBindingDirection::Publishes,
                    root_scope(),
                    "noise.field",
                    WireBindingOrigin::Authored,
                ),
            ],
            vec![
                channel(fire_scope(), "noise.field", vec![1], vec![0]),
                channel(root_scope(), "noise.field", vec![2], vec![]),
            ],
        );
        assert!(check_export_graph(&graph, &exports(), &context()).is_empty());
    }

    /// Consumers outside every export are somebody else's business.
    #[test]
    fn export_lint_ignores_consumers_outside_the_exports() {
        let graph = graph(
            vec![
                consumer(COMMON, root_scope(), "noise.field"),
                binding(
                    ROOT,
                    WireBindingDirection::Publishes,
                    root_scope(),
                    "noise.field",
                    WireBindingOrigin::Authored,
                ),
            ],
            vec![channel(root_scope(), "noise.field", vec![1], vec![0])],
        );
        assert!(check_export_graph(&graph, &exports(), &context()).is_empty());
    }

    /// Two starved slots on the same channel are one problem.
    #[test]
    fn export_lint_reports_one_finding_per_channel() {
        let graph = graph(
            vec![
                consumer(FIRE_SHADER, fire_scope(), "noise.field"),
                consumer(NOISE_IN_FIRE, fire_scope(), "noise.field"),
                binding(
                    COMMON,
                    WireBindingDirection::Publishes,
                    root_scope(),
                    "noise.field",
                    WireBindingOrigin::Authored,
                ),
            ],
            vec![
                channel(fire_scope(), "noise.field", vec![], vec![0, 1]),
                channel(root_scope(), "noise.field", vec![2], vec![]),
            ],
        );
        assert_eq!(check_export_graph(&graph, &exports(), &context()).len(), 1);
    }

    /// A project with no exports is never linted.
    #[test]
    fn export_lint_skips_projects_with_no_exports() {
        let graph = graph(
            vec![
                consumer(FIRE_SHADER, fire_scope(), "noise.field"),
                binding(
                    COMMON,
                    WireBindingDirection::Publishes,
                    root_scope(),
                    "noise.field",
                    WireBindingOrigin::Authored,
                ),
            ],
            vec![
                channel(fire_scope(), "noise.field", vec![], vec![0]),
                channel(root_scope(), "noise.field", vec![1], vec![]),
            ],
        );
        assert!(check_export_graph(&graph, &[], &context()).is_empty());
    }
}
