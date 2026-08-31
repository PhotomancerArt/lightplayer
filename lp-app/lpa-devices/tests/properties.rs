//! The two property tests that replace hand-audited match arms.
//!
//! 1. **The projection is total** — every reachable model state renders
//!    something honest. No panic, no empty label, no card that says nothing.
//! 2. **Every projection includes at least one escape** — cancel, disconnect
//!    or forget; pending links expose dismiss. The shipped system lost its
//!    danger zone in exactly the stuck states (`OperationInFlight` had no
//!    danger section at all, and an anonymous board could never be
//!    forgotten), so this is the invariant that must hold structurally.
//!
//! `proptest` is not a workspace dependency, so the generator is a
//! hand-rolled enumeration over the state space: every link lifecycle ×
//! every user gesture × every interesting instant. That is ~600 rosters,
//! each asserted after EVERY step, and it is clearer than a shrinking
//! random search for a space this shape.

use lpa_devices::replay::{Replay, Step};
use lpa_devices::view::{DeviceView, PendingLinkView, RosterView};
use lpa_devices::{ActivityKind, Escape, Millis, RosterConfig};

#[test]
fn the_projection_is_total_and_always_escapable() {
    let mut checked = 0_usize;
    for (lifecycle_name, lifecycle) in link_lifecycles() {
        for (gesture_name, gesture) in gestures() {
            for at in instants() {
                let case = format!("{lifecycle_name} + {gesture_name} @ {at} ms");
                let mut replay = Replay::new(RosterConfig::default());

                // The world happens first, one step at a time, asserting the
                // projection after each one.
                let mut clock = 0_u64;
                for step in lifecycle.clone() {
                    clock += 10;
                    replay.step(Millis(clock), step);
                    assert_view(&replay.view(), &case);
                }

                // Then the user does something.
                for step in gesture.clone() {
                    clock += 10;
                    replay.step(Millis(clock), step);
                    assert_view(&replay.view(), &case);
                }

                // Then time passes, deadlines fire, and the projection must
                // still hold.
                replay.advance_to(Millis(clock.max(at)));
                assert_view(&replay.view(), &case);
                checked += 1;
            }
        }
    }
    assert!(
        checked > 400,
        "the enumeration got smaller: {checked} cases"
    );
}

#[test]
fn a_view_survives_a_serde_round_trip() {
    // The projection crosses a process boundary in Studio (view channel), so
    // it has to be exactly representable.
    let mut replay = Replay::new(RosterConfig::default());
    replay.step(Millis(0), Step::attach(1, "usb-1"));
    replay.step(Millis(20), Step::opened(1));
    replay.step(Millis(200), Step::hello(1).uid("dev_abc").board("dig-uno"));

    let view = replay.view();
    let json = serde_json::to_string(&view).expect("serialize");
    let back: RosterView = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(back, view);
}

#[test]
fn no_device_is_ever_busy_with_two_activities() {
    // Invariant I5, as a property over the same enumeration: the model has
    // room for exactly one activity per device, and every gesture path must
    // respect it.
    for (_, lifecycle) in link_lifecycles() {
        for (_, gesture) in gestures() {
            let mut replay = Replay::new(RosterConfig::default());
            let mut clock = 0_u64;
            for step in lifecycle.clone().into_iter().chain(gesture.clone()) {
                clock += 10;
                replay.step(Millis(clock), step);
                for device in replay.roster().devices() {
                    // `activity` is an Option, so "two" is unrepresentable;
                    // what this checks is that the journal never brackets a
                    // second start without an end in between.
                    assert!(
                        device.activity.is_some() == device.is_busy(),
                        "busy and activity disagree"
                    );
                }
                // Every ActivityStarted is closed by exactly one
                // ActivityEnded (it finished) or ActivityEvicted (it was
                // removed). The count of unclosed brackets must equal the
                // number of entries that are actually busy — which is what
                // makes "activity in flight" a derived fact instead of a
                // parallel store that can leak (the `device_card_ops`
                // disease).
                let starts = count(&replay, "ActivityStarted");
                let ends = count(&replay, "ActivityEnded");
                let evicted = count(&replay, "ActivityEvicted");
                let busy = replay
                    .roster()
                    .devices()
                    .iter()
                    .filter(|device| device.is_busy())
                    .count()
                    + replay
                        .roster()
                        .pending()
                        .iter()
                        .filter(|pending| pending.is_identifying())
                        .count();
                assert_eq!(
                    starts.saturating_sub(ends + evicted),
                    busy,
                    "open brackets ({starts} started, {ends} ended, {evicted} evicted) \
                     disagree with the {busy} busy entries"
                );
            }
        }
    }
}

