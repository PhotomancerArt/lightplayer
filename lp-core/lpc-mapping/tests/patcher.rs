//! Statements: the patcher (names.html "Patcher"; the spike's walker, D5).
//! One sentence per test; the doc comment is the statement. Ported from the
//! spike's `walker.rs` in the settled words, keeping its numbers, plus new
//! ones.
//!
//! Each test builds its own fixture: one output `out` whose ports are keyed
//! 0, 1, … and hold 60 lamps (180 channels) unless the test says, and RGB
//! lamps (3 channels) unless it says.

use lpc_mapping::part::{Circle, Colors, Mapping, PartKind, Path, Repeat, Shape, TreeId, render};
use lpc_mapping::patching::{
    Address, Amount, ChannelOp, Direction, Flow, OutputName, OutputPorts, Override, PatchTarget,
    Patching, PortKey, PortOp, Problem, Span, Start, StepSource, patch_mapping,
};

/// A fixture with no start patches nothing, and says so.
#[test]
fn a_fixture_with_no_start_patches_nothing() {
    let mut m = Mapping::new();
    path(&mut m, 5);
    let p = patch(&m, &ports(&[Some(60)]));
    assert!(p.segments.is_empty());
    assert_eq!(p.problems, [Problem::NoStart]);
}

/// A path at port 0 from lamp 0 lands on lamps 0 to n−1.
#[test]
fn a_path_lands_from_its_start() {
    let mut m = Mapping::new();
    let a = path(&mut m, 5);
    m.start = Start::at("out", 0, Amount::lamps(0));
    let p = patch(&m, &ports(&[Some(60)]));
    assert_eq!(lamps(&p, &a), [0, 1, 2, 3, 4]);
}

/// The cursor continues: the next part starts where the last one ended.
#[test]
fn the_cursor_continues() {
    let mut m = Mapping::new();
    path(&mut m, 5);
    let b = path(&mut m, 3);
    m.start = Start::at("out", 0, Amount::lamps(10));
    let p = patch(&m, &ports(&[Some(60)]));
    assert_eq!(lamps(&p, &b), [15, 16, 17]);
}

/// Leaving a part doesn't move the cursor: what follows a circle starts
/// where its last ring ended.
#[test]
fn leaving_a_part_doesnt_move_the_cursor() {
    let mut m = Mapping::new();
    circle(&mut m, &[(30.0, 8), (15.0, 4)]);
    let after = path(&mut m, 2);
    m.start = Start::at("out", 0, Amount::lamps(0));
    let p = patch(&m, &ports(&[Some(60)]));
    assert_eq!(lamps(&p, &after), [12, 13]);
}

/// A ring set to port 1 at channel 0 takes the rings after it along.
#[test]
fn a_ring_set_elsewhere_takes_the_rest_along() {
    let mut m = Mapping::new();
    circle(&mut m, &[(40.0, 6), (30.0, 5), (20.0, 4), (10.0, 3)]);
    m.start = Start::at("out", 0, Amount::lamps(0));
    m.overrides.insert(id("circle1/r3"), at_port(1, 0));
    let p = patch(&m, &ports(&[Some(60), Some(60)]));
    assert_eq!(landing(&p, "circle1/r2", 0), Some((0, 6)));
    assert_eq!(landing(&p, "circle1/r3", 0), Some((1, 0)));
    assert_eq!(landing(&p, "circle1/r4", 0), Some((1, 4)), "follows r3");
}

/// One per port from port 1: ring k starts on the k-th port from port 1, at
/// its first lamp.
#[test]
fn one_per_port_puts_each_child_on_its_own_port() {
    let mut m = Mapping::new();
    circle(&mut m, &[(40.0, 6), (30.0, 5), (20.0, 4)]);
    m.parts[0].flow = Flow::OnePerPort { from: PortKey(1) };
    m.start = Start::at("out", 0, Amount::lamps(0));
    let p = patch(&m, &ports(&[Some(60); 4]));
    for (ring, port) in [("circle1/r1", 1), ("circle1/r2", 2), ("circle1/r3", 3)] {
        assert_eq!(landing(&p, ring, 0), Some((port, 0)), "{ring}");
    }
}

