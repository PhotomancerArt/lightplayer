//! Dedicated node-authoring semantics on the project registry.
//!
//! `create_node` and `remove_node` are the operations the `ModuleDef.nodes`
//! `read_only_persisted` policy promises: node creation and removal never
//! arrive as raw slot edits, they arrive here. Both address the node through
//! a [`NodeAttachSite`] — either the policy-locked project `nodes` map (the
//! dedicated-op bypass applies **only** to that site) or any writable
//! `NodeInvocationSlot`-shaped slot such as a playlist's `entries[k].node`.
//!
//! Creation atomically (validate all, then write) creates asset files plus
//! one node-def file and attaches the node. It commits immediately to the
//! project filesystem — it does not stage in the overlay (`ArtifactOverlay`
//! is slot-XOR-asset; a staged node body would vanish on reload). The
//! attachment rewrites the **base** file of the attach artifact, never the
//! effective definition, so pending overlay edits on that artifact stay
//! pending instead of being silently baked to disk. After the writes the
//! registry drives the same refresh path filesystem changes use, so callers
//! can feed the returned change summary straight into
//! `Engine::apply_project_changes`.
//!
//! Removal, unlike creation, **stages in the overlay** so it is revertible
//! until commit: a `Remove` slot edit at the site detaches the entry, and
//! `AssetBodyOverlay::Delete` entries mark the removed subtree's def and
//! exclusively referenced assets for deletion. Staging also sweeps every
//! pending overlay entry belonging to the removed subtree (the `Delete`
//! replaces it) — an orphaned slot overlay whose artifact left the effective
//! inventory would otherwise abort a later `commit_overlay` with
//! `CommitError::Projection`.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use lpc_model::{
    ArtifactChangeSummary, ArtifactLocation, ArtifactOverlay, AssetBodyOverlay, AssetLocation,
    LpValue, MutationOp, MutationRejection, MutationRejectionReason, NodeArtifact, NodeAttachSite,
    NodeDef, NodeDefLocation, NodeInvocation, ProjectChangeSummary, Revision, SlotAccess,
    SlotDataAccess, SlotEdit, SlotEnumShape, SlotMapKey, SlotName, SlotPath, SlotPathSegment,
    SlotShape, SlotShapeLookup, StaticSlotShape, lookup_slot_data,
};
use lpfs::{FsEvent, FsEventKind, LpFs, LpPath, LpPathBuf};

use crate::ParseCtx;
use crate::overlay::apply_op_to_def;
use crate::overlay::inventory_change_summary::change_summary_between;
use crate::registry::base_value_display;
use crate::registry::project_registry::{
    ProjectRegistry, effective_map_entry_presence, resolve_edit_policy, shape_at_path,
};

/// Outcome of an accepted [`ProjectRegistry::create_node`].
#[derive(Clone, Debug, PartialEq)]
pub struct CreateNodeOutcome {
    /// Files written by the operation: created def/asset files in `added`,
    /// the rewritten attach artifact in `changed`.
    pub artifact_changes: ArtifactChangeSummary,
    /// Effective inventory changes observed after the post-write refresh;
    /// feed these to `Engine::apply_project_changes`.
    pub changes: ProjectChangeSummary,
}

/// Outcome of an accepted [`ProjectRegistry::remove_node`].
#[derive(Clone, Debug, PartialEq)]
pub struct RemoveNodeOutcome {
    /// Artifacts staged `AssetBodyOverlay::Delete`: the removed node's def
    /// plus every descendant def and asset exclusively referenced by the
    /// removed subtree. Reverting the removal is `RemoveSlotEdit` at the
    /// site plus `ClearArtifact` on exactly these.
    pub staged_deletes: Vec<ArtifactLocation>,
    /// Whether staging swept pending overlay edits: edits at or under the
    /// removed entry on the site artifact, or pending intent on any
    /// removed-subtree artifact. Swept edits are not restored by revert.
    pub swept_pending_edits: bool,
    /// Effective inventory changes across the whole staging operation; feed
    /// these to `Engine::apply_project_changes` (`uses.removed` tears down
    /// the runtime subtree).
    pub changes: ProjectChangeSummary,
}

impl ProjectRegistry {
    /// Create a node: write `assets`, write the def file `file` with `body`,
    /// and attach the node at `attach` by rewriting the attach artifact's
    /// base file with the canonical writer.
    ///
    /// Everything is validated before anything is written; a rejection leaves
    /// the filesystem, overlay, and inventory untouched. A mid-write
    /// filesystem failure can still leave partial files — the same exposure
    /// `commit_overlay` accepts.
    pub fn create_node(
        &mut self,
        fs: &dyn LpFs,
        file: &LpPath,
        body: &[u8],
        assets: &[(LpPathBuf, Vec<u8>)],
        attach: &NodeAttachSite,
        frame: Revision,
        ctx: &ParseCtx<'_>,
    ) -> Result<CreateNodeOutcome, MutationRejection> {
        let plan = self.validate_create_node(fs, file, body, assets, attach, ctx)?;
        self.apply_create_node(fs, plan, body, assets, frame, ctx)
    }

    /// Validate every part of a create request against the current project
    /// without touching the filesystem or overlay.
    fn validate_create_node(
        &mut self,
        fs: &dyn LpFs,
        file: &LpPath,
        body: &[u8],
        assets: &[(LpPathBuf, Vec<u8>)],
        attach: &NodeAttachSite,
        ctx: &ParseCtx<'_>,
    ) -> Result<CreateNodePlan, MutationRejection> {
        let Some(root) = self.root().cloned() else {
            return Err(reject(
                MutationRejectionReason::Unsupported,
                "no project root is loaded".into(),
            ));
        };
        let root_artifact = root.artifact.clone();
        let root_dir = root_artifact
            .file_path()
            .as_path()
            .parent()
            .unwrap_or_else(|| LpPath::new("/"))
            .to_path_buf();

        // Body parses as a node definition; `Module` cannot be created as a
        // child node (nested sub-projects are future work).
        let text = core::str::from_utf8(body).map_err(|_| {
            reject(
                MutationRejectionReason::InvalidBody,
                "node body is not valid UTF-8".into(),
            )
        })?;
        let def = NodeDef::read_json(ctx.shapes, text).map_err(|err| {
            reject(
                MutationRejectionReason::InvalidBody,
                format!("node body does not parse as a node definition: {err}"),
            )
        })?;
        if matches!(def, NodeDef::Module(_)) {
            return Err(reject(
                MutationRejectionReason::InvalidBody,
                "kind Module cannot be created as a child node".into(),
            ));
        }

        // Def and asset paths: project-relative, path-safe, unoccupied on
        // disk and in the effective inventory, and unique within the request.
        let def_path = resolve_created_path(&root_dir, file).ok_or_else(|| invalid_path(file))?;
        self.reject_occupied_path(fs, &def_path)?;
        let mut asset_paths = Vec::with_capacity(assets.len());
        for (path, _) in assets {
            let resolved = resolve_created_path(&root_dir, path.as_path())
                .ok_or_else(|| invalid_path(path.as_path()))?;
            self.reject_occupied_path(fs, &resolved)?;
            if resolved == def_path || asset_paths.contains(&resolved) {
                return Err(reject(
                    MutationRejectionReason::InvalidPath,
                    format!("path {resolved} appears more than once in the request"),
                ));
            }
            asset_paths.push(resolved);
        }

        // Attach site.
        let (attach_artifact, attach_path) = match attach {
            NodeAttachSite::ProjectNodes { key } => {
                if key.is_empty() {
                    return Err(reject(
                        MutationRejectionReason::InvalidPath,
                        "project node key must not be empty".into(),
                    ));
                }
                let def = self.loaded_def_for_mutation(&root_artifact)?;
                let Some(project) = def.as_module() else {
                    return Err(reject(
                        MutationRejectionReason::UnknownArtifact,
                        "project root definition is not a Project".into(),
                    ));
                };
                // Effective map: an overlay-staged entry counts as occupied.
                if project.nodes.entries.contains_key(key) {
                    return Err(reject(
                        MutationRejectionReason::TargetOccupied,
                        format!("project node key {key} is already occupied"),
                    ));
                }
                let path = SlotPath::root()
                    .child(slot_name("nodes"))
                    .child_key(SlotMapKey::String(key.clone()));
                (root_artifact.clone(), path)
            }
            NodeAttachSite::Slot { artifact, path } => {
                let def = self.loaded_def_for_mutation(artifact)?;
                let Some(resolution) = resolve_edit_policy(def, path, ctx) else {
                    return Err(reject(
                        MutationRejectionReason::UnknownSlotPath,
                        format!(
                            "attach path {path} does not resolve in artifact {}",
                            artifact.file_path()
                        ),
                    ));
                };
                // The dedicated-op policy bypass applies only to the
                // `ProjectNodes` site; slot sites take the standard check.
                if !resolution.policy.writable {
                    return Err(reject(
                        MutationRejectionReason::NotWritable,
                        format!("attach slot {path} is not writable"),
                    ));
                }
                let invocation_shaped =
                    attach_slot_shape(def, path, ctx).is_some_and(is_node_invocation_slot_shape);
                if !invocation_shaped {
                    return Err(reject(
                        MutationRejectionReason::UnknownSlotPath,
                        format!("attach path {path} does not address a node invocation slot"),
                    ));
                }
                self.reject_occupied_attach_slot(def, path, ctx)?;
                (artifact.clone(), path.clone())
            }
        };

        // The attachment rewrites the attach artifact's base file, so the
        // base must parse before anything is written — and a base-present
        // target masked by a pending overlay removal is still occupied
        // (rewriting it would clobber saved state).
        let Some(attach_base) = self.parse_base_def(fs, &attach_artifact, ctx) else {
            return Err(reject(
                MutationRejectionReason::EditFailed,
                format!(
                    "base definition {} is unreadable",
                    attach_artifact.file_path()
                ),
            ));
        };
        if let Some((_, _, prefix)) = deepest_key_prefix(&attach_path)
            && base_value_display::base_presence_in_def(&attach_base, &prefix, ctx)
        {
            return Err(reject(
                MutationRejectionReason::TargetOccupied,
                format!("attach target {prefix} is present in the saved file"),
            ));
        }

        let ref_text = invocation_ref_text(&attach_artifact, def_path.as_path());
        Ok(CreateNodePlan {
            def_path,
            asset_paths,
            attach_artifact,
            attach_path,
            attach_base,
            ref_text,
        })
    }