fn assert_view(view: &RosterView, case: &str) {
    for device in &view.devices {
        assert_device(device, case);
    }
    for pending in &view.pending {
        assert_pending(pending, case);
    }
}

fn assert_device(device: &DeviceView, case: &str) {
    assert!(
        !device.title.is_empty(),
        "[{case}] a device card with no title"
    );
    assert!(
        !device.state_label.is_empty(),
        "[{case}] a device card with no state label"
    );
    assert!(
        !device.escapes.is_empty(),
        "[{case}] no way out of {:?}",
        device.state_label
    );
    assert!(
        device.escapes.contains(&Escape::Forget),
        "[{case}] forget is defined at the model level and must always be offered"
    );
    if let Some(activity) = &device.activity {
        assert!(
            !activity.label.is_empty(),
            "[{case}] a running activity with no label"
        );
        // A cancellable activity must expose its escape, and a
        // cancel-requested one must not pretend it can be cancelled again.
        assert_eq!(
            activity.cancellable,
            device.escapes.contains(&Escape::Cancel),
            "[{case}] cancellable disagrees with the escape list"
        );
        assert_eq!(
            activity.cancellable, !activity.cancel_requested,
            "[{case}] cancel state is inconsistent"
        );
    } else {
        assert!(
            !device.escapes.contains(&Escape::Cancel),
            "[{case}] cancel offered with nothing to cancel"
        );
    }
    if let Some(outcome) = &device.last_outcome {
        assert!(
            !outcome.summary.is_empty(),
            "[{case}] an outcome banner with no text"
        );
    }
    if let Some(freshness) = &device.freshness_label {
        assert!(
            !freshness.is_empty(),
            "[{case}] an empty freshness label is worse than none"
        );
    }
    // The empty face's verb is never drawn over a running activity — one
    // activity per device (I5), and a second gesture would be refused.
    assert!(
        !(device.can_receive_project && device.activity.is_some()),
        "[{case}] a push offered on a busy device"
    );
    if let lpa_devices::view::LoadedProject::Running { label } = &device.loaded_project {
        assert!(
            !label.is_empty(),
            "[{case}] a running face with nothing to name"
        );
    }
}

fn assert_pending(pending: &PendingLinkView, case: &str) {
    assert!(
        !pending.title.is_empty(),
        "[{case}] a pending link with no title"
    );
    assert!(
        !pending.state_label.is_empty(),
        "[{case}] a pending link with no state label"
    );
    assert_eq!(
        pending.escapes,
        vec![Escape::Forget],
        "[{case}] a pending link must always be dismissable"
    );
    assert!(
        pending.can_adopt,
        "[{case}] a blank chip may never identify itself, so adopt is always offered"
    );
}

fn count(replay: &Replay, needle: &str) -> usize {
    replay
        .journal_notes()
        .iter()
        .filter(|note| note.contains(needle))
        .count()
}

/// Every shape a link's life can take, as far as the model can tell.
fn link_lifecycles() -> Vec<(&'static str, Vec<Step>)> {
    let blank = "invalid header: 0xffffffff";
    let rom = "waiting for download";
    let foreign = "Hello from Seeed Studio XIAO ESP32-C6";
    vec![
        ("nothing at all", vec![]),
        ("attach only", vec![Step::attach(1, "usb-1")]),
        (
            "attach + open, silent",
            vec![Step::attach(1, "usb-1"), Step::opened(1)],
        ),
        (
            "blank boot loop",
            vec![
                Step::attach(1, "usb-1"),
                Step::opened(1),
                Step::line(1, "ESP-ROM:esp32c6-20220919"),
                Step::line(1, blank),
            ],
        ),
        (
            "rom download mode",
            vec![
                Step::attach(1, "usb-1"),
                Step::opened(1),
                Step::line(1, rom),
            ],
        ),
        (
            "known foreign firmware",
            vec![
                Step::attach(1, "usb-1"),
                Step::opened(1),
                Step::line(1, foreign),
            ],
        ),
        (
            "heartbeat before hello",
            vec![
                Step::attach(1, "usb-1"),
                Step::opened(1),
                Step::heartbeat(1),
            ],
        ),
        (
            "frames but never a hello",
            vec![
                Step::attach(1, "usb-1"),
                Step::opened(1),
                Step::frame(1, "UnloadProject"),
            ],
        ),
        (
            "ready light player",
            vec![
                Step::attach(1, "usb-1"),
                Step::opened(1),
                Step::hello(1).uid("dev_abc").board("dig-uno"),
            ],
        ),
        (
            "proto mismatch",
            vec![
                Step::attach(1, "usb-1"),
                Step::opened(1),
                Step::hello(1).proto(999).uid("dev_old"),
            ],
        ),
        (
            "ready then unplugged",
            vec![
                Step::attach(1, "usb-1"),
                Step::opened(1),
                Step::hello(1).uid("dev_abc"),
                Step::detach(1),
            ],
        ),
        (
            "ready then port closed",
            vec![
                Step::attach(1, "usb-1"),
                Step::opened(1),
                Step::hello(1).uid("dev_abc"),
                Step::closed(1),
            ],
        ),
        (
            "ready then reset",
            vec![
                Step::attach(1, "usb-1"),
                Step::opened(1),
                Step::hello(1).uid("dev_abc"),
                Step::ResetOutcome { link: 1, ok: true },
            ],
        ),
        (
            "transport error",
            vec![
                Step::attach(1, "usb-1"),
                Step::opened(1),
                Step::Error {
                    link: 1,
                    message: "read failed".to_string(),
                },
            ],
        ),
        (
            "two links, one identified",
            vec![
                Step::attach(1, "usb-1"),
                Step::attach(2, "usb-2"),
                Step::opened(1),
                Step::hello(1).uid("dev_abc"),
            ],
        ),
    ]
}

