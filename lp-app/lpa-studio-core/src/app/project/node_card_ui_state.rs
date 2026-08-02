//! A node card's UI VIEW-STATE — core-owned (the node arm of the #140
//! CardUiState re-home; the device arm's ADR,
//! `docs/adr/2026-07-26-card-view-state-ownership.md`, explicitly deferred
//! this slice).
//!
//! What a node card's face is disclosing: whether the code/advanced/debug
//! drawers are expanded, whether the agent section is collapsed, and the
//! last MIRRORED composer draft. This used to live in the web renderer's
//! `use_signal`s (`NodeCardDrawers`, `AgentChatPane`), which meant it died
//! with the component instance — amnesia across re-renders and mode
//! changes, and unreachable by e2e. Core ownership fixes both, exactly as
//! it did for device cards.
//!
//! The state is keyed by the node's ADDRESS PATH (`UiNodeHeader::path`,
//! e.g. `/demo.module/orbit.shader`) so it follows the node, not the
//! widget, and it is pruned when the loaded project closes.
//!
//! **The composer draft is deliberately a MIRROR, not the live value.**
//! Typing stays view-local (a signal owned by the web `ShaderFace`, ABOVE
//! the collapse boundary): per-keystroke `SetDraft` ops would ride the
//! actor queue and rebuild the full editor DTO on every character — the
//! whole-DTO change gate would re-render the editor per keystroke, and a
//! round-tripped controlled textarea invites the input-lag/IME class of
//! bugs. The web mirrors the draft here on collapse (write-on-collapse)
//! and seeds its signal from here on mount (restore-on-remount), so the
//! draft survives both the collapsed branch's unmount of the chat pane
//! and a full card remount.

/// One node card's UI view-state. `Default` is a fresh card: drawers
/// closed (the Debug section included — most of the time those controls are
/// not wanted, so it opens on demand), agent section expanded, no mirrored
/// draft.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct NodeCardUiState {
    /// Whether the code drawer (inline GLSL editor) is expanded.
    pub code_open: bool,
    /// Whether the advanced drawer (generic slot rows) is expanded.
    pub advanced_open: bool,
    /// Whether the **Debug** section's rows are expanded. Default `false`:
    /// the section is collapsed to its (always striped) header, which keeps
    /// carrying the DEBUG label, the active-override count, and Clear — so
    /// debug territory stays announced and clearable without expanding.
    pub debug_open: bool,
    /// Whether the shader face's agent section is collapsed to its
    /// summary row.
    pub agent_collapsed: bool,
    /// The composer draft as last mirrored by the web (write-on-collapse;
    /// see the module doc — this is the remount seed, not the live text).
    pub composer_draft: String,
}

impl NodeCardUiState {
    /// Apply one mutation's payload (the map-level keying lives in
    /// [`ProjectController`](super::ProjectController)).
    pub fn apply(&mut self, op: &NodeUiOp) {
        match op {
            NodeUiOp::SetDrawer { drawer, open, .. } => match drawer {
                NodeCardDrawer::Code => self.code_open = *open,
                NodeCardDrawer::Advanced => self.advanced_open = *open,
                NodeCardDrawer::Debug => self.debug_open = *open,
            },
            NodeUiOp::SetAgentCollapsed { collapsed, .. } => {
                self.agent_collapsed = *collapsed;
            }
            NodeUiOp::SetDraft { draft, .. } => {
                self.composer_draft = draft.clone();
            }
        }
    }
}

/// The expandable drawers under a node card's face.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NodeCardDrawer {
    /// The inline GLSL editor drawer (shader faces).
    Code,
    /// The generic slot-row drawer (every face).
    Advanced,
    /// The **Debug** section's rows (any node declaring a `SlotRole::Debug`
    /// field). Its header is never hidden — only the rows disclose.
    Debug,
}

/// Mutations to a node card's UI view-state, dispatched by the card
/// renderer (drawer toggles, agent collapse, draft mirroring). Keyed by
/// the node's address path so the mutation lands on the right card
/// regardless of which widget rendered it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NodeUiOp {
    /// Expand or collapse one of the card's drawers.
    SetDrawer {
        node: String,
        drawer: NodeCardDrawer,
        open: bool,
    },
    /// Collapse the agent section to its summary row (or expand it back).
    SetAgentCollapsed { node: String, collapsed: bool },
    /// Mirror the composer draft (write-on-collapse; the web seeds its
    /// view-local draft signal from this on mount).
    SetDraft { node: String, draft: String },
}

