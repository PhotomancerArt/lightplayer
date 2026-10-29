//! Journey: a dome slice, on `lpc-mapping`'s format-6 document
//! (`tests/format6/dome_slice.map2d.json`): a repeat `sector` of five
//! copies of the zook dome's 300-lamp path on output `out`, ports 0–3 of
//! 300 and port 4 taking the rest, one copy per port. Under end to end,
//! anchoring one copy elsewhere carries the next along; under one per port,
//! each copy keeps its slot (jacks are physical). Each step is a statement,
//! driven through the editor's inspector; `lpc-mapping`'s
//! `journey_dome_slice.rs` is the same journey as model edits.

mod common;

use common::*;
use lab_mapping_model::*;
use lpc_mapping::part::TreeId;

#[test]
fn a_dome_slice() {
    let files = project_around(
        &format6_doc("dome_slice"),
        "out",
        &[
            ("ws281x:local:IO18", Some(300)),
            ("ws281x:local:IO13", Some(300)),
            ("ws281x:local:IO2", Some(300)),
            ("ws281x:local:IO14", Some(300)),
            ("ws281x:local:IO16", None),
        ],
    );
    let mut e = editor_of(open_files(&files).project);
    let sector = part("sector");
    let copy = |k: u32| {
        TreeRef::new(
            &fixture(),
            TreeId::parse("path1").unwrap().in_copy(k).canonical(),
        )
    };
    let every_copy_on_its_port = |p: &Project| {
        for k in 0..5 {
            assert_eq!(
                landing(p, &copy(k), 0),
                Some((k.to_string(), 0)),
                "copy {k}"
            );
        }
    };

    // 1. Opened, one copy per port from the first: copy k is on port k at
    //    its first pixel.
    assert_eq!(e.project.lamp_count(), 1500);
    every_copy_on_its_port(&e.project);
    assert!(e.project.effective_patch().problems.is_empty());

    // 2. Set the repeat's Children to end to end: nothing moves, since each
    //    copy fills its port and the next one starts the next port.
    e.select(vec![Target::Part(sector.clone())]);
    choose(&mut e, PatchField::Children, "Flow", "End to end");
    every_copy_on_its_port(&e.project);

    // 3. Under end to end, give copy 3 port 4: copy 4 follows it there,
    //    after it (the last port has no count: it takes the rest).
    e.select(vec![e.project.tree_target(&copy(3))]);
    e.patch_input(PatchInput::Override {
        field: PatchField::Port,
    });
    choose(&mut e, PatchField::Port, "Port", "IO16");
    assert_eq!(landing(&e.project, &copy(3), 0), Some(("4".into(), 0)));
    assert_eq!(
        landing(&e.project, &copy(4), 0),
        Some(("4".into(), 300)),
        "follows"
    );

    // 4. Set the repeat's Children to one per port from the first port:
    //    copy 4 is back on its own port, port 4 — sharing it with copy 3,
    //    which keeps the port it was given (an overlap, reported).
    e.select(vec![Target::Part(sector.clone())]);
    choose(
        &mut e,
        PatchField::Children,
        "Flow",
        "One per port from IO18",
    );
    let p = &e.project;
    assert_eq!(
        landing(p, &copy(4), 0),
        Some(("4".into(), 0)),
        "keeps its slot"
    );
    assert_eq!(landing(p, &copy(2), 0), Some(("2".into(), 0)));
    let overlaps = &p.effective_patch().overlaps;
    assert_eq!(overlaps.len(), 1, "{overlaps:?}");
    assert_eq!(overlaps[0].segments, [copy(3), copy(4)]);
    assert_eq!(overlaps[0].port, "4");

    // 5. Copy 3's port back to its rule: it takes its own slot again.
    e.select(vec![e.project.tree_target(&copy(3))]);
    e.patch_input(PatchInput::Reset {
        field: PatchField::Port,
    });
    every_copy_on_its_port(&e.project);
    assert!(e.project.effective_patch().overlaps.is_empty());
}
