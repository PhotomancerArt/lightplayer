//! A board reached through lightplayer.app's relay, end to end (the network
//! transport's P05, behind `?relay=1`): the `relay:` endpoint routes to the
//! relay half, the board identifies over it like a LAN board, its card says
//! firmware needs USB, the access layer never adds a key over it (the
//! internet is not a cable), and
//! when the relay closes its leg (`relay link lost: …`) the card hears the
//! departure and comes back ready on the next session.

use super::ble_drop_tests::{BENCH_PASSWORD, locked_store_file};
use super::lan_drop_tests::{LossyLink, wait_for_ready};
use super::*;

/// The board's id at the relay: its MAC as twelve hex digits.
const BOARD: &str = "a0f26287b48f";

/// How `browser_websocket.js` reports a relay leg the hub closed (the board
/// reconnected to the relay and dropped its old sessions).
const LOST: &str = "relay link lost: the board closed the link (code 4410: board-gone)";

/// A board through the relay whose leg the test can drop.
struct DroppableRelayBoard {
    device: FakeEsp32Device,
    lose: Rc<Cell<bool>>,
}

impl crate::RelayLinkSource for DroppableRelayBoard {
    fn present(&self) -> Vec<GrantedLink> {
        let url =
            lpa_link::providers::network_link::relay_socket_url("https://lightplayer.app", BOARD);
        let info =
            lpa_link::providers::network_link::relay_link_info(&url).expect("a relay browser leg");
        vec![GrantedLink {
            link: Box::new(LossyLink::new(
                Box::new(bench_link(info.clone(), &self.device)),
                Rc::clone(&self.lose),
                LOST,
            )),
            info,
        }]
    }

    fn forget(&self, _board: &str) -> DeviceTransportFuture<Result<(), String>> {
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

    fn connect(&self, _board: &str) -> DeviceTransportFuture<Result<(), String>> {
        Box::pin(core::future::ready(Ok(())))
    }
}

#[test]
fn a_board_through_the_relay_identifies_refuses_firmware_and_comes_back_after_a_drop() {
    let device = FakeEsp32Device::new(FakeDeviceScript::new(FakeBootState::LightPlayer(
        FakeLightPlayerState::new()
            .with_identity(FakeDeviceIdentity::new("dev00000000relay1", "Relay board"))
            .with_base_mac("a0:f2:62:87:b4:8f")
            .with_heartbeat_interval(Duration::from_millis(20)),
    )));
    // No USB grant: the board is reached through the relay only.
    let (mut bench, tasks) = DeviceBench::build(&device, "usb-unused", false, false);
    let lose = Rc::new(Cell::new(false));
    bench
        .controller
        .set_device_sim_transport(Rc::new(SimDeviceTransport::new(Rc::new(
            ScriptedSimSource {
                device: sim_light_player(),
                restarts: Rc::new(Cell::new(0)),
                manifests: Rc::new(RefCell::new(Vec::new())),
            },
        ))));
    assert!(
        !bench.controller.reaches_relay(),
        "no relay half without ?relay=1"
    );
    bench
        .controller
        .set_relay_transport(Rc::new(crate::RelayDeviceTransport::new(Rc::new(
            DroppableRelayBoard {
                device: device.clone(),
                lose: Rc::clone(&lose),
            },
        ))));
    assert!(bench.controller.reaches_relay());
    bench
        .controller
        .note_device_hotplug(crate::app::studio::studio_command::DeviceHotplug::Connected);
    wait_for_ready(
        &mut bench,
        &tasks,
        "the board to identify through the relay",
    );

    let card = bench.view().devices[0].clone();
    assert!(
        card.firmware_blocked.is_some(),
        "firmware needs USB through the relay"
    );
    assert!(
        bench.controller.view().access_added.is_none(),
        "no key is added over the internet"
    );

    // The relay closes the leg (the board reconnected to the relay); the page
    // redials and says so on its presence edge.
    lose.set(true);
    bench.run_until(&tasks, "the card to hear the drop", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.state_label != "Ready")
    });
    bench
        .controller
        .note_device_hotplug(crate::app::studio::studio_command::DeviceHotplug::Connected);
    wait_for_ready(
        &mut bench,
        &tasks,
        "the card to come back ready on the next session",
    );
    assert_eq!(bench.view().devices.len(), 1, "one board, one card");
}

/// ND7: through the relay Studio presents held keys only — no remembered or
/// typed password. A locked board none of this browser's keys opens is not
/// logged in to over the relay, even with its password remembered (over
/// Bluetooth that password unlocks it by itself: `ble_drop_tests`), and no
/// sheet asks for one.
#[test]
fn a_locked_board_through_the_relay_is_never_logged_in_to_with_a_password() {
    let device = FakeEsp32Device::new(FakeDeviceScript::new(FakeBootState::LightPlayer(
        FakeLightPlayerState::new()
            .with_identity(FakeDeviceIdentity::new("dev00000000relay2", "Relay board"))
            .with_base_mac("a0:f2:62:87:b4:8f")
            .with_heartbeat_interval(Duration::from_millis(20))
            .with_untrusted_link()
            .with_root_files(vec![locked_store_file()]),
    )));
    let (mut bench, tasks) = DeviceBench::build(&device, "usb-unused", false, false);
    let mut remembered = crate::app::access::remembered_passwords::RememberedPasswords::default();
    remembered.remember(BENCH_PASSWORD, 1.0);
    bench
        .controller
        .apply_access_command(crate::AccessCommand::MemoryLoaded {
            passwords_json: Some(remembered.to_json()),
            devices_json: None,
            browser_json: None,
            account_json: None,
        });
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
            DroppableRelayBoard {
                device: device.clone(),
                lose: Rc::new(Cell::new(false)),
            },
        ))));
    bench
        .controller
        .note_device_hotplug(crate::app::studio::studio_command::DeviceHotplug::Connected);
    bench.run_until(
        &tasks,
        "the board to say hello through the relay",
        |bench| !bench.view().devices.is_empty(),
    );
    // Long enough for the automatic unlock a Bluetooth link gets.
    for _ in 0..200 {
        bench.step(&tasks);
    }
    let card = bench.view().devices[0].id;
    let line = bench
        .controller
        .device_roster_view()
        .access
        .get(&card)
        .and_then(|access| access.line.clone());
    assert_ne!(
        line.as_deref(),
        Some("Unlocked by bench password"),
        "no password goes through the relay"
    );
    assert!(
        bench.controller.view().login_prompt.is_none(),
        "no sheet asks for one"
    );
}
