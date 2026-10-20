//! A session through lightplayer.app moves onto Wi‑Fi by itself, and back
//! (`devices/lan_move.rs`, `studio_controller/lan_move_flow.rs`), end to end
//! through the real effects layer, against doubles of the board's one
//! network slot that bite the way the real ones do:
//!
//! - the board holds its slot for the key its relay session proved, hands it
//!   to a LAN connection that proves the same key, and tells any other key
//!   it is busy — so the move must present the relay's keys first;
//! - a relay session the board closes is redialled at once, taking the slot
//!   back, unless it is parked — so the move must park it before the dial;
//! - the move must never hold the editor ("Reconnecting…"), must wait for a
//!   board that is busy, and must not go back through lightplayer.app after
//!   another connection took the board (close code 1000).
//!
//! Every verb is pressed by its offer path; the move itself is the refresh
//! tick's (`drive_lan_moves`).

use lpa_link::providers::network_link::{LinkKeys, relay_socket_url};

use super::best_link_tests::list;
use super::lan_drop_tests::LossyLink;
use super::relay_connect_tests::{BOARD, relay_light_player, unplug};
use super::*;

/// The address the account's list says the board gave on its network.
const ROW_LAN: &str = "192.168.4.20:80";
/// The socket Studio dials for it.
const ROW_URL: &str = "ws://192.168.4.20:80/link";
/// How the page says each Wi‑Fi drop the tests make.
const LOST_REBOOT: &str = "wi-fi link lost: the board closed the link (code 1006)";
const LOST_TAKEN: &str = "wi-fi link lost: the board closed the link (code 1000)";

#[test]
fn an_idle_relay_session_moves_onto_wifi_with_the_editor_open_and_no_curtain() {
    let mut rig = Rig::new("dev000000mvwf000001", true);
    rig.list_on_this_network();
    let held = rig.tick_until("the board's session on Wi‑Fi", |rig| rig.on_lan());
    assert!(
        held.iter().all(|held| !held),
        "the editor was never held across the move"
    );
    let slot = rig.slot.borrow();
    assert_eq!(slot.lan_dials.len(), 1, "{:?}", slot.events);
    let (url, first_key) = &slot.lan_dials[0];
    assert_eq!(url, ROW_URL, "the list's address");
    assert_eq!(
        Some(*first_key),
        slot.holder,
        "the relay session's key first, so the board hands the slot over"
    );
    let parked = slot.events.iter().position(|event| event == "relay park");
    let dialled = slot.events.iter().position(|event| event == "lan dial");
    assert!(
        parked.is_some() && parked < dialled,
        "parked before the dial: {:?}",
        slot.events
    );
    assert_eq!(slot.relay_redials, 0, "nothing took the slot back");
    drop(slot);
    rig.tick_until("the relay session to be let go", |rig| {
        !rig.slot.borrow().relay_present
    });
    // The editor is on the new link, still open, still connected.
    assert_eq!(
        rig.lens_endpoint().as_deref(),
        Some(&*format!("lan:{ROW_URL}"))
    );
    assert!(
        !rig.bench.controller.view().panes.is_empty(),
        "the editor shows"
    );
    assert!(rig.bench.controller.connected().is_some());
    let card = &rig.bench.controller.device_roster_view().lan_links[&rig.id];
    assert_eq!(card.kind, crate::UiLinkKind::Wifi);
    assert_eq!(rig.said("Moved to Wi\u{2011}Fi on your network"), 1);
    // The relay's order was for the move only: the LAN's own is back (this
    // browser's key, then the account's).
    let keys = rig.bench.controller.network_link_keys();
    let presented = keys.keys_for(ROW_URL);
    assert_eq!(presented.len(), 2, "this browser's key and the account's");
    assert_ne!(
        presented.first().map(|key| key.key_id),
        rig.slot.borrow().holder,
        "without the move, the LAN would lead with the key the board refuses"
    );

    // Never again this page load.
    rig.ticks(200);
    assert_eq!(rig.slot.borrow().lan_dials.len(), 1);
    assert_eq!(rig.said("Moved to Wi\u{2011}Fi on your network"), 1);
}

