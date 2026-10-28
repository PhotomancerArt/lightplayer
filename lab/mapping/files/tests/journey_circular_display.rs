//! Journey: the circular display (Yona's lead case, D1). A circle of four
//! rings and a one-lamp centre on one output; its real wiring (the
//! hardware preview) has ring 3 the other way round and ring 3, ring 4
//! and the centre on D1. Each step is a statement, written to carry over
//! to `lpc-mapping` as an M1 journey test: what the person sets, and where
//! the lamps land.

mod common;

use common::*;
use lab_mapping_model::*;

#[test]
fn the_circular_display() {
    let mut e = editor_of(Project::circular_display());
    let display = e.project.named("display").unwrap().id.clone();
    let circle = ComponentId::new("circle1");
    let ring = |k: &str| ObjectId::new(&circle, k);
    let truth = Piece::of(&Project::circular_display_wired(), "as soldered");

    // 1. The display starts at D0's first pixel, and its rings run end to
    //    end, outside in: 24 + 16 + 12 + 8 lamps, then the centre.
    let p = &e.project;
    assert_eq!(span(p, &ring("r1")), Some(("0".into(), 0, 23)));
    assert_eq!(span(p, &ring("r2")), Some(("0".into(), 24, 39)));
    assert_eq!(span(p, &ring("r3")), Some(("0".into(), 40, 51)));
    assert_eq!(span(p, &ring("r4")), Some(("0".into(), 52, 59)));
    assert_eq!(span(p, &ring("r5")), Some(("0".into(), 60, 60)));

    // 2. The centre is one lamp, at the circle's centre.
    let centre = p.object(&ring("r5")).unwrap();
    assert_eq!(centre.lamps.len(), 1);
    let origin = p.origin(&display);
    assert!((centre.lamps[0].pos - origin).distance(Vec2::new(50.0, 50.0)) < 1e-6);

    // 3. The real display is wired otherwise: the preview says ring 3 and
    //    what follows it are somewhere else.
    assert!(!truth.matches(&e.project));

    // 4. Ring 3 was soldered counter-clockwise: select it, give it a
    //    direction of its own, Reversed. Only ring 3 changes, in its slot.
    let before_r4 = span(&e.project, &ring("r4"));
    e.select(vec![Target::Object(ring("r3"))]);
    e.patch_input(PatchInput::Override {
        field: PatchField::Direction,
    });
    choose(&mut e, PatchField::Direction, "Direction", "Reversed");
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
        hints(&e).contains(&"ring r4 and ring r5 follow it".to_string()),
        "{:?}",
        hints(&e)
    );

    // 6. Now the patch is the display's real wiring.
    assert!(
        truth.matches(&e.project),
        "{:?}",
        truth.differences(&e.project)
    );

    // 7. Or one ring per port: hand ring 3's port back to its rule, and set
    //    the circle's Children to one per port from D0. Each ring starts
    //    its own port, the centre on D4.
    e.patch_input(PatchInput::Reset {
        field: PatchField::Port,
    });
    e.select(vec![Target::Component(circle.clone())]);
    choose(
        &mut e,
        PatchField::Children,
        "Children",
        "One per port from D0",
    );
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
    // Each ring's port comes from the circle's rule, and the inspector
    // says so.
    let w = p.effective_patch().walked(&ring("r4")).unwrap();
    assert_eq!(
        w.sources[1],
        StepSource::Children(PatchTarget::Component(circle.clone()))
    );
}
