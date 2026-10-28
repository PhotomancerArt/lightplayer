//! The Graph view's stories: the mapping lab's canvas on real projects,
//! with the lab's tree and inspector beside it. No board is attached in a
//! story, so the lab's simulated board stands in for the board's frames.
//!
//! The projects are the repo's own, at project format 12 with their
//! mappings in map2d format 6 (baked into `lab_mapping_files::embedded`).
//! A story that shows an edit makes it the way an offer does: the verb by
//! name, its values as text (`GraphEdit::bind`), through `Editor::edit`.

use dioxus::prelude::*;
use lab_mapping_files::MemFiles;
use lab_mapping_files::Opened;
use lab_mapping_files::embedded::{
    CHOKER, CIRCULAR_DISPLAY, CIRCULAR_DISPLAY_WALK, EAVES, Embedded, FIBER_HEADBAND, PLASMA,
    ZOOK_DOME,
};
use lab_mapping_model::{
    Editor, GraphEdit, GraphInput, GraphVerb, LayoutKind, NodeId, PartRef, Presentation, Target,
    TreeRef, Vec2,
};
use lpa_studio_core::app::graph::{GraphHandle, UiGraphFrame, UiGraphState, UiGraphView};
use lpa_studio_web_story_macros::story;
use lpc_mapping::part::{Circle, PartKind, Shape};

use super::graph_view::{GraphCenter, GraphPropsPanel, GraphTreePanel, use_graph_page_with};

#[story(
    description = "The PLAYFUL choker on the Graph, nothing selected: every node a card laid out by role — clock and power button on top, the shader feeding the fixture over visual.out, the fixture feeding its output over control.out — and the fixture in Studio's live view, its lamps as cells of light."
)]
fn graph_choker_live() -> Element {
    rsx! { GraphStory { example: "catalog/playful-choker", setup: Setup::Nothing } }
}

#[story(
    description = "The choker's fixture selected: it opens as the mapping editor in place — its twelve paths, where data enters and which way it runs — and the inspector shows its size and patch rule."
)]
fn graph_choker_fixture_selected() -> Element {
    rsx! { GraphStory { example: "catalog/playful-choker", setup: Setup::Fixture } }
}

#[story(
    description = "One of the choker's paths selected: its span on the output's port is lit, the inspector says which pixels it takes, and the rest of the fixture dims."
)]
fn graph_choker_span_selected() -> Element {
    rsx! { GraphStory { example: "catalog/playful-choker", setup: Setup::FirstPath } }
}

#[story(
    description = "A catalog pattern's fixture selected: a one-lamp grid and a disc of eight rings on one strand, each drawn whole (the grid as its box, the disc as its rings), its lamps where the product puts them."
)]
fn graph_pattern_whole_shapes() -> Element {
    rsx! { GraphStory { example: "catalog/plasma", setup: Setup::Fixture } }
}

#[story(
    description = "A fixture of hand-placed lamps (a points part): the Graph opens it as its part, every lamp where the board puts it."
)]
fn graph_point_list_box() -> Element {
    rsx! { GraphStory { example: "catalog/fiber-headband", setup: Setup::Nothing } }
}

#[story(
    description = "A project the Graph can't open: its mapping file is from a newer LightPlayer. The headline, and the reader's own reason, small, under it."
)]
fn graph_refused() -> Element {
    rsx! { GraphStory { example: "catalog/playful-choker", setup: Setup::Refused } }
}

#[story(
    description = "The choker in the stack layout: one column in tree order at a fixed zoom, scrolling natively — the Graph's other layout, switched in its toolbar."
)]
fn graph_choker_stack() -> Element {
    rsx! { GraphStory { example: "catalog/playful-choker", setup: Setup::Stack } }
}

#[story(
    description = "The circular display as soldered, ring 3 selected, presentation A: the inspector's Part · circle1 then Segment · circle1/r3, each start value saying where it came from — port D1 and lamp 0 set here, with Back to the flow — and ring r4 and the centre following it."
)]
fn graph_circular_display_ring3_set_here() -> Element {
    rsx! {
        GraphStory {
            example: "projects/test/circular-display-6",
            setup: Setup::RingThree(Presentation::A),
        }
    }
}

