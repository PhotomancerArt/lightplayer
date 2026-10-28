//! The Graph's state in core: the editor for the open project, opened from
//! its files and kept in step with them.
//!
//! **Opening.** While the Graph tab is the place, the actor asks the
//! controller to settle ([`GraphController::settle`]) after each batch.
//! The first time, it opens the project with the lab's files crate,
//! fetching from the board every file the crate asks for that Studio
//! hasn't read yet ([`GraphSource`]), then builds the editor and lays it
//! out on the canvas (`lab-layout`).
//!
//! **Keeping in step.** When the project moves (an edit in Map or Nodes,
//! the agent, a save), the controller reads again the files the open read.
//! If every one says what it said, nothing is rebuilt: the editor, its
//! selection, camera and undo stay. If any changed, the editor is built
//! again from the files, keeping the camera and whatever of the selection
//! still exists.
//!
//! **Patching.** Each fixture is patched by `lpc-mapping`'s patcher
//! against port tables core builds with the engine's own walk over each
//! output's authored ports (`graph_port_tables.rs`), handed to the editor
//! when it opens, so the Graph and the board can't disagree.
//!
//! **Saving.** After an edit (not mid-drag, not mid-drawing) the files
//! crate says which of the project's files changed — each fixture's
//! mapping in map2d format 6, written by `lpc-mapping`'s own writer, and
//! the node files the Graph edits — and each is written as an ordinary
//! asset edit, so Studio's dirty state, Save and the push to the board work
//! unchanged. A project nobody edited writes nothing. The written files are
//! then the baseline the next save compares against, and the port tables
//! are built again from them (an output's ports edited on the Graph
//! re-patch at once). Where pinned nodes sit is written too (`editor.json`'s
//! `canvas` surface, Debug's Pinning). Undo is the Graph's own (the lab's
//! project snapshots); what it puts back is saved the same way. A deleted
//! node's files (`Saved::removed`) are deleted through the same asset path
//! (ruling DD36), so no file stays that nothing names; undoing the delete
//! writes them again.
//!
//! **Colours.** [`GraphController::view`] carries the board's frame, built
//! from the frames the board published (`graph_frame.rs`) and kept until
//! a new frame or a new project arrives.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use lab_mapping_files::{Opened, ProductProject};
use lab_mapping_model::Project;
use lab_mapping_model::{BoardFrame, Editor, NodeId};

use crate::StudioServerClient;
use crate::app::graph::graph_canvas_meta::{EDITOR_JSON, apply_pins, with_pins};
use crate::app::graph::graph_frame;
use crate::app::graph::graph_pictures;
use crate::app::graph::graph_port_tables::port_tables;
use crate::app::graph::graph_source::{GraphSource, location};
use crate::app::graph::{GraphHandle, UiGraphFrame, UiGraphState, UiGraphView};
use crate::app::project::ProjectController;

/// How many rounds of "open, fetch what it asked for" an open may take: a
/// project's files nest a few levels (project, modules, nodes, their
/// mappings and patches), each round reaching one level further.
const OPEN_ROUNDS: usize = 12;

#[derive(Debug, Default)]
pub struct GraphController {
    handle: GraphHandle,
    opened: Option<GraphOpened>,
    state: Option<UiGraphState>,
    /// Paths the board says aren't there, for the project they were asked
    /// of (its filesystem root).
    absent: BTreeSet<String>,
    absent_root: Option<String>,
    /// The project's revision when the files were last compared.
    checked: Option<crate::app::graph::GraphFilesKey>,
    generation: u64,
    /// The editor's project revision its files last said (opened or
    /// written): another one is an edit to save.
    saved: u64,
    /// The files of nodes the Graph deleted, as they were (path, text), by
    /// node: what its undo writes back when the node returns (DD36).
    deleted: BTreeMap<NodeId, Vec<(String, String)>>,
    /// The selection the board last breathed, while it breathes one.
    pulsed: Option<Vec<lab_mapping_model::Target>>,
    /// The render sources whose pictures the Graph shows, with their
    /// visual products (`graph_pictures.rs`, plan D11).
    pictures: Vec<(NodeId, crate::UiProductRef)>,
    frame: RefCell<Option<(FrameKey, Rc<BoardFrame>)>>,
}