#[test]
fn a_move_that_fails_stays_on_lightplayer_app_and_never_tries_again() {
    for (what, raw) in [
        (
            "blocked",
            "wi-fi connect to ws://192.168.4.20:80/link failed: \
             net::ERR_BLOCKED_BY_LOCAL_NETWORK_ACCESS_CHECKS",
        ),
        ("timed out", "wi-fi connect timed out after 10 s"),
        (
            "busy",
            "wi-fi link lost: the board closed the link (code 1013: try again later)",
        ),
        (
            "refused",
            "wi-fi connect to ws://192.168.4.20:80/link was closed (code 1006)",
        ),
    ] {
        let mut rig = Rig::new("dev000000mvwf000002", false);
        rig.slot.borrow_mut().lan_refuse = Some(raw.to_string());
        rig.list_on_this_network();
        rig.ticks(20);
        let slot = rig.slot.borrow();
        assert_eq!(slot.lan_dials.len(), 1, "{what}");
        assert!(!slot.relay_parked, "{what}: unparked");
        assert!(slot.relay_present, "{what}: the relay session is live");
        drop(slot);
        assert_eq!(rig.said("Staying on lightplayer.app"), 1, "{what}");
        assert!(
            rig.bench
                .controller
                .device_roster_view()
                .wifi_connects
                .is_empty(),
            "{what}: the card says nothing about it"
        );
        assert_eq!(
            rig.bench.controller.device_roster_view().lan_links[&rig.id].kind,
            crate::UiLinkKind::Relay,
            "{what}"
        );
        rig.ticks(200);
        assert_eq!(rig.slot.borrow().lan_dials.len(), 1, "{what}: one try");
    }
}

#[test]
fn a_dial_that_answers_but_never_becomes_the_boards_link_gives_up_at_eight_seconds() {
    let mut rig = Rig::new("dev000000mvwf000003", false);
    // The LAN answers, but no link ever appears for it.
    rig.slot.borrow_mut().lan_vanishes = true;
    rig.list_on_this_network();
    rig.ticks(20);
    assert_eq!(rig.said("Staying on lightplayer.app"), 0, "still waiting");
    rig.bench.clock.set(rig.bench.clock.get() + 8.5);
    rig.ticks(5);
    assert_eq!(rig.said("Staying on lightplayer.app"), 1);
    let slot = rig.slot.borrow();
    assert!(!slot.relay_parked);
    assert!(slot.lan_forgets >= 1, "the LAN session is let go");
}

#[test]
fn a_board_that_is_not_eligible_is_never_dialled_and_its_try_is_not_spent() {
    // The page cannot reach the LAN.
    let mut rig = Rig::new("dev000000mvwf000004", false);
    rig.bench.controller.set_lan_reach(crate::LanReach::Blocked);
    rig.list_on_this_network();
    rig.ticks(50);
    assert!(rig.slot.borrow().lan_dials.is_empty(), "blocked");

    // The list says another network, then gives no address.
    let mut rig = Rig::new("dev000000mvwf000005", false);
    list(&mut rig.bench, &rig.tasks, BOARD, Some(ROW_LAN), false);
    rig.ticks(50);
    list(&mut rig.bench, &rig.tasks, BOARD, None, true);
    rig.ticks(50);
    assert!(
        rig.slot.borrow().lan_dials.is_empty(),
        "another network, no address"
    );
    // Then it does: the board moves, once.
    rig.list_on_this_network();
    rig.tick_until("the move", |rig| rig.on_lan());
    assert_eq!(rig.slot.borrow().lan_dials.len(), 1);

    // Signed out.
    let mut rig = Rig::new("dev000000mvwf000006", false);
    rig.list_on_this_network();
    rig.bench
        .controller
        .apply_access_command(crate::AccessCommand::AccountKeys(None));
    rig.ticks(50);
    assert!(rig.slot.borrow().lan_dials.is_empty(), "signed out");
}