    /// Apply a validated create: assets → def file → attachment rewrite, then
    /// the same refresh path filesystem changes use.
    fn apply_create_node(
        &mut self,
        fs: &dyn LpFs,
        plan: CreateNodePlan,
        body: &[u8],
        assets: &[(LpPathBuf, Vec<u8>)],
        frame: Revision,
        ctx: &ParseCtx<'_>,
    ) -> Result<CreateNodeOutcome, MutationRejection> {
        let mut events = Vec::with_capacity(plan.asset_paths.len() + 2);
        let mut artifact_changes = ArtifactChangeSummary::default();

        for ((_, bytes), path) in assets.iter().zip(&plan.asset_paths) {
            write_created_file(fs, path, bytes)?;
            events.push(FsEvent {
                path: path.clone(),
                kind: FsEventKind::Create,
            });
            artifact_changes
                .added
                .push(ArtifactLocation::file(path.clone()));
        }

        write_created_file(fs, &plan.def_path, body)?;
        events.push(FsEvent {
            path: plan.def_path.clone(),
            kind: FsEventKind::Create,
        });
        artifact_changes
            .added
            .push(ArtifactLocation::file(plan.def_path.clone()));

        // Attachment: rewrite the BASE file of the attach artifact with the
        // canonical writer (kind-first, deterministic). One `AssignValue` at
        // `<attach_path>.ref` does the whole gesture through the shared
        // ensure-then-set machinery: missing map entries (e.g. a playlist's
        // `entries[k]`) default-construct, the invocation enum switches to
        // `ref`, and the path string lands in the leaf.
        let mut base = plan.attach_base;
        let edit = SlotEdit::assign_value(
            plan.attach_path.child(slot_name("ref")),
            LpValue::String(plan.ref_text),
        );
        apply_op_to_def(&mut base, &edit, ctx, frame).map_err(|err| {
            reject(
                MutationRejectionReason::EditFailed,
                format!("cannot attach node at {}: {err}", plan.attach_path),
            )
        })?;
        let bytes = crate::overlay::serialize_slot_draft(&base, ctx).map_err(|err| {
            reject(
                MutationRejectionReason::EditFailed,
                format!(
                    "cannot serialize {}: {err}",
                    plan.attach_artifact.file_path()
                ),
            )
        })?;
        fs.write_file(plan.attach_artifact.file_path().as_path(), &bytes)
            .map_err(|err| {
                reject(
                    MutationRejectionReason::EditFailed,
                    format!("cannot write {}: {err}", plan.attach_artifact.file_path()),
                )
            })?;
        events.push(FsEvent {
            path: plan.attach_artifact.file_path().clone(),
            kind: FsEventKind::Modify,
        });
        artifact_changes.changed.push(plan.attach_artifact.clone());

        let changes = self.refresh_artifacts(fs, &events, frame, ctx);
        Ok(CreateNodeOutcome {
            artifact_changes,
            changes,
        })
    }

    /// Reject a def/asset target path that already exists on disk or in the
    /// effective inventory.
    fn reject_occupied_path(
        &self,
        fs: &dyn LpFs,
        path: &LpPathBuf,
    ) -> Result<(), MutationRejection> {
        let occupied_on_disk = fs.file_exists(path.as_path()).unwrap_or(false);
        let artifact = ArtifactLocation::file(path.clone());
        let occupied_as_def = self
            .inventory()
            .defs
            .get(&NodeDefLocation::artifact_root(artifact.clone()))
            .is_some();
        let occupied_as_asset = self
            .inventory()
            .assets
            .get(&AssetLocation::artifact(artifact))
            .is_some();
        if occupied_on_disk || occupied_as_def || occupied_as_asset {
            return Err(reject(
                MutationRejectionReason::TargetOccupied,
                format!("file {path} already exists"),
            ));
        }
        Ok(())
    }

    /// Reject a slot-site attach path whose target already exists in the
    /// **effective** definition: the deepest map-entry prefix must be absent
    /// (fresh playlist entry), and a bare invocation slot must still be
    /// `unset`.
    fn reject_occupied_attach_slot(
        &self,
        def: &NodeDef,
        path: &SlotPath,
        ctx: &ParseCtx<'_>,
    ) -> Result<(), MutationRejection> {
        if let Some((map_path, key, prefix)) = deepest_key_prefix(path) {
            match effective_map_entry_presence(def, ctx, &map_path, &key) {
                Ok(true) => Err(reject(
                    MutationRejectionReason::TargetOccupied,
                    format!("attach target {prefix} already exists in the effective definition"),
                )),
                Ok(false) => Ok(()),
                Err(message) => Err(reject(MutationRejectionReason::UnknownSlotPath, message)),
            }
        } else {
            // No map entry to create: the invocation slot itself must be
            // vacant (`unset` is `NodeInvocation`'s authored empty variant).
            let occupied = lookup_slot_data(def, ctx.shapes, path).is_ok_and(
                |data| matches!(data, SlotDataAccess::Enum(en) if en.variant() != "unset"),
            );
            if occupied {
                Err(reject(
                    MutationRejectionReason::TargetOccupied,
                    format!("attach slot {path} already invokes a node"),
                ))
            } else {
                Ok(())
            }
        }
    }

