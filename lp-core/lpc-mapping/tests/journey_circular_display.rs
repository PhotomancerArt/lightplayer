//! Journey: the circular display (Yona's lead case; the spike's
//! `circular_display.rs`). One circle of four rings and a one-lamp centre on
//! output `main`, ports 0 to 4 of 64 lamps. As soldered, ring 3 runs the
//! other way and ring 3, ring 4 and the centre are on port 1. Each step is
//! a statement; each is a model edit and an assertion on where the lamps
//! land.

use lpc_mapping::part::{Circle, Mapping, Part, PartId, PartKind, Shape, TreeId, render};
use lpc_mapping::patching::{
    Amount, ChannelOp, Direction, Flow, OutputPorts, Override, PatchTarget, Patching, PortKey,
    PortOp, Start, StepSource, patch_mapping,
};

#[test]
fn the_circular_display() {
    let mut m = display();
    let outputs = [OutputPorts::of_lamps("main", &[Some(64); 5], 3)];

    // 1. From port 0 lamp 0 the rings run end to end, outside in: r1 on
    //    lamps 0–23, r2 24–39, r3 40–51, r4 52–59, the centre r5 on 60.
    let p = patch(&m, &outputs);
    assert_eq!(span(&p, "r1"), Some((0, 0, 23)));
    assert_eq!(span(&p, "r2"), Some((0, 24, 39)));
    assert_eq!(span(&p, "r3"), Some((0, 40, 51)));
    assert_eq!(span(&p, "r4"), Some((0, 52, 59)));
    assert_eq!(span(&p, "r5"), Some((0, 60, 60)));

    // 2. The centre is one lamp, at the circle's centre.
    let tree = render(&m).unwrap();
    let centre = tree.segment(&ring("r5")).unwrap();
    assert_eq!(tree.lamps_of(centre), [[50.0, 50.0]]);

    // 3. Ring 3 was soldered counter-clockwise: give it a direction of its
    //    own, Reverse. It runs 51 down to 40, in its slot; r4 is unchanged.
    m.overrides.insert(
        ring("r3"),
        Override {
            direction: Some(Direction::Reverse),
            ..Override::default()
        },
    );
    let p = patch(&m, &outputs);
    assert_eq!(span(&p, "r3"), Some((0, 51, 40)));
    assert_eq!(span(&p, "r4"), Some((0, 52, 59)));

    // 4. Ring 3 is on port 1, from its first lamp: give it that port and
    //    channel 0 (choosing a port starts the segment at its first lamp).
    //    r2 stays on port 0; r3 runs 11 down to 0 on port 1, r4 and the
    //    centre follow it there. This is the display as soldered.
    let r3 = m.overrides.get_mut(&ring("r3")).unwrap();
    r3.port = Some(PortOp::set(1));
    r3.channel = Some(ChannelOp::Set(Amount::lamps(0)));
    let p = patch(&m, &outputs);
    assert_eq!(span(&p, "r2"), Some((0, 24, 39)));
    assert_eq!(span(&p, "r3"), Some((1, 11, 0)));
    assert_eq!(span(&p, "r4"), Some((1, 12, 19)));
    assert_eq!(span(&p, "r5"), Some((1, 20, 20)));
    assert!(p.problems.is_empty(), "{:?}", p.problems);

    // 5. Or one ring per port: clear r3's port and channel (its direction
    //    stays) and set the circle's flow to one per port from port 0. Ring
    //    k starts port k−1 at its first channel, and r4's port step comes
    //    from the circle's flow.
    let r3 = m.overrides.get_mut(&ring("r3")).unwrap();
    r3.port = None;
    r3.channel = None;
    m.parts[0].flow = Flow::OnePerPort { from: PortKey(0) };
    let p = patch(&m, &outputs);
    for (k, key) in ["r1", "r2", "r3", "r4", "r5"].iter().enumerate() {
        let landed = p.segment(&ring(key)).unwrap();
        assert_eq!(landed.address.port, PortKey(k as u32), "{key}");
        assert_eq!(landed.address.channel, 0, "{key}");
    }
    assert_eq!(span(&p, "r3"), Some((2, 11, 0)), "still reversed");
    assert_eq!(
        p.walked(&ring("r4")).unwrap().sources[1],
        StepSource::Flow(PatchTarget::Part(PartId::new("circle1").unwrap()))
    );
}

// ---- helpers --------------------------------------------------------------

/// The display: rings of 24, 16, 12, 8 and a one-lamp centre (a ring of
/// radius 0), outside in, starting at port 0, lamp 0.
fn display() -> Mapping {
    let circle = Circle::new(
        [50.0, 50.0],
        &[(42.0, 24), (32.0, 16), (22.0, 12), (12.0, 8), (0.0, 1)],
    );
    let mut m = Mapping::new();
    let mut part = Part::new(
        PartId::new("circle1").unwrap(),
        PartKind::Shape(Shape::Circle(circle)),
    );
    part.name = "display".to_string();
    m.parts.push(part);
    m.start = Start::at("main", 0, Amount::lamps(0));
    m
}

fn patch(m: &Mapping, outputs: &[OutputPorts]) -> Patching {
    patch_mapping(m, &render(m).unwrap(), outputs)
}

fn ring(key: &str) -> TreeId {
    TreeId::parse(&format!("circle1/{key}")).unwrap()
}

/// A ring's span as (port, the lamp its first lamp lands on, the lamp its
/// last lamp lands on), in RGB lamps.
fn span(p: &Patching, key: &str) -> Option<(u32, u32, u32)> {
    let id = ring(key);
    let n = p.segment(&id)?.lamps;
    let first = p.landing(&id, 0)?;
    let last = p.landing(&id, n - 1)?;
    assert_eq!(first.port, last.port);
    Some((first.port.0, first.channel / 3, last.channel / 3))
}