/// What the open read, and what saving starts from.
#[derive(Debug)]
struct GraphOpened {
    files: ProductProject,
    /// Every path the open asked for, and what it said.
    read: BTreeMap<String, Option<String>>,
    /// The lab's nodes, as Studio's.
    nodes: BTreeMap<NodeId, lpc_model::NodeId>,
}

/// What the board's frame was built from: the board's frame count, the
/// project it was laid onto, and each picture's revision.
#[derive(Debug, Clone, PartialEq, Eq)]
struct FrameKey {
    frames_seen: u64,
    revision: u64,
    pictures: Vec<Option<i64>>,
}

impl GraphController {
    /// The handle the page holds (see [`GraphHandle`]).
    pub fn handle(&self) -> GraphHandle {
        self.handle.clone()
    }

    /// The Graph's slice of the studio view: `None` until the Graph has
    /// been asked for.
    pub fn view(&self, project: &ProjectController) -> Option<UiGraphView> {
        let state = self.state.clone()?;
        let state = match state {
            UiGraphState::Open { .. } => UiGraphState::Open {
                live: project.graph_frames_seen().is_some_and(|n| n > 0),
            },
            other => other,
        };
        Some(UiGraphView {
            state,
            generation: self.generation,
            frame: UiGraphFrame(self.frame(project)),
        })
    }

    /// The board's frame for the editor's project, kept until a new frame
    /// or a new project.
    fn frame(&self, project: &ProjectController) -> Rc<BoardFrame> {
        let Some(opened) = self.opened.as_ref() else {
            return Rc::default();
        };
        let editor = self.handle.editor().borrow();
        let key = FrameKey {
            frames_seen: project.graph_frames_seen().unwrap_or(0),
            revision: editor.project.revision(),
            pictures: self
                .pictures
                .iter()
                .map(|(_, product)| project.graph_picture_revision(product))
                .collect(),
        };
        let mut kept = self.frame.borrow_mut();
        if let Some((k, frame)) = kept.as_ref()
            && *k == key
        {
            return frame.clone();
        }
        let frame = graph_frame::board_frame(&editor.project, &opened.nodes, |node| {
            project.graph_output_frame(node)
        });
        let frame = Rc::new(graph_pictures::with_pictures(
            frame,
            project,
            &self.pictures,
        ));
        *kept = Some((key, frame.clone()));
        frame
    }

    /// Open the project on the Graph, or bring it back in step with its
    /// files, when `wanted` (the Graph is the place). Returns whether the
    /// Graph's view changed.
    pub async fn settle(
        &mut self,
        project: &mut ProjectController,
        mut server: Option<&mut StudioServerClient>,
        wanted: bool,
    ) -> bool {
        let Some(key) = project.graph_files_key() else {
            // No project: nothing to show.
            let had = self.state.is_some() || self.opened.is_some();
            self.close();
            return had;
        };
        let mut changed = false;
        if let Some(server) = server.as_deref_mut() {
            changed |= self.save(project, server).await;
            self.highlight(project, server, wanted).await;
        }
        changed |= self.ask_for_pictures(project, wanted);
        if !wanted {
            return changed;
        }
        if self.absent_root.as_deref() != Some(key.root.as_str()) {
            self.close();
            self.absent_root = Some(key.root.clone());
        }
        if self.checked.as_ref() == Some(&key) && self.state.is_some() {
            return changed;
        }
        let Some(server) = server else {
            return changed;
        };
        let first = self.state.is_none();
        if first {
            self.state = Some(UiGraphState::Opening);
        }
        // Still what the open read? Then the editor stays as it is.
        if let Some(opened) = self.opened.as_ref() {
            let paths: Vec<String> = opened.read.keys().cloned().collect();
            self.fetch(project, server, &paths).await;
            let opened = self.opened.as_ref().expect("checked above");
            let same = opened.read.iter().all(|(path, text)| {
                GraphSource::text_of(project, &self.absent, path).as_ref() == Some(text)
            });
            if same {
                self.checked = Some(key);
                return first || changed;
            }
        }
        self.open(project, server).await;
        self.checked = Some(key);
        true
    }

    /// Publish the Graph's verbs (`project/graph/…`), once it is open.
    pub fn publish_offers(&self, offers: &mut crate::UiOfferTree) {
        if matches!(self.state, Some(UiGraphState::Open { .. })) {
            crate::app::graph::publish_graph_offers(offers, &self.handle.editor().borrow());
        }
    }

