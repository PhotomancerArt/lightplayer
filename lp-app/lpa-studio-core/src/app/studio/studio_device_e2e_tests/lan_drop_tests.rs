//! A LAN link that closes and redials, end to end: the board's socket drops
//! (`wi-fi link lost: …`, as `browser_websocket.js` says it), the page's
//! session redials on its own, and the card must come back to "Ready" on
//! the new link. PR C's emulated walk found it stuck on "Attached — not
//! listening · quiet" while the board held a live session from the page:
//! the loss closed the link but never let it go, so the redialled session
//! found its endpoint still attached and nothing opened it again.

use super::*;

/// The board's link socket (a made-up address).
const URL: &str = "ws://192.168.4.100/link";

/// How `browser_websocket.js` reports a socket the board closed.
const LOST: &str = "wi-fi link lost: the board closed the link (code 1001: rebooting)";

/// A LAN board whose socket the test can drop. Its session stays present
/// throughout, as the page's does when the redial is quick: the drop is
/// only ever heard on the link itself.
struct DroppableLanBoard {
    device: FakeEsp32Device,
    /// Set to drop the link that is open now.
    lose: Rc<Cell<bool>>,
}

impl crate::LanLinkSource for DroppableLanBoard {
    fn present(&self) -> Vec<GrantedLink> {
        let info = lpa_link::providers::network_link::lan_link_info(URL);
        vec![GrantedLink {
            link: Box::new(LossyLink {
                inner: Box::new(fake_device_link(info.clone(), &self.device)),
                lose: Rc::clone(&self.lose),
                open: false,
                lost: VecDeque::new(),
                dead: false,
            }),
            info,
        }]
    }

    fn forget(&self, _url: &str) -> DeviceTransportFuture<Result<(), String>> {
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
}

/// The fake board's link, which drops the way `BrowserWebsocketLink` does:
/// the loss as an error, then `Closed`, then nothing more.
struct LossyLink {
    inner: Box<dyn lpa_devices::link::Link>,
    lose: Rc<Cell<bool>>,
    open: bool,
    lost: VecDeque<lpa_devices::link::LinkEvent>,
    dead: bool,
}

impl lpa_devices::link::Link for LossyLink {
    fn info(&self) -> &lpa_devices::link::LinkInfo {
        self.inner.info()
    }

    fn submit(&mut self, command: lpa_devices::link::LinkCommand) {
        if !self.dead {
            self.inner.submit(command);
        }
    }

    fn poll_event(&mut self) -> Option<lpa_devices::link::LinkEvent> {
        if self.open && self.lose.replace(false) {
            self.open = false;
            self.dead = true;
            self.lost
                .push_back(lpa_devices::link::LinkEvent::Error(LOST.to_string()));
            self.lost.push_back(lpa_devices::link::LinkEvent::Closed {
                reason: LOST.to_string(),
            });
        }
        if let Some(event) = self.lost.pop_front() {
            return Some(event);
        }
        if self.dead {
            return None;
        }
        let event = self.inner.poll_event()?;
        if matches!(event, lpa_devices::link::LinkEvent::Opened { .. }) {
            self.open = true;
        }
        Some(event)
    }
}

#[test]
fn a_lan_link_that_drops_and_redials_comes_back_ready() {
    let device = FakeEsp32Device::new(FakeDeviceScript::new(FakeBootState::LightPlayer(
        FakeLightPlayerState::new()
            .with_identity(FakeDeviceIdentity::new("dev000000landrop1", "Bench board"))
            .with_base_mac("a0:f2:62:87:b4:8e")
            .with_heartbeat_interval(Duration::from_millis(20)),
    )));
    // No USB grant: the board is reached over the LAN only.
    let (mut bench, tasks) = DeviceBench::build(&device, "usb-unused", false, false);
    let lose = Rc::new(Cell::new(false));
    // The shipped build's shape: the LAN half joins the composite beside a
    // sim half (no sim is ever created here).
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
            DroppableLanBoard {
                device: device.clone(),
                lose: Rc::clone(&lose),
            },
        ))));
    bench
        .controller
        .note_device_hotplug(crate::app::studio::studio_command::DeviceHotplug::Connected);
    wait_for_ready(&mut bench, &tasks, "the board to identify over Wi-Fi");

    // The board closes its socket (a reboot); the page's session redials at
    // once and says so on its presence edge.
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
        "the card to come back ready on the new link",
    );
    assert_eq!(bench.view().devices.len(), 1, "one board, one card");
}

fn wait_for_ready(bench: &mut DeviceBench, tasks: &TaskPool, what: &str) {
    bench.run_until(tasks, what, |bench| {
        bench
            .view()
            .devices
            .first()
            .is_some_and(|card| card.activity.is_none() && card.state_label == "Ready")
    });
}