#[story(
    description = "The same ring 3, presentation B: the start the circle's flow would give it, as one quiet line, then only what is set here, each with Back to the flow, and Set here… for the rest."
)]
fn graph_circular_display_ring3_presentation_b() -> Element {
    rsx! {
        GraphStory {
            example: "projects/test/circular-display-6",
            setup: Setup::RingThree(Presentation::B),
        }
    }
}

#[story(
    description = "The circular display mapped from nothing on its five ports, its circle's flow one ring per port from D0: the circle selected, Flow says so, and each ring lands at lamp 0 of its own port (ring 3 still reversed)."
)]
fn graph_circular_display_one_ring_per_port() -> Element {
    rsx! {
        GraphStory {
            example: "projects/test/circular-display-walk",
            setup: Setup::OneRingPerPort,
        }
    }
}

#[story(
    description = "The zook dome slice, one copy per port, copy 3 moved to the last port: copy 3 selected, its port set here, and the overlap with copy 4 on that port shown on the output and as a problem."
)]
fn graph_zook_copy3_moved() -> Element {
    rsx! { GraphStory { example: "catalog/zook-dome", setup: Setup::ZookCopyThree } }
}

#[story(
    description = "The choker with the F's lower stroke started at lamp 60: f-lower selected, Starts at set here, and the strokes after it following on from it."
)]
fn graph_choker_f_at_60() -> Element {
    rsx! { GraphStory { example: "catalog/playful-choker", setup: Setup::ChokerF } }
}

#[story(
    description = "Sean's eaves with the front run reversed: the front selected, its Direction set here; the side run wraps onto the next port, end to end."
)]
fn graph_eaves_front_reversed() -> Element {
    rsx! { GraphStory { example: "lab/mapping/projects/eaves", setup: Setup::EavesFront } }
}

#[story(
    description = "Problems, all three kinds on the zook dome: copies past the last port, two copies sharing a port, and an override on a copy counted away. The fixture selected: its title tab counts them and its Problems section says each in a sentence, with Remove on the gone override."
)]
fn graph_problems() -> Element {
    rsx! { GraphStory { example: "catalog/zook-dome", setup: Setup::Problems } }
}

#[story(
    description = "The circular display's output, its two ports as rows of spans: rings r1 and r2 then a gap on D0, ring 3, ring 4 and the centre then a gap on D1. The gap on D0 selected."
)]
fn graph_output_port_rows() -> Element {
    rsx! {
        GraphStory {
            example: "projects/test/circular-display-6",
            setup: Setup::PortGap,
        }
    }
}