/// Moving copy 3 of a one-per-port repeat elsewhere leaves copy 4 in its
/// slot: the slots are absolute, as jacks are physical.
#[test]
fn moving_one_copy_leaves_the_others_in_their_slots() {
    let mut m = Mapping::new();
    let arm = m.new_part(straight(10));
    let mut repeat = m.new_part(PartKind::Repeat(Repeat::new(arm, [50.0, 50.0], 5)));
    repeat.flow = Flow::OnePerPort { from: PortKey(0) };
    m.parts.push(repeat);
    m.start = Start::at("out", 0, Amount::lamps(0));
    m.overrides.insert(id("path1@3"), at_port(7, 0));
    let p = patch(&m, &ports(&[Some(60); 8]));
    assert_eq!(landing(&p, "path1@3", 0), Some((7, 0)));
    assert_eq!(landing(&p, "path1@4", 0), Some((4, 0)), "keeps its slot");
}

/// A segment that doesn't fit what is left of its port starts on the next
/// port, at channel 0, and says it wrapped.
#[test]
fn a_segment_that_doesnt_fit_starts_on_the_next_port() {
    let mut m = Mapping::new();
    path(&mut m, 6);
    let b = path(&mut m, 6);
    m.start = Start::at("out", 0, Amount::lamps(0));
    let p = patch(&m, &ports(&[Some(10), Some(10)]));
    assert_eq!(landing(&p, &b.to_string(), 0), Some((1, 0)));
    assert!(p.segment(&b).unwrap().wrapped);
}

/// A start that was set and doesn't fit is reported, not moved: the segment
/// lands as far as its port reaches.
#[test]
fn a_set_start_that_doesnt_fit_is_reported_not_moved() {
    let mut m = Mapping::new();
    let a = path(&mut m, 6);
    m.start = Start::at("out", 0, Amount::lamps(0));
    m.overrides.insert(a.clone(), at_port(0, 7));
    let p = patch(&m, &ports(&[Some(10), Some(10)]));
    assert_eq!(landing(&p, "path1", 0), Some((0, 7)));
    assert_eq!(landing(&p, "path1", 3), None, "past the port's end");
    assert_eq!(
        p.problems,
        [Problem::DoesntFit {
            segment: a,
            port: PortKey(0)
        }]
    );
}

/// Past the last port is an error: it never moves to another output, and
/// what follows on lands nowhere until something sets a new start.
#[test]
fn past_the_last_port_is_an_error() {
    let mut m = Mapping::new();
    path(&mut m, 8);
    let b = path(&mut m, 4);
    let c = path(&mut m, 4);
    m.start = Start::at("out", 0, Amount::lamps(0));
    let outputs = [
        OutputPorts::of_lamps("out", &[Some(10)], 3),
        OutputPorts::of_lamps("other", &[None], 3),
    ];
    let p = patch(&m, &outputs);
    assert!(p.segment(&b).is_none());
    assert!(p.segment(&c).is_none(), "follows on: nowhere");
    assert_eq!(p.problems, [Problem::PastLastPort { segment: b }]);
    m.overrides.insert(c.clone(), at_port(0, 0));
    assert!(
        patch(&m, &outputs).segment(&c).is_some(),
        "a new start lands again"
    );
}

/// Off: a segment makes no span, and the next one starts where it would
/// have.
#[test]
fn off_makes_no_span_and_doesnt_move_the_cursor() {
    let mut m = Mapping::new();
    let a = path(&mut m, 5);
    let b = path(&mut m, 3);
    m.start = Start::at("out", 0, Amount::lamps(0));
    m.overrides.insert(
        a.clone(),
        Override {
            off: true,
            ..Override::default()
        },
    );
    let p = patch(&m, &ports(&[Some(60)]));
    assert!(p.segment(&a).is_none());
    assert_eq!(lamps(&p, &b), [0, 1, 2]);
}

/// Next port (a part's start, port Add 1, channel Set 0) is the next key,
/// whatever the gaps between keys.
#[test]
fn next_port_is_the_next_key() {
    let mut m = Mapping::new();
    path(&mut m, 5);
    let b = path(&mut m, 3);
    m.parts[1].start = Start::next_port();
    m.start = Start::at("out", 0, Amount::lamps(0));
    let mut outputs = ports(&[Some(60); 3]);
    outputs[0].ports.remove(1);
    let p = patch(&m, &outputs);
    assert_eq!(landing(&p, &b.to_string(), 0), Some((2, 0)));
}