#[test]
fn a_board_on_its_cable_never_moves() {
    let device = relay_light_player("dev000000mvwf000007");
    let (mut bench, tasks) = identified(&device, "usb-lan-move-7");
    let slot = Rig::install(&mut bench, &device);
    sign_in(&mut bench, &tasks);
    list(&mut bench, &tasks, BOARD, Some(ROW_LAN), true);
    for _ in 0..100 {
        bench.step(&tasks);
        drive(bench.controller.drive_lan_moves());
    }
    assert!(slot.borrow().lan_dials.is_empty());
}

/// F4: a board busy with work over lightplayer.app is not moved under it;
/// the move waits (its one try unspent) and happens once the work is over.
#[test]
fn a_push_running_through_lightplayer_app_defers_the_move_until_it_ends() {
    let mut rig = Rig::new("dev000000mvwf000008", false);
    rig.list_on_this_network();
    let card = rig.bench.view().devices[0].clone();
    rig.bench.push_gesture(card.id, bundled_example());
    let mut looks = 0;
    while rig
        .bench
        .view()
        .devices
        .iter()
        .any(|card| card.activity.is_some())
    {
        drive(rig.bench.controller.drive_lan_moves());
        assert!(
            rig.slot.borrow().lan_dials.is_empty(),
            "no move during the push"
        );
        looks += 1;
        rig.bench.step(&rig.tasks);
    }
    assert!(looks > 0, "the move looked while the push ran");
    rig.bench
        .run_until(&rig.tasks, "the push to finish", |bench| {
            bench
                .view()
                .devices
                .iter()
                .any(|card| card.activity.is_none() && card.last_outcome.is_some())
        });
    let outcome = rig.bench.view().devices[0].last_outcome.clone();
    assert!(
        outcome.as_ref().is_some_and(|outcome| outcome.ok),
        "the push was never interrupted: {outcome:?}"
    );
    rig.tick_until("the move after the push", |rig| rig.on_lan());
    assert_eq!(rig.slot.borrow().lan_dials.len(), 1);
}

#[test]
fn a_lost_wifi_link_goes_back_through_lightplayer_app_at_five_seconds() {
    let mut rig = moved("dev000000mvwf000009", true, LOST_REBOOT);
    let relay_dials = rig.slot.borrow().relay_connects;
    rig.drop_lan();
    rig.tick_until("the editor to hold across the drop", |rig| {
        rig.bench.controller.lens_is_held()
    });
    rig.bench.clock.set(rig.bench.clock.get() + 4.0);
    rig.ticks(3);
    assert_eq!(
        rig.slot.borrow().relay_connects,
        relay_dials,
        "not before 5 s"
    );
    rig.bench.clock.set(rig.bench.clock.get() + 1.5);
    rig.tick_until("the board back through lightplayer.app", |rig| {
        rig.relay_bound()
    });
    assert_eq!(rig.slot.borrow().relay_connects, relay_dials + 1, "once");
    assert!(
        rig.slot.borrow().lan_forgets >= 1,
        "the LAN stops redialling"
    );
    assert_eq!(
        rig.said("Wi\u{2011}Fi dropped; back through lightplayer.app"),
        1
    );
    rig.tick_until("the editor to resume on the relay link", |rig| {
        !rig.bench.controller.lens_is_held()
    });
    assert!(
        rig.lens_endpoint()
            .is_some_and(|endpoint| endpoint.starts_with("relay:")),
        "{:?}",
        rig.lens_endpoint()
    );
    // Never moves again this page load.
    rig.ticks(100);
    assert_eq!(rig.slot.borrow().lan_dials.len(), 1);
}

