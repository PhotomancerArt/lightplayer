//! Journey: the PLAYFUL choker (`catalog/projects/playful-choker`): twelve
//! letter paths end to end on its one port (`ws281x:local:D10`, which takes
//! the rest). Giving F's lower stroke a channel of its own restarts the run
//! there: what follows F flows on after it, and the letters before keep
//! their lamps. Each step is a statement; the spike drove its editor, here
//! each step is a model edit and an assertion on `Patching::landing`.

use lpc_mapping::part::{Mapping, Part, PartId, PartKind, Path, Shape, TreeId, render};
use lpc_mapping::patching::{
    Amount, ChannelOp, OutputPorts, Override, Patching, Start, patch_mapping,
};

#[test]
fn the_playful_choker() {
    let mut m = choker();
    let outputs = [OutputPorts::of_lamps("out", &[None], 3)];
    let before_f = ["p", "l1", "a", "a-bar", "y-stem", "y-left", "y-right"];

    // 1. From port 0 lamp 0 the letters run end to end: P (13), L (8),
    //    A (10), its bar (2), Y's three strokes (3 each), then F's lower
    //    stroke at lamp 42, U at 52, the last L at 65.
    let p = patch(&m, &outputs);
    assert_eq!(lamp(&p, "p"), Some(0));
    assert_eq!(lamp(&p, "f-lower"), Some(42));
    assert_eq!(lamp(&p, "u"), Some(52));
    assert_eq!(lamp(&p, "l2"), Some(65));
    let letters_before: Vec<_> = before_f.iter().map(|id| lamp(&p, id)).collect();

    // 2. Give F's lower stroke a channel of its own, lamp 60: it starts
    //    there, and F's bar, U and L flow on after it (a restart). P to
    //    Y's right stroke keep their lamps.
    m.overrides.insert(
        id("f-lower"),
        Override {
            channel: Some(ChannelOp::Set(Amount::lamps(60))),
            ..Override::default()
        },
    );
    let p = patch(&m, &outputs);
    assert_eq!(lamp(&p, "f-lower"), Some(60));
    assert_eq!(lamp(&p, "f-bar"), Some(63));
    assert_eq!(lamp(&p, "u"), Some(70));
    assert_eq!(lamp(&p, "l2"), Some(83));
    let letters_after: Vec<_> = before_f.iter().map(|id| lamp(&p, id)).collect();
    assert_eq!(letters_after, letters_before);

    // 3. Clear it: F's lower stroke follows Y again, at lamp 42.
    m.overrides.remove(&id("f-lower"));
    assert_eq!(lamp(&patch(&m, &outputs), "f-lower"), Some(42));
}

// ---- helpers --------------------------------------------------------------

/// The choker's twelve strokes, ids from their names lower-cased, starting
/// at port 0, lamp 0.
fn choker() -> Mapping {
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
    m
}

fn patch(m: &Mapping, outputs: &[OutputPorts]) -> Patching {
    patch_mapping(m, &render(m).unwrap(), outputs)
}

fn id(text: &str) -> TreeId {
    TreeId::parse(text).unwrap()
}

/// Where a stroke's first lamp lands on port 0, in lamps.
fn lamp(p: &Patching, stroke: &str) -> Option<u32> {
    p.landing(&id(stroke), 0).map(|a| {
        assert_eq!(a.port.0, 0);
        a.channel / 3
    })
}