/// Values keep the unit they were written in: 6 channels is lamp 2; 4
/// channels is part-way into a lamp, and is reported.
#[test]
fn values_keep_the_unit_they_were_written_in() {
    let mut m = Mapping::new();
    let a = path(&mut m, 3);
    m.start = Start::at("out", 0, Amount::lamps(0));
    let channel = |n| Override {
        channel: Some(ChannelOp::Set(Amount::channels(n))),
        ..Override::default()
    };
    m.overrides.insert(a.clone(), channel(6));
    assert_eq!(lamps(&patch(&m, &ports(&[Some(60)])), &a), [2, 3, 4]);
    m.overrides.insert(a.clone(), channel(4));
    let p = patch(&m, &ports(&[Some(60)]));
    assert_eq!(
        p.segment(&a).unwrap().address.channel,
        4,
        "it has its address"
    );
    assert_eq!(p.problems, [Problem::OffLampBoundary { segment: a }]);
}

/// Reverse and offset land as today's arithmetic does: orientation first,
/// then rotation (5 lamps at 10, reversed, offset 2 → 11, 10, 14, 13, 12).
#[test]
fn reverse_and_offset_land_as_today() {
    let mut m = Mapping::new();
    let a = path(&mut m, 5);
    m.start = Start::at("out", 0, Amount::lamps(10));
    m.overrides.insert(
        a.clone(),
        Override {
            direction: Some(Direction::Reverse),
            offset: Some(2),
            ..Override::default()
        },
    );
    let p = patch(&m, &ports(&[Some(60)]));
    assert_eq!(lamps(&p, &a), [11, 10, 14, 13, 12]);
}

/// An override for a node that's gone is kept and reported, never moved to
/// another node.
#[test]
fn an_override_for_a_gone_node_is_kept_and_reported() {
    let mut m = Mapping::new();
    let a = path(&mut m, 5);
    let b = path(&mut m, 5);
    m.start = Start::at("out", 0, Amount::lamps(0));
    m.overrides.insert(
        a.clone(),
        Override {
            off: true,
            ..Override::default()
        },
    );
    m.parts.remove(0);
    let p = patch(&m, &ports(&[Some(60)]));
    assert!(m.overrides.contains_key(&a), "kept");
    assert_eq!(p.problems, [Problem::GoneNode(a)]);
    assert_eq!(lamps(&p, &b), [0, 1, 2, 3, 4], "b is not off");
}

/// Nothing flows across fixtures: two fixtures on one output each land
/// where their own start says.
#[test]
fn nothing_flows_across_fixtures() {
    let mut f = Mapping::new();
    let a = path(&mut f, 5);
    f.start = Start::at("out", 0, Amount::lamps(0));
    let mut g = Mapping::new();
    let b = path(&mut g, 3);
    let outputs = ports(&[Some(60)]);
    assert_eq!(lamps(&patch(&f, &outputs), &a), [0, 1, 2, 3, 4]);
    let p = patch(&g, &outputs);
    assert!(p.segment(&b).is_none(), "g starts nowhere");
    assert_eq!(p.problems, [Problem::NoStart]);
    g.start = Start::at("out", 0, Amount::lamps(40));
    assert_eq!(lamps(&patch(&g, &outputs), &b), [40, 41, 42]);
}

