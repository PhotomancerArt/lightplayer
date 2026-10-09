//! "Connect through lightplayer.app" on a remembered board's tile (the
//! network transport's PR C: the relay on for everyone, one offer, no list),
//! end to end through the real effects layer:
//!
//! - a board met over USB, then unplugged, is offered
//!   `devices/<board>/connect-relay` while someone is signed in, and not
//!   while nobody is; pressed, it dials `relay:<mac>` and the board comes
//!   back as the SAME device (merged by MAC), its card saying "Wi‑Fi via
//!   lightplayer.app";
//! - a board the relay says is not online says so on its tile, in plain
//!   words, and can be pressed again.
//!
//! Every verb is pressed by its offer path, as the card and the app agent
//! press it.

use crate::app::access::account_keys::tests::account;

use super::*;

/// The board's base MAC, and the same as its relay id.
const MAC: &str = "a0:f2:62:87:b4:9a";
const BOARD: &str = "a0f26287b49a";

/// How `browser_websocket.js` says the relay turned a connect away because
/// the board is not connected to it.
const OFFLINE: &str = "relay link lost: the board closed the link (code 4404: board-offline)";

/// A relay board the test answers for: a connect makes its session present
/// (or fails with the socket's words the test set), and present sessions
/// are what discovery finds — `browser_relay_source.rs`, in small.
struct ConnectableRelayBoard {
    device: FakeEsp32Device,
    present: Rc<RefCell<Vec<String>>>,
    /// The socket's words the next connect fails with; `None` answers.
    refuse: Rc<RefCell<Option<String>>>,
    asked: Rc<RefCell<Vec<String>>>,
}

impl crate::RelayLinkSource for ConnectableRelayBoard {
    fn present(&self) -> Vec<GrantedLink> {
        self.present
            .borrow()
            .iter()
            .map(|board| {
                let url = lpa_link::providers::network_link::relay_socket_url(
                    "https://lightplayer.app",
                    board,
                );
                let info = lpa_link::providers::network_link::relay_link_info(&url)
                    .expect("a relay browser leg");
                GrantedLink {
                    link: Box::new(bench_link(info.clone(), &self.device)),
                    info,
                }
            })
            .collect()
    }

    fn forget(&self, board: &str) -> DeviceTransportFuture<Result<(), String>> {
        self.present.borrow_mut().retain(|present| present != board);
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
        self.asked.borrow_mut().push(board.to_string());
        let answer = match self.refuse.borrow_mut().take() {
            Some(words) => Err(words),
            None => {
                let mut present = self.present.borrow_mut();
                if !present.iter().any(|present| present == board) {
                    present.push(board.to_string());
                }
                Ok(())
            }
        };
        Box::pin(core::future::ready(answer))
    }
}

/// The relay half and what the test reads back from it.
struct RelayRig {
    refuse: Rc<RefCell<Option<String>>>,
    asked: Rc<RefCell<Vec<String>>>,
}

/// Install the shipped build's halves beside the bench's USB: a sim half
/// (none is created) and the relay half over [`ConnectableRelayBoard`].
fn with_relay(bench: &mut DeviceBench, device: &FakeEsp32Device) -> RelayRig {
    let rig = RelayRig {
        refuse: Rc::default(),
        asked: Rc::default(),
    };
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
        .set_relay_transport(Rc::new(crate::RelayDeviceTransport::new(Rc::new(
            ConnectableRelayBoard {
                device: device.clone(),
                present: Rc::default(),
                refuse: Rc::clone(&rig.refuse),
                asked: Rc::clone(&rig.asked),
            },
        ))));
    rig
}