    /// Press a verb from its offer: as its key does it, or, for a verb
    /// that takes values, the edit it was bound to, through the function
    /// the inspector's rows and the keys run too. A refused edit says why.
    pub fn press(&mut self, op: crate::app::graph::GraphOp) -> Result<bool, String> {
        let done = {
            let mut editor = self.handle.editor().borrow_mut();
            match &op.edit {
                Some(edit) => editor.edit(std::slice::from_ref(edit)).map(|()| true),
                None => Ok(editor.press(op.verb)),
            }
        };
        self.generation += 1;
        done
    }

    /// Write what the editor changed to the project's files, after an
    /// edit. Returns whether the Graph's view changed (a notice).
    async fn save(
        &mut self,
        project: &mut ProjectController,
        server: &mut StudioServerClient,
    ) -> bool {
        let (saved, revision, gone, back) = {
            let Some(opened) = self.opened.as_ref() else {
                return false;
            };
            let e = self.handle.editor().borrow();
            if e.dragging() || e.drawing() || e.project.revision() == self.saved {
                return false;
            }
            // Nodes deleted since the files were read, with their files;
            // and nodes deleted before that the editor holds again (undo).
            let gone: Vec<(NodeId, Vec<String>)> = opened
                .files
                .nodes
                .iter()
                .filter(|(id, _)| e.project.node(id).is_none())
                .map(|(id, f)| (id.clone(), f.own_files()))
                .collect();
            let back: Vec<NodeId> = self
                .deleted
                .keys()
                .filter(|id| e.project.node(id).is_some())
                .cloned()
                .collect();
            (
                opened.files.save(&e.project),
                e.project.revision(),
                gone,
                back,
            )
        };
        self.saved = revision;
        let saved = match saved {
            Ok(saved) => saved,
            Err(why) => {
                self.refuse_save(why.to_string());
                return true;
            }
        };
        for (path, text) in &saved.writes {
            let written = project
                .apply_asset_body(server, location(path), text.clone().into_bytes())
                .await;
            if let Err(why) = written {
                self.refuse_save(format!("{path}: {why}"));
                return true;
            }
        }
        // A node the Graph deleted takes its files with it, in the same
        // step (DD36): nothing stays on disk that nothing names. What they
        // said is kept, so the Graph's undo writes them back as they were.
        for (id, paths) in gone {
            self.fetch(project, server, &paths).await;
            let texts: Vec<(String, String)> = paths
                .iter()
                .filter_map(|path| {
                    let text = GraphSource::text_of(project, &self.absent, path).flatten()?;
                    Some((path.clone(), text))
                })
                .collect();
            self.deleted.insert(id, texts);
        }
        for id in back {
            for (path, text) in self.deleted.remove(&id).unwrap_or_default() {
                let written = project
                    .apply_asset_body(server, location(&path), text.into_bytes())
                    .await;
                if let Err(why) = written {
                    self.refuse_save(format!("{path}: {why}"));
                    return true;
                }
            }
        }
        for path in &saved.removed {
            let deleted = project.apply_asset_delete(server, location(path)).await;
            if let Err(why) = deleted {
                self.refuse_save(format!("{path}: {why}"));
                return true;
            }
        }
        // Pins: editor.json's `canvas` surface.
        let paths = [EDITOR_JSON.to_string()];
        self.fetch(project, server, &paths).await;
        let meta = GraphSource::text_of(project, &self.absent, EDITOR_JSON).flatten();
        let pins = match self.opened.as_ref() {
            Some(opened) => with_pins(
                meta.as_deref(),
                &self.handle.editor().borrow().project,
                |id| address_of(project, &opened.nodes, id),
            ),
            None => None,
        };
        if let Some(text) = pins {
            let written = project
                .apply_asset_body(server, location(EDITOR_JSON), text.into_bytes())
                .await;
            if let Err(why) = written {
                self.refuse_save(format!("{EDITOR_JSON}: {why}"));
                return true;
            }
            self.absent.remove(EDITOR_JSON);
            // What was written is what the open read now.
            if let Some(opened) = self.opened.as_mut() {
                opened.read.insert(
                    EDITOR_JSON.to_string(),
                    GraphSource::text_of(project, &self.absent, EDITOR_JSON).flatten(),
                );
            }
        }
        if !saved.writes.is_empty() || !saved.removed.is_empty() {
            self.rebase(project);
        }
        false
    }

