//! Unlock as an offer, end to end: `devices/<board>/unlock` on a board
//! reached over Bluetooth whose link holds nothing (or only play), pressed
//! by path the way a click and the app agent do, against the fake board's
//! REAL `LpServer` checking the password.
//!
//! The password never leaves the press: these tests look for it in every
//! string Studio can show or say — the view (with its offer tree), the
//! console, the app agent's readout — and in the session recorder's lines.

use super::ble_drop_tests::{
    BENCH_PASSWORD, bench_over_bluetooth, bluetooth_board, locked_store_file, wait_for_access_line,
};
use super::*;

/// The play password a friend was given, on the board's store beside the
/// author one.
pub(super) const PLAY_PASSWORD: &str = "friends-play-2";

/// A locked board nothing this browser holds unlocks → the sheet rises and
/// the card offers Unlock → a press with the password runs the login
/// conversation (the real board checks it) → unlocked at edit, so the offer
/// is gone, the sheet is down, and the password was remembered.
#[test]
fn a_password_press_unlocks_a_locked_board_and_the_offer_goes() {
    let device = locked_board("dev000000unlock01");
    let (mut bench, tasks, _present) = bench_over_bluetooth(&device, |_| {});
    let (card, unlock) = locked_and_asking(&mut bench, &tasks);

    let offer = bench.offered(&unlock);
    assert_eq!(offer.label(), "Unlock");
    assert!(offer.is_enabled());
    assert!(offer.consequence().is_routine());
    assert!(offer.takes_a_secret(), "the agent hands it over as a card");
    assert_eq!(
        bench
            .controller
            .view()
            .settings
            .devices
            .remembered_passwords,
        0
    );

    // The recorder's third channel, the protocol requests, is watched while
    // the login conversation runs: it names each request and never carries
    // its payload.
    let requests = Rc::new(RefCell::new(Vec::<String>::new()));
    lpa_client::set_client_observer(Some(Rc::new({
        let requests = Rc::clone(&requests);
        move |observation| requests.borrow_mut().push(format!("{observation:?}"))
    })));
    bench
        .press(
            &unlock,
            OfferArgs::new().with(crate::UNLOCK_PASSWORD_PARAM, BENCH_PASSWORD),
        )
        .expect("the unlock starts");
    wait_for_access_line(&mut bench, &tasks, card, "Unlocked by bench password");
    lpa_client::set_client_observer(None);

    let requests = requests.borrow();
    assert!(
        requests.iter().any(|seen| seen.contains("login.answer")),
        "the board was asked: {requests:?}"
    );
    assert!(
        requests.iter().all(|seen| !seen.contains(BENCH_PASSWORD)),
        "no observed request holds the password: {requests:?}"
    );
    assert!(
        bench.controller.view().login_prompt.is_none(),
        "the sheet comes down on its own"
    );
    bench.not_offered(&unlock);
    assert_eq!(
        bench
            .controller
            .view()
            .settings
            .devices
            .remembered_passwords,
        1,
        "remember starts on"
    );
    assert_no_password_anywhere(&mut bench);
}

/// `remember` off: the password unlocks this board and is kept nowhere.
#[test]
fn remember_off_keeps_the_password_nowhere() {
    let device = locked_board("dev000000unlock02");
    let (mut bench, tasks, _present) = bench_over_bluetooth(&device, |_| {});
    let (card, unlock) = locked_and_asking(&mut bench, &tasks);

    bench
        .press(
            &unlock,
            OfferArgs::new()
                .with(crate::UNLOCK_PASSWORD_PARAM, BENCH_PASSWORD)
                .with(crate::UNLOCK_REMEMBER_PARAM, "false"),
        )
        .expect("the unlock starts");
    wait_for_access_line(&mut bench, &tasks, card, "Unlocked by bench password");

    assert_eq!(
        bench
            .controller
            .view()
            .settings
            .devices
            .remembered_passwords,
        0
    );
    assert_no_password_anywhere(&mut bench);
}

