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
/// not wanted, so it opens on demand), agent section COLLAPSED (G1 R-F —
/// cards carry many sections now, and the chat is the one you go looking
/// for), no mirrored draft.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NodeCardUiState {
    /// Whether the code drawer (inline GLSL editor) is expanded.
    pub code_open: bool,
    /// Whether the advanced drawer (generic slot rows) is expanded.
    pub advanced_open: bool,
    /// Whether the module face's **wiring** drawer is expanded — the
    /// bus-as-writers/readers view that replaced the sidebar bus pane
    /// (`docs/design/modules.md` §5). Default `false`: wiring is an
    /// authoring diagnostic, and the panel above it is the product
    /// surface.
    pub wiring_open: bool,
    /// Whether the **Debug** section's rows are expanded. Default `false`:
    /// the section is collapsed to its (always striped) header, which keeps
    /// carrying the DEBUG label, the active-override count, and Clear — so
    /// debug territory stays announced and clearable without expanding.
    pub debug_open: bool,
    /// Whether the shader face's agent section is collapsed to its
    /// summary row. Default `true` (G1 R-F): a shader card now stacks
    /// output, controls, code and advanced, and the agent chat is a thing
    /// you go to on purpose — its collapsed row still names it and carries
    /// the status summary, so nothing about it becomes unfindable.
    pub agent_collapsed: bool,
    /// Which product the module face's hero leads with, when the module's
    /// scope resolves both. Default [`ModuleHeroProduct::Control`].
    pub hero_product: ModuleHeroProduct,
    /// Which spaces this card's visual preview is showing (D15's 1D/2D
    /// checkboxes). `None` — the default — means "whatever this producer's
    /// PRIMARY space is, and only that": the honest default cannot be
    /// spelled as a fixed pair here, because which space is primary is a
    /// probe answer, not card state. Resolve it with
    /// [`NodeCardUiState::preview_spaces_for`]; the first explicit toggle
    /// materializes a concrete pair.
    pub preview_spaces: Option<UiPreviewSpaces>,
    /// The composer draft as last mirrored by the web (write-on-collapse;
    /// see the module doc — this is the remount seed, not the live text).
    pub composer_draft: String,
}

impl Default for NodeCardUiState {
    fn default() -> Self {
        Self {
            code_open: false,
            advanced_open: false,
            wiring_open: false,
            debug_open: false,
            agent_collapsed: true,
            hero_product: ModuleHeroProduct::default(),
            preview_spaces: None,
            composer_draft: String::new(),
        }
    }
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
                NodeCardDrawer::Wiring => self.wiring_open = *open,
            },
            NodeUiOp::SetAgentCollapsed { collapsed, .. } => {
                self.agent_collapsed = *collapsed;
            }
            NodeUiOp::SetDraft { draft, .. } => {
                self.composer_draft = draft.clone();
            }
            NodeUiOp::SetHeroProduct { product, .. } => {
                self.hero_product = *product;
            }
            // D15's "one must stay on": a preview showing nothing is not a
            // state the checkboxes can reach, so an all-off write is
            // dropped rather than applied. Enforced HERE, in the reducer,
            // so no dispatcher — component, story, or e2e — can route
            // around it.
            NodeUiOp::SetPreviewSpaces { spaces, .. } => {
                if spaces.any_checked() {
                    self.preview_spaces = Some(*spaces);
                }
            }
        }
    }

    /// The spaces this card's preview shows, resolved against the
    /// producer's `primary` space: an untouched card shows its primary
    /// space and nothing else (D15's default), and once toggled it shows
    /// exactly what was asked for.
    pub fn preview_spaces_for(&self, primary: crate::UiVisualSpace) -> UiPreviewSpaces {
        self.preview_spaces
            .unwrap_or_else(|| UiPreviewSpaces::only(primary))
    }
}

/// Which spaces a card's visual preview is showing (D15).
///
/// Both on = the stacked view. Never both off — the invariant lives in
/// [`NodeCardUiState::apply`] and in [`UiPreviewSpaces::toggled`], so the
/// type can be constructed freely by tests without a fallible builder.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UiPreviewSpaces {
    /// Show the producer rendered along a 1D strip.
    pub one_d: bool,
    /// Show the producer rendered into 2D texture space.
    pub two_d: bool,
}