/// A part's start applies to every copy of it; an override applies to one
/// node.
#[test]
fn a_parts_start_applies_to_every_copy_and_an_override_to_one_node() {
    let mut m = Mapping::new();
    let mut arm = m.new_part(straight(5));
    arm.start = Start::next_port();
    let repeat = m.new_part(PartKind::Repeat(Repeat::new(arm, [50.0, 50.0], 3)));
    m.parts.push(repeat);
    m.start = Start::at("out", 0, Amount::lamps(0));
    m.overrides.insert(
        id("path1@1"),
        Override {
            channel: Some(ChannelOp::Set(Amount::lamps(10))),
            ..Override::default()
        },
    );
    let p = patch(&m, &ports(&[Some(60); 4]));
    assert_eq!(landing(&p, "path1", 0), Some((1, 0)));
    assert_eq!(
        landing(&p, "path1@1", 0),
        Some((2, 10)),
        "its port still from the part's start"
    );
    assert_eq!(landing(&p, "path1@2", 0), Some((3, 0)));
    let copy2 = p.walked(&id("path1@2")).unwrap();
    assert_eq!(
        copy2.sources[1],
        StepSource::Start(PatchTarget::Part(id("path1").part))
    );
    assert_eq!(
        p.walked(&id("path1@1")).unwrap().sources[2],
        StepSource::Override
    );
}

/// A segment's direction comes from its own override, else the nearest part
/// above it that has one.
#[test]
fn a_segments_direction_comes_from_the_nearest_that_says() {
    let mut m = Mapping::new();
    let a = m.new_part(straight(3));
    let b = m.new_part(straight(3));
    let group = m.new_part(PartKind::Group { parts: vec![a, b] });
    m.parts.push(group);
    let c = path(&mut m, 3);
    m.start = Start::at("out", 0, Amount::lamps(0));
    let direction = |d| Override {
        direction: Some(d),
        ..Override::default()
    };
    m.overrides
        .insert(id("group1"), direction(Direction::Reverse));
    m.overrides
        .insert(id("path2"), direction(Direction::Forward));
    let p = patch(&m, &ports(&[Some(60)]));
    assert_eq!(lamps(&p, &id("path1")), [2, 1, 0], "from the group");
    assert_eq!(lamps(&p, &id("path2")), [3, 4, 5], "its own");
    assert_eq!(lamps(&p, &c), [6, 7, 8], "outside the group");
}

/// A port is a row of spans: segments and the gaps between them, and a
/// port that takes the rest has no trailing gap.
#[test]
fn a_port_is_a_row_of_spans() {
    let mut m = Mapping::new();
    let a = path(&mut m, 5);
    let b = path(&mut m, 3);
    m.start = Start::at("out", 0, Amount::lamps(2));
    m.overrides.insert(b.clone(), at_port(0, 10));
    let p = patch(&m, &ports(&[Some(20), None]));
    assert_eq!(
        p.ports[0].spans,
        [
            Span::Gap { at: 0, channels: 6 },
            Span::Segment {
                id: a,
                at: 6,
                channels: 15
            },
            Span::Gap {
                at: 21,
                channels: 9
            },
            Span::Segment {
                id: b,
                at: 30,
                channels: 9
            },
            Span::Gap {
                at: 39,
                channels: 21
            },
        ]
    );
    assert!(
        p.ports[1].spans.is_empty(),
        "takes the rest: no gap to draw"
    );
}

/// Two segments that share channels on a port are reported as an overlap.
#[test]
fn two_segments_sharing_channels_are_an_overlap() {
    let mut m = Mapping::new();
    let a = path(&mut m, 5);
    let b = path(&mut m, 3);
    m.start = Start::at("out", 0, Amount::lamps(0));
    m.overrides.insert(b.clone(), at_port(0, 3));
    let p = patch(&m, &ports(&[Some(60)]));
    assert_eq!(
        p.problems,
        [Problem::Overlap {
            a,
            b,
            output: OutputName::new("out"),
            port: PortKey(0)
        }]
    );
}

/// An RGBW segment advances the cursor four channels a lamp, and "lamp 2"
/// on it is channel 8.
#[test]
fn an_rgbw_segment_counts_four_channels_a_lamp() {
    let mut m = Mapping::new();
    let mut rgbw = m.new_part(straight(3));
    rgbw.lamp.colors = Some(Colors::Rgbw);
    let w = rgbw.id.clone();
    m.parts.push(rgbw);
    let after = path(&mut m, 1);
    m.start = Start::at("out", 0, Amount::lamps(0));
    let p = patch(&m, &ports(&[Some(60)]));
    assert_eq!(address(&p, &after).channel, 12);
    m.overrides.insert(
        TreeId::of(&w),
        Override {
            channel: Some(ChannelOp::Set(Amount::lamps(2))),
            ..Override::default()
        },
    );
    let p = patch(&m, &ports(&[Some(60)]));
    assert_eq!(address(&p, &TreeId::of(&w)).channel, 8);
    assert_eq!(address(&p, &after).channel, 20);
}

