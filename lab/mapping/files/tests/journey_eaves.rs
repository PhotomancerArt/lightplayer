//! Journey: Sean's eaves (`lab/mapping/projects/eaves`): two long runs, the
//! front 60 lamps on D10 (60 pixels), the side 50 after it. The side
//! doesn't fit what is left of D10, so it starts D9; then the house's
//! real wiring (`eaves-wired`: the front fed from its far end) is one
//! override. Each step is a statement, written to carry over to
//! `lpc-mapping` as an M1 journey test.

mod common;

use common::*;
use lab_mapping_files::embedded::{EAVES, EAVES_WIRED};
use lab_mapping_model::*;

#[test]
fn seans_eaves() {
    let mut e = editor_of(open(&EAVES).project);
    let truth = Piece::of(&open(&EAVES_WIRED).project, "the house");
    let front = ObjectId::of(&ComponentId::new("front"));
    let side = ObjectId::of(&ComponentId::new("side"));

    // 1. The front fills D10 from its first pixel; the side doesn't fit
    //    what is left of D10, so it starts on the next port, D9.
    let p = &e.project;
    assert_eq!(span(p, &front), Some(("0".into(), 0, 59)));
    assert_eq!(span(p, &side), Some(("1".into(), 0, 49)));
    assert!(p.effective_patch().walked(&side).unwrap().wrapped);

    // 2. In the house the controller feeds the front from its far end:
    //    give the front a direction of its own, Reversed, and the patch is
    //    the house's wiring.
    assert!(!truth.matches(&e.project));
    e.select(vec![Target::Component(ComponentId::new("front"))]);
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
