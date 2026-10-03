//! The app agent's `edit_project` batch, applied in order through the
//! project's own ops (D14, PD1).
//!
//! A `MutationOp` cannot create or remove a node — the project's `nodes` map
//! is fixed — so a batch sequences two kinds of command: node create /
//! import / remove ride the add-node menu's and the delete verb's paths,
//! and slot / asset edits ride the field editor's. Nothing here writes
//! anything a person's click could not: each edit lands in the same
//! overlay, dirty tracking and Revert as theirs, and its status is the
//! notice the person would have seen.
//!
//! Order rule: a rejected slot edit does not stop the batch (later edits do
//! not depend on it), but a failed node create or import does — later
//! edits would address a node that does not exist — and the rest come back
//! `skipped`.

use std::collections::BTreeSet;

use lpa_agent::{EditStatus, ProjectEdit};
use lpc_model::{NodeDef, NodeKind, SlotAccess, SlotPath, SlotShapeLookup};

use super::agent_slot_json::json_slot_edits;
use super::node::NodeController;
use crate::app::project::node::{ProjectNodeAddress, UiAttachTarget};
use crate::{
    AssetEditOp, ImportSource, ProjectController, ProjectEditRun, ProjectSlotAddress,
    ProjectSlotRoot, StudioServerClient, UiError, UiNoticeLevel,
};

impl ProjectController {
    /// Apply one `edit_project` batch's edits, in order. Returns one
    /// [`AgentEditLanding`] per edit (its status, and the node it landed on)
    /// plus every run (for the caller's notices and logs). The save is the
    /// caller's (it is a project op of its own).
    pub(crate) async fn apply_agent_project_edits(
        &mut self,
        server: &mut StudioServerClient,
        edits: &[ProjectEdit],
    ) -> (Vec<AgentEditLanding>, Vec<ProjectEditRun>) {
        let mut statuses = Vec::with_capacity(edits.len());
        let mut runs = Vec::new();
        let mut halted: Option<String> = None;
        for (index, edit) in edits.iter().enumerate() {
            if let Some(reason) = &halted {
                statuses.push(AgentEditLanding {
                    status: EditStatus::Skipped {
                        reason: reason.clone(),
                    },
                    node: None,
                });
                continue;
            }
            let (status, run, node) = self.apply_agent_content_edit(server, edit).await;
            let creates = matches!(
                edit,
                ProjectEdit::CreateNode(_) | ProjectEdit::ImportPattern(_)
            );
            if creates && let EditStatus::Rejected { reason } = &status {
                halted = Some(format!("edit {index} ({}) failed: {reason}", edit.verb()));
            }
            // Only an edit that landed has a node to point at.
            let node = node.filter(|_| matches!(status, EditStatus::Applied { .. }));
            statuses.push(AgentEditLanding { status, node });
            runs.extend(run);
        }
        (statuses, runs)
    }

    /// One edit: its status, its run, and the node it is about (the
    /// created one for a create).
    async fn apply_agent_content_edit(
        &mut self,
        server: &mut StudioServerClient,
        edit: &ProjectEdit,
    ) -> (
        EditStatus,
        Option<ProjectEditRun>,
        Option<ProjectNodeAddress>,
    ) {
        let named = match edit {
            ProjectEdit::RemoveNode(node) => Some(node.node.as_str()),
            ProjectEdit::Set(set) => Some(set.node.as_str()),
            ProjectEdit::Ensure(slot) | ProjectEdit::Remove(slot) => Some(slot.node.as_str()),
            ProjectEdit::SetAsset(asset) => Some(asset.node.as_str()),
            ProjectEdit::CreateNode(_)
            | ProjectEdit::ImportPattern(_)
            | ProjectEdit::SetTarget(_) => None,
        };
        // Resolved before the edit runs: a removed node is gone after it.
        let node = named
            .and_then(|name| self.agent_node(name).ok())
            .map(|node| node.address().clone());
        let creates = matches!(
            edit,
            ProjectEdit::CreateNode(_) | ProjectEdit::ImportPattern(_)
        );
        let before = creates.then(|| self.agent_node_addresses());
        let (status, run) = self.apply_agent_content_edit_status(server, edit).await;
        let node = match before {
            Some(before) => self.created_node(&before),
            None => node,
        };
        (status, run, node)
    }

