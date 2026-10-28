//! Statements: the Graph asks the patcher and shows its answer. One
//! sentence per test; the doc comment is the statement.
//!
//! Where lamps land is decided by `lpc_mapping::patching::patch_mapping`,
//! the patcher the engine runs, and its rules are tested in `lpc-mapping`
//! (`tests/patcher.rs`, `tests/journey_*.rs`). These statements don't
//! restate them: each builds a project and checks that the Graph's
//! effective patch is what the patcher gives for every fixture's mapping
//! against the fixture's port tables — every lamp's landing (a pixel is a
//! lamp of three channels), what every node did on the walk, and the
//! problems.
//!
//! Each test builds its own project: a fixture `f`, an output `out` with the
//! ports named (keys 0, 1, …), RGB lamps.

use lab_mapping_model::*;
use lpc_mapping::part::{Circle, PartKind, Path, Repeat, Shape, TreeId, render};
use lpc_mapping::patching::{
    Amount, ChannelOp, Direction, Flow, PatchTarget, PortKey, Problem, patch_mapping,
};

/// The Graph's landing for every lamp of the circular display equals
/// Patching::landing from lpc-mapping directly, as authored and as it is
/// really wired.
#[test]
fn the_circular_display_lands_where_the_patcher_says() {
    for p in [
        Project::circular_display(),
        Project::circular_display_wired(),
    ] {
        let lamps: usize = p.placed_segments().iter().map(|(_, s)| s.lamps.len()).sum();
        assert_eq!(the_patchers_answer_shown(&p), lamps, "every lamp lands");
    }
}

/// A fixture of paths lands every lamp where the patcher says.
#[test]
fn a_fixture_of_paths_lands_where_the_patcher_says() {
    let (mut p, f, out) = bare(&[("D0", 60)]);
    path(&mut p, &f, 5);
    path(&mut p, &f, 3);
    p.start_at(&f, &out, "0", 10);
    assert_eq!(the_patchers_answer_shown(&p), 8);
}

/// A circle with one ring moved to another port, one reversed and one
/// turned lands where the patcher says.
#[test]
fn a_circle_with_rings_overridden_lands_where_the_patcher_says() {
    let (mut p, f, out) = bare(&[("D0", 60), ("D1", 60)]);
    let circle = p
        .add(
            &f,
            PartKind::Shape(Shape::Circle(Circle::new(
                [50.0, 50.0],
                &[(40.0, 6), (30.0, 5), (20.0, 4), (10.0, 3)],
            ))),
        )
        .unwrap();
    p.start_at(&f, &out, "0", 0);
    let ring = |k: &str| TreeRef::parse(&f, &format!("{}/{k}", circle.part)).unwrap();
    p.patch_segment(&ring("r3"), &out, "1", 0).unwrap();
    p.edit_override(&ring("r2"), |o| o.direction = Some(Direction::Reverse))
        .unwrap();
    p.edit_override(&ring("r1"), |o| o.offset = Some(2))
        .unwrap();
    assert_eq!(the_patchers_answer_shown(&p), 18);
}

/// A one-per-port repeat with one copy moved elsewhere lands every copy
/// where the patcher says.
#[test]
fn a_repeat_with_a_copy_moved_lands_where_the_patcher_says() {
    let ports: Vec<(String, u32)> = (0..8).map(|k| (format!("D{k}"), 60)).collect();
    let ports: Vec<(&str, u32)> = ports.iter().map(|(n, c)| (n.as_str(), *c)).collect();
    let (mut p, f, out) = bare(&ports);
    let arm = p.new_part(&f, straight(10.0, 10)).unwrap();
    let arm_id = arm.id.clone();
    let repeat = p
        .add(&f, PartKind::Repeat(Repeat::new(arm, [50.0, 50.0], 5)))
        .unwrap();
    p.start_at(&f, &out, "0", 0);
    assert!(p.set_flow(
        &f,
        &PatchTarget::Part(repeat.part.clone()),
        Flow::OnePerPort { from: PortKey(0) },
    ));
    let copy3 = TreeRef::new(&f, TreeId::of(&arm_id).in_copy(3).canonical());
    p.patch_segment(&copy3, &out, "7", 0).unwrap();
    assert_eq!(the_patchers_answer_shown(&p), 50);
}