#[test]
fn wifi_back_within_five_seconds_keeps_the_session_on_wifi() {
    let mut rig = moved("dev000000mvwf000010", false, LOST_REBOOT);
    let relay_dials = rig.slot.borrow().relay_connects;
    rig.drop_lan();
    rig.ticks(5);
    rig.bench.clock.set(rig.bench.clock.get() + 3.0);
    rig.lan_back();
    rig.tick_until("the board back on Wi‑Fi, and the move with it", |rig| {
        rig.on_lan()
            && matches!(
                rig.bench.controller.lan_moves_stage_for_test(board_key()),
                Some(crate::app::devices::lan_move::LanMoveStage::Moved { .. })
            )
    });
    rig.bench.clock.set(rig.bench.clock.get() + 10.0);
    rig.ticks(20);
    assert_eq!(
        rig.slot.borrow().relay_connects,
        relay_dials,
        "no relay dial"
    );
    assert_eq!(
        rig.said("Wi\u{2011}Fi dropped; back through lightplayer.app"),
        0
    );
}

/// The editor is closed here: the bench's lens io never says the page's
/// drop words (it reads the fake board, not the socket), and the 1000 is
/// read from them. In the page the lens's tap hands them to the fold
/// (`device_effects.rs`, `LensTapEvent::PortError`), as the pump does.
#[test]
fn a_wifi_link_taken_by_another_connection_does_not_go_back() {
    let mut rig = moved("dev000000mvwf000011", false, LOST_TAKEN);
    let relay_dials = rig.slot.borrow().relay_connects;
    rig.drop_lan();
    rig.ticks(5);
    rig.bench.clock.set(rig.bench.clock.get() + 8.0);
    rig.ticks(20);
    assert_eq!(
        rig.slot.borrow().relay_connects,
        relay_dials,
        "no relay dial"
    );
    assert!(rig.slot.borrow().lan_forgets >= 1, "nor a LAN redial");
    let said = rig
        .bench
        .controller
        .device_roster_view()
        .wifi_connects
        .get(&rig.id)
        .cloned()
        .expect("the card says why");
    assert!(said.busy, "someone else is connected: {said:?}");
}

#[test]
fn a_board_no_longer_listed_does_not_go_back() {
    let mut rig = moved("dev000000mvwf000012", true, LOST_REBOOT);
    let relay_dials = rig.slot.borrow().relay_connects;
    rig.drop_lan();
    rig.ticks(5);
    list_nothing(&mut rig);
    rig.bench.clock.set(rig.bench.clock.get() + 8.0);
    rig.ticks(20);
    assert_eq!(
        rig.slot.borrow().relay_connects,
        relay_dials,
        "no relay dial"
    );
}

// --- the rig ---------------------------------------------------------------

/// The board's one network slot, as its two doubles share it.
#[derive(Default)]
struct Slot {
    /// The key id the relay session holds the slot with.
    holder: Option<[u8; 16]>,
    relay_present: bool,
    relay_parked: bool,
    /// Closed by the board while parked: redialled by `unpark`.
    relay_closed: bool,
    /// The board closed the relay session and it was redialled, taking the
    /// slot back.
    relay_redials: u32,
    relay_connects: u32,
    lan_present: bool,
    /// Each LAN dial: its URL and the first key it presented.
    lan_dials: Vec<(String, [u8; 16])>,
    /// The socket's words the next LAN dial fails with.
    lan_refuse: Option<String>,
    /// The LAN answers but its session never becomes present.
    lan_vanishes: bool,
    lan_forgets: u32,
    /// Set to drop the LAN link that is open now, with these words.
    lan_lose: Rc<Cell<bool>>,
    /// The relay route the board closed for the LAN connection says
    /// nothing more.
    relay_silent: Rc<Cell<bool>>,
    lan_words: &'static str,
    events: Vec<String>,
    /// A session stopped being present, or became present: the page's
    /// presence edges (`disconnect`, `connect`), said on the next turn.
    left: bool,
    /// A drop's `disconnect` edge, said a turn after the drop itself: the
    /// page's link hears its socket close (the words) before the actor
    /// folds the edge's sweep.
    left_later: bool,
    arrived: bool,
}