/// A press with no password is the card's button: it raises the sheet, for
/// the person to type into. (Not now puts it away first.)
#[test]
fn a_press_with_no_password_raises_the_sheet() {
    let device = locked_board("dev000000unlock03");
    let (mut bench, tasks, _present) = bench_over_bluetooth(&device, |_| {});
    let (card, unlock) = locked_and_asking(&mut bench, &tasks);

    bench
        .controller
        .apply_access_command(crate::AccessCommand::Dismiss { device: card });
    assert!(
        bench.controller.view().login_prompt.is_none(),
        "Not now put the sheet away"
    );
    bench.offered(&unlock);

    bench
        .press(&unlock, OfferArgs::new())
        .expect("the press raises the sheet");
    let prompt = bench
        .controller
        .view()
        .login_prompt
        .expect("the sheet is up");
    assert_eq!(prompt.device, card);
    assert!(!prompt.busy, "nothing was tried");
}

/// Unlocked for play with a friend's password: the card offers "Unlock to
/// edit", a press with the author password unlocks it at edit, and the
/// offer goes.
#[test]
fn a_play_only_board_offers_unlock_to_edit() {
    let device = FakeEsp32Device::new(FakeDeviceScript::new(FakeBootState::LightPlayer(
        bluetooth_board("dev000000unlock04")
            .with_untrusted_link()
            .with_root_files(vec![two_password_store_file()]),
    )));
    let (mut bench, tasks, _present) = bench_over_bluetooth(&device, |bench| {
        let mut remembered =
            crate::app::access::remembered_passwords::RememberedPasswords::default();
        remembered.remember(PLAY_PASSWORD, 1.0);
        bench
            .controller
            .apply_access_command(crate::AccessCommand::MemoryLoaded {
                passwords_json: Some(remembered.to_json()),
                devices_json: None,
                browser_json: None,
                account_json: None,
            });
    });
    bench.run_until(&tasks, "the board to identify over Bluetooth", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.activity.is_none() && card.state_label == "Ready")
    });
    let card = bench.view().devices[0].id;
    bench.wait_for_verb(&tasks, card, crate::UNLOCK_VERB);
    let unlock = bench.device_verb(card, crate::UNLOCK_VERB);
    assert_eq!(
        bench
            .controller
            .device_roster_view()
            .access
            .get(&card)
            .and_then(|access| access.unlock),
        Some(crate::UiUnlockOffer::PlayOnly)
    );
    assert_eq!(bench.offered(&unlock).label(), "Unlock to edit");

    bench
        .press(
            &unlock,
            OfferArgs::new().with(crate::UNLOCK_PASSWORD_PARAM, BENCH_PASSWORD),
        )
        .expect("the unlock starts");
    wait_for_access_line(&mut bench, &tasks, card, "Unlocked by bench password");
    bench.not_offered(&unlock);
    assert_no_password_anywhere(&mut bench);
}

/// The app agent never handles a password: a value for the secret is
/// refused, and an `act` without one — which would only raise the sheet —
/// becomes the user's card instead (an offer that takes a secret is always
/// the user's to press). Nothing was unlocked and no sheet was raised on
/// the agent's say-so.
#[test]
fn the_agent_hands_unlock_to_the_user() {
    let device = locked_board("dev000000unlock06");
    let (mut bench, tasks, _present) = bench_over_bluetooth(&device, |_| {});
    let (card, unlock) = locked_and_asking(&mut bench, &tasks);
    bench
        .controller
        .apply_access_command(crate::AccessCommand::Dismiss { device: card });
    assert!(bench.controller.view().login_prompt.is_none());
    let unlock = unlock.to_string();

    let refused = act(&mut bench, &unlock, &[("password", BENCH_PASSWORD)]);
    assert!(
        matches!(&refused, lpa_agent::ActOutcome::Refused { reason, .. }
            if reason.contains("never handles passwords") && !reason.contains(BENCH_PASSWORD)),
        "{refused:?}"
    );
    assert!(app_cards(&mut bench).is_empty(), "nothing was handed over");

    let carded = act(&mut bench, &unlock, &[]);
    assert!(
        matches!(carded, lpa_agent::ActOutcome::NeedsUser { .. }),
        "{carded:?}"
    );
    assert_eq!(app_cards(&mut bench).len(), 1);
    assert!(
        bench.controller.view().login_prompt.is_none(),
        "the agent raised no sheet itself"
    );
    assert_no_password_anywhere(&mut bench);
}

