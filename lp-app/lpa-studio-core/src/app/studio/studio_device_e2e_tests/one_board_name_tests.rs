//! One board name (`docs/adr/2026-10-28-one-board-name.md`), end to end
//! through the real effects layer and the fake board's own `LpServer`:
//!
//! - an example pushed to a board lands as a library copy named after the
//!   board;
//! - renaming the board writes the name to the board (its identity file and
//!   its hello say it) and renames the project only it plays;
//! - renaming that project renames the board (the step the project
//!   rename runs, `board_follows_project_rename`).
//!
//! Every verb is pressed by its offer path.

use crate::app::library::package_slug::{slugify, strip_date_prefix};

use super::*;

#[test]
fn a_pushed_example_is_named_after_the_board_and_a_rename_follows_everywhere() {
    let device = empty_light_player("dev000000daqf6dvvs7");
    let (mut bench, tasks) = identified(&device, "usb-one-name-1");
    bench.run_until(&tasks, "the board to report nothing loaded", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.loaded_project == lpa_devices::view::LoadedProject::Empty)
    });
    let card = bench.view().devices[0].clone();
    bench.wait_for_verb(&tasks, card.id, "push");
    bench.push_gesture(card.id, bundled_example());
    pushed(&mut bench, &tasks);

    let library = bench.library();
    assert_eq!(library.len(), 1, "{library:?}");
    assert_eq!(
        strip_date_prefix(&library[0].slug),
        slugify(&card.title),
        "the copy carries the board's name, not the example's"
    );
    let project = library[0].uid;
    bench.settle_library();

    bench.press_device(
        card.id,
        "rename",
        OfferArgs::new().with("name", "Flower-Viatrix"),
    );
    bench.run_until(&tasks, "the board to keep its new name", |bench| {
        bench.view().devices[0].title == "Flower-Viatrix"
            && bench.registry().first().is_some_and(|row| {
                !row.name_unsent && row.device_name.as_deref() == Some("Flower-Viatrix")
            })
    });
    let renamed = bench
        .library()
        .into_iter()
        .find(|summary| summary.uid == project)
        .expect("the project is still there");
    assert_eq!(
        strip_date_prefix(&renamed.slug),
        "flower-viatrix",
        "the board's only project took its name"
    );

    // And back: a rename of the project (the gallery's or the editor's
    // title; a web-built verb with no offer yet) renames its board.
    bench.settle_library();
    drive(
        bench
            .controller
            .board_follows_project_rename(&project.to_string(), "Porch"),
    );
    bench.run_until(&tasks, "the board to take the project's name", |bench| {
        bench.view().devices[0].title == "Porch"
            && bench
                .registry()
                .first()
                .is_some_and(|row| !row.name_unsent && row.device_name.as_deref() == Some("Porch"))
    });
}

fn pushed(bench: &mut DeviceBench, tasks: &TaskPool) {
    bench.run_until(tasks, "the push to finish", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.activity.is_none() && card.last_outcome.is_some())
    });
    let outcome = bench.view().devices[0].last_outcome.clone();
    assert!(outcome.is_some_and(|outcome| outcome.ok), "the push landed");
}
