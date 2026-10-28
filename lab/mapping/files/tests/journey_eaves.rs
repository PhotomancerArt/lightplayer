//! Journey: Sean's eaves, on `lpc-mapping`'s format-6 document
//! (`tests/format6/eaves.map2d.json`): two long runs on output `out`, the
//! front 60 lamps on D10 (60 pixels), the side 50 after it. The side
//! doesn't fit what is left of D10, so it starts D9. The document is the
//! house as wired — the front fed from its far end, one override — so that
//! is the piece's truth; the journey hands the front back to its rule and
//! wires it again from the inspector. Each step is a statement;
//! `lpc-mapping`'s `journey_eaves.rs` is the same journey as model edits.

mod common;

use common::*;
use lab_mapping_model::*;

#[test]
fn seans_eaves() {
    let files = project_around(
        &format6_doc("eaves"),
        "out",
        &[("ws281x:local:D10", Some(60)), ("ws281x:local:D9", None)],
    );
    let mut e = editor_of(open_files(&files).project);
    let front = part("front").node();
    let side = part("side").node();

    // 1. Opened, it is the house: the front fills D10 from its far end,
    //    59 down to 0; the side doesn't fit what is left of D10, so it
    //    starts on the next port, D9, and is marked as moved on.
    let p = &e.project;
    assert_eq!(span(p, &front), Some(("0".into(), 59, 0)));
    assert_eq!(span(p, &side), Some(("1".into(), 0, 49)));
    assert!(p.effective_patch().walked(&side).unwrap().wrapped);
    assert!(!p.effective_patch().walked(&front).unwrap().wrapped);
    let truth = Piece::of(p, "the house");

    // 2. Hand the front's direction back to its rule: it runs as drawn,
    //    0 to 59, and the piece says that is the one difference — the
    //    front, backwards.
    e.select(vec![Target::Part(part("front"))]);
    e.patch_input(PatchInput::Reset {
        field: PatchField::Direction,
    });
    assert_eq!(span(&e.project, &front), Some(("0".into(), 0, 59)));
    assert_eq!(span(&e.project, &side), Some(("1".into(), 0, 49)));
    let kinds: Vec<_> = truth
        .differences(&e.project)
        .into_iter()
        .map(|d| (d.object, d.kind))
        .collect();
    assert_eq!(kinds, [(front.clone(), DifferenceKind::Reversed)]);

    // 3. In the house the controller feeds the front from its far end:
    //    give the front a direction of its own, Reversed, and the patch is
    //    the house's wiring again.
    e.patch_input(PatchInput::Override {
        field: PatchField::Direction,
    });
    choose(&mut e, PatchField::Direction, "Direction", "Reversed");
    assert_eq!(span(&e.project, &front), Some(("0".into(), 59, 0)));
    assert!(
        truth.matches(&e.project),
        "{:?}",
        truth.differences(&e.project)
    );
}
