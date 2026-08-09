//! The fixture's Shape declaration moment (vision D13, plan-B P5): the
//! dimensionality section **in guided clothing**.
//!
//! A freshly created fixture doesn't know what it is yet — the starter is
//! a generic grid, not a declared shape — so instead of the compact
//! section, the card asks once: *what shape is this fixture?* Four preset
//! tiles (spike §4's set, judged at G2): **Strip**, **Matrix**, **Mapped
//! shape**, and a disabled **3D (soon)**. The moment fires wherever a
//! fixture is born (`+ fixture`, paste of an undeclared fixture) and only
//! there: the trigger is `NodeCardUiState::shape_guided`, set by the
//! controller's create/paste paths and absent for every existing project.
//!
//! **No parallel write path.** A preset is nothing but a batch of the
//! ordinary slot ops at the addresses [`UiShapePresets`] carries — the
//! same `EnsurePresent`/`SetValue` the advanced-drawer rows and the
//! dimensionality section dispatch. [`shape_preset_actions`] is THE
//! preset→slot-writes seam (see the DTO's doc for the D15 retrofit
//! contract: a declared fixture space later means one more address there
//! and one more write here).
//!
//! What the presets mean today, in the engine's own terms
//! (`fixture_carries_2d_coords`: 2D membership = a map, or a render area
//! taller than one row):
//!
//! - **Strip** — no map, a 1-row render area, strip order meaningful:
//!   `{1D}`, wire order IS the layout (the dropdown reads `along the
//!   wire`).
//! - **Matrix** — no map, a square render area, strip order NOT
//!   meaningful: `{2D}` from authored intent, wire order is plumbing.
//! - **Mapped shape** — the map is the shape: opens the in-place mapping
//!   editor (the output section's edit mode — no new mapping UI). The D3
//!   strip-order follow-up question is NOT asked here (dropped at the
//!   strip-order unification: the bit is the dimensionality dropdown's
//!   first choice, one drawer below).
//! - **3D (soon)** — disabled; honesty about the roadmap.

use dioxus::prelude::*;
use lpa_studio_core::{LpValue, NodeUiOp, UiAction, UiShapePresets};

use crate::app::node::face::node_ui_action;
use crate::app::node::slot_edit_actions::{slot_ensure_present_action, slot_set_value_action};

/// The guided moment's question line (spike §4 wording).
const SHAPE_PROMPT: &str = "What shape is this fixture?";
/// Why the card is asking (spike §4 wording, trimmed of pack-matching
/// promises that don't exist yet).
const SHAPE_PROMPT_SUB: &str = "This is the fixture's identity — effects are matched to it.";
/// The dismiss affordance: the moment is an offer, never a gate.
const SHAPE_SKIP: &str = "skip — keep it as it is";

/// One preset tile of the Shape moment (Q7's proposed set).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ShapePreset {
    /// A line of pixels: 1D, wire order is the layout.
    Strip,
    /// A grid or panel: 2D by authored render area.
    Matrix,
    /// A strip placed in space: the map is the shape.
    MappedShape,
    /// A cube or volume — not yet.
    ThreeD,
}

impl ShapePreset {
    const fn label(self) -> &'static str {
        match self {
            Self::Strip => "Strip",
            Self::Matrix => "Matrix",
            Self::MappedShape => "Mapped shape",
            Self::ThreeD => "3D (soon)",
        }
    }

    const fn hint(self) -> &'static str {
        match self {
            Self::Strip => "1D — a line of pixels",
            Self::Matrix => "2D — a grid or panel",
            Self::MappedShape => "a strip placed in space",
            Self::ThreeD => "a cube or volume",
        }
    }
}

/// The render area a Strip preset declares: the board wizard's strip
/// width, one row tall — the 1-row area IS the declared-strip idiom the
/// engine reads (`fixture_carries_2d_coords`).
const STRIP_RENDER_SIZE: (u32, u32) = (lpa_studio_core::DEFAULT_STRIP_PIXELS, 1);
/// The render area a Matrix preset declares: the model default restated
/// as authored intent (taller than one row = 2D membership).
const MATRIX_RENDER_SIZE: (u32, u32) = (16, 16);