/// Every gesture a user can make. Device 1 is the first entry the model
/// mints; gestures aimed at nothing are part of the space on purpose.
fn gestures() -> Vec<(&'static str, Vec<Step>)> {
    vec![
        ("no gesture", vec![]),
        ("connect", vec![Step::Connect { device: 1 }]),
        ("disconnect", vec![Step::Disconnect { device: 1 }]),
        ("identify", vec![Step::Identify { device: 1 }]),
        (
            "identify then cancel",
            vec![Step::Identify { device: 1 }, Step::Cancel { device: 1 }],
        ),
        (
            "cancel with nothing running",
            vec![Step::Cancel { device: 1 }],
        ),
        (
            "rename",
            vec![Step::SetName {
                device: 1,
                name: "Kitchen".to_string(),
            }],
        ),
        ("adopt the link", vec![Step::Adopt { link: 1 }]),
        ("dismiss the link", vec![Step::Dismiss { link: 1 }]),
        ("forget", vec![Step::Forget { device: 1 }]),
        ("flash", vec![flash_step()]),
        ("factory reset", vec![Step::Erase { device: 1 }]),
        (
            "factory reset then the effect ends",
            vec![
                Step::Erase { device: 1 },
                Step::EffectEnded {
                    device: 1,
                    ok: true,
                    message: None,
                    effect: None,
                    kind: Some(ActivityKind::Erase),
                },
            ],
        ),
        (
            "flash then the effect fails",
            vec![
                flash_step(),
                Step::EffectEnded {
                    device: 1,
                    ok: false,
                    message: Some("write failed at 0x2000".to_string()),
                    effect: None,
                    kind: None,
                },
            ],
        ),
        (
            "flash then the effect succeeds",
            vec![
                flash_step(),
                Step::EffectEnded {
                    device: 1,
                    ok: true,
                    message: None,
                    effect: None,
                    kind: None,
                },
            ],
        ),
        (
            "flash then cancel",
            vec![flash_step(), Step::Cancel { device: 1 }],
        ),
        ("push", vec![Step::Push { device: 1 }]),
        (
            "push then the effect fails",
            vec![
                Step::Push { device: 1 },
                Step::EffectEnded {
                    device: 1,
                    ok: false,
                    message: Some("the board refused the write".to_string()),
                    effect: None,
                    kind: Some(ActivityKind::Push),
                },
            ],
        ),
        (
            "push then the effect succeeds",
            vec![
                Step::Push { device: 1 },
                Step::EffectEnded {
                    device: 1,
                    ok: true,
                    message: Some("project sent".to_string()),
                    effect: None,
                    kind: Some(ActivityKind::Push),
                },
            ],
        ),
        (
            "push then cancel",
            vec![Step::Push { device: 1 }, Step::Cancel { device: 1 }],
        ),
        (
            "adopt then forget",
            vec![Step::Adopt { link: 1 }, Step::Forget { device: 1 }],
        ),
        ("add from usb", vec![Step::AddFromUsb]),
    ]
}

/// Instants worth landing on: before any deadline, past the identify
/// deadline, past the cancel grace, past the quiet window, long after, and
/// past the flash ladder + deadline.
fn instants() -> Vec<u64> {
    vec![0, 1_500, 6_500, 14_000, 40_000, 300_000]
}

/// The flash gesture aimed at entry 1 (a device, a pending link — which the
/// gesture adopts — or nothing at all; the space includes the misses on
/// purpose).
fn flash_step() -> Step {
    Step::Flash {
        device: 1,
        board: "seeed-xiao-esp32c6".to_string(),
        build: "esp32c6-4mb".to_string(),
    }
}
