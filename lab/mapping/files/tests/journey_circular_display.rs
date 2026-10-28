//! Journey: the circular display (Yona's lead case, D1), on `lpc-mapping`'s
//! format-6 document (`tests/format6/circular_display.map2d.json`): a
//! circle of four rings and a one-lamp centre on output `main`, five ports
//! of 64. The document is the display as soldered — ring 3 the other way
//! round, and ring 3, ring 4 and the centre on D1 — so that is the piece's
//! truth; the journey hands ring 3 back to its rule, then wires it again by
//! hand from the inspector. Each step is a statement; `lpc-mapping`'s
//! `journey_circular_display.rs` is the same journey as model edits.

mod common;

use common::*;
use lab_mapping_model::*;
use lpc_mapping::part::PartId;
use lpc_mapping::patching::{PatchTarget, StepSource};

#[test]
fn the_circular_display() {
    let ports: Vec<(String, Option<u32>)> = (0..5)
        .map(|k| (format!("ws281x:local:D{k}"), Some(64)))
        .collect();
    let ports: Vec<(&str, Option<u32>)> = ports.iter().map(|(n, c)| (n.as_str(), *c)).collect();
    let files = project_around(&format6_doc("circular_display"), "main", &ports);
    let mut e = editor_of(open_files(&files).project);
    let circle = part("circle1");
    let ring = |k: &str| node(&format!("circle1/{k}"));

    // 1. Opened, it is the display as soldered: rings 1 and 2 end to end
    //    from D0's first pixel; ring 3 on D1 from its first pixel, running
    //    11 down to 0; ring 4 and the centre follow it there. That is the
    //    piece's truth.
    let p = &e.project;
    assert_eq!(span(p, &ring("r1")), Some(("0".into(), 0, 23)));
    assert_eq!(span(p, &ring("r2")), Some(("0".into(), 24, 39)));
    assert_eq!(span(p, &ring("r3")), Some(("1".into(), 11, 0)));
    assert_eq!(span(p, &ring("r4")), Some(("1".into(), 12, 19)));
    assert_eq!(span(p, &ring("r5")), Some(("1".into(), 20, 20)));
    assert!(p.effective_patch().problems.is_empty());
    let truth = Piece::of(p, "as soldered");

    // 2. The centre is one lamp, at the circle's centre.
    let centre = p.segment(&ring("r5")).unwrap();
    assert_eq!(centre.lamps.len(), 1);
    let origin = p.origin(&fixture());
    assert!((centre.lamps[0].pos - origin).distance(Vec2::new(50.0, 50.0)) < 1e-6);

    // 3. Hand ring 3's port and direction back to its rule: the rings run
    //    end to end from D0's first pixel, outside in — 24 + 16 + 12 + 8
    //    lamps, then the centre — and the piece no longer matches.
    e.select(vec![Target::Generated(ring("r3"))]);
    e.patch_input(PatchInput::Reset {
        field: PatchField::Port,
    });
    e.patch_input(PatchInput::Reset {
        field: PatchField::Direction,
    });
    let p = &e.project;
    assert_eq!(span(p, &ring("r1")), Some(("0".into(), 0, 23)));
    assert_eq!(span(p, &ring("r2")), Some(("0".into(), 24, 39)));
    assert_eq!(span(p, &ring("r3")), Some(("0".into(), 40, 51)));
    assert_eq!(span(p, &ring("r4")), Some(("0".into(), 52, 59)));
    assert_eq!(span(p, &ring("r5")), Some(("0".into(), 60, 60)));
    assert!(!truth.matches(p));

    // 4. Ring 3 was soldered counter-clockwise: give it a direction of its
    //    own, Reversed. Only ring 3 changes, in its slot.
    let before_r4 = span(&e.project, &ring("r4"));
    e.patch_input(PatchInput::Override {
        field: PatchField::Direction,
    });
    choose(&mut e, PatchField::Direction, "Direction", "Reverse");
    assert_eq!(span(&e.project, &ring("r3")), Some(("0".into(), 51, 40)));
    assert_eq!(span(&e.project, &ring("r4")), before_r4);

    // 5. Ring 3 is on D1, from its first pixel: give it that port. Ring 4
    //    and the centre follow it there; rings 1 and 2 stay on D0. The
    //    inspector says what moved with it.
    e.patch_input(PatchInput::Override {
        field: PatchField::Port,
    });
    choose(&mut e, PatchField::Port, "Port", "D1");
    let p = &e.project;
    assert_eq!(span(p, &ring("r2")), Some(("0".into(), 24, 39)));
    assert_eq!(span(p, &ring("r3")), Some(("1".into(), 11, 0)));
    assert_eq!(span(p, &ring("r4")), Some(("1".into(), 12, 19)));
    assert_eq!(span(p, &ring("r5")), Some(("1".into(), 20, 20)));
    assert!(
        hints(&e).contains(&"ring r4 and the centre follow it".to_string()),
        "{:?}",
        hints(&e)
    );

    // 6. Now the patch is the display as soldered again.
    assert!(
        truth.matches(&e.project),
        "{:?}",
        truth.differences(&e.project)
    );

    // 7. Or one ring per port: hand ring 3's port back to its rule, and set
    //    the circle's Children to one per port from D0. Each ring starts
    //    its own port, the centre on D4; ring 3 still runs backwards.
    e.patch_input(PatchInput::Reset {
        field: PatchField::Port,
    });
    e.select(vec![Target::Part(circle.clone())]);
    choose(&mut e, PatchField::Children, "Flow", "One per port from D0");
    let p = &e.project;
    for (k, port) in [
        ("r1", "0"),
        ("r2", "1"),
        ("r3", "2"),
        ("r4", "3"),
        ("r5", "4"),
    ] {
        assert_eq!(
            landing(p, &ring(k), 0).map(|l| l.0),
            Some(port.into()),
            "{k}"
        );
    }
    assert_eq!(span(p, &ring("r3")), Some(("2".into(), 11, 0)));
    // Each ring's port comes from the circle's rule, and the inspector
    // says so.
    let w = p.effective_patch().walked(&ring("r4")).unwrap();
    assert_eq!(
        w.sources[1],
        StepSource::Flow(PatchTarget::Part(PartId::new("circle1").unwrap()))
    );
    e.select(vec![Target::Generated(ring("r4"))]);
    assert!(
        rows(&e).iter().any(|i| matches!(
            i,
            UiItem::Field(UiField {
                label: "Port",
                source: Some(UiSource::From { words, .. }),
                ..
            }) if words == "from circle1's flow: one per port"
        )),
        "{:?}",
        rows(&e)
    );
}