    /// Remove the node attached at `site` by **staging** in the overlay: a
    /// `Remove` slot edit at the attachment entry (the whole map entry for
    /// `nodes[key]` / `entries[k]` sites), plus `AssetBodyOverlay::Delete` on
    /// the removed node's def and everything exclusively referenced by the
    /// removed subtree. Shared defs/assets (still referenced by a surviving
    /// node) are never staged for deletion — the entry removal alone detaches
    /// them here.
    ///
    /// Staging replaces any pending overlay intent belonging to the removed
    /// subtree (the recursive sweep): edits at or under the removed entry are
    /// cleared by the slot overlay's `Remove` canonicalization, and each
    /// staged `Delete` supersedes the artifact's previous overlay entry, so
    /// no orphaned slot overlay can wedge a later `commit_overlay`.
    ///
    /// The site must resolve in the **effective** inventory. Everything is
    /// validated before anything is staged; a rejection leaves the overlay
    /// and inventory untouched. The `ProjectNodes` site bypasses the `nodes`
    /// map's `read_only_persisted` policy (the dedicated-op contract); slot
    /// sites take the standard writable-policy check.
    pub fn remove_node(
        &mut self,
        fs: &dyn LpFs,
        site: &NodeAttachSite,
        frame: Revision,
        ctx: &ParseCtx<'_>,
    ) -> Result<RemoveNodeOutcome, MutationRejection> {
        let (site_artifact, entry_path) = self.validate_remove_site(site, ctx)?;
        let before = self.inventory().clone();

        // Pending edits at or under the removed entry are swept by the slot
        // overlay's `Remove` canonicalization; record them before staging.
        let mut swept_pending_edits = self
            .overlay()
            .get()
            .artifact(&site_artifact)
            .and_then(ArtifactOverlay::as_slot)
            .is_some_and(|overlay| {
                overlay
                    .edits
                    .iter()
                    .any(|(path, _)| is_at_or_under(&entry_path, path))
            });

        // Stage the entry removal through the singular mutation path: it is
        // deliberately unvalidated (the dedicated-op bypass of the `nodes`
        // map's read_only_persisted policy), normalizes an overlay-only entry
        // away instead of storing a no-op edit, and re-derives the effective
        // inventory. `PutSlotEdit` cannot fail on that path.
        let removal = self
            .mutate(
                fs,
                MutationOp::PutSlotEdit {
                    artifact: site_artifact.clone(),
                    edit: SlotEdit::remove(entry_path.clone()),
                },
                frame,
                ctx,
            )
            .map_err(|err| {
                reject(
                    MutationRejectionReason::EditFailed,
                    format!("cannot stage removal at {entry_path}: {err}"),
                )
            })?;

        // Everything that left the effective inventory is exactly the
        // subtree exclusively referenced through the removed entry: defs and
        // assets still referenced by a surviving node stay in the inventory
        // by construction and are never staged for deletion.
        let mut subtree = Vec::new();
        for location in &removal.changes.defs.removed {
            push_unique(&mut subtree, location.artifact.clone());
        }
        for asset in &removal.changes.assets.removed {
            let AssetLocation::Artifact { location } = asset;
            push_unique(&mut subtree, location.clone());
        }

        let mut staged_deletes = Vec::new();
        for artifact in subtree {
            // Cross-class guard: an artifact that left the inventory in one
            // role but still serves another effective role is shared.
            if self.artifact_is_effective(&artifact) {
                continue;
            }
            let had_overlay = self.overlay().get().contains_artifact(&artifact);
            let on_disk = fs
                .file_exists(artifact.file_path().as_path())
                .unwrap_or(false);
            // Nothing on disk and nothing pending (e.g. a dangling-ref def
            // that never existed): no delete to stage, no orphan to sweep.
            if !had_overlay && !on_disk {
                continue;
            }
            swept_pending_edits |= had_overlay;
            // `Delete` replaces the artifact's previous overlay entry — this
            // is the sweep: an orphaned slot overlay on an artifact outside
            // the effective inventory would abort `commit_overlay` with
            // `CommitError::Projection`.
            self.mutate(
                fs,
                MutationOp::SetArtifactBody {
                    artifact: artifact.clone(),
                    edit: AssetBodyOverlay::Delete,
                },
                frame,
                ctx,
            )
            .map_err(|err| {
                reject(
                    MutationRejectionReason::EditFailed,
                    format!("cannot stage deletion of {}: {err}", artifact.file_path()),
                )
            })?;
            staged_deletes.push(artifact);
        }

        let changes = change_summary_between(&before, self.inventory());
        Ok(RemoveNodeOutcome {
            staged_deletes,
            swept_pending_edits,
            changes,
        })
    }

    /// Resolve a removal site against the effective inventory: the site
    /// artifact plus the slot path of the **whole entry** to remove (the map
    /// entry for keyed sites, the invocation slot itself for bare slots).
    fn validate_remove_site(
        &self,
        site: &NodeAttachSite,
        ctx: &ParseCtx<'_>,
    ) -> Result<(ArtifactLocation, SlotPath), MutationRejection> {
        match site {
            NodeAttachSite::ProjectNodes { key } => {
                let Some(root) = self.root() else {
                    return Err(reject(
                        MutationRejectionReason::Unsupported,
                        "no project root is loaded".into(),
                    ));
                };
                let root_artifact = root.artifact.clone();
                let def = self.loaded_def_for_mutation(&root_artifact)?;
                let Some(project) = def.as_module() else {
                    return Err(reject(
                        MutationRejectionReason::UnknownArtifact,
                        "project root definition is not a Project".into(),
                    ));
                };
                // Effective map: an overlay-staged entry counts as present.
                if !project.nodes.entries.contains_key(key) {
                    return Err(reject(
                        MutationRejectionReason::UnknownSlotPath,
                        format!("project node key {key} does not exist"),
                    ));
                }
                let path = SlotPath::root()
                    .child(slot_name("nodes"))
                    .child_key(SlotMapKey::String(key.clone()));
                Ok((root_artifact, path))
            }
            NodeAttachSite::Slot { artifact, path } => {
                let def = self.loaded_def_for_mutation(artifact)?;
                let Some(resolution) = resolve_edit_policy(def, path, ctx) else {
                    return Err(reject(
                        MutationRejectionReason::UnknownSlotPath,
                        format!(
                            "remove path {path} does not resolve in artifact {}",
                            artifact.file_path()
                        ),
                    ));
                };
                // The dedicated-op policy bypass applies only to the
                // `ProjectNodes` site; slot sites take the standard check.
                if !resolution.policy.writable {
                    return Err(reject(
                        MutationRejectionReason::NotWritable,
                        format!("remove slot {path} is not writable"),
                    ));
                }
                let invocation_shaped =
                    attach_slot_shape(def, path, ctx).is_some_and(is_node_invocation_slot_shape);
                if !invocation_shaped {
                    return Err(reject(
                        MutationRejectionReason::UnknownSlotPath,
                        format!("remove path {path} does not address a node invocation slot"),
                    ));
                }
                if let Some((map_path, key, prefix)) = deepest_key_prefix(path) {
                    // Keyed site (e.g. a playlist's `entries[k].node`): the
                    // whole `entries[k]` entry is removed, and it must exist
                    // in the effective definition.
                    match effective_map_entry_presence(def, ctx, &map_path, &key) {
                        Ok(true) => Ok((artifact.clone(), prefix)),
                        Ok(false) => Err(reject(
                            MutationRejectionReason::UnknownSlotPath,
                            format!(
                                "remove target {prefix} does not exist in the effective definition"
                            ),
                        )),
                        Err(message) => {
                            Err(reject(MutationRejectionReason::UnknownSlotPath, message))
                        }
                    }
                } else {
                    // Bare invocation slot outside a map entry: removing it
                    // resets the slot, and it must currently invoke a node.
                    let occupied = lookup_slot_data(def, ctx.shapes, path).is_ok_and(
                        |data| matches!(data, SlotDataAccess::Enum(en) if en.variant() != "unset"),
                    );
                    if occupied {
                        Ok((artifact.clone(), path.clone()))
                    } else {
                        Err(reject(
                            MutationRejectionReason::UnknownSlotPath,
                            format!("remove slot {path} does not invoke a node"),
                        ))
                    }
                }
            }
        }
    }