/// THE op batch one preset dispatches — the preset→slot-writes seam (see
/// the module doc and [`UiShapePresets`]). Every op is one the generic
/// rows already send; the batch ends with the card-UI clear so the
/// section collapses back to its compact form. `MappedShape` writes no
/// slots (the map already is the shape — the caller opens the mapping
/// editor); `ThreeD` is unreachable (disabled tile) and dispatches
/// nothing.
pub(crate) fn shape_preset_actions(
    preset: ShapePreset,
    node: &str,
    presets: &UiShapePresets,
) -> Vec<UiAction> {
    let mut actions = Vec::new();
    let clear = |actions: &mut Vec<UiAction>| {
        actions.push(node_ui_action(NodeUiOp::SetShapeGuided {
            node: node.to_string(),
            guided: false,
        }));
    };
    match preset {
        ShapePreset::Strip | ShapePreset::Matrix => {
            let (size, strip_order) = if preset == ShapePreset::Strip {
                (STRIP_RENDER_SIZE, true)
            } else {
                (MATRIX_RENDER_SIZE, false)
            };
            if let Some(strip) = &presets.strip_order {
                actions.push(slot_set_value_action(
                    strip.clone(),
                    LpValue::Bool(strip_order),
                ));
            }
            // The area shapes carry no map: their shape IS the render
            // area (ensure is the enum row's own gesture; a mapping
            // already `Unset` makes it a no-op).
            if let Some(target) = presets
                .mapping
                .as_ref()
                .and_then(|mapping| mapping.child_field("Unset"))
            {
                actions.push(slot_ensure_present_action(target));
            }
            if let Some(render_size) = &presets.render_size {
                actions.push(slot_set_value_action(
                    render_size.clone(),
                    dim2u_value(size.0, size.1),
                ));
            }
            clear(&mut actions);
        }
        ShapePreset::MappedShape => clear(&mut actions),
        ShapePreset::ThreeD => {}
    }
    actions
}

/// A whole `Dim2u` value, shaped exactly as the dimensions field
/// dispatches it (one `SetValue` of the full struct).
fn dim2u_value(width: u32, height: u32) -> LpValue {
    LpValue::Struct {
        name: Some("Dim2u".to_string()),
        fields: vec![
            ("width".to_string(), LpValue::U32(width)),
            ("height".to_string(), LpValue::U32(height)),
        ],
    }
}