impl UiPreviewSpaces {
    /// Exactly one space checked.
    pub fn only(space: crate::UiVisualSpace) -> Self {
        Self {
            one_d: space == crate::UiVisualSpace::OneD,
            two_d: space == crate::UiVisualSpace::TwoD,
        }
    }

    /// Whether `space` is checked.
    pub fn is_checked(self, space: crate::UiVisualSpace) -> bool {
        match space {
            crate::UiVisualSpace::OneD => self.one_d,
            crate::UiVisualSpace::TwoD => self.two_d,
        }
    }

    /// Whether anything is checked at all — the invariant's predicate.
    pub fn any_checked(self) -> bool {
        self.one_d || self.two_d
    }

    /// Every checked space, 1D first (the native reading order for a strip
    /// producer, and a stable order for the stacked view).
    pub fn checked(self) -> impl Iterator<Item = crate::UiVisualSpace> {
        [
            self.one_d.then_some(crate::UiVisualSpace::OneD),
            self.two_d.then_some(crate::UiVisualSpace::TwoD),
        ]
        .into_iter()
        .flatten()
    }

    /// This set with `space` set to `checked`. `None` when that would turn
    /// the last checkbox off — the caller then dispatches nothing, which
    /// is what "one must stay on" looks like at the gesture.
    #[must_use]
    pub fn toggled(self, space: crate::UiVisualSpace, checked: bool) -> Option<Self> {
        let next = match space {
            crate::UiVisualSpace::OneD => Self {
                one_d: checked,
                ..self
            },
            crate::UiVisualSpace::TwoD => Self {
                two_d: checked,
                ..self
            },
        };
        next.any_checked().then_some(next)
    }
}

/// Which of a module scope's two primary products its face's hero leads
/// with.
///
/// **Default is `Control`** (Yona's ruling, 2026-08-07, reversing the
/// visual-first reading of `docs/design/modules.md` R7): a fixture
/// project's output IS the lamps, so leading with the raster the shader
/// painted shows the intermediate instead of the thing the project drives.
/// The visual stays one gesture away — the hero's upper-right toggle
/// writes this per card.
///
/// Whichever kind this names but the scope does not resolve falls back to
/// the other, so a single-product module renders the same either way.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ModuleHeroProduct {
    /// The scope's `control.out` — the fixtures' lamps.
    #[default]
    Control,
    /// The scope's `visual.out` — the R7 output mirror's raster.
    Visual,
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
    /// The module face's **wiring** drawer: this scope's channels with
    /// their writers and readers.
    Wiring,
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
    /// Point the module face's hero at one of its scope's two products
    /// (the hero's upper-right toggle).
    SetHeroProduct {
        node: String,
        product: ModuleHeroProduct,
    },
    /// Set which spaces the card's visual preview shows (D15's 1D/2D
    /// checkboxes). An all-off payload is dropped by the reducer; build
    /// the op with [`NodeUiOp::toggle_preview_space`] and the case never
    /// arises.
    SetPreviewSpaces {
        node: String,
        spaces: UiPreviewSpaces,
    },
}

impl NodeUiOp {
    /// The node address this op targets.
    pub fn node(&self) -> &str {
        match self {
            Self::SetDrawer { node, .. }
            | Self::SetAgentCollapsed { node, .. }
            | Self::SetDraft { node, .. }
            | Self::SetHeroProduct { node, .. }
            | Self::SetPreviewSpaces { node, .. } => node,
        }
    }

