//! Reaching a board over Wi‑Fi with no flag (the network-transport plan's
//! P01/P02), end to end through the real effects layer:
//!
//! - a board met over USB says it is on Wi‑Fi; Studio remembers where, in
//!   this browser's book; unplugged, its tile offers "Connect over Wi‑Fi";
//!   pressed, it comes back on a LAN link as the SAME device (merged by
//!   MAC), presenting the keys typed for that board at another address;
//!   Forget forgets the address with the board;
//! - a board never seen is reached by an address typed into the add slot;
//! - a connect that fails says why in plain words, where it was pressed.
//!
//! Every verb is pressed by its offer path, as the card and the app agent
//! press it.

use lpa_link::providers::network_link::LinkKeys;

use super::*;

/// The board's Wi‑Fi address, as its station reports it (made up).
const IP: &str = "192.168.4.100";
/// The socket Studio dials for it.
const URL: &str = "ws://192.168.4.100/link";
/// Its base MAC, and the same as a board key.
const MAC: &str = "a0:f2:62:87:b4:8e";
const KEY: &str = "a0f26287b48e";

/// A LAN board the test answers for: a connect makes its session present
/// (or fails with the socket's words the test set), and present sessions
/// are what discovery finds — the page's `browser_websocket.js`, in small.
struct ConnectableLanBoard {
    device: FakeEsp32Device,
    present: Rc<RefCell<Vec<String>>>,
    /// The socket's words the next connect fails with; `None` answers.
    refuse: Rc<RefCell<Option<String>>>,
    asked: Rc<RefCell<Vec<String>>>,
}

impl crate::LanLinkSource for ConnectableLanBoard {
    fn present(&self) -> Vec<GrantedLink> {
        self.present
            .borrow()
            .iter()
            .map(|url| {
                let info = lpa_link::providers::network_link::lan_link_info(url);
                GrantedLink {
                    link: Box::new(fake_device_link(info.clone(), &self.device)),
                    info,
                }
            })
            .collect()
    }

    fn forget(&self, url: &str) -> DeviceTransportFuture<Result<(), String>> {
        self.present.borrow_mut().retain(|present| present != url);
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
        self.asked.borrow_mut().push(url.to_string());
        let answer = match self.refuse.borrow_mut().take() {
            Some(words) => Err(words),
            None => {
                let mut present = self.present.borrow_mut();
                if !present.iter().any(|present| present == url) {
                    present.push(url.to_string());
                }
                Ok(())
            }
        };
        Box::pin(core::future::ready(answer))
    }
}

/// The LAN half and what the test reads back from it.
struct LanRig {
    refuse: Rc<RefCell<Option<String>>>,
    asked: Rc<RefCell<Vec<String>>>,
    /// Every stored form of the address book the web edge was handed.
    stored: Rc<RefCell<Vec<String>>>,
}