/// The guided body the fixture's dimensionality section renders while
/// `NodeCardUiState::shape_guided` is set: prompt, preset tiles, skip.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn FixtureShapeMoment(
    presets: UiShapePresets,
    /// The node's address path — the card-UI key the clear op targets.
    /// Absent (stories) renders the tiles inert.
    #[props(default = None)]
    node: Option<String>,
    /// Opens the in-place mapping editor (the output section's edit
    /// mode) — the Mapped-shape tile's landing. The caller owns the
    /// editor's disclosure signal.
    #[props(default = None)]
    on_open_mapping: Option<EventHandler<()>>,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let mappable = presets.has_map2d && on_open_mapping.is_some();
    let dispatch_preset = {
        let presets = presets.clone();
        let node = node.clone();
        move |preset: ShapePreset| {
            let (Some(node), Some(handler)) = (node.as_ref(), on_action) else {
                return;
            };
            for action in shape_preset_actions(preset, node, &presets) {
                handler.call(action);
            }
        }
    };
    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:gap-2 tw:px-4 tw:py-3",
            div { class: "tw:grid tw:gap-0.5",
                span { class: "tw:text-sm tw:font-bold tw:text-strong-foreground", "{SHAPE_PROMPT}" }
                span { class: "tw:text-[11px] tw:leading-snug tw:text-dim-foreground",
                    "{SHAPE_PROMPT_SUB}"
                }
            }
            div { class: "tw:grid tw:min-w-0 tw:grid-cols-[repeat(auto-fill,minmax(7.5rem,1fr))] tw:gap-1.5",
                for preset in [ShapePreset::Strip, ShapePreset::Matrix] {
                    button {
                        key: "{preset:?}",
                        class: shape_tile_class(true),
                        r#type: "button",
                        title: "{preset.hint()}",
                        onclick: {
                            let dispatch_preset = dispatch_preset.clone();
                            move |event: MouseEvent| {
                                event.stop_propagation();
                                dispatch_preset(preset);
                            }
                        },
                        span { class: "tw:block tw:h-10 tw:w-full tw:overflow-hidden tw:rounded-xs tw:bg-page",
                            ShapeGlyph { preset }
                        }
                        span { class: "tw:min-w-0 tw:truncate tw:text-[11px] tw:font-bold", "{preset.label()}" }
                        span { class: "tw:min-w-0 tw:truncate tw:text-[10px] tw:leading-tight tw:text-dim-foreground",
                            "{preset.hint()}"
                        }
                    }
                }
                button {
                    key: "mapped",
                    class: shape_tile_class(mappable),
                    r#type: "button",
                    disabled: !mappable,
                    title: if mappable { "open the mapping editor — the map is the shape" } else { "needs a map2d mapping document (add one in the advanced drawer)" },
                    onclick: {
                        let dispatch_preset = dispatch_preset.clone();
                        move |event: MouseEvent| {
                            event.stop_propagation();
                            if !mappable {
                                return;
                            }
                            dispatch_preset(ShapePreset::MappedShape);
                            if let Some(open) = on_open_mapping {
                                open.call(());
                            }
                        }
                    },
                    span { class: "tw:block tw:h-10 tw:w-full tw:overflow-hidden tw:rounded-xs tw:bg-page",
                        ShapeGlyph { preset: ShapePreset::MappedShape }
                    }
                    span { class: "tw:min-w-0 tw:truncate tw:text-[11px] tw:font-bold",
                        "{ShapePreset::MappedShape.label()}"
                    }
                    span { class: "tw:min-w-0 tw:truncate tw:text-[10px] tw:leading-tight tw:text-dim-foreground",
                        "{ShapePreset::MappedShape.hint()}"
                    }
                }
                button {
                    key: "threed",
                    class: shape_tile_class(false),
                    r#type: "button",
                    disabled: true,
                    title: "{ShapePreset::ThreeD.hint()} — not yet",
                    span { class: "tw:block tw:h-10 tw:w-full tw:overflow-hidden tw:rounded-xs tw:bg-page",
                        ShapeGlyph { preset: ShapePreset::ThreeD }
                    }
                    span { class: "tw:min-w-0 tw:truncate tw:text-[11px] tw:font-bold",
                        "{ShapePreset::ThreeD.label()}"
                    }
                    span { class: "tw:min-w-0 tw:truncate tw:text-[10px] tw:leading-tight tw:text-dim-foreground",
                        "{ShapePreset::ThreeD.hint()}"
                    }
                }
            }
            button {
                class: "tw:justify-self-start tw:cursor-pointer tw:appearance-none tw:border-0 tw:bg-transparent tw:p-0 tw:text-[11px] tw:text-subtle-foreground tw:underline tw:hover:text-soft-foreground",
                r#type: "button",
                onclick: move |event: MouseEvent| {
                    event.stop_propagation();
                    let (Some(node), Some(handler)) = (node.clone(), on_action) else {
                        return;
                    };
                    handler
                        .call(
                            node_ui_action(NodeUiOp::SetShapeGuided {
                                node,
                                guided: false,
                            }),
                        );
                },
                "{SHAPE_SKIP}"
            }
        }
    }
}

/// A preset tile's frame — the choice-tile language, with the disabled
/// state visibly inert.
fn shape_tile_class(enabled: bool) -> &'static str {
    if enabled {
        "tw:grid tw:min-w-0 tw:cursor-pointer tw:appearance-none tw:gap-0.5 tw:rounded-xs tw:border tw:border-border-subtle tw:bg-transparent tw:p-1.5 tw:text-left tw:text-muted-foreground tw:hover:border-accent-border tw:hover:text-strong-foreground"
    } else {
        "tw:grid tw:min-w-0 tw:appearance-none tw:gap-0.5 tw:rounded-xs tw:border tw:border-border-subtle tw:bg-transparent tw:p-1.5 tw:text-left tw:text-dim-foreground tw:opacity-60"
    }
}