/// A fixture with no start lands nothing and the Graph says so, as the
/// patcher does, while another fixture on the same output lands from its
/// own start.
#[test]
fn a_fixture_with_no_start_shows_the_patchers_answer() {
    let (mut p, f, out) = bare(&[("D0", 60)]);
    path(&mut p, &f, 5);
    let g = p.add_fixture(None, "g").unwrap();
    path(&mut p, &g, 3);
    p.start_at(&g, &out, "0", 40);
    assert_eq!(the_patchers_answer_shown(&p), 3, "only g's lamps");
    assert!(problems_of(&p, &f).contains(&ProblemKind::NoStart));
}

/// Past the last port, the Graph lands nothing more and reports it, as the
/// patcher does.
#[test]
fn past_the_last_port_shows_the_patchers_answer() {
    let (mut p, f, out) = bare(&[("D0", 10)]);
    path(&mut p, &f, 8);
    path(&mut p, &f, 4);
    path(&mut p, &f, 4);
    p.start_at(&f, &out, "0", 0);
    assert_eq!(the_patchers_answer_shown(&p), 8);
    assert!(
        problems_of(&p, &f)
            .iter()
            .any(|k| matches!(k, ProblemKind::PastLastPort { .. }))
    );
}

/// A segment that wraps to the next port, one set where it doesn't fit,
/// one starting part-way into a pixel and an override for a part that's
/// gone show as the patcher answers them.
#[test]
fn the_patchers_problems_are_the_graphs() {
    let (mut p, f, out) = bare(&[("D0", 10), ("D1", 10)]);
    path(&mut p, &f, 6);
    let wraps = path(&mut p, &f, 6);
    let doesnt_fit = path(&mut p, &f, 4);
    let mid_pixel = path(&mut p, &f, 2);
    let gone = path(&mut p, &f, 2);
    p.start_at(&f, &out, "0", 0);
    p.patch_segment(&doesnt_fit, &out, "0", 7).unwrap();
    p.edit_override(&mid_pixel, |o| {
        o.channel = Some(ChannelOp::Set(Amount::channels(4)))
    })
    .unwrap();
    p.edit_override(&gone, |o| o.off = true).unwrap();
    p.remove_part(&gone.part()).unwrap();
    the_patchers_answer_shown(&p);
    assert!(p.effective_patch().walked(&wraps).unwrap().wrapped);
    let kinds = problems_of(&p, &f);
    for expected in [
        ProblemKind::DoesntFit {
            segment: doesnt_fit,
            port: "0".into(),
        },
        ProblemKind::MidPixel { segment: mid_pixel },
        ProblemKind::GoneNode(gone),
    ] {
        assert!(kinds.contains(&expected), "{expected:?} in {kinds:?}");
    }
}

// ---- helpers --------------------------------------------------------------