    async fn apply_agent_content_edit_status(
        &mut self,
        server: &mut StudioServerClient,
        edit: &ProjectEdit,
    ) -> (EditStatus, Option<ProjectEditRun>) {
        match edit {
            ProjectEdit::CreateNode(create) => {
                let kind = match parse_kind(&create.kind) {
                    Ok(kind) => kind,
                    Err(reason) => return (EditStatus::Rejected { reason }, None),
                };
                let attach = match self.agent_attach(create.in_playlist.as_deref()) {
                    Ok(attach) => attach,
                    Err(reason) => return (EditStatus::Rejected { reason }, None),
                };
                let before = self.agent_node_addresses();
                let run = self.create_node(server, kind, &attach).await;
                self.settle_create(run, &before, &attach, &format!("{kind:?}"))
            }
            ProjectEdit::ImportPattern(import) => {
                let slug = import.pattern.trim().trim_start_matches("catalog/");
                let Some(example) = crate::app::home::embedded_example(&format!("catalog/{slug}"))
                else {
                    return (
                        EditStatus::Rejected {
                            reason: format!("there is no catalog pattern {slug:?}"),
                        },
                        None,
                    );
                };
                let attach = match self.agent_attach(import.in_playlist.as_deref()) {
                    Ok(attach) => attach,
                    Err(reason) => return (EditStatus::Rejected { reason }, None),
                };
                let before = self.agent_node_addresses();
                let run = self
                    .import_pattern(
                        server,
                        &ImportSource::BuiltIn {
                            example_id: example.id.to_string(),
                        },
                        "effect",
                        &attach,
                    )
                    .await;
                self.settle_create(run, &before, &attach, &format!("pattern {slug}"))
            }
            ProjectEdit::RemoveNode(node) => {
                let address = match self.agent_node(&node.node) {
                    Ok(node) => node.address().clone(),
                    Err(reason) => return (EditStatus::Rejected { reason }, None),
                };
                let run = self.remove_node(server, &address).await;
                settle(run, || format!("removed `{}` (until Save)", node.node))
            }
            ProjectEdit::Set(set) => {
                let (node, edits) =
                    match self.agent_slot_edits(&set.node, &set.path, Some(&set.value)) {
                        Ok(found) => found,
                        Err(reason) => return (EditStatus::Rejected { reason }, None),
                    };
                self.apply_agent_slot_edits(server, node, edits, || {
                    format!("set {}.{} = {}", set.node, set.path, set.value)
                })
                .await
            }
            ProjectEdit::Ensure(slot) => {
                let (node, edits) = match self.agent_slot_edits(&slot.node, &slot.path, None) {
                    Ok(found) => found,
                    Err(reason) => return (EditStatus::Rejected { reason }, None),
                };
                self.apply_agent_slot_edits(server, node, edits, || {
                    format!("{}.{} is present", slot.node, slot.path)
                })
                .await
            }
            ProjectEdit::Remove(slot) => {
                let address = match self.agent_slot_address(&slot.node, &slot.path) {
                    Ok(address) => address,
                    Err(reason) => return (EditStatus::Rejected { reason }, None),
                };
                let edits = vec![lpc_model::SlotEdit::remove(address.path.clone())];
                self.apply_agent_slot_edits(server, address.node, edits, || {
                    format!("removed {}.{}", slot.node, slot.path)
                })
                .await
            }
            ProjectEdit::SetAsset(asset) => {
                let artifact = match self.agent_node(&asset.node).and_then(|node| {
                    self.node_asset_artifact(node, &asset.file).ok_or_else(|| {
                        format!(
                            "`{}` does not reference a file {:?}",
                            asset.node, asset.file
                        )
                    })
                }) {
                    Ok(artifact) => artifact,
                    Err(reason) => return (EditStatus::Rejected { reason }, None),
                };
                let run = self
                    .apply_asset_edit(
                        server,
                        AssetEditOp::ApplyBody {
                            artifact,
                            bytes: asset.text.as_bytes().to_vec(),
                        },
                    )
                    .await;
                settle(run, || {
                    format!("replaced {} ({} bytes)", asset.file, asset.text.len())
                })
            }
            // The Hardware row's own write: the library manifest and the
            // runtime's copy, kept byte-identical.
            ProjectEdit::SetTarget(target) => {
                let board = target.board.trim();
                if lpa_boards::board_by_id(board).is_none() {
                    return (
                        EditStatus::Rejected {
                            reason: format!("there is no board {board:?}"),
                        },
                        None,
                    );
                }
                match self.set_active_project_target(server, Some(board)).await {
                    Ok(()) => (
                        EditStatus::Applied {
                            detail: format!("board set to {board}"),
                        },
                        None,
                    ),
                    Err(error) => (
                        EditStatus::Rejected {
                            reason: error.to_string(),
                        },
                        None,
                    ),
                }
            }
        }
    }