/// A start naming an output or a port that isn't there is reported against
/// what set it, and lands nowhere.
#[test]
fn a_gone_output_or_port_is_reported_against_what_set_it() {
    let mut m = Mapping::new();
    let a = path(&mut m, 2);
    m.start = Start::at("gone", 0, Amount::lamps(0));
    let p = patch(&m, &ports(&[Some(60)]));
    assert!(p.segment(&a).is_none());
    assert_eq!(
        p.problems,
        [Problem::GoneOutput {
            output: OutputName::new("gone"),
            set_by: PatchTarget::Fixture
        }]
    );
    m.start = Start::at("out", 0, Amount::lamps(0));
    m.overrides.insert(
        a.clone(),
        Override {
            port: Some(PortOp::set(9)),
            ..Override::default()
        },
    );
    let p = patch(&m, &ports(&[Some(60)]));
    assert_eq!(
        p.problems,
        [Problem::GonePort {
            output: OutputName::new("out"),
            port: PortKey(9),
            set_by: PatchTarget::Node(a)
        }]
    );
}

/// An order that doesn't fit its colors is reported, naming the segment.
#[test]
fn an_order_that_doesnt_fit_is_reported() {
    let mut m = Mapping::new();
    let a = path(&mut m, 2);
    m.parts[0].lamp.colors = Some(Colors::Rgbw);
    m.lamp.order = Some(lpc_mapping::part::Order::parse("GRB").unwrap());
    m.start = Start::at("out", 0, Amount::lamps(0));
    let p = patch(&m, &ports(&[Some(60)]));
    assert_eq!(p.problems, [Problem::OrderMismatch { segment: a }]);
}

// ---- helpers --------------------------------------------------------------

fn patch(m: &Mapping, outputs: &[OutputPorts]) -> Patching {
    let tree = render(m).unwrap();
    patch_mapping(m, &tree, outputs)
}

/// Output `out`, ports keyed 0, 1, … of these sizes in RGB lamps.
fn ports(lamps: &[Option<u32>]) -> Vec<OutputPorts> {
    vec![OutputPorts::of_lamps("out", lamps, 3)]
}

fn straight(count: u32) -> PartKind {
    PartKind::Shape(Shape::Path(Path::through(
        &[[10.0, 10.0], [80.0, 10.0]],
        count,
    )))
}

/// A straight path of `n` lamps, added at the top: its segment's id.
fn path(m: &mut Mapping, n: u32) -> TreeId {
    let part = m.new_part(straight(n));
    let id = TreeId::of(&part.id);
    m.parts.push(part);
    id
}

fn circle(m: &mut Mapping, rings: &[(f32, u32)]) {
    let part = m.new_part(PartKind::Shape(Shape::Circle(Circle::new(
        [50.0, 50.0],
        rings,
    ))));
    m.parts.push(part);
}

fn id(text: &str) -> TreeId {
    TreeId::parse(text).unwrap()
}

/// Port `port`, from lamp `lamp`: an override's port and channel.
fn at_port(port: u32, lamp: u32) -> Override {
    Override {
        port: Some(PortOp::set(port)),
        channel: Some(ChannelOp::Set(Amount::lamps(lamp))),
        ..Override::default()
    }
}

fn address(p: &Patching, id: &TreeId) -> Address {
    p.segment(id).expect("landed").address.clone()
}

/// Where lamp `lamp` of a node lands, as (port, RGB lamp on that port).
fn landing(p: &Patching, node: &str, lamp: u32) -> Option<(u32, u32)> {
    p.landing(&id(node), lamp)
        .map(|a| (a.port.0, a.channel / 3))
}

/// Every lamp of a segment, as RGB lamps on its port.
fn lamps(p: &Patching, id: &TreeId) -> Vec<u32> {
    let n = p.segment(id).expect("landed").lamps;
    (0..n)
        .map(|j| p.landing(id, j).expect("on the port").channel / 3)
        .collect()
}