/// Check that the Graph shows the patcher's answer for every fixture of
/// `p`: every lamp's landing, what every node did on the walk, and the
/// problems. Returns how many lamps land, so a statement can say the check
/// had something to check.
fn the_patchers_answer_shown(p: &Project) -> usize {
    let graph = p.effective_patch();
    let mut landed = 0;
    for node in p.fixtures() {
        let f = &node.id;
        let mapping = &node.fixture().unwrap().mapping;
        let tree = render(mapping).expect("the mapping renders");
        let patching = patch_mapping(mapping, &tree, &p.port_tables(f));

        for segment in tree.segments() {
            let id = TreeRef::new(f, segment.id.clone());
            for lamp in 0..segment.count() {
                let expected = patching.landing(&segment.id, lamp).and_then(|at| {
                    let per = patching.segment(&segment.id)?.channels_per_lamp.max(1);
                    (at.channel % per == 0).then_some(())?;
                    Some(Landing {
                        output: p.output_named(at.output.as_str())?,
                        port: at.port.0.to_string(),
                        pixel: at.channel / per,
                    })
                });
                let shown = graph.landing(&id, lamp);
                assert_eq!(shown, expected, "lamp {lamp} of {id}");
                landed += usize::from(shown.is_some());
            }
        }

        for w in &patching.walked {
            let node = TreeRef::new(f, w.id.clone());
            let shown = graph.walked(&node).expect("the Graph shows every node");
            assert_eq!(shown.steps, w.steps, "{node}");
            assert_eq!(shown.sources, w.sources, "{node}");
            assert_eq!(shown.off, w.off, "{node}");
            assert_eq!(shown.direction, w.direction, "{node}");
            assert_eq!(shown.offset, w.offset, "{node}");
            assert_eq!(
                shown.at.output_name.as_deref(),
                w.at.output.as_ref().map(|o| o.as_str()),
                "{node}"
            );
            assert_eq!(shown.at.port, w.at.port.map(|k| k.0.to_string()), "{node}");
            assert_eq!(shown.at.channel, w.at.channel, "{node}");
            let wrapped = patching.segment(&w.id).is_some_and(|s| s.wrapped);
            assert_eq!(shown.wrapped, wrapped, "{node}");
        }

        let expected: Vec<ProblemKind> = patching
            .problems
            .iter()
            .filter_map(|problem| problem_kind(f, problem))
            .collect();
        assert_eq!(problems_of(p, f), expected, "{f}'s problems");
    }
    landed
}

/// The patcher's problem as the Graph names it; `None` for an overlap,
/// which the Graph reports as an overlap, not a problem.
fn problem_kind(f: &NodeId, problem: &Problem) -> Option<ProblemKind> {
    let at = |id: &TreeId| TreeRef::new(f, id.clone());
    Some(match problem {
        Problem::NoStart => ProblemKind::NoStart,
        Problem::GoneNode(id) => ProblemKind::GoneNode(at(id)),
        Problem::GoneOutput { output, set_by } => ProblemKind::GoneOutput {
            on: set_by.clone(),
            output: output.as_str().to_string(),
        },
        Problem::GonePort {
            output,
            port,
            set_by,
        } => ProblemKind::GonePort {
            on: set_by.clone(),
            output: output.as_str().to_string(),
            port: port.0.to_string(),
        },
        Problem::DoesntFit { segment, port } => ProblemKind::DoesntFit {
            segment: at(segment),
            port: port.0.to_string(),
        },
        Problem::PastLastPort { segment } => ProblemKind::PastLastPort {
            segment: at(segment),
        },
        Problem::OffLampBoundary { segment } => ProblemKind::MidPixel {
            segment: at(segment),
        },
        Problem::OrderMismatch { segment } => ProblemKind::OrderMismatch {
            segment: at(segment),
        },
        Problem::Overlap { .. } => return None,
    })
}

/// The problems the Graph reports for one fixture, in order.
fn problems_of(p: &Project, f: &NodeId) -> Vec<ProblemKind> {
    p.effective_patch()
        .problems
        .iter()
        .filter(|q| &q.fixture == f)
        .map(|q| q.kind.clone())
        .collect()
}

fn bare(ports: &[(&str, u32)]) -> (Project, NodeId, NodeId) {
    let mut p = Project::new("t");
    let f = p.add_fixture(None, "f").unwrap();
    let out = p.add_output(None, "out", ports).unwrap();
    (p, f, out)
}

/// A straight path of `n` lamps added to `f`: its segment.
fn path(p: &mut Project, f: &NodeId, n: u32) -> TreeRef {
    let y = 10.0 + 5.0 * p.fixture(f).unwrap().parts().len() as f32;
    p.add(f, straight(y, n)).unwrap().node()
}

/// A straight path of `n` lamps across the fixture at height `y`.
fn straight(y: f32, n: u32) -> PartKind {
    PartKind::Shape(Shape::Path(Path::through(&[[10.0, y], [80.0, y]], n)))
}