    /// Whether `artifact` still backs any def or asset in the effective
    /// inventory.
    fn artifact_is_effective(&self, artifact: &ArtifactLocation) -> bool {
        self.inventory()
            .defs
            .get(&NodeDefLocation::artifact_root(artifact.clone()))
            .is_some()
            || self
                .inventory()
                .assets
                .get(&AssetLocation::artifact(artifact.clone()))
                .is_some()
    }
}

/// Validated create request, ready to apply.
struct CreateNodePlan {
    def_path: LpPathBuf,
    asset_paths: Vec<LpPathBuf>,
    attach_artifact: ArtifactLocation,
    attach_path: SlotPath,
    attach_base: NodeDef,
    ref_text: String,
}

/// Resolve a requested create path against the project root directory.
///
/// Accepts `./name.json`, `name.json`, and absolute in-project paths; resolves
/// `.`/`..` components and returns `None` when the path is empty, escapes the
/// project root, or names the root directory itself.
fn resolve_created_path(root_dir: &LpPath, path: &LpPath) -> Option<LpPathBuf> {
    if path.as_str().is_empty() {
        return None;
    }
    let resolved = if path.is_absolute() {
        LpPathBuf::from("/").join_relative(path.as_str().trim_start_matches('/'))?
    } else {
        root_dir.to_path_buf().join_relative(path.as_str())?
    };
    if !resolved.as_path().starts_with(root_dir.as_str()) || resolved.as_path() == root_dir {
        return None;
    }
    Some(resolved)
}

/// Authored invocation ref for `def_path` as seen from the attach artifact:
/// `./relative` when the def lives under the artifact's directory, the
/// absolute project path otherwise.
fn invocation_ref_text(attach_artifact: &ArtifactLocation, def_path: &LpPath) -> String {
    let dir = attach_artifact
        .file_path()
        .as_path()
        .parent()
        .unwrap_or_else(|| LpPath::new("/"));
    match def_path.strip_prefix(dir.as_str()) {
        Some(rest) => format!("./{}", rest.as_str().trim_start_matches('/')),
        None => def_path.as_str().to_string(),
    }
}

/// Shape at `path` inside `def`, following the same root-shape rule as
/// mutation validation (a leading artifact root variant segment resolves
/// against the artifact wrapper shape).
fn attach_slot_shape<'s>(
    def: &NodeDef,
    path: &SlotPath,
    ctx: &ParseCtx<'s>,
) -> Option<lpc_model::SlotShapeView<'s>> {
    let root_shape_id = match path.segments().first() {
        Some(SlotPathSegment::Field(name)) if NodeDef::is_variant_name(name.as_str()) => {
            NodeArtifact::SHAPE_ID
        }
        _ => def.shape_id(),
    };
    let root = ctx.shapes.get_shape(root_shape_id)?;
    shape_at_path(root, ctx, path.segments())
}

/// True when `shape` is the `NodeInvocationSlot` enum. Keyed to the model's
/// own [`NodeInvocation::slot_enum_shape`] variant list so the check follows
/// the model if invocation variants ever change.
fn is_node_invocation_slot_shape(shape: lpc_model::SlotShapeView<'_>) -> bool {
    let SlotShape::Enum { variants, .. } = NodeInvocation::slot_enum_shape() else {
        return false;
    };
    let mut index = 0;
    while let Some(variant) = shape.enum_variant(index) {
        let Some(expected) = variants.get(index) else {
            return false;
        };
        if variant.name_str() != expected.name.as_str() {
            return false;
        }
        index += 1;
    }
    index == variants.len() && index > 0
}

/// Split the deepest map-entry prefix out of `path`: the map path, the key,
/// and the prefix ending at the key segment. `None` when the path carries no
/// key segment.
fn deepest_key_prefix(path: &SlotPath) -> Option<(SlotPath, SlotMapKey, SlotPath)> {
    let segments = path.segments();
    let index = segments
        .iter()
        .rposition(|segment| matches!(segment, SlotPathSegment::Key(_)))?;
    let SlotPathSegment::Key(key) = &segments[index] else {
        unreachable!("rposition matched a key segment");
    };
    Some((
        SlotPath::from_segments(segments[..index].to_vec()),
        key.clone(),
        SlotPath::from_segments(segments[..=index].to_vec()),
    ))
}

/// True when `path` equals `ancestor` or lies strictly under it.
fn is_at_or_under(ancestor: &SlotPath, path: &SlotPath) -> bool {
    path.segments().len() >= ancestor.segments().len()
        && path.segments().starts_with(ancestor.segments())
}

fn push_unique(artifacts: &mut Vec<ArtifactLocation>, artifact: ArtifactLocation) {
    if !artifacts.contains(&artifact) {
        artifacts.push(artifact);
    }
}

fn write_created_file(
    fs: &dyn LpFs,
    path: &LpPathBuf,
    bytes: &[u8],
) -> Result<(), MutationRejection> {
    fs.write_file(path.as_path(), bytes).map_err(|err| {
        reject(
            MutationRejectionReason::EditFailed,
            format!("cannot write {path}: {err}"),
        )
    })
}

fn reject(reason: MutationRejectionReason, message: String) -> MutationRejection {
    MutationRejection::new(reason, message)
}

fn invalid_path(path: &LpPath) -> MutationRejection {
    reject(
        MutationRejectionReason::InvalidPath,
        format!(
            "path `{}` is empty, not project-relative, or escapes the project root",
            path.as_str()
        ),
    )
}