    /// Resolve a create's outcome: the node that appeared is the result.
    /// A playlist entry is the exception — only the playing entry is
    /// mounted, so a new dormant entry never appears in the node tree; its
    /// acceptance is the op's own outcome.
    fn settle_create(
        &mut self,
        run: Result<ProjectEditRun, UiError>,
        before: &BTreeSet<ProjectNodeAddress>,
        attach: &UiAttachTarget,
        what: &str,
    ) -> (EditStatus, Option<ProjectEditRun>) {
        let run = match run {
            Ok(run) => run,
            Err(error) => {
                return (
                    EditStatus::Rejected {
                        reason: error.to_string(),
                    },
                    None,
                );
            }
        };
        if let Some(reason) = warning_text(&run) {
            return (EditStatus::Rejected { reason }, Some(run));
        }
        let Some(node) = self.created_node(before) else {
            if let UiAttachTarget::Playlist { node } = attach {
                return (
                    EditStatus::Applied {
                        detail: format!(
                            "added {what} as the next entry of `{}` (it is mounted when it plays)",
                            node_display_name(node)
                        ),
                    },
                    Some(run),
                );
            }
            return (
                EditStatus::Rejected {
                    reason: format!("{what} was accepted but no new node appeared"),
                },
                Some(run),
            );
        };
        let name = node_display_name(&node);
        (
            EditStatus::Applied {
                detail: format!("created {what} as `{name}`"),
            },
            Some(run),
        )
    }

    /// Send one agent edit's slot edits as one overlay batch.
    async fn apply_agent_slot_edits(
        &mut self,
        server: &mut StudioServerClient,
        node: ProjectNodeAddress,
        edits: Vec<lpc_model::SlotEdit>,
        detail: impl FnOnce() -> String,
    ) -> (EditStatus, Option<ProjectEditRun>) {
        match self.apply_def_edit_batch(server, &node, edits).await {
            Ok((run, None)) => (EditStatus::Applied { detail: detail() }, Some(run)),
            Ok((run, Some(reason))) => (EditStatus::Rejected { reason }, Some(run)),
            Err(error) => (
                EditStatus::Rejected {
                    reason: error.to_string(),
                },
                None,
            ),
        }
    }

    /// The leaf edits that write `value` (or, for `None`, make the path
    /// present) at `path` of `node`.
    fn agent_slot_edits(
        &self,
        node: &str,
        path: &str,
        value: Option<&serde_json::Value>,
    ) -> Result<(ProjectNodeAddress, Vec<lpc_model::SlotEdit>), String> {
        let node_controller = self.agent_node(node)?;
        let address = node_controller.address().clone();
        let path = SlotPath::parse(path).map_err(|_| format!("{path:?} is not a slot path"))?;
        let registry = self.slot_shape_registry();
        let kind = node_kind(node_controller)
            .ok_or_else(|| format!("`{node}` is a {} node", node_controller.kind()))?;
        let root_id = NodeDef::default_for_kind(kind).shape_id();
        let root = registry
            .get_shape(root_id)
            .ok_or_else(|| format!("no shape for {kind:?}"))?;
        let edits = match value {
            Some(value) => json_slot_edits(registry, root, &path, value)?,
            None => {
                let (_, path) = super::agent_slot_json::canonical_slot(registry, root, &path)
                    .ok_or_else(|| format!("`{path}` is not a field of `{node}`"))?;
                vec![lpc_model::SlotEdit::ensure_present(path)]
            }
        };
        Ok((address, edits))
    }

    fn agent_slot_address(&self, node: &str, path: &str) -> Result<ProjectSlotAddress, String> {
        let node = self.agent_node(node)?;
        let path = SlotPath::parse(path).map_err(|_| format!("{path:?} is not a slot path"))?;
        Ok(ProjectSlotAddress::new(
            node.address().clone(),
            ProjectSlotRoot::Def,
            path,
        ))
    }

    /// `None` = the project root; a playlist's name = its next entry.
    fn agent_attach(&self, playlist: Option<&str>) -> Result<UiAttachTarget, String> {
        let Some(name) = playlist else {
            return Ok(UiAttachTarget::ProjectRoot);
        };
        let node = self.agent_node(name)?;
        if !node.kind().eq_ignore_ascii_case("playlist") {
            return Err(format!("`{name}` is a {}, not a playlist", node.kind()));
        }
        Ok(UiAttachTarget::Playlist {
            node: node.address().clone(),
        })
    }

