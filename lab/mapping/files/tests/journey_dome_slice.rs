//! Journey: a dome slice (`catalog/projects/zook-dome`): one repeat of five
//! 300-lamp copies on five ports (the last one takes the rest). Under end
//! to end, anchoring one copy elsewhere carries the next along; under one
//! per port, each copy keeps its slot (jacks are physical). Each step is a statement, written to
//! carry over to `lpc-mapping` as an M1 journey test.

mod common;

use common::*;
use lab_mapping_files::embedded::ZOOK_DOME;
use lab_mapping_model::*;

#[test]
fn a_dome_slice() {
    let mut e = editor_of(open(&ZOOK_DOME).project);
    let fixture = e.project.fixtures()[0].id.clone();
    let sector = e.project.fixture(&fixture).unwrap().components[0]
        .id
        .clone();
    let arm = e.project.component(&sector).unwrap().children()[0]
        .id
        .clone();
    let copy = |k: u32| TreeRef::of(&arm).in_copy(k).canonical();

    // 1. Opened, copy k is on port k: each copy fills its port, and the
    //    next one starts the next port.
    for k in 0..5 {
        assert_eq!(landing(&e.project, &copy(k), 0), Some((k.to_string(), 0)));
    }

    // 2. Under end to end, give copy 3 port 4: copy 4 follows it there,
    //    after it (the last port has no count: it takes the rest).
    e.select(vec![Target::Generated(copy(3))]);
    e.patch_input(PatchInput::Override {
        field: PatchField::Port,
    });
    choose(&mut e, PatchField::Port, "Port", "ws281x:local:IO16");
    assert_eq!(landing(&e.project, &copy(3), 0), Some(("4".into(), 0)));
    assert_eq!(
        landing(&e.project, &copy(4), 0),
        Some(("4".into(), 300)),
        "follows"
    );

    // 3. Set the repeat's Children to one per port from the first port:
    //    copy 4 is back on its own port, port 4 — sharing it with copy 3,
    //    which keeps the port it was given (an overlap, reported).
    e.select(vec![Target::Part(sector.clone())]);
    choose(
        &mut e,
        PatchField::Children,
        "Children",
        "One per port from ws281x:local:IO18",
    );
    let p = &e.project;
    assert_eq!(
        landing(p, &copy(4), 0),
        Some(("4".into(), 0)),
        "keeps its slot"
    );
    assert_eq!(landing(p, &copy(2), 0), Some(("2".into(), 0)));
    assert!(!p.effective_patch().overlaps.is_empty());

    // 4. Copy 3's port back to its rule: it takes its own slot again.
    e.select(vec![Target::Generated(copy(3))]);
    e.patch_input(PatchInput::Reset {
        field: PatchField::Port,
    });
    assert_eq!(landing(&e.project, &copy(3), 0), Some(("3".into(), 0)));
    assert!(e.project.effective_patch().overlaps.is_empty());
}