/// The spike §4 shape drawings: a line of lamps, a grid, a ring of lamps,
/// a cube — stroke schematics in the section's quiet foreground.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ShapeGlyph(preset: ShapePreset) -> Element {
    rsx! {
        svg {
            class: "tw:block tw:h-full tw:w-full tw:text-soft-foreground",
            view_box: "0 0 44 30",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "1.2",
            role: "img",
            match preset {
                ShapePreset::Strip => rsx! {
                    line { x1: "4", y1: "15", x2: "40", y2: "15" }
                    for (index , x) in [8u32, 15, 22, 29, 36].into_iter().enumerate() {
                        circle {
                            key: "{index}",
                            cx: "{x}",
                            cy: "15",
                            r: "1.6",
                            fill: "currentColor",
                        }
                    }
                },
                ShapePreset::Matrix => rsx! {
                    rect {
                        x: "8",
                        y: "3",
                        width: "28",
                        height: "24",
                        rx: "2",
                    }
                    path { d: "M8 11h28M8 19h28M17 3v24M26 3v24" }
                },
                ShapePreset::MappedShape => rsx! {
                    circle { cx: "22", cy: "15", r: "11" }
                    for (index , (x , y)) in [
                        (22u32, 4u32),
                        (30, 7),
                        (33, 15),
                        (30, 23),
                        (22, 26),
                        (14, 23),
                        (11, 15),
                        (14, 7),
                    ]
                        .into_iter()
                        .enumerate()
                    {
                        circle {
                            key: "{index}",
                            cx: "{x}",
                            cy: "{y}",
                            r: "1.6",
                            fill: "currentColor",
                        }
                    }
                },
                ShapePreset::ThreeD => rsx! {
                    path { d: "M22 3 36 9v13l-14 6-14-6V9Z" }
                    path { d: "M8 9l14 6 14-6M22 15v13" }
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use lpa_studio_core::{ProjectNodeAddress, ProjectSlotAddress, ProjectSlotRoot};

    use super::*;

    fn presets() -> UiShapePresets {
        let address = |path: &str| {
            Some(ProjectSlotAddress::new(
                ProjectNodeAddress::parse("/demo.module/halo.fixture").expect("address"),
                ProjectSlotRoot::def(),
                lpc_model::SlotPath::parse(path).expect("path"),
            ))
        };
        UiShapePresets {
            mapping: address("mapping"),
            render_size: address("render_size"),
            strip_order: address("strip_order_meaningful"),
            has_map2d: true,
        }
    }

    /// The seam's op batches: Strip and Matrix each write the three real
    /// slots (strip-order bit, mapping → Unset, render size) and clear
    /// the guided bit; Mapped shape only clears (the map already is the
    /// shape); the disabled 3D tile dispatches nothing.
    #[test]
    fn presets_fill_real_slots_and_clear_the_guided_bit() {
        let node = "/demo.module/halo.fixture";
        assert_eq!(
            shape_preset_actions(ShapePreset::Strip, node, &presets()).len(),
            4,
            "strip order, mapping ensure, render size, clear"
        );
        assert_eq!(
            shape_preset_actions(ShapePreset::Matrix, node, &presets()).len(),
            4
        );
        assert_eq!(
            shape_preset_actions(ShapePreset::MappedShape, node, &presets()).len(),
            1,
            "no slot writes — the mapping editor is the landing"
        );
        assert!(shape_preset_actions(ShapePreset::ThreeD, node, &presets()).is_empty());
    }

    /// Missing rows drop their writes rather than inventing addresses —
    /// the batch still ends in the clear so the moment always resolves.
    #[test]
    fn missing_rows_drop_their_writes_but_the_moment_still_resolves() {
        let bare = UiShapePresets {
            mapping: None,
            render_size: None,
            strip_order: None,
            has_map2d: false,
        };
        let actions = shape_preset_actions(ShapePreset::Strip, "/demo.module/halo.fixture", &bare);
        assert_eq!(actions.len(), 1, "only the guided-bit clear remains");
    }

    /// The declared areas: a strip is ONE row (the engine's
    /// `fixture_carries_2d_coords` reads height > 1 as 2D membership), a
    /// matrix is taller.
    #[test]
    fn the_preset_areas_encode_the_dimensionality() {
        assert_eq!(STRIP_RENDER_SIZE.1, 1);
        assert!(MATRIX_RENDER_SIZE.1 > 1);
    }
}
