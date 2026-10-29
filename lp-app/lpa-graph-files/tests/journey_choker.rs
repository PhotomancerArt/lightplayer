//! Journey: the PLAYFUL choker, on `lpc-mapping`'s format-6 document
//! (`tests/format6/choker.map2d.json`): twelve letter paths end to end on
//! output `out`'s one port, which takes the rest. Anchoring F somewhere new
//! restarts the run there: U and L flow on after F, and the letters before
//! keep their pixels. Each step is a statement, driven through the editor's
//! inspector as a person does it; `lpc-mapping`'s `journey_choker.rs` is
//! the same journey as model edits.

mod common;

use common::*;
use lab_mapping_model::*;

#[test]
fn the_playful_choker() {
    let files = project_around(&format6_doc("choker"), "out", &[("ws281x:local:D10", None)]);
    let mut e = editor_of(open_files(&files).project);
    let letter = |id: &str| part(id).node();
    let before_f = ["p", "l1", "a", "a-bar", "y-stem", "y-left", "y-right"];

    // 1. Opened, the letters run end to end from D10's first pixel:
    //    P (13), L (8), A (10), its bar (2), Y's three strokes (3 each),
    //    then F's lower stroke at pixel 42, F's bar, F's upper stroke, U, L.
    let p = &e.project;
    assert_eq!(p.lamp_count(), 73);
    assert_eq!(landing(p, &letter("p"), 0), Some(("0".into(), 0)));
    assert_eq!(landing(p, &letter("f-lower"), 0), Some(("0".into(), 42)));
    assert_eq!(landing(p, &letter("u"), 0), Some(("0".into(), 52)));
    assert_eq!(landing(p, &letter("l2"), 0), Some(("0".into(), 65)));
    let letters_before: Vec<_> = before_f
        .iter()
        .map(|id| landing(p, &letter(id), 0))
        .collect();

    // 2. Anchor F at pixel 60: select its first stroke, make its pixel its
    //    own, and step it up 18. F's other strokes, U and L flow on after
    //    it (a restart); everything before F keeps its pixels, and the
    //    inspector says what moved with it.
    e.select(vec![Target::Part(part("f-lower"))]);
    e.patch_input(PatchInput::Override {
        field: PatchField::Pixel,
    });
    e.patch_input(PatchInput::Nudge {
        field: PatchField::Pixel,
        by: 18,
    });
    let p = &e.project;
    assert_eq!(landing(p, &letter("f-lower"), 0), Some(("0".into(), 60)));
    assert_eq!(landing(p, &letter("f-bar"), 0), Some(("0".into(), 63)));
    assert_eq!(landing(p, &letter("u"), 0), Some(("0".into(), 70)));
    assert_eq!(landing(p, &letter("l2"), 0), Some(("0".into(), 83)));
    let letters_after: Vec<_> = before_f
        .iter()
        .map(|id| landing(p, &letter(id), 0))
        .collect();
    assert_eq!(letters_after, letters_before);
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
        landing(&e.project, &letter("f-lower"), 0),
        Some(("0".into(), 42))
    );
}
