//! Drawing on the picture, on the board's card (plan
//! `drawing-on-the-picture` P5, D12): a board running a pattern that reads
//! `touches` has a drawable picture only while this tab's session is
//! connected on its card. Watched, it is not; after Done, it is not.
//!
//! The in-process half (where the touches go, the stroke's pace, the
//! release, the picture's speed) is `studio_draw_e2e_tests.rs`.

use super::*;
use crate::UiDrawSurface;
use crate::app::studio::studio_draw_e2e_tests::playlist_files;

#[test]
fn only_the_connected_boards_card_picture_is_drawable() {
    let device = empty_light_player("dev000000draw000001");
    let (mut bench, tasks) = identified(&device, "usb-draw-1");
    bench.run_until(&tasks, "the board to report nothing loaded", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.loaded_project == lpa_devices::view::LoadedProject::Empty)
    });
    let files: Vec<(String, Vec<u8>)> = playlist_files()
        .into_iter()
        .map(|(name, body)| (name, body.into_bytes()))
        .collect();
    let uid = bench
        .store
        .install_package(
            "Drawing",
            &files,
            crate::app::library::PackageProvenance::Created,
            1.0,
        )
        .expect("the drawing project installs")
        .uid;
    bench.settle_library();
    let id = bench.view().devices[0].id;
    bench.push_gesture(
        id,
        crate::PushSource::Library {
            project_uid: uid.to_string(),
        },
    );
    bench.run_until(&tasks, "the board to run the drawing project", |bench| {
        bench.view().devices.iter().any(|card| {
            card.id == id
                && card.activity.is_none()
                && matches!(
                    card.loaded_project,
                    lpa_devices::view::LoadedProject::Running { .. }
                )
        })
    });

    // Watched: the picture is the board's, and nobody draws on it.
    assert_eq!(drawable_card(&bench, id), None, "a watched card");

    // Connected, on its card: drawable once the session has read the
    // board's graph and a frame, into the playing pattern's own scope.
    bench.wait_for_verb(&tasks, id, crate::CONNECT_VERB);
    bench.press_device(id, crate::CONNECT_VERB, OfferArgs::new());
    let deadline = std::time::Instant::now() + REAL_TIME_LIMIT;
    let drawable = loop {
        if let Some(drawable) = drawable_card(&bench, id) {
            break drawable;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the connected card's picture never became drawable"
        );
        bench.clock.set(bench.clock.get() + 0.2);
        let _ = bench.tick();
        for _ in 0..40 {
            bench.step(&tasks);
        }
    };
    assert_eq!(drawable.surface, UiDrawSurface::Board(id));
    assert_eq!(drawable.target.channel, "touches");
    let panel = bench
        .controller
        .view()
        .home
        .expect("the home page")
        .devices
        .cards
        .into_iter()
        .find(|card| card.device == id)
        .and_then(|card| card.panel)
        .expect("the connected card's panel");
    assert!(!drawable.target.scope.is_sink());
    assert_ne!(
        Some(drawable.target.scope),
        panel.target,
        "the playing pattern's scope, not the root's"
    );
    let readout = bench.controller.app_agent_readout_for_test();
    assert!(
        readout.text.contains("'s picture is drawable"),
        "the agent reads it: {}",
        readout.text
    );

    // Done: the session is gone, and so is drawing.
    bench.press_device(id, "done", OfferArgs::new());
    assert_eq!(drawable_card(&bench, id), None, "after Done");
}

/// `device`'s card picture's drawable fact, as the home view publishes it.
fn drawable_card(bench: &DeviceBench, device: crate::DeviceId) -> Option<crate::UiDrawable> {
    bench
        .controller
        .view()
        .home?
        .devices
        .cards
        .into_iter()
        .find(|card| card.device == device)?
        .picture
        .drawable
}