    /// What was written is what the next save compares against, and what
    /// the open read now: read the files again, and build the port tables
    /// from them, so an output whose ports the Graph edited re-patches at
    /// once instead of on the next open.
    fn rebase(&mut self, project: &ProjectController) {
        let source = GraphSource::new(project, &self.absent);
        let Ok(reopened) = Opened::open(&source) else {
            return;
        };
        if !source.misses().is_empty() {
            return;
        }
        let mut read = source.into_read();
        read.insert(
            EDITOR_JSON.to_string(),
            GraphSource::text_of(project, &self.absent, EDITOR_JSON).flatten(),
        );
        {
            let mut editor = self.handle.editor().borrow_mut();
            let tables = port_tables(&editor.project, &reopened.files);
            if editor.project.set_port_tables(tables) {
                self.generation += 1;
                *self.frame.borrow_mut() = None;
            }
        }
        if let Some(opened) = self.opened.as_mut() {
            opened.files = reopened.files;
            opened.read = read;
        }
    }

    /// Show the selection on the board while the Graph is the place, and
    /// stop when it isn't or nothing is selected: each fixture's
    /// `selection` slot for what is selected in fixtures (the lab's rules,
    /// plan M4b), and the outputs' `highlight` slots — the Patch view's
    /// pulse — for outputs, ports and pixels.
    async fn highlight(
        &mut self,
        project: &mut ProjectController,
        server: &mut StudioServerClient,
        wanted: bool,
    ) {
        let selection = match (&self.opened, wanted) {
            (Some(_), true) => self.handle.editor().borrow().selection.clone(),
            _ => Vec::new(),
        };
        let want = (!selection.is_empty()).then_some(selection);
        if self.pulsed == want {
            return;
        }
        use crate::app::graph::graph_highlight::{board_selection, highlight_of};
        let (fixtures, op) = match (&self.opened, &want) {
            (Some(opened), Some(selection)) => {
                let editor = self.handle.editor().borrow();
                let p = &editor.project;
                // The lab's own rules when every fixture's firmware has the
                // `selection` slot (plan M4b); M3's `highlight` otherwise.
                let has_slot = p.fixtures().iter().all(|n| {
                    opened
                        .nodes
                        .get(&n.id)
                        .is_some_and(|node| project.graph_fixture_has_selection(*node))
                });
                if has_slot {
                    let b = board_selection(p, &opened.nodes, selection);
                    (b.fixtures, b.outputs)
                } else {
                    (Vec::new(), highlight_of(p, &opened.nodes, selection))
                }
            }
            _ => (
                Vec::new(),
                crate::PatchPulseOp {
                    subjects: Vec::new(),
                },
            ),
        };
        // What didn't land is tried again with the next change.
        let fixtures_landed = project
            .apply_fixture_selection(server, fixtures)
            .await
            .is_ok();
        if project.apply_patch_pulse(server, op).await.is_ok() && fixtures_landed {
            self.pulsed = want;
        }
    }

    /// Ask the lens for the pictures the Graph shows now (plan D11), and
    /// for none while it isn't the place. Returns whether that changed.
    fn ask_for_pictures(&mut self, project: &mut ProjectController, wanted: bool) -> bool {
        let pictures = match (&self.opened, wanted) {
            (Some(opened), true) => {
                let sources = graph_pictures::wanted_sources(
                    &self.handle.editor().borrow(),
                    project.graph_lens_is_device(),
                );
                graph_pictures::wanted_products(project, &opened.nodes, &sources)
            }
            _ => Vec::new(),
        };
        if pictures == self.pictures {
            return false;
        }
        project.set_graph_pictures(pictures.iter().map(|(_, product)| *product).collect());
        self.pictures = pictures;
        true
    }

    /// Say on the canvas that a save didn't happen.
    fn refuse_save(&mut self, why: String) {
        self.handle.editor().borrow_mut().notice = Some(lab_mapping_model::Notice::refused(
            "Couldn't save the Graph's edit",
            why,
            "the edit is still on the Graph; undo it, or try again",
        ));
        self.generation += 1;
    }