    /// THE op sequence a preview-space checkbox dispatches (shared so the
    /// face e2e drives exactly what the component does, like
    /// [`Self::toggle_agent_section`]).
    ///
    /// `current` is the card's RESOLVED set
    /// ([`NodeCardUiState::preview_spaces_for`]) — the checkbox is
    /// toggling what it can see, not the unmaterialized `None`. Empty when
    /// the gesture would uncheck the last space: nothing to dispatch, and
    /// the box stays visibly checked (D15).
    pub fn toggle_preview_space(
        node: &str,
        current: UiPreviewSpaces,
        space: crate::UiVisualSpace,
        checked: bool,
    ) -> Vec<NodeUiOp> {
        current
            .toggled(space, checked)
            .map(|spaces| {
                vec![NodeUiOp::SetPreviewSpaces {
                    node: node.to_string(),
                    spaces,
                }]
            })
            .unwrap_or_default()
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
        assert!(!state.code_open && !state.advanced_open);
        assert!(
            state.agent_collapsed,
            "the agent section defaults to collapsed (G1 R-F)"
        );
        assert!(
            !state.wiring_open,
            "the wiring drawer defaults to collapsed: the panel is the \
             product surface, wiring is the diagnostic"
        );
        assert!(
            !state.debug_open,
            "the Debug section defaults to collapsed (G1 feedback)"
        );
        assert_eq!(
            state.hero_product,
            ModuleHeroProduct::Control,
            "a module face leads with its lamps, not the raster behind them"
        );
        assert_eq!(
            state.preview_spaces, None,
            "an untouched card follows its producer's primary space"
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
        state.apply(&NodeUiOp::SetDrawer {
            node: node.clone(),
            drawer: NodeCardDrawer::Wiring,
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
        state.apply(&NodeUiOp::SetHeroProduct {
            node: node.clone(),
            product: ModuleHeroProduct::Visual,
        });
        state.apply(&NodeUiOp::SetPreviewSpaces {
            node: node.clone(),
            spaces: UiPreviewSpaces {
                one_d: true,
                two_d: true,
            },
        });
        assert_eq!(
            state,
            NodeCardUiState {
                code_open: true,
                advanced_open: true,
                wiring_open: true,
                debug_open: true,
                agent_collapsed: true,
                hero_product: ModuleHeroProduct::Visual,
                preview_spaces: Some(UiPreviewSpaces {
                    one_d: true,
                    two_d: true,
                }),
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
            NodeUiOp::SetHeroProduct {
                node: "/a.module/b.shader".into(),
                product: ModuleHeroProduct::Visual,
            },
            NodeUiOp::SetPreviewSpaces {
                node: "/a.module/b.shader".into(),
                spaces: UiPreviewSpaces::only(crate::UiVisualSpace::TwoD),
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

    /// D15: an untouched card previews its producer's primary space, and
    /// only that — including a 1D producer, which is the whole point of
    /// the default being resolved rather than baked.
    #[test]
    fn preview_spaces_default_to_the_producers_primary_space() {
        let state = NodeCardUiState::default();
        assert_eq!(
            state.preview_spaces_for(crate::UiVisualSpace::TwoD),
            UiPreviewSpaces {
                one_d: false,
                two_d: true,
            }
        );
        assert_eq!(
            state.preview_spaces_for(crate::UiVisualSpace::OneD),
            UiPreviewSpaces {
                one_d: true,
                two_d: false,
            }
        );
        assert_eq!(
            state
                .preview_spaces_for(crate::UiVisualSpace::OneD)
                .checked()
                .collect::<Vec<_>>(),
            vec![crate::UiVisualSpace::OneD],
        );
    }

    /// The at-least-one-on invariant, at both enforcement points: the
    /// gesture builder dispatches nothing, and the reducer would drop the
    /// write even if something hand-rolled it.
    #[test]
    fn the_last_checked_space_cannot_be_turned_off() {
        let node = "/a.module/b.shader";
        let only_2d = UiPreviewSpaces::only(crate::UiVisualSpace::TwoD);

        assert_eq!(
            NodeUiOp::toggle_preview_space(node, only_2d, crate::UiVisualSpace::TwoD, false),
            Vec::new(),
            "unchecking the last box dispatches nothing"
        );
        assert_eq!(
            NodeUiOp::toggle_preview_space(node, only_2d, crate::UiVisualSpace::OneD, true),
            vec![NodeUiOp::SetPreviewSpaces {
                node: node.to_string(),
                spaces: UiPreviewSpaces {
                    one_d: true,
                    two_d: true,
                },
            }],
            "checking the other box stacks the two"
        );

        let mut state = NodeCardUiState::default();
        state.apply(&NodeUiOp::SetPreviewSpaces {
            node: node.to_string(),
            spaces: only_2d,
        });
        state.apply(&NodeUiOp::SetPreviewSpaces {
            node: node.to_string(),
            spaces: UiPreviewSpaces {
                one_d: false,
                two_d: false,
            },
        });
        assert_eq!(
            state.preview_spaces,
            Some(only_2d),
            "the reducer refuses a preview showing nothing"
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