fn slot_name(name: &str) -> SlotName {
    SlotName::parse(name).expect("static slot name is valid")
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use lpc_model::{MutationOp, NodeDefState, NodeUseLocation, SlotShapeRegistry, TextureDef};
    use lpfs::LpFsMemory;

    #[test]
    fn create_and_attach_at_project_nodes_lands_in_inventory_tree_and_files() {
        let shapes = SlotShapeRegistry::default();
        let (fs, mut registry) = clock_project(&shapes);
        let body = texture_body(&shapes);

        let outcome = create(
            &fs,
            &mut registry,
            &shapes,
            "./texture.json",
            body.as_bytes(),
            &[],
            &project_nodes("texture"),
        )
        .expect("create accepted");

        // Inventory: the def loads; the tree carries the new use.
        let def_location = root_def("/texture.json");
        let entry = registry.def(&def_location).expect("texture def entry");
        assert!(matches!(entry.state, NodeDefState::Loaded(_)));
        let texture_use = NodeUseLocation::root().child(SlotPath::parse("nodes[texture]").unwrap());
        assert!(
            registry
                .inventory()
                .tree
                .nodes
                .values()
                .any(|node| node.key == texture_use)
        );
        assert!(outcome.changes.defs.added.contains(&def_location));
        assert!(outcome.changes.uses.added.contains(&texture_use));
        assert_eq!(
            outcome.artifact_changes.added,
            vec![ArtifactLocation::file("/texture.json")]
        );
        assert_eq!(
            outcome.artifact_changes.changed,
            vec![ArtifactLocation::file("/project.json")]
        );

        // Files byte-match the canonical writer: the def is written verbatim
        // (the caller sent canonical bytes) and the rewritten project root
        // re-serializes to exactly its on-disk bytes.
        assert_eq!(
            fs.read_file(LpPath::new("/texture.json")).unwrap(),
            body.as_bytes()
        );
        let project_bytes = fs.read_file(LpPath::new("/project.json")).unwrap();
        let project_text = core::str::from_utf8(&project_bytes).unwrap();
        let project = NodeDef::read_json(&shapes, project_text).expect("project reparses");
        assert_eq!(
            project.write_json(&shapes).unwrap().as_bytes(),
            project_bytes.as_slice(),
            "attachment rewrite must be canonical-writer output"
        );
        let invocation = project
            .as_module()
            .unwrap()
            .nodes
            .entries
            .get("texture")
            .expect("texture entry")
            .value()
            .ref_specifier()
            .expect("ref invocation");
        assert_eq!(
            lpc_model::resolve_artifact_specifier(LpPath::new("/project.json"), &invocation)
                .unwrap(),
            LpPathBuf::from("/texture.json")
        );
    }

    #[test]
    fn create_with_scaffold_asset_writes_both_files_and_def_references_asset() {
        let shapes = SlotShapeRegistry::default();
        let (fs, mut registry) = clock_project(&shapes);
        let body = NodeDef::from_json_str(
            r#"{ "kind": "Shader", "source": { "path": "./shader-2.glsl" } }"#,
        )
        .expect("shader def")
        .write_json(&shapes)
        .expect("canonical shader body");
        let glsl = b"vec4 render(vec2 pos) { return vec4(1.0); }".to_vec();

        create(
            &fs,
            &mut registry,
            &shapes,
            "./shader-2.json",
            body.as_bytes(),
            &[(LpPathBuf::from("./shader-2.glsl"), glsl.clone())],
            &project_nodes("shader-2"),
        )
        .expect("create accepted");

        assert!(fs.file_exists(LpPath::new("/shader-2.json")).unwrap());
        assert_eq!(fs.read_file(LpPath::new("/shader-2.glsl")).unwrap(), glsl);
        assert!(
            registry
                .asset(&AssetLocation::artifact(ArtifactLocation::file(
                    "/shader-2.glsl"
                )))
                .is_some(),
            "created def must reference the scaffold asset in the inventory"
        );
    }

    #[test]
    fn create_and_attach_at_playlist_entry_discovers_child_def() {
        let shapes = SlotShapeRegistry::default();
        let (fs, mut registry) = playlist_project(&shapes);
        let body = texture_body(&shapes);
        let attach = NodeAttachSite::Slot {
            artifact: ArtifactLocation::file("/playlist.json"),
            path: SlotPath::parse("entries[2].node").unwrap(),
        };

        let outcome = create(
            &fs,
            &mut registry,
            &shapes,
            "./active.json",
            body.as_bytes(),
            &[],
            &attach,
        )
        .expect("create accepted");

        assert!(
            outcome
                .changes
                .defs
                .added
                .contains(&root_def("/active.json"))
        );
        let child_use = NodeUseLocation::root()
            .child(SlotPath::parse("nodes[playlist]").unwrap())
            .child(SlotPath::parse("entries[2].node").unwrap());
        assert!(outcome.changes.uses.added.contains(&child_use));

        // The playlist base file was rewritten with the new entry while the
        // existing entry survived.
        let playlist_bytes = fs.read_file(LpPath::new("/playlist.json")).unwrap();
        let playlist_text = core::str::from_utf8(&playlist_bytes).unwrap();
        let playlist = NodeDef::read_json(&shapes, playlist_text).expect("playlist reparses");
        let playlist = playlist.as_playlist().expect("playlist def");
        assert!(playlist.entries.entries.contains_key(&1));
        let entry = playlist.entries.entries.get(&2).expect("new entry");
        assert_eq!(
            lpc_model::resolve_artifact_specifier(
                LpPath::new("/playlist.json"),
                &entry.node.value().ref_specifier().unwrap(),
            )
            .unwrap(),
            LpPathBuf::from("/active.json")
        );
    }

    #[test]
    fn attach_rewrite_preserves_pending_overlay_edits_on_the_attach_artifact() {
        let shapes = SlotShapeRegistry::default();
        let (fs, mut registry) = clock_project(&shapes);
        let project = ArtifactLocation::file("/project.json");
        let name_path = SlotPath::parse("name.some").unwrap();
        registry
            .mutate(
                &fs,
                MutationOp::PutSlotEdit {
                    artifact: project.clone(),
                    edit: SlotEdit::assign_value(
                        name_path.clone(),
                        LpValue::String("Renamed".into()),
                    ),
                },
                Revision::new(4),
                &ParseCtx { shapes: &shapes },
            )
            .expect("stage name edit");

        create(
            &fs,
            &mut registry,
            &shapes,
            "./texture.json",
            texture_body(&shapes).as_bytes(),
            &[],
            &project_nodes("texture"),
        )
        .expect("create accepted");

        // The pending edit is still pending, not baked into the file.
        let overlay = registry
            .overlay()
            .get()
            .artifact(&project)
            .and_then(lpc_model::ArtifactOverlay::as_slot)
            .expect("project slot overlay survives");
        assert!(overlay.contains_path(&name_path));
        let written =
            String::from_utf8(fs.read_file(LpPath::new("/project.json")).unwrap()).unwrap();
        assert!(
            !written.contains("Renamed"),
            "pending overlay edit must not be baked into the base file: {written}"
        );

        // The effective def carries both the pending rename and the new node.
        let effective = registry
            .def(&root_def("/project.json"))
            .and_then(|entry| entry.state.loaded_def())
            .and_then(NodeDef::as_module)
            .expect("effective project def");
        assert_eq!(effective.name(), Some("Renamed"));
        assert!(effective.nodes.entries.contains_key("texture"));
    }

    #[test]
    fn create_rejects_occupied_key_including_overlay_staged_and_writes_nothing() {
        let shapes = SlotShapeRegistry::default();
        let (fs, mut registry) = clock_project(&shapes);
        let body = texture_body(&shapes);

        // Base-occupied key.
        let rejection = create(
            &fs,
            &mut registry,
            &shapes,
            "./texture.json",
            body.as_bytes(),
            &[],
            &project_nodes("clock"),
        )
        .expect_err("occupied key rejects");
        assert_eq!(rejection.reason, MutationRejectionReason::TargetOccupied);
        assert_nothing_written(&fs, "/texture.json");

        // Overlay-staged key: stage `nodes[strip]` through the unvalidated
        // single-mutation path (the validated path rejects the read-only
        // map), then create at the same key — the EFFECTIVE map counts.
        registry
            .mutate(
                &fs,
                MutationOp::PutSlotEdit {
                    artifact: ArtifactLocation::file("/project.json"),
                    edit: SlotEdit::ensure_present(SlotPath::parse("nodes[strip]").unwrap()),
                },
                Revision::new(4),
                &ParseCtx { shapes: &shapes },
            )
            .expect("stage overlay entry");
        let rejection = create(
            &fs,
            &mut registry,
            &shapes,
            "./texture.json",
            body.as_bytes(),
            &[],
            &project_nodes("strip"),
        )
        .expect_err("overlay-occupied key rejects");
        assert_eq!(rejection.reason, MutationRejectionReason::TargetOccupied);
        assert_nothing_written(&fs, "/texture.json");
    }

    #[test]
    fn create_rejects_occupied_file_path_and_writes_nothing() {
        let shapes = SlotShapeRegistry::default();
        let (fs, mut registry) = clock_project(&shapes);

        let rejection = create(
            &fs,
            &mut registry,
            &shapes,
            "./clock.json",
            texture_body(&shapes).as_bytes(),
            &[],
            &project_nodes("clock-2"),
        )
        .expect_err("occupied file path rejects");

        assert_eq!(rejection.reason, MutationRejectionReason::TargetOccupied);
        assert_nothing_written(&fs, "/never-written.json");
    }

    #[test]
    fn create_rejects_unparsable_and_project_kind_bodies() {
        let shapes = SlotShapeRegistry::default();
        let (fs, mut registry) = clock_project(&shapes);

        let rejection = create(
            &fs,
            &mut registry,
            &shapes,
            "./new.json",
            b"{ not json",
            &[],
            &project_nodes("new"),
        )
        .expect_err("unparsable body rejects");
        assert_eq!(rejection.reason, MutationRejectionReason::InvalidBody);
        assert_nothing_written(&fs, "/new.json");

        let project_body = br#"{ "kind": "Module", "format": 2, "nodes": {} }"#;
        let rejection = create(
            &fs,
            &mut registry,
            &shapes,
            "./new.json",
            project_body,
            &[],
            &project_nodes("new"),
        )
        .expect_err("Project kind body rejects");
        assert_eq!(rejection.reason, MutationRejectionReason::InvalidBody);
        assert_nothing_written(&fs, "/new.json");
    }

    #[test]
    fn create_rejects_bad_attach_sites_and_writes_nothing() {
        let shapes = SlotShapeRegistry::default();
        let (fs, mut registry) = clock_project(&shapes);
        let body = texture_body(&shapes);

        // Read-only slot site: `nodes[…]` on the project root is invocation-
        // shaped but policy-locked — only the dedicated `ProjectNodes` site
        // bypasses the policy.
        let rejection = create(
            &fs,
            &mut registry,
            &shapes,
            "./new.json",
            body.as_bytes(),
            &[],
            &NodeAttachSite::Slot {
                artifact: ArtifactLocation::file("/project.json"),
                path: SlotPath::parse("nodes[new]").unwrap(),
            },
        )
        .expect_err("read-only attach path rejects");
        assert_eq!(rejection.reason, MutationRejectionReason::NotWritable);
        assert_nothing_written(&fs, "/new.json");

        // Writable but not invocation-shaped.
        let rejection = create(
            &fs,
            &mut registry,
            &shapes,
            "./new.json",
            body.as_bytes(),
            &[],
            &NodeAttachSite::Slot {
                artifact: ArtifactLocation::file("/clock.json"),
                path: SlotPath::parse("controls.rate").unwrap(),
            },
        )
        .expect_err("non-invocation attach path rejects");
        assert_eq!(rejection.reason, MutationRejectionReason::UnknownSlotPath);
        assert_nothing_written(&fs, "/new.json");

        // Missing attach artifact.
        let rejection = create(
            &fs,
            &mut registry,
            &shapes,
            "./new.json",
            body.as_bytes(),
            &[],
            &NodeAttachSite::Slot {
                artifact: ArtifactLocation::file("/missing.json"),
                path: SlotPath::parse("entries[2].node").unwrap(),
            },
        )
        .expect_err("missing attach artifact rejects");
        assert_eq!(rejection.reason, MutationRejectionReason::UnknownArtifact);
        assert_nothing_written(&fs, "/new.json");
    }

    #[test]
    fn create_rejects_occupied_playlist_entry() {
        let shapes = SlotShapeRegistry::default();
        let (fs, mut registry) = playlist_project(&shapes);

        let rejection = create(
            &fs,
            &mut registry,
            &shapes,
            "./active.json",
            texture_body(&shapes).as_bytes(),
            &[],
            &NodeAttachSite::Slot {
                artifact: ArtifactLocation::file("/playlist.json"),
                path: SlotPath::parse("entries[1].node").unwrap(),
            },
        )
        .expect_err("occupied playlist entry rejects");

        assert_eq!(rejection.reason, MutationRejectionReason::TargetOccupied);
        assert_nothing_written(&fs, "/active.json");
    }

    #[test]
    fn create_rejects_unsafe_paths() {
        let shapes = SlotShapeRegistry::default();
        let (fs, mut registry) = clock_project(&shapes);
        let body = texture_body(&shapes);

        for path in ["../evil.json", ""] {
            let rejection = create(
                &fs,
                &mut registry,
                &shapes,
                path,
                body.as_bytes(),
                &[],
                &project_nodes("evil"),
            )
            .expect_err("unsafe path rejects");
            assert_eq!(rejection.reason, MutationRejectionReason::InvalidPath);
        }
        assert_nothing_written(&fs, "/evil.json");
    }

    #[test]
    fn remove_project_child_stages_entry_removal_def_and_exclusive_asset_deletes() {
        let shapes = SlotShapeRegistry::default();
        let (fs, mut registry) = shader_project(&shapes);
        let shader_use = NodeUseLocation::root().child(SlotPath::parse("nodes[shader]").unwrap());

        let outcome =
            remove(&fs, &mut registry, &shapes, &project_nodes("shader")).expect("remove accepted");

        // The node left the effective tree and the change summary reports the
        // teardown (`uses.removed` drives engine subtree removal).
        assert!(outcome.changes.uses.removed.contains(&shader_use));
        assert!(
            !registry
                .inventory()
                .tree
                .nodes
                .values()
                .any(|node| node.key == shader_use)
        );
        assert!(
            outcome
                .changes
                .defs
                .removed
                .contains(&root_def("/shader.json"))
        );

        // Def + exclusive asset staged `Delete`; nothing was swept.
        assert_eq!(
            outcome.staged_deletes,
            vec![
                ArtifactLocation::file("/shader.json"),
                ArtifactLocation::file("/shader.glsl"),
            ]
        );
        assert!(!outcome.swept_pending_edits);
        for artifact in &outcome.staged_deletes {
            assert_eq!(
                registry
                    .overlay()
                    .get()
                    .artifact(artifact)
                    .and_then(ArtifactOverlay::as_body),
                Some(&AssetBodyOverlay::Delete),
                "{} must be staged for deletion",
                artifact.file_path()
            );
        }
        let site_overlay = registry
            .overlay()
            .get()
            .artifact(&ArtifactLocation::file("/project.json"))
            .and_then(ArtifactOverlay::as_slot)
            .expect("site slot overlay");
        assert_eq!(
            site_overlay
                .edits
                .get(&SlotPath::parse("nodes[shader]").unwrap()),
            Some(&lpc_model::SlotEditOp::Remove)
        );

        // Nothing touched the filesystem yet: staging is commit-deferred.
        assert!(fs.file_exists(LpPath::new("/shader.json")).unwrap());
        assert!(fs.file_exists(LpPath::new("/shader.glsl")).unwrap());
    }

    #[test]
    fn remove_playlist_entry_removes_whole_entry_and_leaves_siblings() {
        let shapes = SlotShapeRegistry::default();
        let (fs, mut registry) = playlist_two_entry_project(&shapes);
        let site = NodeAttachSite::Slot {
            artifact: ArtifactLocation::file("/playlist.json"),
            path: SlotPath::parse("entries[2].node").unwrap(),
        };

        let outcome = remove(&fs, &mut registry, &shapes, &site).expect("remove accepted");

        // The whole `entries[2]` entry is gone from the effective playlist;
        // the sibling entry survives untouched.
        let playlist = registry
            .def(&root_def("/playlist.json"))
            .and_then(|entry| entry.state.loaded_def())
            .and_then(NodeDef::as_playlist)
            .expect("effective playlist def");
        assert!(playlist.entries.entries.contains_key(&1));
        assert!(!playlist.entries.entries.contains_key(&2));

        // Child def + its exclusive asset staged; the playlist itself and the
        // sibling's def are not.
        assert_eq!(
            outcome.staged_deletes,
            vec![
                ArtifactLocation::file("/blast.json"),
                ArtifactLocation::file("/blast.glsl"),
            ]
        );
        assert!(registry.def(&root_def("/idle.json")).is_some());
        let child_use = NodeUseLocation::root()
            .child(SlotPath::parse("nodes[playlist]").unwrap())
            .child(SlotPath::parse("entries[2].node").unwrap());
        assert!(outcome.changes.uses.removed.contains(&child_use));
    }

    #[test]
    fn remove_does_not_stage_shared_assets() {
        let shapes = SlotShapeRegistry::default();
        let (fs, mut registry) = shared_asset_project(&shapes);

        let outcome =
            remove(&fs, &mut registry, &shapes, &project_nodes("a")).expect("remove accepted");

        // Only the removed shader's def is staged; the `.glsl` shared with
        // the surviving shader stays effective and undeleted.
        assert_eq!(
            outcome.staged_deletes,
            vec![ArtifactLocation::file("/a.json")]
        );
        assert!(
            registry
                .asset(&AssetLocation::artifact(ArtifactLocation::file(
                    "/shared.glsl"
                )))
                .is_some(),
            "shared asset must survive in the effective inventory"
        );
        assert!(registry.def(&root_def("/b.json")).is_some());
    }

    #[test]
    fn remove_sweeps_pending_edits_so_commit_succeeds() {
        // THE sweep hazard: an orphaned slot overlay on an artifact outside
        // the effective inventory aborts `commit_overlay` mid-write with
        // `CommitError::Projection`. The remove op must leave no such state.
        let shapes = SlotShapeRegistry::default();
        let (fs, mut registry) = clock_project(&shapes);
        registry
            .mutate(
                &fs,
                MutationOp::PutSlotEdit {
                    artifact: ArtifactLocation::file("/clock.json"),
                    edit: SlotEdit::assign_value(
                        SlotPath::parse("controls.rate").unwrap(),
                        LpValue::F32(2.0),
                    ),
                },
                Revision::new(4),
                &ParseCtx { shapes: &shapes },
            )
            .expect("stage clock edit");

        let outcome =
            remove(&fs, &mut registry, &shapes, &project_nodes("clock")).expect("remove accepted");
        assert!(
            outcome.swept_pending_edits,
            "the pending clock edit was swept"
        );

        let commit = registry
            .commit_overlay(&fs, Revision::new(20), &ParseCtx { shapes: &shapes })
            .expect("commit succeeds despite the removed node's pending edits");
        assert!(
            commit
                .artifact_changes
                .removed
                .contains(&ArtifactLocation::file("/clock.json"))
        );

        // Files materialized: the def is deleted and project.json no longer
        // references it.
        assert!(!fs.file_exists(LpPath::new("/clock.json")).unwrap());
        let project_bytes = fs.read_file(LpPath::new("/project.json")).unwrap();
        let project_text = core::str::from_utf8(&project_bytes).unwrap();
        let project = NodeDef::read_json(&shapes, project_text).expect("project reparses");
        assert!(project.as_module().unwrap().nodes.entries.is_empty());
        assert!(registry.overlay().get().is_empty());
    }

    #[test]
    fn remove_container_sweeps_descendant_pending_edits() {
        let shapes = SlotShapeRegistry::default();
        let (fs, mut registry) = playlist_two_entry_project(&shapes);
        // Pending edit on a child of the playlist, not on the playlist
        // itself: the container removal must sweep the descendant too.
        registry
            .mutate(
                &fs,
                MutationOp::PutSlotEdit {
                    artifact: ArtifactLocation::file("/idle.json"),
                    edit: SlotEdit::assign_value(
                        SlotPath::parse("controls.rate").unwrap(),
                        LpValue::F32(3.0),
                    ),
                },
                Revision::new(4),
                &ParseCtx { shapes: &shapes },
            )
            .expect("stage child edit");

        let outcome = remove(&fs, &mut registry, &shapes, &project_nodes("playlist"))
            .expect("remove accepted");

        assert!(outcome.swept_pending_edits);
        let staged: Vec<_> = outcome
            .staged_deletes
            .iter()
            .map(|artifact| artifact.file_path().as_str())
            .collect();
        for expected in ["/playlist.json", "/idle.json", "/blast.json", "/blast.glsl"] {
            assert!(staged.contains(&expected), "{expected} staged: {staged:?}");
        }
        assert_eq!(
            registry
                .overlay()
                .get()
                .artifact(&ArtifactLocation::file("/idle.json"))
                .and_then(ArtifactOverlay::as_body),
            Some(&AssetBodyOverlay::Delete),
            "the child's slot overlay was swept into a staged delete"
        );

        let commit = registry
            .commit_overlay(&fs, Revision::new(20), &ParseCtx { shapes: &shapes })
            .expect("commit succeeds after container removal");
        assert_eq!(commit.artifact_changes.removed.len(), 4);
        assert!(!fs.file_exists(LpPath::new("/playlist.json")).unwrap());
        assert!(!fs.file_exists(LpPath::new("/idle.json")).unwrap());
        assert!(!fs.file_exists(LpPath::new("/blast.json")).unwrap());
        assert!(!fs.file_exists(LpPath::new("/blast.glsl")).unwrap());
    }

    #[test]
    fn remove_reverts_cleanly_with_existing_overlay_ops() {
        use lpc_model::{MutationCmd, MutationCmdBatch, MutationCmdId, MutationCmdStatus};

        let shapes = SlotShapeRegistry::default();
        let (fs, mut registry) = shader_project(&shapes);
        let shader_use = NodeUseLocation::root().child(SlotPath::parse("nodes[shader]").unwrap());
        let outcome =
            remove(&fs, &mut registry, &shapes, &project_nodes("shader")).expect("remove accepted");

        // Revert = RemoveSlotEdit at the site (policy-exempt) + ClearArtifact
        // per staged delete — existing ops only, through the validated path.
        let mut commands = vec![MutationCmd {
            id: MutationCmdId::new(1),
            mutation: MutationOp::RemoveSlotEdit {
                artifact: ArtifactLocation::file("/project.json"),
                path: SlotPath::parse("nodes[shader]").unwrap(),
            },
        }];
        for (index, artifact) in outcome.staged_deletes.iter().enumerate() {
            commands.push(MutationCmd {
                id: MutationCmdId::new(2 + index as u64),
                mutation: MutationOp::ClearArtifact {
                    artifact: artifact.clone(),
                },
            });
        }
        let results = registry.mutate_batch(
            &fs,
            MutationCmdBatch::new(commands),
            Revision::new(30),
            &ParseCtx { shapes: &shapes },
        );
        for result in &results.commands.results {
            assert!(
                matches!(result.status, MutationCmdStatus::Accepted { .. }),
                "revert command rejected: {result:?}"
            );
        }

        // The node is fully back and no staged state remains.
        assert!(registry.overlay().get().is_empty());
        assert!(
            registry
                .inventory()
                .tree
                .nodes
                .values()
                .any(|node| node.key == shader_use)
        );
        assert!(matches!(
            registry.def(&root_def("/shader.json")).map(|e| &e.state),
            Some(NodeDefState::Loaded(_))
        ));
        assert!(
            registry
                .asset(&AssetLocation::artifact(ArtifactLocation::file(
                    "/shader.glsl"
                )))
                .is_some()
        );
    }

    #[test]
    fn remove_rejects_bad_sites_and_stages_nothing() {
        let shapes = SlotShapeRegistry::default();
        let (fs, mut registry) = clock_project(&shapes);

        // Unknown project node key.
        let rejection = remove(&fs, &mut registry, &shapes, &project_nodes("nope"))
            .expect_err("unknown key rejects");
        assert_eq!(rejection.reason, MutationRejectionReason::UnknownSlotPath);

        // Non-invocation slot path.
        let rejection = remove(
            &fs,
            &mut registry,
            &shapes,
            &NodeAttachSite::Slot {
                artifact: ArtifactLocation::file("/clock.json"),
                path: SlotPath::parse("controls.rate").unwrap(),
            },
        )
        .expect_err("non-invocation path rejects");
        assert_eq!(rejection.reason, MutationRejectionReason::UnknownSlotPath);

        // The policy bypass applies only to the dedicated `ProjectNodes`
        // site; the same target addressed as a raw slot stays locked.
        let rejection = remove(
            &fs,
            &mut registry,
            &shapes,
            &NodeAttachSite::Slot {
                artifact: ArtifactLocation::file("/project.json"),
                path: SlotPath::parse("nodes[clock]").unwrap(),
            },
        )
        .expect_err("read-only slot site rejects");
        assert_eq!(rejection.reason, MutationRejectionReason::NotWritable);

        // Missing site artifact.
        let rejection = remove(
            &fs,
            &mut registry,
            &shapes,
            &NodeAttachSite::Slot {
                artifact: ArtifactLocation::file("/missing.json"),
                path: SlotPath::parse("entries[1].node").unwrap(),
            },
        )
        .expect_err("missing artifact rejects");
        assert_eq!(rejection.reason, MutationRejectionReason::UnknownArtifact);
        assert!(registry.overlay().get().is_empty(), "nothing was staged");

        // Unknown playlist entry.
        let (fs, mut registry) = playlist_two_entry_project(&shapes);
        let rejection = remove(
            &fs,
            &mut registry,
            &shapes,
            &NodeAttachSite::Slot {
                artifact: ArtifactLocation::file("/playlist.json"),
                path: SlotPath::parse("entries[9].node").unwrap(),
            },
        )
        .expect_err("unknown entry rejects");
        assert_eq!(rejection.reason, MutationRejectionReason::UnknownSlotPath);
        assert!(registry.overlay().get().is_empty(), "nothing was staged");
    }

    // --- Helpers ---

    fn clock_project(shapes: &SlotShapeRegistry) -> (LpFsMemory, ProjectRegistry) {
        let mut fs = LpFsMemory::new();
        crate::test::fixtures::write_file(
            &mut fs,
            "/project.json",
            r#"{
  "kind": "Module",
  "format": 2,
  "nodes": {
    "clock": { "ref": "./clock.json" }
  }
}"#,
        );
        crate::test::fixtures::write_file(
            &mut fs,
            "/clock.json",
            r#"{
  "kind": "Clock",
  "controls": { "rate": 1.0 }
}"#,
        );
        let registry = load_registry(&fs, shapes);
        (fs, registry)
    }

    fn playlist_project(shapes: &SlotShapeRegistry) -> (LpFsMemory, ProjectRegistry) {
        let mut fs = LpFsMemory::new();
        crate::test::fixtures::write_file(
            &mut fs,
            "/project.json",
            r#"{
  "kind": "Module",
  "format": 2,
  "nodes": {
    "playlist": { "ref": "./playlist.json" }
  }
}"#,
        );
        crate::test::fixtures::write_file(
            &mut fs,
            "/playlist.json",
            r#"{
  "kind": "Playlist",
  "entries": {
    "1": {
      "name": "idle",
      "node": { "ref": "./idle.json" }
    }
  }
}"#,
        );
        crate::test::fixtures::write_file(
            &mut fs,
            "/idle.json",
            r#"{
  "kind": "Clock"
}"#,
        );
        let registry = load_registry(&fs, shapes);
        (fs, registry)
    }

    fn shader_project(shapes: &SlotShapeRegistry) -> (LpFsMemory, ProjectRegistry) {
        let mut fs = LpFsMemory::new();
        crate::test::fixtures::write_file(
            &mut fs,
            "/project.json",
            r#"{
  "kind": "Module",
  "format": 2,
  "nodes": {
    "shader": { "ref": "./shader.json" }
  }
}"#,
        );
        crate::test::fixtures::write_file(
            &mut fs,
            "/shader.json",
            r#"{
  "kind": "Shader",
  "source": { "path": "./shader.glsl" }
}"#,
        );
        crate::test::fixtures::write_file(
            &mut fs,
            "/shader.glsl",
            "vec4 render(vec2 pos) { return vec4(1.0); }",
        );
        let registry = load_registry(&fs, shapes);
        (fs, registry)
    }

    fn shared_asset_project(shapes: &SlotShapeRegistry) -> (LpFsMemory, ProjectRegistry) {
        let mut fs = LpFsMemory::new();
        crate::test::fixtures::write_file(
            &mut fs,
            "/project.json",
            r#"{
  "kind": "Module",
  "format": 2,
  "nodes": {
    "a": { "ref": "./a.json" },
    "b": { "ref": "./b.json" }
  }
}"#,
        );
        for def in ["/a.json", "/b.json"] {
            crate::test::fixtures::write_file(
                &mut fs,
                def,
                r#"{
  "kind": "Shader",
  "source": { "path": "./shared.glsl" }
}"#,
            );
        }
        crate::test::fixtures::write_file(
            &mut fs,
            "/shared.glsl",
            "vec4 render(vec2 pos) { return vec4(0.5); }",
        );
        let registry = load_registry(&fs, shapes);
        (fs, registry)
    }

    fn playlist_two_entry_project(shapes: &SlotShapeRegistry) -> (LpFsMemory, ProjectRegistry) {
        let mut fs = LpFsMemory::new();
        crate::test::fixtures::write_file(
            &mut fs,
            "/project.json",
            r#"{
  "kind": "Module",
  "format": 2,
  "nodes": {
    "playlist": { "ref": "./playlist.json" }
  }
}"#,
        );
        crate::test::fixtures::write_file(
            &mut fs,
            "/playlist.json",
            r#"{
  "kind": "Playlist",
  "entries": {
    "1": {
      "name": "idle",
      "node": { "ref": "./idle.json" }
    },
    "2": {
      "name": "blast",
      "node": { "ref": "./blast.json" }
    }
  }
}"#,
        );
        crate::test::fixtures::write_file(
            &mut fs,
            "/idle.json",
            r#"{
  "kind": "Clock",
  "controls": { "rate": 1.0 }
}"#,
        );
        crate::test::fixtures::write_file(
            &mut fs,
            "/blast.json",
            r#"{
  "kind": "Shader",
  "source": { "path": "./blast.glsl" }
}"#,
        );
        crate::test::fixtures::write_file(
            &mut fs,
            "/blast.glsl",
            "vec4 render(vec2 pos) { return vec4(0.0); }",
        );
        let registry = load_registry(&fs, shapes);
        (fs, registry)
    }

    fn load_registry(fs: &LpFsMemory, shapes: &SlotShapeRegistry) -> ProjectRegistry {
        let mut registry = ProjectRegistry::new();
        registry
            .load_root(
                fs,
                LpPath::new("/project.json"),
                Revision::new(1),
                &ParseCtx { shapes },
            )
            .expect("load project root");
        registry
    }

    fn project_nodes(key: &str) -> NodeAttachSite {
        NodeAttachSite::ProjectNodes { key: key.into() }
    }

    fn root_def(path: &str) -> NodeDefLocation {
        NodeDefLocation::artifact_root(ArtifactLocation::file(path))
    }

    fn texture_body(shapes: &SlotShapeRegistry) -> String {
        NodeDef::Texture(TextureDef::new(8, 8))
            .write_json(shapes)
            .expect("canonical texture body")
    }

    fn create(
        fs: &LpFsMemory,
        registry: &mut ProjectRegistry,
        shapes: &SlotShapeRegistry,
        file: &str,
        body: &[u8],
        assets: &[(LpPathBuf, Vec<u8>)],
        attach: &NodeAttachSite,
    ) -> Result<CreateNodeOutcome, MutationRejection> {
        registry.create_node(
            fs,
            LpPath::new(file),
            body,
            assets,
            attach,
            Revision::new(10),
            &ParseCtx { shapes },
        )
    }

    fn remove(
        fs: &LpFsMemory,
        registry: &mut ProjectRegistry,
        shapes: &SlotShapeRegistry,
        site: &NodeAttachSite,
    ) -> Result<RemoveNodeOutcome, MutationRejection> {
        registry.remove_node(fs, site, Revision::new(10), &ParseCtx { shapes })
    }

    /// A rejection must leave the filesystem untouched: the candidate new
    /// file is absent and the project root still parses to its pre-call
    /// content (only the `clock`/`playlist` entry, no additions).
    fn assert_nothing_written(fs: &LpFsMemory, new_file: &str) {
        assert!(!fs.file_exists(LpPath::new(new_file)).unwrap());
        let bytes = fs.read_file(LpPath::new("/project.json")).unwrap();
        let text = core::str::from_utf8(&bytes).unwrap();
        let project = NodeDef::read_json(&SlotShapeRegistry::default(), text).unwrap();
        assert_eq!(
            project.as_module().unwrap().nodes.entries.len(),
            1,
            "project root gained an entry despite the rejection"
        );
    }
}