struct RelayDouble {
    device: FakeEsp32Device,
    slot: Rc<RefCell<Slot>>,
    keys: crate::NetworkLinkKeys,
}

impl crate::RelayLinkSource for RelayDouble {
    fn present(&self) -> Vec<GrantedLink> {
        if !self.slot.borrow().relay_present {
            return Vec::new();
        }
        let url = relay_socket_url("https://lightplayer.app", BOARD);
        let info = lpa_link::providers::network_link::relay_link_info(&url).unwrap();
        vec![GrantedLink {
            link: Box::new(LazyLink::silenced_by(
                info.clone(),
                &self.device,
                Rc::clone(&self.slot.borrow().relay_silent),
            )),
            info,
        }]
    }

    fn forget(&self, _board: &str) -> DeviceTransportFuture<Result<(), String>> {
        let mut slot = self.slot.borrow_mut();
        slot.relay_present = false;
        slot.relay_parked = false;
        slot.events.push("relay forget".to_string());
        slot.left = true;
        Box::pin(core::future::ready(Ok(())))
    }

    fn client_io(
        &self,
        _board: &str,
        tap: Option<LensLineTap>,
    ) -> Result<Box<dyn lpa_client::ClientIo>, String> {
        let io = FakeDeviceIo::new(&self.device);
        Ok(Box::new(match tap {
            Some(tap) => io.with_tap(tap),
            None => io,
        }))
    }

    fn connect(&self, board: &str) -> DeviceTransportFuture<Result<(), String>> {
        let first = self
            .keys
            .keys_for(&relay_socket_url("https://lightplayer.app", board))
            .first()
            .map(|key| key.key_id);
        let mut slot = self.slot.borrow_mut();
        slot.relay_connects += 1;
        slot.relay_present = true;
        slot.relay_closed = false;
        slot.relay_silent.set(false);
        slot.holder = first;
        slot.lan_present = false;
        slot.events.push("relay connect".to_string());
        slot.arrived = true;
        Box::pin(core::future::ready(Ok(())))
    }

    fn park(&self, _board: &str) {
        let mut slot = self.slot.borrow_mut();
        slot.relay_parked = true;
        slot.events.push("relay park".to_string());
    }

    fn unpark(&self, _board: &str) {
        let mut slot = self.slot.borrow_mut();
        slot.relay_parked = false;
        slot.events.push("relay unpark".to_string());
        if std::mem::take(&mut slot.relay_closed) {
            slot.relay_silent.set(false);
            slot.relay_present = true;
        }
    }
}

struct LanDouble {
    device: FakeEsp32Device,
    slot: Rc<RefCell<Slot>>,
    keys: crate::NetworkLinkKeys,
}

impl crate::LanLinkSource for LanDouble {
    fn present(&self) -> Vec<GrantedLink> {
        let slot = self.slot.borrow();
        if !slot.lan_present {
            return Vec::new();
        }
        let info = lpa_link::providers::network_link::lan_link_info(ROW_URL);
        vec![GrantedLink {
            link: Box::new(LossyLink::new(
                Box::new(LazyLink::new(info.clone(), &self.device)),
                Rc::clone(&slot.lan_lose),
                slot.lan_words,
            )),
            info,
        }]
    }

    fn forget(&self, _url: &str) -> DeviceTransportFuture<Result<(), String>> {
        let mut slot = self.slot.borrow_mut();
        slot.lan_present = false;
        slot.lan_forgets += 1;
        slot.events.push("lan forget".to_string());
        slot.left = true;
        Box::pin(core::future::ready(Ok(())))
    }

