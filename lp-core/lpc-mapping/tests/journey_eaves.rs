//! Journey: Sean's eaves (`lab/mapping/projects/eaves`, `eaves-wired`): two
//! long runs, `front` (60 lamps) and `side` (50), on port 0 (60 lamps) and
//! port 1 (takes the rest). The side doesn't fit what is left of port 0, so
//! it starts port 1; the house's real wiring (the front fed from its far
//! end) is one override. Each step is a statement; each is a model edit and
//! an assertion on where the lamps land.

use lpc_mapping::part::{
    Mapping, Part, PartId, PartKind, Path, PathCommand, Shape, TreeId, render,
};
use lpc_mapping::patching::{
    Amount, Direction, OutputPorts, Override, Patching, Start, patch_mapping,
};

#[test]
fn seans_eaves() {
    let mut m = eaves();
    let outputs = [OutputPorts::of_lamps("out", &[Some(60), None], 3)];

    // 1. The front fills port 0 (lamps 0–59); the side doesn't fit what is
    //    left, so it starts port 1 at 0, and is marked wrapped.
    let p = patch(&m, &outputs);
    assert_eq!(span(&p, "front"), Some((0, 0, 59)));
    assert_eq!(span(&p, "side"), Some((1, 0, 49)));
    assert!(p.segment(&id("side")).unwrap().wrapped);
    assert!(!p.segment(&id("front")).unwrap().wrapped);

    // 2. In the house the controller feeds the front from its far end:
    //    give the front a direction of its own, Reverse. It runs 59 down
    //    to 0, as the house is wired; the side is where it was.
    m.overrides.insert(
        id("front"),
        Override {
            direction: Some(Direction::Reverse),
            ..Override::default()
        },
    );
    let p = patch(&m, &outputs);
    assert_eq!(span(&p, "front"), Some((0, 59, 0)));
    assert_eq!(span(&p, "side"), Some((1, 0, 49)));
}

// ---- helpers --------------------------------------------------------------

/// The eaves' two runs, their real routes (each with its jumper), starting
/// at port 0, lamp 0.
fn eaves() -> Mapping {
    let run = |id: &str, points: [[f32; 2]; 4], count| {
        // Today's `gaps: [1]`: the segment from point 1 to point 2 is jumper.
        let commands = vec![
            PathCommand::Move(points[0]),
            PathCommand::Line(points[1]),
            PathCommand::Move(points[2]),
            PathCommand::Line(points[3]),
        ];
        let mut part = Part::new(
            PartId::new(id).unwrap(),
            PartKind::Shape(Shape::Path(Path::of(commands, count))),
        );
        part.name = id.to_string();
        part
    };
    let mut m = Mapping::new();
    m.parts.push(run(
        "front",
        [[10.0, 50.0], [10.0, 10.0], [12.0, 8.0], [100.0, 8.0]],
        60,
    ));
    m.parts.push(run(
        "side",
        [[110.0, 8.0], [188.0, 8.0], [190.0, 10.0], [190.0, 50.0]],
        50,
    ));
    m.start = Start::at("out", 0, Amount::lamps(0));
    m
}

fn patch(m: &Mapping, outputs: &[OutputPorts]) -> Patching {
    patch_mapping(m, &render(m).unwrap(), outputs)
}

fn id(text: &str) -> TreeId {
    TreeId::parse(text).unwrap()
}

/// A run's span as (port, the lamp its first lamp lands on, the lamp its
/// last lamp lands on), in RGB lamps.
fn span(p: &Patching, run: &str) -> Option<(u32, u32, u32)> {
    let id = id(run);
    let n = p.segment(&id)?.lamps;
    let first = p.landing(&id, 0)?;
    let last = p.landing(&id, n - 1)?;
    assert_eq!(first.port, last.port);
    Some((first.port.0, first.channel / 3, last.channel / 3))
}
