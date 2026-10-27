//! The four journeys' mappings (`journey_*.rs`), each at its last step: its
//! parts, its start and the overrides the journey set, with the outputs it
//! patches onto. What the edit statements check "every lamp lands where it
//! did" against. Built here, not shared with the journeys: those build their
//! mapping step by step, and stay as they are.

use lpc_mapping::part::{
    Circle, Mapping, Part, PartId, PartKind, Path, PathCommand, Repeat, Shape, TreeId,
};
use lpc_mapping::patching::{Amount, ChannelOp, Direction, OutputPorts, Override, PortOp, Start};

/// One journey's mapping and the outputs it patches onto.
pub struct Journey {
    pub name: &'static str,
    pub mapping: Mapping,
    pub outputs: Vec<OutputPorts>,
}

/// All four, at their last step.
pub fn journeys() -> Vec<Journey> {
    vec![choker(), circular_display(), dome_slice(), eaves()]
}

/// The PLAYFUL choker: twelve strokes end to end on one port, F's lower
/// stroke restarted at lamp 60.
pub fn choker() -> Journey {
    let strokes = [
        ("P", 13),
        ("L1", 8),
        ("A", 10),
        ("A-bar", 2),
        ("Y-stem", 3),
        ("Y-left", 3),
        ("Y-right", 3),
        ("F-lower", 3),
        ("F-bar", 2),
        ("F-upper", 5),
        ("U", 13),
        ("L2", 8),
    ];
    let mut m = Mapping::new();
    for (i, (name, count)) in strokes.into_iter().enumerate() {
        let x = 10.0 * i as f32;
        let kind = PartKind::Shape(Shape::Path(Path::through(&[[x, 0.0], [x, 20.0]], count)));
        let mut part = Part::new(PartId::new(&name.to_lowercase()).unwrap(), kind);
        part.name = name.to_string();
        m.parts.push(part);
    }
    m.start = Start::at("out", 0, Amount::lamps(0));
    m.overrides.insert(
        id("f-lower"),
        Override {
            channel: Some(ChannelOp::Set(Amount::lamps(60))),
            ..Override::default()
        },
    );
    Journey {
        name: "choker",
        mapping: m,
        outputs: vec![OutputPorts::of_lamps("out", &[None], 3)],
    }
}

/// The circular display as soldered: four rings and a centre, ring 3
/// reversed and on port 1 from its first lamp, the rest following it.
pub fn circular_display() -> Journey {
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
    m.next.raise("circle", 2);
    m.start = Start::at("main", 0, Amount::lamps(0));
    m.overrides.insert(
        id("circle1/r3"),
        Override {
            port: Some(PortOp::set(1)),
            channel: Some(ChannelOp::Set(Amount::lamps(0))),
            direction: Some(Direction::Reverse),
            ..Override::default()
        },
    );
    Journey {
        name: "circular display",
        mapping: m,
        outputs: vec![OutputPorts::of_lamps("main", &[Some(64); 5], 3)],
    }
}

/// A dome slice: five copies of one 300-lamp sector path, copy 3 moved to
/// port 4 (copy 4 follows it there).
pub fn dome_slice() -> Journey {
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
    m.next.raise("path", 2);
    m.start = Start::at("out", 0, Amount::lamps(0));
    m.overrides.insert(
        id("path1@3"),
        Override {
            port: Some(PortOp::set(4)),
            channel: Some(ChannelOp::Set(Amount::lamps(0))),
            ..Override::default()
        },
    );
    Journey {
        name: "dome slice",
        mapping: m,
        outputs: vec![OutputPorts::of_lamps(
            "out",
            &[Some(300), Some(300), Some(300), Some(300), None],
            3,
        )],
    }
}

/// Sean's eaves: the front (fed from its far end) fills port 0, the side
/// wraps to port 1.
pub fn eaves() -> Journey {
    let run = |id: &str, points: [[f32; 2]; 4], count| {
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
    m.overrides.insert(
        id("front"),
        Override {
            direction: Some(Direction::Reverse),
            ..Override::default()
        },
    );
    Journey {
        name: "eaves",
        mapping: m,
        outputs: vec![OutputPorts::of_lamps("out", &[Some(60), None], 3)],
    }
}

fn id(text: &str) -> TreeId {
    TreeId::parse(text).unwrap()
}