    /// A node by name (`fixture`) or by a `/`-separated path of names
    /// (`playlist/palette_waves/shader`) matched against the end of its
    /// address. Exactly one match, or an error that lists what exists.
    pub(crate) fn agent_node(&self, wanted: &str) -> Result<&NodeController, String> {
        let wanted: Vec<&str> = wanted
            .trim_matches('/')
            .split('/')
            .filter(|segment| !segment.is_empty())
            .collect();
        let mut all = Vec::new();
        collect_nodes(self.root_nodes(), &mut all);
        // The project root module is never an edit target by name.
        let candidates: Vec<&NodeController> = all
            .iter()
            .copied()
            .filter(|node| node.address().path().0.len() > 1)
            .filter(|node| {
                let names: Vec<&str> = node
                    .address()
                    .path()
                    .0
                    .iter()
                    .map(|segment| segment.name.as_str())
                    .collect();
                names.ends_with(&wanted)
            })
            .collect();
        match candidates.as_slice() {
            [one] => Ok(one),
            [] => Err(format!(
                "no node named {:?}; nodes: {}",
                wanted.join("/"),
                all.iter()
                    .filter(|node| node.address().path().0.len() > 1)
                    .map(|node| node_display_name(node.address()))
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
            many => Err(format!(
                "{:?} names {} nodes ({}); use a path",
                wanted.join("/"),
                many.len(),
                many.iter()
                    .map(|node| node_display_name(node.address()))
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
        }
    }

    /// A node's facts for the `read` tool: name, kind, status, issues, and
    /// the def artifact its definition is read from.
    pub(crate) fn agent_node_facts(
        &self,
        wanted: &str,
    ) -> Result<(serde_json::Value, Option<lpc_model::ArtifactLocation>), String> {
        let node = self.agent_node(wanted)?;
        let status = node.status();
        let facts = serde_json::json!({
            "node": node_display_name(node.address()),
            "kind": node.kind(),
            "status": status.label,
            "message": status.detail,
            "issues": node.issues(),
        });
        Ok((facts, self.def_artifact_for(node)))
    }

    /// The node a create made: the shallowest address that was not there
    /// `before` (an import also brings the module's own children).
    fn created_node(&self, before: &BTreeSet<ProjectNodeAddress>) -> Option<ProjectNodeAddress> {
        self.agent_node_addresses()
            .into_iter()
            .filter(|address| !before.contains(address))
            .min_by_key(|address| address.path().0.len())
    }

    fn agent_node_addresses(&self) -> BTreeSet<ProjectNodeAddress> {
        let mut all = Vec::new();
        collect_nodes(self.root_nodes(), &mut all);
        all.into_iter().map(|node| node.address().clone()).collect()
    }
}

/// One edit's outcome as the batch reports it: its status, and the node it
/// landed on (`None` for an edit about no node, or one that did not land).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AgentEditLanding {
    pub status: EditStatus,
    pub node: Option<ProjectNodeAddress>,
}

/// A node's name as the agent addresses it: its path of names below the
/// project root (`playlist/palette_waves`).
pub(crate) fn node_display_name(address: &ProjectNodeAddress) -> String {
    address
        .path()
        .0
        .iter()
        .skip(1)
        .map(|segment| segment.name.as_str())
        .collect::<Vec<_>>()
        .join("/")
}

fn collect_nodes<'a>(nodes: &'a [NodeController], out: &mut Vec<&'a NodeController>) {
    for node in nodes {
        out.push(node);
        collect_nodes(node.children(), out);
    }
}

/// A node's kind from its address's type segment (`fixture`, `playlist`,
/// `compute_shader`).
fn node_kind(node: &NodeController) -> Option<NodeKind> {
    let ty = node.address().path().0.last()?.ty.as_str().replace('_', "");
    NodeKind::ALL
        .into_iter()
        .find(|kind| format!("{kind:?}").eq_ignore_ascii_case(&ty))
        .or_else(|| match ty.as_str() {
            "show" | "project" => Some(NodeKind::Module),
            _ => None,
        })
}

fn parse_kind(text: &str) -> Result<NodeKind, String> {
    let folded = text.replace(['_', ' ', '-'], "");
    NodeKind::ALL
        .into_iter()
        .find(|kind| format!("{kind:?}").eq_ignore_ascii_case(&folded))
        .ok_or_else(|| {
            format!(
                "no node kind {text:?} (kinds: {})",
                NodeKind::ALL
                    .iter()
                    .map(|kind| format!("{kind:?}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
}

/// The first warning or error a run surfaced — what the person would have
/// read in a toast.
fn warning_text(run: &ProjectEditRun) -> Option<String> {
    let texts: Vec<&str> = run
        .notices
        .notices
        .iter()
        .filter(|notice| matches!(notice.level, UiNoticeLevel::Warning | UiNoticeLevel::Error))
        .map(|notice| notice.message.as_str())
        .collect();
    (!texts.is_empty()).then(|| texts.join("; "))
}

fn settle(
    run: Result<ProjectEditRun, UiError>,
    detail: impl FnOnce() -> String,
) -> (EditStatus, Option<ProjectEditRun>) {
    match run {
        Ok(run) => match warning_text(&run) {
            Some(reason) => (EditStatus::Rejected { reason }, Some(run)),
            None => (EditStatus::Applied { detail: detail() }, Some(run)),
        },
        Err(error) => (
            EditStatus::Rejected {
                reason: error.to_string(),
            },
            None,
        ),
    }
}