/// What a story does to the editor before it draws.
#[derive(Clone, Copy, PartialEq)]
enum Setup {
    Nothing,
    Fixture,
    FirstPath,
    Stack,
    /// A project the reader refuses.
    Refused,
    /// Ring 3 of the circular display, shown in a presentation.
    RingThree(Presentation),
    /// The display from nothing, one ring per port.
    OneRingPerPort,
    /// The zook slice's copy 3 moved onto copy 4's port.
    ZookCopyThree,
    /// The choker's F lower started at lamp 60.
    ChokerF,
    /// The eaves' front run reversed.
    EavesFront,
    /// Past the last port, an overlap, a gone override.
    Problems,
    /// A gap on the circular display's first port.
    PortGap,
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn GraphStory(example: &'static str, setup: Setup) -> Element {
    let (handle, refused) = use_hook(|| story_handle(example, setup));
    let view = {
        let e = handle.editor().borrow();
        UiGraphView {
            state: match &refused {
                Some(why) => UiGraphState::Refused { why: why.clone() },
                None => UiGraphState::Open { live: true },
            },
            generation: 1,
            frame: UiGraphFrame(std::rc::Rc::new(e.board_frame(0.0))),
        }
    };
    let page = use_graph_page_with(handle, Some(view.clone()));
    rsx! {
        div { class: "tw:flex tw:h-[640px] tw:overflow-hidden tw:rounded-md tw:border tw:border-border-strong",
            div { class: "tw:w-[240px] tw:flex-none tw:overflow-y-auto tw:bg-card-subtle tw:p-2.5",
                GraphTreePanel { page: page.clone() }
            }
            div { class: "tw:flex tw:min-w-0 tw:flex-1 tw:flex-col tw:border-x tw:border-border-subtle",
                GraphCenter {
                    page: page.clone(),
                    state: Some(view.state.clone()),
                    on_action: move |_| {},
                }
            }
            div { class: "tw:w-[280px] tw:flex-none tw:overflow-y-auto tw:bg-card-subtle tw:p-2.5",
                GraphPropsPanel { page }
            }
        }
    }
}

/// A handle on an editor holding the embedded example, edited and
/// selected as the story says, with its layout pinned; or, for a project
/// the reader refuses, an empty editor (as core leaves it) and the
/// reader's own reason.
///
/// Every story chooses its layout: the canvas, or the stack for the one
/// that is about it. Left unchosen, the layout follows the size class the
/// canvas reports on its first resize, and which one a capture sees then
/// depends on whether that report lands before the shot. A chosen layout
/// is never undone by a size report (`Editor::layout`).
fn story_handle(example: &str, setup: Setup) -> (GraphHandle, Option<String>) {
    if matches!(setup, Setup::Refused) {
        return (GraphHandle::default(), Some(newer_mapping_refusal(example)));
    }
    let opened = Opened::open(&embedded(example).files()).expect("the example opens on the Graph");
    let fixture = NodeId::new("fixture");
    let handle = GraphHandle::default();
    {
        let mut e = handle.editor().borrow_mut();
        *e = Editor::new(opened.project);
        edit(&mut e, &fixture, setup);
        e.resize(Vec2::new(720.0, 560.0));
        let placement = lab_layout::canvas_layout(&e.layout_input());
        e.set_placement(&placement);
        e.fit();
    }
    let layout = match setup {
        Setup::Stack => LayoutKind::Stack,
        _ => LayoutKind::Canvas,
    };
    handle.input(GraphInput::SetLayout(Some(layout)));
    if let Setup::RingThree(presentation) = setup {
        handle.input(GraphInput::Presentation(presentation));
    }
    let selected = selection(&handle.editor().borrow(), &fixture, setup);
    if let Some(selected) = selected {
        handle.input(GraphInput::Select(vec![selected]));
    }
    (handle, None)
}

/// The story's edits, each pressed by its verb's name and values.
fn edit(e: &mut Editor, f: &NodeId, setup: Setup) {
    let at = |id: &str| format!("{f}:{id}");
    match setup {
        Setup::OneRingPerPort => {
            from_nothing(e, f);
            press(
                e,
                "back-to-the-flow",
                &[("subject", &at("circle1/r3")), ("field", "start")],
            );
            press(
                e,
                "set-flow",
                &[("subject", &at("circle1")), ("flow", "one-per-port:0")],
            );
        }
        Setup::ZookCopyThree => {
            press(
                e,
                "set-flow",
                &[("subject", &at("repeat1")), ("flow", "one-per-port:0")],
            );
            press(
                e,
                "set-port",
                &[("subject", &at("sector@3")), ("port", "output:4")],
            );
        }
        Setup::ChokerF => {
            press(
                e,
                "set-starts-at",
                &[("subject", &at("f-lower")), ("lamps", "60")],
            );
        }
        Setup::EavesFront => {
            press(
                e,
                "set-direction",
                &[("subject", &at("front")), ("direction", "reverse")],
            );
        }
        Setup::Problems => {
            // An override on copy 4, then the copies counted down past it.
            press(
                e,
                "set-direction",
                &[("subject", &at("sector@4")), ("direction", "reverse")],
            );
            press(
                e,
                "set-count",
                &[("subject", &at("repeat1")), ("count", "4")],
            );
            // One copy per port from D2: copy 3 lands past the last port.
            press(
                e,
                "set-flow",
                &[("subject", &at("repeat1")), ("flow", "one-per-port:2")],
            );
            // Copy 1 moved onto copy 2's port.
            press(
                e,
                "set-port",
                &[("subject", &at("sector@1")), ("port", "output:4")],
            );
        }
        _ => {}
    }
}

/// What the story selects.
fn selection(e: &Editor, f: &NodeId, setup: Setup) -> Option<Target> {
    let node = |id: &str| TreeRef::parse(f, id).map(Target::Generated);
    let part = |id: &str| PartRef::parse(f, id).map(Target::Part);
    match setup {
        Setup::Nothing | Setup::Stack | Setup::Refused => None,
        Setup::Fixture | Setup::Problems => Some(Target::Node(f.clone())),
        Setup::FirstPath => e
            .project
            .fixture(f)
            .map(|x| Target::Part(PartRef::new(f, &x.parts()[0].id))),
        Setup::RingThree(_) => node("circle1/r3"),
        Setup::OneRingPerPort => part("circle1"),
        Setup::ZookCopyThree => node("sector@3"),
        Setup::ChokerF => part("f-lower"),
        Setup::EavesFront => part("front"),
        Setup::PortGap => {
            let output = NodeId::new("output");
            let row = e.project.port_row(&output, "0");
            let gap = row.spans.iter().find(|s| s.segment.is_none())?;
            Some(Target::Span {
                output,
                key: "0".to_string(),
                segment: None,
                start: gap.channels.start / 3,
            })
        }
    }
}

/// The circular display mapped from nothing on the walk project, as the
/// core journey `the_circular_display_from_nothing` presses it: the
/// starter strip deleted, the box made 100 square, a circle drawn (the
/// pointer's part, added here), its ring 24 lamps, rings of 16, 12 and 8
/// and a centre lamp, the fixture started on D0 (naming its output), ring
/// 3 reversed and on D1.
fn from_nothing(e: &mut Editor, f: &NodeId) {
    let at = |id: &str| format!("{f}:{id}");
    press(e, "delete", &[("subject", &at("strip"))]);
    press(
        e,
        "set-fixture-size",
        &[("subject", &at("")), ("width", "100"), ("height", "100")],
    );
    e.project
        .add(
            f,
            PartKind::Shape(Shape::Circle(Circle::new([50.0, 50.0], &[(42.0, 24)]))),
        )
        .expect("a circle drawn");
    for (count, radius) in [("16", "32"), ("12", "22"), ("8", "12")] {
        press(
            e,
            "add-ring",
            &[
                ("subject", &at("circle1")),
                ("count", count),
                ("radius", radius),
            ],
        );
    }
    press(e, "add-centre-lamp", &[("subject", &at("circle1"))]);
    press(
        e,
        "set-fixture-start",
        &[("subject", &at("")), ("port", "output:0"), ("lamps", "0")],
    );
    press(
        e,
        "set-direction",
        &[("subject", &at("circle1/r3")), ("direction", "reverse")],
    );
    press(
        e,
        "set-port",
        &[("subject", &at("circle1/r3")), ("port", "output:1")],
    );
}

/// Press the verb `name` with `args`, as its offer does.
fn press(e: &mut Editor, name: &str, args: &[(&str, &str)]) {
    let verb = GraphVerb::from_name(name).unwrap_or_else(|| panic!("no verb {name}"));
    let edit = GraphEdit::bind(verb, &|param| {
        args.iter()
            .find(|(k, _)| *k == param)
            .map(|(_, v)| v.to_string())
    })
    .unwrap_or_else(|why| panic!("{name}: {why}"));
    e.edit(&[edit])
        .unwrap_or_else(|why| panic!("{name}: {why}"));
}

/// What the reader says of `example` with its mapping one format newer
/// than this build reads: the Graph's real refusal.
fn newer_mapping_refusal(example: &str) -> String {
    let files: Vec<(String, String)> = embedded(example)
        .files()
        .files
        .into_iter()
        .map(|(path, text)| {
            let text = if path.ends_with(".map2d.json") {
                text.replacen("\"format\": 6", "\"format\": 7", 1)
            } else {
                text
            };
            (path, text)
        })
        .collect();
    Opened::open(&MemFiles::new(files))
        .err()
        .expect("the reader refuses a mapping from a newer LightPlayer")
        .to_string()
}

/// A repo project, as baked in.
fn embedded(example: &str) -> &'static Embedded {
    match example {
        "catalog/playful-choker" => &CHOKER,
        "catalog/plasma" => &PLASMA,
        "catalog/fiber-headband" => &FIBER_HEADBAND,
        "catalog/zook-dome" => &ZOOK_DOME,
        "lab/mapping/projects/eaves" => &EAVES,
        "projects/test/circular-display-6" => &CIRCULAR_DISPLAY,
        "projects/test/circular-display-walk" => &CIRCULAR_DISPLAY_WALK,
        other => panic!("{other} isn't baked into lab_mapping_files::embedded"),
    }
}