impl NodeUiOp {
    /// The node address this op targets.
    pub fn node(&self) -> &str {
        match self {
            Self::SetDrawer { node, .. }
            | Self::SetAgentCollapsed { node, .. }
            | Self::SetDraft { node, .. } => node,
        }
    }

    /// The agent section's toggle choreography — THE op sequence the web's
    /// collapse control dispatches (shared so the face e2e drives exactly
    /// what the component does). Collapsing mirrors the live draft FIRST
    /// (the collapsed branch unmounts the composer, and a full card
    /// remount while collapsed reseeds from the mirror), then flips;
    /// expanding is the flip alone — it must never touch the mirror.
    pub fn toggle_agent_section(node: &str, collapsed: bool, live_draft: &str) -> Vec<NodeUiOp> {
        let mut ops = Vec::with_capacity(2);
        if !collapsed {
            ops.push(NodeUiOp::SetDraft {
                node: node.to_string(),
                draft: live_draft.to_string(),
            });
        }
        ops.push(NodeUiOp::SetAgentCollapsed {
            node: node.to_string(),
            collapsed: !collapsed,
        });
        ops
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ops_round_trip_through_the_state() {
        let node = "/demo.module/orbit.shader".to_string();
        let mut state = NodeCardUiState::default();
        assert!(!state.code_open && !state.advanced_open && !state.agent_collapsed);
        assert!(
            !state.debug_open,
            "the Debug section defaults to collapsed (G1 feedback)"
        );
        assert!(state.composer_draft.is_empty());

        state.apply(&NodeUiOp::SetDrawer {
            node: node.clone(),
            drawer: NodeCardDrawer::Code,
            open: true,
        });
        state.apply(&NodeUiOp::SetDrawer {
            node: node.clone(),
            drawer: NodeCardDrawer::Advanced,
            open: true,
        });
        state.apply(&NodeUiOp::SetDrawer {
            node: node.clone(),
            drawer: NodeCardDrawer::Debug,
            open: true,
        });
        state.apply(&NodeUiOp::SetDraft {
            node: node.clone(),
            draft: "make it pulse".to_string(),
        });
        state.apply(&NodeUiOp::SetAgentCollapsed {
            node: node.clone(),
            collapsed: true,
        });
        assert_eq!(
            state,
            NodeCardUiState {
                code_open: true,
                advanced_open: true,
                debug_open: true,
                agent_collapsed: true,
                composer_draft: "make it pulse".to_string(),
            }
        );

        // Collapse/expand never touches the mirrored draft — the survival
        // contract.
        state.apply(&NodeUiOp::SetAgentCollapsed {
            node: node.clone(),
            collapsed: false,
        });
        assert_eq!(state.composer_draft, "make it pulse");

        state.apply(&NodeUiOp::SetDrawer {
            node,
            drawer: NodeCardDrawer::Code,
            open: false,
        });
        assert!(!state.code_open && state.advanced_open);
    }

    #[test]
    fn every_op_names_its_node() {
        let ops = [
            NodeUiOp::SetDrawer {
                node: "/a.module/b.shader".into(),
                drawer: NodeCardDrawer::Advanced,
                open: true,
            },
            NodeUiOp::SetAgentCollapsed {
                node: "/a.module/b.shader".into(),
                collapsed: true,
            },
            NodeUiOp::SetDraft {
                node: "/a.module/b.shader".into(),
                draft: String::new(),
            },
        ];
        for op in &ops {
            assert_eq!(op.node(), "/a.module/b.shader");
        }
    }

    #[test]
    fn collapsing_mirrors_the_live_draft_before_the_flip() {
        // Order is the contract: the mirror must land before the collapse
        // flips, so the state a collapsed remount seeds from already
        // carries the half-typed text.
        let ops = NodeUiOp::toggle_agent_section("/a.module/b.shader", false, "half a thought");
        assert_eq!(
            ops,
            vec![
                NodeUiOp::SetDraft {
                    node: "/a.module/b.shader".into(),
                    draft: "half a thought".into(),
                },
                NodeUiOp::SetAgentCollapsed {
                    node: "/a.module/b.shader".into(),
                    collapsed: true,
                },
            ]
        );
    }

    #[test]
    fn expanding_only_flips_the_collapse_bit() {
        // Expanding must never write the draft: the composer is unmounted
        // while collapsed, so its live value is whatever the caller has on
        // hand — the mirror is the truth and stays untouched.
        let ops = NodeUiOp::toggle_agent_section("/a.module/b.shader", true, "");
        assert_eq!(
            ops,
            vec![NodeUiOp::SetAgentCollapsed {
                node: "/a.module/b.shader".into(),
                collapsed: false,
            }]
        );
    }
}