/// Install the shipped build's halves beside the bench's USB: a sim half
/// (none is created) and the LAN half over [`ConnectableLanBoard`]; and the
/// address book's web edge.
fn with_lan(bench: &mut DeviceBench, device: &FakeEsp32Device) -> LanRig {
    let rig = LanRig {
        refuse: Rc::default(),
        asked: Rc::default(),
        stored: Rc::default(),
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
        .set_lan_transport(Rc::new(crate::LanDeviceTransport::new(Rc::new(
            ConnectableLanBoard {
                device: device.clone(),
                present: Rc::default(),
                refuse: Rc::clone(&rig.refuse),
                asked: Rc::clone(&rig.asked),
            },
        ))));
    bench.controller.set_on_wifi_addresses({
        let stored = Rc::clone(&rig.stored);
        move |json| stored.borrow_mut().push(json.to_string())
    });
    rig
}

#[test]
fn a_board_met_over_usb_is_reached_over_wifi_after_unplug_as_the_same_device() {
    let device = joined_light_player("dev000000wificon01");
    let (mut bench, tasks) = identified(&device, "usb-wifi-connect");
    let rig = with_lan(&mut bench, &device);
    let target = bench.view().devices[0].id;
    let key = lpa_devices::BoardKey::parse(KEY).unwrap();

    // Over USB the board says it is on Wi‑Fi: the book learns where, and
    // the web edge is handed it to keep.
    bench.run_until(&tasks, "the board's address to be learned", |bench| {
        bench.controller.wifi_addresses().get(&key).is_some()
    });
    let learned = bench.controller.wifi_addresses().get(&key).unwrap().clone();
    assert_eq!(learned.ip, IP);
    assert_eq!(learned.host, "lp-b48e.local");
    assert!(
        rig.stored.borrow().last().unwrap().contains(IP),
        "{:?}",
        rig.stored.borrow()
    );
    // While the board is here, there is nothing to connect.
    let connect = bench.device_verb(target, "connect-wifi");
    bench.not_offered(&connect);

    // A password typed for this board at another address (it moved): the
    // keys are the board's, by MAC.
    let keys = bench.controller.network_link_keys();
    keys.alias("ws://10.9.9.9/link", KEY);
    keys.offer("ws://10.9.9.9/link", vec![typed_key()]);

    // Unplugged: the board is remembered, and its tile offers Wi‑Fi.
    bench.granted.set(false);
    bench
        .controller
        .note_device_hotplug(crate::app::studio::studio_command::DeviceHotplug::Disconnected);
    bench.run_until(&tasks, "the board to go offline", |bench| {
        bench
            .view()
            .devices
            .iter()
            .any(|card| card.id == target && card.status == crate::DeviceStatus::Offline)
    });
    let offer = bench.offered(&connect);
    assert!(offer.is_enabled());
    assert!(offer.consequence().is_routine());
    assert_eq!(offer.label(), "Connect over Wi\u{2011}Fi");
    assert!(offer.summary().contains(IP), "{}", offer.summary());

    bench
        .press(&connect, OfferArgs::new())
        .expect("the connect starts");
    bench.run_until(&tasks, "the board to come back over Wi‑Fi", |bench| {
        bench
            .view()
            .devices
            .iter()
            .any(|card| card.id == target && card.activity.is_none() && card.state_label == "Ready")
    });
    assert_eq!(rig.asked.borrow().as_slice(), [URL]);
    let roster = bench.view();
    assert_eq!(roster.devices.len(), 1, "one board, one card: {roster:?}");
    let device_now = bench.controller.device_roster_view();
    assert_eq!(
        device_now.lan_links[&target].url, URL,
        "the card says it is reached on Wi‑Fi"
    );
    assert!(
        device_now.wifi_connects.is_empty(),
        "a success says nothing"
    );
    bench.not_offered(&connect);
    // The keys typed for the board at its old address are presented here.
    assert_eq!(
        keys.keys_for(URL).first(),
        Some(&typed_key()),
        "the board's typed key leads at its new address"
    );

    // Forget takes the address with the board.
    let forget = bench.device_verb(target, "forget");
    bench
        .press_lasting(forget, OfferArgs::new())
        .expect("forget");
    bench.step(&tasks);
    assert!(bench.controller.wifi_addresses().get(&key).is_none());
    assert_eq!(rig.stored.borrow().last().map(String::as_str), Some("{}"));
}

#[test]
fn a_connect_that_fails_says_why_on_the_tile_and_can_be_pressed_again() {
    let device = joined_light_player("dev000000wificon02");
    let (mut bench, tasks) = identified(&device, "usb-wifi-connect-2");
    let rig = with_lan(&mut bench, &device);
    let target = bench.view().devices[0].id;
    let key = lpa_devices::BoardKey::parse(KEY).unwrap();
    bench.run_until(&tasks, "the board's address to be learned", |bench| {
        bench.controller.wifi_addresses().get(&key).is_some()
    });
    bench.granted.set(false);
    bench
        .controller
        .note_device_hotplug(crate::app::studio::studio_command::DeviceHotplug::Disconnected);
    bench.run_until(&tasks, "the board to go offline", |bench| {
        bench
            .view()
            .devices
            .iter()
            .any(|card| card.id == target && card.status == crate::DeviceStatus::Offline)
    });

    *rig.refuse.borrow_mut() = Some("wi-fi connect timed out after 10 s".to_string());
    let connect = bench.device_verb(target, "connect-wifi");
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
    assert_eq!(
        said.error.as_deref(),
        Some("Couldn't reach the board at 192.168.4.100. Is it on this network?")
    );
    let tile = crate::split_roster(&roster).remembered.remove(0);
    assert_eq!(tile.wifi_connect.as_ref(), Some(said), "the tile says it");
    assert!(
        bench.offered(&connect).is_enabled(),
        "a failed connect can be pressed again"
    );

    // Pressed again, it reaches the board, and the failure goes.
    bench
        .press(&connect, OfferArgs::new())
        .expect("the connect starts again");
    bench.run_until(&tasks, "the board to come back over Wi‑Fi", |bench| {
        bench
            .view()
            .devices
            .iter()
            .any(|card| card.id == target && card.activity.is_none() && card.state_label == "Ready")
    });
    assert!(
        bench
            .controller
            .device_roster_view()
            .wifi_connects
            .is_empty()
    );
}

#[test]
fn a_board_never_seen_is_reached_by_its_address_and_remembered_by_its_mac() {
    let device = joined_light_player("dev000000wificon03");
    // No USB grant: the board is only ever reached over the LAN.
    let (mut bench, tasks) = DeviceBench::build(&device, "usb-unused", false, false);
    let rig = with_lan(&mut bench, &device);
    let add = crate::OfferPath::devices().child("connect-wifi-address");
    assert_eq!(
        bench.offered(&add).label(),
        "Connect a board on Wi\u{2011}Fi"
    );

    // Not an address: refused by the offer, with its reason, before any
    // socket.
    let refused = bench
        .offered(&add)
        .press(&OfferArgs::new().with(crate::WIFI_ADDRESS_PARAM, "http://192.168.4.100/"))
        .expect_err("not a board's address")
        .to_string();
    assert!(refused.contains("not http://"), "{refused}");

    // The board is busy with another connection: said where it was typed.
    *rig.refuse.borrow_mut() =
        Some("wi-fi link lost: the board closed the link (code 1013: try again later)".to_string());
    bench
        .press(&add, OfferArgs::new().with(crate::WIFI_ADDRESS_PARAM, IP))
        .expect("the connect starts");
    bench.run_until(&tasks, "the busy board to say so", |bench| {
        bench
            .controller
            .device_roster_view()
            .wifi_address_connect
            .is_some_and(|connect| connect.error.is_some())
    });
    assert_eq!(
        bench
            .controller
            .device_roster_view()
            .wifi_address_connect
            .unwrap()
            .error
            .as_deref(),
        Some(crate::WIFI_BUSY_WORDS)
    );

    // Typed again once it is free: it connects, identifies, and its status
    // teaches the book its address, by its MAC.
    bench
        .press(&add, OfferArgs::new().with(crate::WIFI_ADDRESS_PARAM, IP))
        .expect("the connect starts");
    bench.run_until(&tasks, "the board to identify over Wi‑Fi", |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.activity.is_none() && card.state_label == "Ready")
    });
    assert_eq!(rig.asked.borrow().as_slice(), [URL, URL]);
    assert!(
        bench
            .controller
            .device_roster_view()
            .wifi_address_connect
            .is_none()
    );
    let key = lpa_devices::BoardKey::parse(KEY).unwrap();
    bench.run_until(&tasks, "the board's address to be learned", |bench| {
        bench.controller.wifi_addresses().get(&key).is_some()
    });
    assert_eq!(bench.controller.wifi_addresses().get(&key).unwrap().ip, IP);
}

/// A LightPlayer board joined to a network: its station says it is
/// connected at [`IP`] (the firmware's station, scripted).
fn joined_light_player(uid: &str) -> FakeEsp32Device {
    FakeEsp32Device::new(
        FakeDeviceScript::new(FakeBootState::LightPlayer(
            FakeLightPlayerState::new()
                .with_identity(FakeDeviceIdentity::new(uid, "Bench board"))
                .with_base_mac(MAC)
                .with_heartbeat_interval(Duration::from_millis(20)),
        ))
        .with_wifi_station(lpa_link::providers::fake_device::FakeWifiStation {
            state: || lpc_wire::StationState::Connected {
                ssid: "lp-walk-net".to_string(),
                ip: IP.to_string(),
                rssi: -50,
                host: "lp-b48e.local".to_string(),
            },
            scan: || lpc_wire::NetworkScan::Heard(Vec::new()),
        }),
    )
}

/// A key typed for this board (made up).
fn typed_key() -> lpa_link::providers::network_link::LinkKey {
    lpa_link::providers::network_link::LinkKey {
        key_id: [7; 16],
        psk: [7; 32],
    }
}