#[cfg(test)]
mod tests {
    use lab_mapping_model::ProblemKind;

    use super::*;

    /// Every Graph story's project opens and its edits are made by their
    /// verbs, natively: a story never panics in a capture.
    #[test]
    fn every_graph_story_sets_up() {
        let stories = [
            ("catalog/playful-choker", Setup::Nothing),
            ("catalog/playful-choker", Setup::Fixture),
            ("catalog/playful-choker", Setup::FirstPath),
            ("catalog/playful-choker", Setup::Stack),
            ("catalog/plasma", Setup::Fixture),
            ("catalog/fiber-headband", Setup::Nothing),
            (
                "projects/test/circular-display-6",
                Setup::RingThree(Presentation::A),
            ),
            (
                "projects/test/circular-display-6",
                Setup::RingThree(Presentation::B),
            ),
            ("projects/test/circular-display-walk", Setup::OneRingPerPort),
            ("catalog/zook-dome", Setup::ZookCopyThree),
            ("catalog/playful-choker", Setup::ChokerF),
            ("lab/mapping/projects/eaves", Setup::EavesFront),
            ("catalog/zook-dome", Setup::Problems),
            ("projects/test/circular-display-6", Setup::PortGap),
        ];
        for (example, setup) in stories {
            let (handle, refused) = story_handle(example, setup);
            assert!(refused.is_none(), "{example}");
            let e = handle.editor().borrow();
            let selected = selection(&e, &NodeId::new("fixture"), setup);
            assert_eq!(
                e.selection,
                selected.into_iter().collect::<Vec<_>>(),
                "{example}"
            );
        }
    }

