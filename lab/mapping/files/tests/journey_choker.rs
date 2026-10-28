//! Journey: the PLAYFUL choker (`catalog/projects/playful-choker`): twelve
//! letter parts end to end on one port. Anchoring F somewhere new restarts
//! the run there: U and L flow on after F, and the letters before keep
//! their pixels. Each step is a statement, written to carry over to
//! `lpc-mapping` as an M1 journey test.

mod common;

use common::*;
use lab_mapping_files::embedded::CHOKER;
use lab_mapping_model::*;

#[test]
fn the_playful_choker() {
    let mut e = editor_of(open(&CHOKER).project);
    let part = |id: &str| TreeRef::of(&PartRef::new(id));

    // 1. Opened, the letters run end to end from D10's first pixel:
    //    P (13), L (8), A (10), its bar (2), Y's three strokes (3 each),
    //    then F's lower stroke at pixel 42, F's bar, F's upper stroke, U, L.
    let p = &e.project;
    assert_eq!(landing(p, &part("p"), 0), Some(("0".into(), 0)));
    assert_eq!(landing(p, &part("f-lower"), 0), Some(("0".into(), 42)));
    assert_eq!(landing(p, &part("u"), 0), Some(("0".into(), 52)));
    assert_eq!(landing(p, &part("l2"), 0), Some(("0".into(), 65)));
    let before_y: Vec<_> = ["p", "l1", "a", "a-bar", "y-stem", "y-left", "y-right"]
        .iter()
        .map(|id| landing(p, &part(id), 0))
        .collect();

    // 2. Anchor F at pixel 60: select its first stroke, make its pixel its
    //    own, and step it up 18. F's other strokes, U and L flow on after
    //    it (a restart); everything before F keeps its pixels.
    e.select(vec![Target::Part(PartRef::new("f-lower"))]);
    e.patch_input(PatchInput::Override {
        field: PatchField::Pixel,
    });
    e.patch_input(PatchInput::Nudge {
        field: PatchField::Pixel,
        by: 18,
    });
    let p = &e.project;
    assert_eq!(landing(p, &part("f-lower"), 0), Some(("0".into(), 60)));
    assert_eq!(landing(p, &part("f-bar"), 0), Some(("0".into(), 63)));
    assert_eq!(landing(p, &part("u"), 0), Some(("0".into(), 70)));
    assert_eq!(landing(p, &part("l2"), 0), Some(("0".into(), 83)));
    let after_y: Vec<_> = ["p", "l1", "a", "a-bar", "y-stem", "y-left", "y-right"]
        .iter()
        .map(|id| landing(p, &part(id), 0))
        .collect();
    assert_eq!(after_y, before_y);
    assert!(
        hints(&e).contains(&"f-bar, f-upper, u and l2 follow it".to_string()),
        "{:?}",
        hints(&e)
    );

    // 3. Reset hands the pixel back to the run: F follows Y again.
    e.patch_input(PatchInput::Reset {
        field: PatchField::Pixel,
    });
    assert_eq!(
        landing(&e.project, &part("f-lower"), 0),
        Some(("0".into(), 42))
    );
}