#[test]
fn a_board_met_over_usb_is_reached_through_lightplayer_app_after_unplug_as_the_same_device() {
    let device = relay_light_player("dev00000relaycon01");
    let (mut bench, tasks) = identified(&device, "usb-relay-connect");
    let rig = with_relay(&mut bench, &device);
    let target = bench.view().devices[0].id;
    let connect = bench.device_verb(target, "connect-relay");
    // While the board is here, there is nothing to connect.
    bench.not_offered(&connect);

    unplug(&mut bench, &tasks, target);
    // Signed out: nothing in this browser opens a board through the relay,
    // so the verb is not offered at all.
    bench.not_offered(&connect);

    // Signed in: the account's key is what opens it.
    bench
        .controller
        .apply_access_command(crate::AccessCommand::AccountKeys(Some(account(None))));
    bench.step(&tasks);
    let offer = bench.offered(&connect);
    assert!(offer.is_enabled());
    assert!(offer.consequence().is_routine());
    assert_eq!(offer.label(), "Connect through lightplayer.app");
    // The app agent reads the same verb.
    let readout = bench.controller.app_agent_readout_for_test().render();
    assert!(readout.contains("connect-relay"), "{readout}");

    bench
        .press(&connect, OfferArgs::new())
        .expect("the connect starts");
    bench.run_until(
        &tasks,
        "the board to come back through lightplayer.app",
        |bench| {
            bench.view().devices.iter().any(|card| {
                card.id == target && card.activity.is_none() && card.state_label == "Ready"
            })
        },
    );
    assert_eq!(rig.asked.borrow().as_slice(), [BOARD]);
    let roster = bench.view();
    assert_eq!(roster.devices.len(), 1, "one board, one card: {roster:?}");
    let device_now = bench.controller.device_roster_view();
    let line = &device_now.lan_links[&target];
    assert_eq!(line.kind, crate::UiLinkKind::Relay);
    assert_eq!(
        line.line(),
        "Wi\u{2011}Fi via lightplayer.app",
        "the card says how it is reached"
    );
    assert!(
        device_now.wifi_connects.is_empty(),
        "a success says nothing"
    );
    bench.not_offered(&connect);
}

#[test]
fn a_board_the_relay_says_is_offline_says_so_on_its_tile_and_can_be_pressed_again() {
    let device = relay_light_player("dev00000relaycon02");
    let (mut bench, tasks) = identified(&device, "usb-relay-connect-2");
    let rig = with_relay(&mut bench, &device);
    bench
        .controller
        .apply_access_command(crate::AccessCommand::AccountKeys(Some(account(None))));
    let target = bench.view().devices[0].id;
    unplug(&mut bench, &tasks, target);

    *rig.refuse.borrow_mut() = Some(OFFLINE.to_string());
    let connect = bench.device_verb(target, "connect-relay");
    bench
        .press(&connect, OfferArgs::new())
        .expect("the connect starts");
    bench.run_until(&tasks, "the connect to fail", |bench| {
        bench
            .controller
            .device_roster_view()
            .wifi_connects
            .get(&target)
            .is_some_and(|connect| connect.error.is_some())
    });
    let roster = bench.controller.device_roster_view();
    let said = &roster.wifi_connects[&target];
    assert!(said.through_relay);
    assert_eq!(said.error.as_deref(), Some(crate::RELAY_OFFLINE_WORDS));
    let tile = crate::split_roster(&roster).remembered.remove(0);
    assert_eq!(tile.wifi_connect.as_ref(), Some(said), "the tile says it");
    assert!(
        bench.offered(&connect).is_enabled(),
        "a failed connect can be pressed again"
    );

    // The board came online: pressed again, it is reached, and the words go.
    bench
        .press(&connect, OfferArgs::new())
        .expect("the connect starts again");
    bench.run_until(
        &tasks,
        "the board to come back through lightplayer.app",
        |bench| {
            bench.view().devices.iter().any(|card| {
                card.id == target && card.activity.is_none() && card.state_label == "Ready"
            })
        },
    );
    assert!(
        bench
            .controller
            .device_roster_view()
            .wifi_connects
            .is_empty()
    );
}

/// Pull the bench's USB cable and wait for the card to go offline.
fn unplug(bench: &mut DeviceBench, tasks: &TaskPool, target: crate::DeviceId) {
    bench.granted.set(false);
    bench
        .controller
        .note_device_hotplug(crate::app::studio::studio_command::DeviceHotplug::Disconnected);
    bench.run_until(tasks, "the board to go offline", |bench| {
        bench
            .view()
            .devices
            .iter()
            .any(|card| card.id == target && card.status == crate::DeviceStatus::Offline)
    });
}

/// A LightPlayer board with a base MAC (its relay id).
fn relay_light_player(uid: &str) -> FakeEsp32Device {
    FakeEsp32Device::new(FakeDeviceScript::new(FakeBootState::LightPlayer(
        FakeLightPlayerState::new()
            .with_identity(FakeDeviceIdentity::new(uid, "Relay board"))
            .with_base_mac(MAC)
            .with_heartbeat_interval(Duration::from_millis(20)),
    )))
}