    fn client_io(
        &self,
        _url: &str,
        tap: Option<LensLineTap>,
    ) -> Result<Box<dyn lpa_client::ClientIo>, String> {
        let io = FakeDeviceIo::new(&self.device);
        Ok(Box::new(match tap {
            Some(tap) => io.with_tap(tap),
            None => io,
        }))
    }

    fn connect(&self, url: &str) -> DeviceTransportFuture<Result<(), String>> {
        let first = self.keys.keys_for(url).first().map(|key| key.key_id);
        let mut slot = self.slot.borrow_mut();
        slot.events.push("lan dial".to_string());
        slot.lan_dials
            .push((url.to_string(), first.unwrap_or_default()));
        if let Some(words) = slot.lan_refuse.take() {
            return Box::pin(core::future::ready(Err(words)));
        }
        // The board holds its slot for the relay session's key: any other
        // is told busy at once.
        if slot.relay_present && !slot.relay_closed && slot.holder.is_some() && first != slot.holder
        {
            return Box::pin(core::future::ready(Err(
                "wi-fi link lost: the board closed the link (code 1013: try again later)"
                    .to_string(),
            )));
        }
        // The board hands the slot over and closes the relay route: a parked
        // session waits; any other is redialled and takes it straight back.
        if slot.relay_present && !slot.relay_closed {
            match slot.relay_parked {
                true => {
                    slot.relay_closed = true;
                    slot.relay_silent.set(true);
                }
                false => {
                    slot.relay_redials += 1;
                    slot.events.push("relay redial".to_string());
                    return Box::pin(core::future::ready(Ok(())));
                }
            }
        }
        slot.lan_present = !slot.lan_vanishes;
        slot.arrived |= slot.lan_present;
        Box::pin(core::future::ready(Ok(())))
    }
}

/// A link to the fake board that attaches to it only when it is opened.
///
/// The fake board answers on the last link made for it, and the sweep makes
/// (then discards) a fresh link for every session present: made eagerly, a
/// discarded one would take the board from the live one. The page's real
/// sessions have no such side effect.
///
/// It also goes silent while `silent` is set: the relay route the board
/// closed for the LAN connection (the fake board would otherwise hear two
/// sessions at once on its one stream; the real one serves one).
struct LazyLink {
    info: lpa_devices::link::LinkInfo,
    device: FakeEsp32Device,
    inner: Option<lpa_link::device_link::fake::FakeDeviceLink>,
    silent: Rc<Cell<bool>>,
}

impl LazyLink {
    fn new(info: lpa_devices::link::LinkInfo, device: &FakeEsp32Device) -> Self {
        Self::silenced_by(info, device, Rc::default())
    }

    fn silenced_by(
        info: lpa_devices::link::LinkInfo,
        device: &FakeEsp32Device,
        silent: Rc<Cell<bool>>,
    ) -> Self {
        Self {
            info,
            device: device.clone(),
            inner: None,
            silent,
        }
    }
}

impl lpa_devices::link::Link for LazyLink {
    fn info(&self) -> &lpa_devices::link::LinkInfo {
        &self.info
    }

    fn submit(&mut self, command: lpa_devices::link::LinkCommand) {
        if self.silent.get() {
            return;
        }
        let (info, device) = (&self.info, &self.device);
        self.inner
            .get_or_insert_with(|| bench_link(info.clone(), device))
            .submit(command);
    }

    fn poll_event(&mut self) -> Option<lpa_devices::link::LinkEvent> {
        if self.silent.get() {
            return None;
        }
        self.inner.as_mut()?.poll_event()
    }
}

/// A board running a library project, met over USB, unplugged and reached
/// through lightplayer.app (with the editor open on it when `editing`),
/// with both network halves installed and someone signed in.
struct Rig {
    bench: DeviceBench,
    tasks: TaskPool,
    slot: Rc<RefCell<Slot>>,
    id: crate::DeviceId,
}

