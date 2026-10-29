//! Journey: a dome slice (`catalog/projects/zook-dome`): a repeat `sector`
//! of five copies of one 300-lamp path (the dome's own sector, jumper and
//! all) on output `out`, ports 0–3 of 300 lamps and port 4 taking the rest.
//! End to end, moving one copy elsewhere carries the next along; one per
//! port, each copy keeps its slot (jacks are physical). Each step is a
//! statement; each is a model edit and an assertion on where the lamps
//! land.

use lpc_mapping::part::{
    Mapping, Part, PartId, PartKind, Path, PathCommand, Repeat, Shape, TreeId, render,
};
use lpc_mapping::patching::{
    Amount, ChannelOp, Flow, OutputPorts, Override, Patching, PortKey, PortOp, Problem, Start,
    patch_mapping,
};

#[test]
fn a_dome_slice() {
    let mut m = dome();
    let outputs = [OutputPorts::of_lamps(
        "out",
        &[Some(300), Some(300), Some(300), Some(300), None],
        3,
    )];

    // 1. From port 0 lamp 0, copy k is on port k at its first lamp: each
    //    copy fills its port, and the next one wraps to the next port.
    let p = patch(&m, &outputs);
    for k in 0..5 {
        assert_eq!(landing(&p, k), Some((k, 0)), "copy {k}");
    }
    assert!(p.problems.is_empty(), "{:?}", p.problems);

    // 2. Give copy 3 port 4, from channel 0: it starts port 4, and copy 4
    //    follows it there, at lamp 300 (port 4 takes the rest).
    m.overrides.insert(
        copy(3),
        Override {
            port: Some(PortOp::set(4)),
            channel: Some(ChannelOp::Set(Amount::lamps(0))),
            ..Override::default()
        },
    );
    let p = patch(&m, &outputs);
    assert_eq!(landing(&p, 3), Some((4, 0)));
    assert_eq!(landing(&p, 4), Some((4, 300)), "follows copy 3");

    // 3. Set the repeat's flow to one per port from port 0: copy 4 is back
    //    on port 4 at 0 and copy 2 on port 2; copy 3 keeps its override, so
    //    copies 3 and 4 share port 4, and the overlap is reported.
    m.parts[0].flow = Flow::OnePerPort { from: PortKey(0) };
    let p = patch(&m, &outputs);
    assert_eq!(landing(&p, 4), Some((4, 0)), "keeps its slot");
    assert_eq!(landing(&p, 2), Some((2, 0)));
    assert!(p.problems.contains(&Problem::Overlap {
        a: copy(3),
        b: copy(4),
        output: lpc_mapping::patching::OutputName::new("out"),
        port: PortKey(4),
    }));

    // 4. Clear copy 3's override: it is on port 3 at 0 again, and no
    //    overlap is reported.
    m.overrides.remove(&copy(3));
    let p = patch(&m, &outputs);
    assert_eq!(landing(&p, 3), Some((3, 0)));
    assert!(p.problems.is_empty(), "{:?}", p.problems);
}

// ---- helpers --------------------------------------------------------------

/// The zook dome's sector: its real path (a jumper after the seventh point),
/// 300 lamps, repeated five times about its hub, starting at port 0, lamp 0.
fn dome() -> Mapping {
    let points = [
        [580.7, 571.2],
        [442.5, 767.4],
        [445.1, 995.2],
        [723.6, 991.3],
        [942.0, 831.2],
        [1019.0, 911.2],
        [800.0, 1081.6],
        [1019.0, 911.2],
        [1118.3, 745.9],
        [1133.9, 520.4],
        [1024.4, 565.2],
        [807.8, 493.8],
        [724.5, 763.3],
    ];
    let commands = points
        .iter()
        .enumerate()
        .map(|(i, p)| {
            // Today's `gaps: [6]`: the segment from point 6 to point 7 is
            // jumper, so point 7 is a move.
            if i == 0 || i == 7 {
                PathCommand::Move(*p)
            } else {
                PathCommand::Line(*p)
            }
        })
        .collect();
    let arm = Part::new(
        PartId::new("path1").unwrap(),
        PartKind::Shape(Shape::Path(Path::of(commands, 300))),
    );
    let mut m = Mapping::new();
    m.parts.push(Part::new(
        PartId::new("sector").unwrap(),
        PartKind::Repeat(Repeat::new(arm, [581.5, 573.9], 5)),
    ));
    m.start = Start::at("out", 0, Amount::lamps(0));
    m
}

fn patch(m: &Mapping, outputs: &[OutputPorts]) -> Patching {
    patch_mapping(m, &render(m).unwrap(), outputs)
}

/// Copy `k` of the sector's path: `path1`, `path1@k`.
fn copy(k: u32) -> TreeId {
    TreeId::parse("path1").unwrap().in_copy(k).canonical()
}

/// Where copy `k`'s first lamp lands, as (port, RGB lamp on that port).
fn landing(p: &Patching, k: u32) -> Option<(u32, u32)> {
    p.landing(&copy(k), 0).map(|a| (a.port.0, a.channel / 3))
}