    /// Read the project's files into a new editor.
    async fn open(&mut self, project: &mut ProjectController, server: &mut StudioServerClient) {
        for _ in 0..OPEN_ROUNDS {
            let (result, misses, read) = {
                let source = GraphSource::new(project, &self.absent);
                let result = Opened::open(&source);
                let misses = source.misses();
                (result, misses, source.into_read())
            };
            if !misses.is_empty() {
                let misses: Vec<String> = misses.into_iter().collect();
                self.fetch(project, server, &misses).await;
                continue;
            }
            match result {
                Ok(mut opened) => {
                    let mut read = read;
                    // editor.json's `canvas` surface: where pinned nodes sit.
                    let paths = [EDITOR_JSON.to_string()];
                    self.fetch(project, server, &paths).await;
                    let meta = GraphSource::text_of(project, &self.absent, EDITOR_JSON).flatten();
                    let nodes = node_map(project, &opened.files);
                    apply_pins(&mut opened.project, meta.as_deref(), |id| {
                        address_of(project, &nodes, id)
                    });
                    read.insert(EDITOR_JSON.to_string(), meta);
                    self.adopt(opened, read, nodes)
                }
                Err(why) => {
                    self.opened = None;
                    self.state = Some(UiGraphState::Refused {
                        why: why.to_string(),
                    });
                }
            }
            return;
        }
        self.state = Some(UiGraphState::Refused {
            why: "its files kept asking for more files to read".to_string(),
        });
    }

    /// Make the opened project the editor's, keeping the view.
    fn adopt(
        &mut self,
        opened: Opened,
        read: BTreeMap<String, Option<String>>,
        nodes: BTreeMap<NodeId, lpc_model::NodeId>,
    ) {
        let first = self.opened.is_none();
        let Opened {
            project: lab,
            files,
        } = opened;
        {
            let mut editor = self.handle.editor().borrow_mut();
            let mut lab: Project = lab;
            lab.set_port_tables(port_tables(&lab, &files));
            let mut e = Editor::new(lab).with_view_of(&editor);
            // The board (or the preview host) runs the project: its
            // shaders' pictures are their own.
            e.runs_for_real = true;
            e.selection = editor
                .selection
                .iter()
                .filter(|t| e.project.exists(t))
                .cloned()
                .collect();
            e.set_placement(&lab_layout::canvas_layout(&e.layout_input()));
            if first {
                e.fit();
            } else {
                e.camera = editor.camera;
            }
            *editor = e;
            self.saved = editor.project.revision();
        }
        self.opened = Some(GraphOpened { files, read, nodes });
        self.state = Some(UiGraphState::Open { live: false });
        self.generation += 1;
        *self.frame.borrow_mut() = None;
    }

    /// Fetch every path Studio doesn't hold yet; one the board says isn't
    /// there is absent from now on.
    async fn fetch(
        &mut self,
        project: &mut ProjectController,
        server: &mut StudioServerClient,
        paths: &[String],
    ) {
        for path in paths {
            if GraphSource::text_of(project, &self.absent, path).is_some() {
                continue;
            }
            if project
                .asset_content(server, &location(path))
                .await
                .is_err()
            {
                self.absent.insert(path.clone());
            }
        }
    }

    fn close(&mut self) {
        self.opened = None;
        self.deleted.clear();
        self.state = None;
        self.absent.clear();
        self.absent_root = None;
        self.checked = None;
        self.pulsed = None;
        *self.frame.borrow_mut() = None;
    }
}

/// The lab's nodes, as Studio's: each by the file its definition lives in.
fn node_map(
    project: &ProjectController,
    files: &ProductProject,
) -> BTreeMap<NodeId, lpc_model::NodeId> {
    files
        .nodes
        .iter()
        .filter_map(|(id, file)| {
            let node = project.graph_node_for_def(&location(&file.path))?;
            Some((id.clone(), node))
        })
        .collect()
}

/// A lab node as `editor.json` keys it: its tree path.
fn address_of(
    project: &ProjectController,
    nodes: &BTreeMap<NodeId, lpc_model::NodeId>,
    id: &NodeId,
) -> Option<String> {
    project.graph_node_address(*nodes.get(id)?)
}
