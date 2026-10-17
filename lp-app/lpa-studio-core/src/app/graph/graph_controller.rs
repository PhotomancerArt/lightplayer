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
//! **Colours.** [`GraphController::view`] carries the board's frame, built
//! from the frames the board published (`graph_frame.rs`) and kept until
//! a new frame or a new project arrives.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use lab_mapping_files::{Opened, ProductProject};
use lab_mapping_model::{BoardFrame, Editor, NodeId};

use crate::StudioServerClient;
use crate::app::graph::graph_frame;
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
    frame: RefCell<Option<(FrameKey, Rc<BoardFrame>)>>,
}

/// What the open read, and what saving starts from.
#[derive(Debug)]
struct GraphOpened {
    files: ProductProject,
    /// Every path the open asked for, and what it said.
    read: BTreeMap<String, Option<String>>,
    /// The lab's output nodes, as Studio's nodes.
    outputs: BTreeMap<NodeId, lpc_model::NodeId>,
}

/// What the board's frame was built from: the board's frame count and
/// the project it was laid onto.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FrameKey {
    frames_seen: u64,
    revision: u64,
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
        };
        let mut kept = self.frame.borrow_mut();
        if let Some((k, frame)) = kept.as_ref()
            && *k == key
        {
            return frame.clone();
        }
        let frame = Rc::new(graph_frame::board_frame(
            &editor.project,
            &opened.outputs,
            |node| project.graph_output_frame(node),
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
        server: Option<&mut StudioServerClient>,
        wanted: bool,
    ) -> bool {
        let Some(key) = project.graph_files_key() else {
            // No project: nothing to show.
            let had = self.state.is_some() || self.opened.is_some();
            self.close();
            return had;
        };
        if !wanted {
            return false;
        }
        if self.absent_root.as_deref() != Some(key.root.as_str()) {
            self.close();
            self.absent_root = Some(key.root.clone());
        }
        if self.checked.as_ref() == Some(&key) && self.state.is_some() {
            return false;
        }
        let Some(server) = server else {
            return false;
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
                return first;
            }
        }
        self.open(project, server).await;
        self.checked = Some(key);
        true
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
                Ok(opened) => self.adopt(project, opened, read),
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
        project: &ProjectController,
        opened: Opened,
        read: BTreeMap<String, Option<String>>,
    ) {
        let outputs = opened
            .project
            .outputs()
            .iter()
            .filter_map(|n| {
                let file = opened.files.nodes.get(&n.id)?;
                let node = project.graph_node_for_def(&location(&file.path))?;
                Some((n.id.clone(), node))
            })
            .collect();
        let first = self.opened.is_none();
        let Opened {
            project: lab,
            files,
        } = opened;
        {
            let mut editor = self.handle.editor().borrow_mut();
            let mut e = Editor::new(lab).with_view_of(&editor);
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
        }
        self.opened = Some(GraphOpened {
            files,
            read,
            outputs,
        });
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
        self.state = None;
        self.absent.clear();
        self.absent_root = None;
        self.checked = None;
        *self.frame.borrow_mut() = None;
    }
}