    /// The refusal story says the reader's own words: a mapping from a
    /// newer LightPlayer.
    #[test]
    fn the_refused_story_is_a_real_refusal() {
        let (_, refused) = story_handle("catalog/playful-choker", Setup::Refused);
        let why = refused.expect("refused");
        assert!(why.contains("newer"), "{why}");
    }

    /// The problems story holds all three: past the last port, an overlap
    /// and a gone override; the one-ring-per-port story none, every ring
    /// starting at lamp 0 of its own port.
    #[test]
    fn the_problems_story_shows_three_kinds_and_one_per_port_none() {
        let (handle, _) = story_handle("catalog/zook-dome", Setup::Problems);
        let p = handle.editor().borrow().project.clone();
        let problems = &p.effective_patch().problems;
        assert!(
            problems
                .iter()
                .any(|x| matches!(x.kind, ProblemKind::PastLastPort { .. }))
        );
        assert!(
            problems
                .iter()
                .any(|x| matches!(x.kind, ProblemKind::GoneNode(_)))
        );
        assert!(!p.row_overlaps().is_empty(), "an overlap");

        let (handle, _) =
            story_handle("projects/test/circular-display-walk", Setup::OneRingPerPort);
        let p = handle.editor().borrow().project.clone();
        let f = NodeId::new("fixture");
        assert!(p.effective_patch().problems.is_empty());
        for (k, ring) in ["r1", "r2", "r3", "r4", "r5"].iter().enumerate() {
            let seg = TreeRef::parse(&f, &format!("circle1/{ring}")).unwrap();
            let lamps = p.segment(&seg).map(|s| s.lamps.len() as u32).unwrap();
            // Ring 3 runs reversed: its last lamp lands first on its port.
            let first = if *ring == "r3" { lamps - 1 } else { 0 };
            let landing = p.effective_patch().landing(&seg, first).expect("lit");
            assert_eq!((landing.port, landing.pixel), (k.to_string(), 0), "{ring}");
        }
    }
}