impl Rig {
    fn new(uid: &str, editing: bool) -> Self {
        let device = relay_light_player(uid);
        let (mut bench, tasks) = identified(&device, &format!("usb-{uid}"));
        // The project goes over the cable before the network halves join.
        a_board_running_a_library_project(&mut bench, &tasks);
        let slot = Self::install(&mut bench, &device);
        sign_in(&mut bench, &tasks);
        let id = bench.view().devices[0].id;
        unplug(&mut bench, &tasks, id);
        bench.press_device(id, "connect-relay", OfferArgs::new());
        bench.run_until(&tasks, "the board through lightplayer.app", |bench| {
            bench
                .view()
                .devices
                .iter()
                .any(|card| card.id == id && card.activity.is_none() && card.state_label == "Ready")
        });
        bench.run_until(&tasks, "the board to say what it runs", |bench| {
            bench.view().devices.iter().any(|card| {
                card.id == id
                    && matches!(
                        card.loaded_project,
                        lpa_devices::view::LoadedProject::Running { .. }
                    )
            })
        });
        let mut rig = Self {
            bench,
            tasks,
            slot,
            id,
        };
        if editing {
            rig.bench.press_device(id, "edit", OfferArgs::new());
            rig.tick_until("the editor open through lightplayer.app", |rig| {
                rig.lens_endpoint()
                    .is_some_and(|endpoint| endpoint.starts_with("relay:"))
                    && !rig.bench.controller.view().panes.is_empty()
            });
        }
        rig
    }

    /// Install the network halves over one shared slot.
    fn install(bench: &mut DeviceBench, device: &FakeEsp32Device) -> Rc<RefCell<Slot>> {
        let slot = Rc::new(RefCell::new(Slot {
            lan_words: LOST_REBOOT,
            ..Default::default()
        }));
        let keys = bench.controller.network_link_keys();
        bench
            .controller
            .set_device_sim_transport(Rc::new(SimDeviceTransport::new(Rc::new(
                ScriptedSimSource {
                    device: sim_light_player(),
                    restarts: Rc::new(Cell::new(0)),
                    manifests: Rc::new(RefCell::new(Vec::new())),
                },
            ))));
        bench
            .controller
            .set_lan_transport(Rc::new(crate::LanDeviceTransport::new(Rc::new(
                LanDouble {
                    device: device.clone(),
                    slot: Rc::clone(&slot),
                    keys: keys.clone(),
                },
            ))));
        bench
            .controller
            .set_relay_transport(Rc::new(crate::RelayDeviceTransport::new(Rc::new(
                RelayDouble {
                    device: device.clone(),
                    slot: Rc::clone(&slot),
                    keys,
                },
            ))));
        slot
    }

    fn list_on_this_network(&mut self) {
        list(&mut self.bench, &self.tasks, BOARD, Some(ROW_LAN), true);
    }

    /// One actor turn: the bench's step, then the refresh tick's looks.
    /// Whether the editor was held after it.
    fn tick(&mut self) -> bool {
        self.bench.step(&self.tasks);
        // The page's presence edges, as `browser_websocket.js` says them.
        let (left, arrived) = {
            let mut slot = self.slot.borrow_mut();
            let left = std::mem::take(&mut slot.left);
            slot.left = std::mem::take(&mut slot.left_later);
            (left, std::mem::take(&mut slot.arrived))
        };
        if left {
            self.bench.controller.note_device_hotplug(
                crate::app::studio::studio_command::DeviceHotplug::Disconnected,
            );
        }
        if arrived {
            self.bench
                .controller
                .note_device_hotplug(crate::app::studio::studio_command::DeviceHotplug::Connected);
        }
        drive(self.bench.controller.try_pending_device_lens());
        drive(self.bench.controller.try_resume_held_lens());
        drive(self.bench.controller.drive_lan_moves());
        self.bench.controller.lens_is_held()
    }

    fn ticks(&mut self, n: usize) {
        for _ in 0..n {
            self.tick();
        }
    }

