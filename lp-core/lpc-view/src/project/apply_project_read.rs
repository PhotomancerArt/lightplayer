//! Apply stateless project read responses to [`ProjectView`].

use alloc::vec::Vec;

use lpc_model::NodeId;
use lpc_wire::{ProjectReadResponse, ProjectReadResult};

use super::ProjectView;
use crate::slot::SlotMirrorError;
use crate::tree::{ApplyError, apply_tree_deltas_collecting_removed};

/// Error applying a project read response.
#[derive(Clone, Debug, PartialEq)]
pub enum ProjectReadApplyError {
    Tree(ApplyError),
    Slot(SlotMirrorError),
}

impl core::fmt::Display for ProjectReadApplyError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Tree(error) => write!(f, "tree apply error: {error}"),
            Self::Slot(error) => write!(f, "slot apply error: {error}"),
        }
    }
}

impl core::error::Error for ProjectReadApplyError {}

impl From<ApplyError> for ProjectReadApplyError {
    fn from(value: ApplyError) -> Self {
        Self::Tree(value)
    }
}

impl From<SlotMirrorError> for ProjectReadApplyError {
    fn from(value: SlotMirrorError) -> Self {
        Self::Slot(value)
    }
}

/// Apply one stateless project read response to a client-side mirror.
pub fn apply_project_read_response(
    view: &mut ProjectView,
    response: ProjectReadResponse,
) -> Result<(), ProjectReadApplyError> {
    let revision = response.revision;
    for result in response.results {
        match result {
            ProjectReadResult::Shapes(shapes) => {
                // Merge (upsert) the shape entries so a gated read that carries only
                // changed shapes retains the unchanged ones, then prune any shape
                // absent from the membership list (present only when the id set
                // changed since `since`). Full reads carry every id in membership, so
                // the prune is a no-op there.
                if let Some(registry) = shapes.registry {
                    view.slots.apply_registry_page(registry);
                }
                if let Some(membership) = &shapes.membership {
                    view.slots.prune_shapes(membership);
                }
            }
            ProjectReadResult::Nodes(nodes) => {
                let mut removed_nodes: Vec<NodeId> = Vec::new();
                apply_tree_deltas_collecting_removed(
                    &mut view.tree,
                    &nodes.tree_deltas,
                    revision,
                    &mut removed_nodes,
                )?;
                if let Some(slots) = nodes.slots {
                    // Upsert the roots present in the payload; a gated read sends only
                    // changed roots, so unchanged roots must survive.
                    view.slots.apply_roots_snapshot(slots)?;
                }
                // Drop slot roots owned by nodes removed via the tree deltas (root
                // removal rides the tree, resolved Q2).
                if !removed_nodes.is_empty() {
                    view.slots.drop_roots_for_nodes(&removed_nodes);
                }
            }
            ProjectReadResult::Resources(resources) => {
                // Additively upsert summaries, then prune to membership (present only
                // when the store's id set changed since `since`).
                view.resource_cache.apply_summaries(&resources.summaries);
                view.resource_cache
                    .apply_runtime_buffer_payloads(&resources.runtime_buffer_payloads);
                if let Some(membership) = &resources.membership {
                    view.resource_cache.prune_to_membership(membership);
                }
            }
            ProjectReadResult::Runtime(runtime) => {
                view.runtime = Some(runtime);
            }
        }
    }
    view.revision = revision;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use lpc_model::{NodeId, Revision, TreePath};
    use lpc_wire::{
        NodeReadResult, NodeRuntimeStatus, ProjectReadResponse, ReadLevel, ResourceReadResult,
        ShapeReadResult, WireEntryState, WireTreeDelta,
    };

    #[test]
    fn apply_project_read_updates_revision_and_tree() {
        let mut view = ProjectView::new();
        let response = ProjectReadResponse {
            revision: Revision::new(3),
            results: vec![ProjectReadResult::Nodes(NodeReadResult {
                level: ReadLevel::Detail,
                tree_deltas: vec![WireTreeDelta::Created {
                    id: NodeId::new(0),
                    path: TreePath::parse("/basic.project").unwrap(),
                    parent: None,
                    child_kind: None,
                    children: vec![],
                    status: NodeRuntimeStatus::Created,
                    state: WireEntryState::Pending,
                    created_frame: Revision::new(0),
                    change_frame: Revision::new(0),
                    children_ver: Revision::new(0),
                }],
                slots: None,
            })],
            probes: vec![],
        };

        apply_project_read_response(&mut view, response).unwrap();

        assert_eq!(view.revision, Revision::new(3));
        assert!(view.tree.get(NodeId::new(0)).is_some());
    }

    #[test]
    fn apply_project_read_updates_resource_cache() {
        let mut view = ProjectView::new();
        let response = ProjectReadResponse {
            revision: Revision::new(1),
            results: vec![ProjectReadResult::Resources(ResourceReadResult {
                level: ReadLevel::Summary,
                summaries: vec![],
                runtime_buffer_payloads: vec![],
                membership: None,
            })],
            probes: vec![],
        };

        apply_project_read_response(&mut view, response).unwrap();

        assert_eq!(view.revision, Revision::new(1));
    }

    #[test]
    fn apply_project_read_retains_runtime_status() {
        let mut view = ProjectView::new();
        let response = ProjectReadResponse {
            revision: Revision::new(9),
            results: vec![ProjectReadResult::Runtime(lpc_wire::RuntimeReadResult {
                project: lpc_wire::ProjectRuntimeStatus {
                    revision: Revision::new(9),
                    frame_num: 42,
                    frame_delta_ms: 16,
                    frame_total_ms: 17,
                    demand_root_count: 2,
                    runtime_buffer_count: 3,
                },
                server: Some(lpc_wire::ServerRuntimeStatus {
                    theoretical_fps: Some(60.0),
                    last_frame_time_us: Some(16_000),
                    memory: Some(lpc_wire::server::MemoryStats {
                        free_bytes: 1024,
                        used_bytes: 2048,
                        total_bytes: 3072,
                    }),
                }),
            })],
            probes: vec![],
        };

        apply_project_read_response(&mut view, response).unwrap();

        let runtime = view.runtime.as_ref().expect("runtime retained");
        assert_eq!(runtime.project.frame_num, 42);
        assert_eq!(runtime.project.runtime_buffer_count, 3);
        assert_eq!(
            runtime
                .server
                .as_ref()
                .and_then(|server| server.memory.as_ref()),
            Some(&lpc_wire::server::MemoryStats {
                free_bytes: 1024,
                used_bytes: 2048,
                total_bytes: 3072,
            })
        );
    }

    // ---- Revision-gated (delta) partial-apply coverage (M5 P4) ----

    use alloc::string::String;
    use lpc_model::{
        LpType, LpValue, ResourceRef, RuntimeBufferId, SlotData, SlotShape, SlotShapeEntry,
        SlotShapeId, SlotShapeRegistry, WithRevision,
    };
    use lpc_wire::{
        WireChannelSampleFormat, WireChildKind, WireResourceAvailability, WireResourceKindSummary,
        WireResourceMetadataSummary, WireResourceSummary, WireRuntimeBufferKind, WireSlotIndex,
        WireSlotRootSnapshot, WireSlotRootsSnapshot, wire_slot_data_from_slot_access,
    };

    fn value_shape_id(name: &str) -> SlotShapeId {
        SlotShapeId::from_static_name(name)
    }

    /// Registry holding one f32 value shape per given name, all stamped `rev`.
    fn value_registry(names: &[&str], rev: i64) -> SlotShapeRegistry {
        let mut registry = SlotShapeRegistry::default();
        for name in names {
            registry
                .register_shape_with_version(
                    Revision::new(rev),
                    value_shape_id(name),
                    SlotShape::value(LpType::F32),
                )
                .unwrap();
        }
        registry
    }

    fn value_root(
        registry: &SlotShapeRegistry,
        name: &str,
        shape: SlotShapeId,
        v: f32,
    ) -> WireSlotRootSnapshot {
        let data = SlotData::Value(WithRevision::new(Revision::new(1), LpValue::F32(v)));
        WireSlotRootSnapshot {
            name: String::from(name),
            shape,
            data: wire_slot_data_from_slot_access(registry, shape, data.access()),
        }
    }

    fn buffer_summary(id: u32, rev: i64) -> WireResourceSummary {
        WireResourceSummary {
            resource_ref: ResourceRef::runtime_buffer(RuntimeBufferId::new(id)),
            owner: None,
            revision: Revision::new(rev),
            kind: WireResourceKindSummary::RuntimeBuffer(WireRuntimeBufferKind::OutputChannels),
            metadata: WireResourceMetadataSummary::OutputChannels {
                channels: 1,
                sample_format: WireChannelSampleFormat::U8,
            },
            byte_length_hint: Some(1),
            availability: WireResourceAvailability::Available,
        }
    }

    fn created_node(
        id: u32,
        parent: Option<NodeId>,
        path: &str,
        children: alloc::vec::Vec<NodeId>,
    ) -> WireTreeDelta {
        WireTreeDelta::Created {
            id: NodeId::new(id),
            path: TreePath::parse(path).unwrap(),
            parent,
            child_kind: parent.map(|_| WireChildKind::Input {
                source: WireSlotIndex(0),
            }),
            children,
            status: NodeRuntimeStatus::Created,
            state: WireEntryState::Pending,
            created_frame: Revision::new(1),
            change_frame: Revision::new(1),
            children_ver: Revision::new(1),
        }
    }

    fn f32_root_value(view: &ProjectView, root: &str) -> f32 {
        match view.slots.roots.get(root) {
            Some(SlotData::Value(v)) => match v.value() {
                LpValue::F32(f) => *f,
                other => panic!("root {root} not f32: {other:?}"),
            },
            other => panic!("root {root} missing/not value: {other:?}"),
        }
    }

    /// Build a baseline mirror from a full read: two shapes, one node (id 1) with
    /// two slot roots, and two resources.
    fn baseline_view() -> ProjectView {
        let mut view = ProjectView::new();
        let registry = value_registry(&["shape.a", "shape.b"], 1);
        let shape_a = value_shape_id("shape.a");

        let full = ProjectReadResponse {
            revision: Revision::new(1),
            results: vec![
                ProjectReadResult::Shapes(ShapeReadResult {
                    level: ReadLevel::Detail,
                    registry: Some(registry.snapshot()),
                    // Full read: membership names every id (prune is a no-op).
                    membership: Some(vec![value_shape_id("shape.a"), value_shape_id("shape.b")]),
                }),
                ProjectReadResult::Nodes(NodeReadResult {
                    level: ReadLevel::Detail,
                    tree_deltas: vec![
                        created_node(0, None, "/root.show", vec![NodeId::new(1)]),
                        created_node(1, Some(NodeId::new(0)), "/root.show/child.vis", vec![]),
                    ],
                    slots: Some(WireSlotRootsSnapshot {
                        roots: vec![
                            value_root(&registry, "node.1.def", shape_a, 1.0),
                            value_root(&registry, "node.1.state", shape_a, 2.0),
                        ],
                    }),
                }),
                ProjectReadResult::Resources(ResourceReadResult {
                    level: ReadLevel::Summary,
                    summaries: vec![buffer_summary(1, 1), buffer_summary(2, 1)],
                    runtime_buffer_payloads: vec![],
                    membership: Some(vec![
                        ResourceRef::runtime_buffer(RuntimeBufferId::new(1)),
                        ResourceRef::runtime_buffer(RuntimeBufferId::new(2)),
                    ]),
                }),
            ],
            probes: vec![],
        };

        apply_project_read_response(&mut view, full).unwrap();
        view
    }

    #[test]
    fn partial_apply_survives_gated_read() {
        let mut view = baseline_view();

        // Sanity: baseline populated.
        assert!(
            view.slots
                .registry
                .get(&value_shape_id("shape.a"))
                .is_some()
        );
        assert!(
            view.slots
                .registry
                .get(&value_shape_id("shape.b"))
                .is_some()
        );
        assert_eq!(f32_root_value(&view, "node.1.def"), 1.0);
        assert_eq!(f32_root_value(&view, "node.1.state"), 2.0);
        assert_eq!(view.resource_cache.summary_count(), 2);

        // Add a second node (id 2) with its own roots so a later removal has
        // something to drop.
        let registry_c = value_registry(&["shape.c"], 5);
        let shape_c = value_shape_id("shape.c");
        // The gated stream that adds shape.c carries the current ids_revision.
        let mut shape_c_snapshot = registry_c.snapshot();
        shape_c_snapshot.ids_revision = Revision::new(5);
        let add_node2 = ProjectReadResponse {
            revision: Revision::new(5),
            results: vec![
                ProjectReadResult::Shapes(ShapeReadResult {
                    level: ReadLevel::Detail,
                    registry: Some(shape_c_snapshot),
                    membership: Some(vec![
                        value_shape_id("shape.a"),
                        value_shape_id("shape.b"),
                        value_shape_id("shape.c"),
                    ]),
                }),
                ProjectReadResult::Nodes(NodeReadResult {
                    level: ReadLevel::Detail,
                    tree_deltas: vec![
                        WireTreeDelta::ChildrenChanged {
                            id: NodeId::new(0),
                            children: vec![NodeId::new(1), NodeId::new(2)],
                            children_ver: Revision::new(5),
                        },
                        created_node(2, Some(NodeId::new(0)), "/root.show/other.vis", vec![]),
                    ],
                    slots: Some(WireSlotRootsSnapshot {
                        roots: vec![value_root(&registry_c, "node.2.def", shape_c, 9.0)],
                    }),
                }),
            ],
            probes: vec![],
        };
        apply_project_read_response(&mut view, add_node2).unwrap();
        assert_eq!(f32_root_value(&view, "node.2.def"), 9.0);

        // Now the gated (delta) read at since=5 -> revision 6:
        //  - shapes: one changed entry (shape.a) + membership WITHOUT shape.b
        //    (shape.b removed) but WITH shape.c (added earlier).
        //  - resources: one changed summary (buffer 1) + membership WITHOUT
        //    buffer 2 (removed).
        //  - slots: one changed root (node.1.state).
        //  - tree: ChildrenChanged on node 0 removing node 2.
        // shape.a's entry changed at rev 6 (re-stamped); its shape stays F32 so the
        // node.1.state root still decodes. We assert the changed_at advanced.
        let mut changed_shapes = SlotShapeRegistry::default();
        changed_shapes
            .register_shape_with_version(
                Revision::new(6),
                value_shape_id("shape.a"),
                SlotShape::value(LpType::F32),
            )
            .unwrap();
        let mut changed_snapshot = changed_shapes.snapshot();
        // The server's snapshot carries the current ids_revision; emulate it.
        changed_snapshot.ids_revision = Revision::new(6);

        let registry_state = value_registry(&["shape.a"], 1);

        let gated = ProjectReadResponse {
            revision: Revision::new(6),
            results: vec![
                ProjectReadResult::Shapes(ShapeReadResult {
                    level: ReadLevel::Detail,
                    registry: Some(changed_snapshot),
                    membership: Some(vec![value_shape_id("shape.a"), value_shape_id("shape.c")]),
                }),
                ProjectReadResult::Nodes(NodeReadResult {
                    level: ReadLevel::Detail,
                    tree_deltas: vec![WireTreeDelta::ChildrenChanged {
                        id: NodeId::new(0),
                        children: vec![NodeId::new(1)],
                        children_ver: Revision::new(6),
                    }],
                    slots: Some(WireSlotRootsSnapshot {
                        roots: vec![value_root(
                            &registry_state,
                            "node.1.state",
                            value_shape_id("shape.a"),
                            42.0,
                        )],
                    }),
                }),
                ProjectReadResult::Resources(ResourceReadResult {
                    level: ReadLevel::Summary,
                    summaries: vec![buffer_summary(1, 6)],
                    runtime_buffer_payloads: vec![],
                    membership: Some(vec![ResourceRef::runtime_buffer(RuntimeBufferId::new(1))]),
                }),
            ],
            probes: vec![],
        };

        apply_project_read_response(&mut view, gated).unwrap();

        // Shapes: unchanged shape.c retained; removed shape.b pruned; changed
        // shape.a updated (entry now stamped at rev 6).
        assert!(
            view.slots
                .registry
                .get(&value_shape_id("shape.c"))
                .is_some()
        );
        assert!(
            view.slots
                .registry
                .get(&value_shape_id("shape.b"))
                .is_none()
        );
        assert_eq!(
            view.slots
                .registry
                .entry(&value_shape_id("shape.a"))
                .map(SlotShapeEntry::changed_at),
            Some(Revision::new(6))
        );

        // Slots: node.1.def unchanged retained; node.1.state updated; node.2.*
        // dropped (node 2 removed by the tree delta).
        assert_eq!(f32_root_value(&view, "node.1.def"), 1.0);
        assert_eq!(f32_root_value(&view, "node.1.state"), 42.0);
        assert!(view.slots.roots.get("node.2.def").is_none());
        assert!(view.slots.root_shapes.get("node.2.def").is_none());
        assert!(view.tree.get(NodeId::new(2)).is_none());
        assert!(view.tree.get(NodeId::new(1)).is_some());

        // Resources: buffer 1 updated; buffer 2 pruned by membership.
        assert_eq!(
            view.resource_cache
                .summary(ResourceRef::runtime_buffer(RuntimeBufferId::new(1)))
                .map(|s| s.revision),
            Some(Revision::new(6))
        );
        assert!(
            view.resource_cache
                .summary(ResourceRef::runtime_buffer(RuntimeBufferId::new(2)))
                .is_none()
        );

        assert_eq!(view.revision, Revision::new(6));
    }

    #[test]
    fn full_apply_matches_prior_snapshot_behavior() {
        // Regression guard for the upsert refactor: applying the same full read
        // twice (idempotent full snapshot) yields the same state as applying it
        // once, and a fresh full read replaces content wholesale.
        let once = baseline_view();
        let mut twice = baseline_view();
        // Re-apply the identical full read.
        let registry = value_registry(&["shape.a", "shape.b"], 1);
        let shape_a = value_shape_id("shape.a");
        let full_again = ProjectReadResponse {
            revision: Revision::new(1),
            results: vec![
                ProjectReadResult::Shapes(ShapeReadResult {
                    level: ReadLevel::Detail,
                    registry: Some(registry.snapshot()),
                    membership: Some(vec![value_shape_id("shape.a"), value_shape_id("shape.b")]),
                }),
                ProjectReadResult::Nodes(NodeReadResult {
                    level: ReadLevel::Detail,
                    tree_deltas: vec![],
                    slots: Some(WireSlotRootsSnapshot {
                        roots: vec![
                            value_root(&registry, "node.1.def", shape_a, 1.0),
                            value_root(&registry, "node.1.state", shape_a, 2.0),
                        ],
                    }),
                }),
                ProjectReadResult::Resources(ResourceReadResult {
                    level: ReadLevel::Summary,
                    summaries: vec![buffer_summary(1, 1), buffer_summary(2, 1)],
                    runtime_buffer_payloads: vec![],
                    membership: Some(vec![
                        ResourceRef::runtime_buffer(RuntimeBufferId::new(1)),
                        ResourceRef::runtime_buffer(RuntimeBufferId::new(2)),
                    ]),
                }),
            ],
            probes: vec![],
        };
        apply_project_read_response(&mut twice, full_again).unwrap();

        assert_eq!(once.slots, twice.slots);
        assert_eq!(once.revision, twice.revision);
        assert_eq!(
            once.resource_cache.summary_count(),
            twice.resource_cache.summary_count()
        );
        assert_eq!(
            f32_root_value(&once, "node.1.def"),
            f32_root_value(&twice, "node.1.def")
        );
        assert_eq!(
            f32_root_value(&once, "node.1.state"),
            f32_root_value(&twice, "node.1.state")
        );
    }
}