/// A board whose link already holds edit (a USB cable is trusted) has
/// nothing to unlock: no offer.
#[test]
fn a_board_unlocked_at_edit_offers_no_unlock() {
    let device = empty_light_player("dev000000unlock05");
    let (mut bench, _tasks) = identified(&device, "usb-unlock-5");
    let card = bench.view().devices[0].id;
    let unlock = bench.device_verb(card, crate::UNLOCK_VERB);
    bench.not_offered(&unlock);
}

/// Wait for the sheet to rise on the locked board and for the card to offer
/// Unlock — the sheet and the offer are the same fact — and return the
/// board and where its offer lives.
fn locked_and_asking(
    bench: &mut DeviceBench,
    tasks: &TaskPool,
) -> (crate::DeviceId, crate::OfferPath) {
    bench.run_until(tasks, "the unlock sheet", |bench| {
        bench.controller.view().login_prompt.is_some()
    });
    let card = bench.view().devices[0].id;
    assert_eq!(
        bench.controller.view().login_prompt.map(|p| p.device),
        Some(card)
    );
    bench.wait_for_verb(tasks, card, crate::UNLOCK_VERB);
    let unlock = bench.device_verb(card, crate::UNLOCK_VERB);
    (card, unlock)
}

/// A Bluetooth board whose store holds one author password
/// ([`BENCH_PASSWORD`]) and lets nobody in without it.
fn locked_board(uid: &str) -> FakeEsp32Device {
    FakeEsp32Device::new(FakeDeviceScript::new(FakeBootState::LightPlayer(
        bluetooth_board(uid)
            .with_untrusted_link()
            .with_root_files(vec![locked_store_file()]),
    )))
}

/// A store with a play password ([`PLAY_PASSWORD`]) and the author one
/// ([`BENCH_PASSWORD`]).
pub(super) fn two_password_store_file() -> (String, Vec<u8>) {
    let author = lpc_access::SecretEntry::from_password(
        "bench password",
        lpc_access::Tier::Edit,
        BENCH_PASSWORD.as_bytes(),
        [7; lpc_access::SALT_BYTES],
        16,
    );
    let play = lpc_access::SecretEntry::from_password(
        "friends",
        lpc_access::Tier::Play,
        PLAY_PASSWORD.as_bytes(),
        [9; lpc_access::SALT_BYTES],
        16,
    );
    let store = lpc_access::DeviceAccessFile {
        version: lpc_access::DeviceAccessFile::VERSION,
        secrets: vec![author, play],
        ble_enabled: true,
        open: lpc_access::OpenTo::Nobody,
    };
    (
        lpc_access::DeviceAccessFile::PATH.to_string(),
        store.to_json().expect("the store serializes").into_bytes(),
    )
}

/// Every string Studio can show or say: the whole view (the offer tree, the
/// cards, the agent's transcript), the console, the app agent's readout.
fn assert_no_password_anywhere(bench: &mut DeviceBench) {
    let view = format!("{:?}", bench.controller.view());
    let logs = format!("{:?}", bench.controller.logs());
    let readout = bench.controller.app_agent_readout_for_test().render();
    let events = bench.controller.device_events().to_jsonl();
    for password in [BENCH_PASSWORD, PLAY_PASSWORD] {
        assert!(!view.contains(password), "the view holds a password");
        assert!(!logs.contains(password), "the console holds a password");
        assert!(!readout.contains(password), "the readout holds a password");
        assert!(!events.contains(password), "the event log holds a password");
    }
}