    /// Tick until `ready`; whether the editor was held, tick by tick.
    fn tick_until(&mut self, what: &str, ready: impl Fn(&Self) -> bool) -> Vec<bool> {
        let deadline = std::time::Instant::now() + REAL_TIME_LIMIT;
        let mut held = Vec::new();
        loop {
            held.push(self.tick());
            if ready(self) {
                return held;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "timed out waiting for {what}; slot: {:?}; roster: {:?}",
                self.slot.borrow().events,
                self.bench.view()
            );
        }
    }

    /// The board's own link is the LAN connection.
    fn on_lan(&self) -> bool {
        self.bench
            .controller
            .device_roster_view()
            .lan_links
            .get(&self.id)
            .is_some_and(|line| line.kind == crate::UiLinkKind::Wifi)
            && self.bench.view().devices.iter().any(|card| {
                card.id == self.id && card.activity.is_none() && card.state_label == "Ready"
            })
    }

    /// The board's own link is the relay's, ready.
    fn relay_bound(&self) -> bool {
        self.bench
            .controller
            .device_roster_view()
            .lan_links
            .get(&self.id)
            .is_some_and(|line| line.kind == crate::UiLinkKind::Relay)
            && self.bench.view().devices.iter().any(|card| {
                card.id == self.id && card.activity.is_none() && card.state_label == "Ready"
            })
    }

    /// The endpoint of the link the editor's session is on.
    fn lens_endpoint(&self) -> Option<String> {
        let link = self
            .bench
            .controller
            .runtime_pool_for_test()
            .attached_session()?
            .attachment()
            .link;
        let info = self
            .bench
            .controller
            .devices_for_test()
            .roster()
            .link_info(link)?;
        Some(info.endpoint.0.clone())
    }

    /// The board's Wi‑Fi socket drops, in the words the rig was made with;
    /// its session is not present while it would redial.
    fn drop_lan(&mut self) {
        let mut slot = self.slot.borrow_mut();
        slot.lan_present = false;
        slot.left_later = true;
        slot.lan_lose.set(true);
    }

    /// The Wi‑Fi session redialled and answered.
    fn lan_back(&mut self) {
        {
            let mut slot = self.slot.borrow_mut();
            slot.lan_present = true;
            // A link the lens held never polled its drop: the next one is
            // whole.
            slot.lan_lose.set(false);
        }
        self.bench
            .controller
            .note_device_hotplug(crate::app::studio::studio_command::DeviceHotplug::Connected);
    }

    /// How many log lines start with `start`.
    fn said(&self, start: &str) -> usize {
        self.bench
            .controller
            .logs()
            .iter()
            .filter(|entry| entry.message.starts_with(start))
            .count()
    }
}

/// A board moved onto Wi‑Fi (with the editor open on it when `editing`),
/// whose Wi‑Fi link will say `drop` when it drops.
fn moved(uid: &str, editing: bool, drop: &'static str) -> Rig {
    let mut rig = Rig::new(uid, editing);
    rig.slot.borrow_mut().lan_words = drop;
    rig.list_on_this_network();
    rig.tick_until("the move", |rig| {
        rig.on_lan() && !rig.slot.borrow().relay_present
    });
    rig
}

/// The account's list names no board.
fn list_nothing(rig: &mut Rig) {
    rig.bench
        .controller
        .set_account_boards(Some(crate::AccountBoardRows {
            at: 200.0,
            boards: Vec::new(),
        }));
    rig.bench.step(&rig.tasks);
}

/// Sign in: the account's keys reach core.
fn sign_in(bench: &mut DeviceBench, tasks: &TaskPool) {
    bench
        .controller
        .apply_access_command(crate::AccessCommand::AccountKeys(Some(
            crate::app::access::account_keys::tests::account(None),
        )));
    bench.step(tasks);
}

/// The relay board's MAC.
fn board_key() -> lpa_devices::BoardKey {
    lpa_devices::BoardKey::parse(BOARD).unwrap()
}
