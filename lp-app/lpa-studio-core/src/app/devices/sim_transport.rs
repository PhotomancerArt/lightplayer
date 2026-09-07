//! [`DeviceTransport`] over the sims this tab has powered on.
//!
//! The serial transport answers "which ports has this origin been granted?".
//! This one answers a question with a different shape — "which sims are
//! running?" — because a sim is not discovered, it is **made**. A picker mints
//! a record; powering it on is what puts a link behind it. The
//! [`DeviceTransport`] trait is the same either way, which is the point: the
//! effects layer, the roster and the card cannot tell the two apart.
//!
//! # The platform lives behind one seam
//!
//! What a running sim actually IS — a `fw-browser` worker in the browser, a
//! scripted fake in the host e2e bench — arrives through
//! [`SimLinkSource`]. That seam is what lets the whole of this file, and
//! therefore power on/off, the effect vocabulary and the borrow discipline,
//! be `just test`-covered on the host while the worker half stays wasm-only
//! (the plan's validation strategy). It is a test double for the transport,
//! not a second product path.
//!
//! # What the effect vocabulary means on a sim
//!
//! Every verb the card offers still runs, still borrows the wire
//! exclusively, and still ends with a summary — but the summary says what
//! actually happened rather than borrowing silicon's story:
//!
//! | effect | on a sim |
//! |---|---|
//! | Flash firmware | nothing is written: a sim runs the build it was started with. The runtime restarts, so the card sees a fresh boot. |
//! | Factory reset (erase) | nothing is erased: a sim's storage is memory and goes when the runtime does. The runtime restarts. |
//! | Write the board manifest | the next runtime wears it, then the runtime restarts — the same "effective next boot" a `/hardware.json` write has on silicon. |
//! | Push / remove a project | the REAL `lpa-client` conversation, over the sim's own protocol channel. Nothing is scripted. |
//!
//! No flash reports a probed MAC or a chip name. Studio minted this device's
//! identity and the sim reports it in its hello; claiming a preflight read it
//! out of efuse would be a fact about something that never happened, and the
//! fold's rule is that facts are stated when reported.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use lpa_devices::link::LinkInfo;

use super::device_transport::{
    DeviceEffectCall, DeviceEffectFacts, DeviceEffectProgress, DeviceTransport,
    DeviceTransportFuture, GrantedLink, LensLineTap,
};
use super::sim_record::uid_from_sim_endpoint;

/// One sim, as the transport needs to know it.
///
/// Everything here comes from the device's registry row and its sidecar; the
/// transport reads none of them itself (sans-IO), it is told.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SimSession {
    /// The derived uid — the registry key, and the sim's endpoint.
    pub uid: String,
    /// The hardware target this sim runs (a board id, or Desktop).
    pub target: String,
    /// What the card calls it, for the link's label.
    pub display_name: String,
    /// The minted base MAC the runtime reports as its identity.
    pub base_mac: String,
}

/// A handle on ONE running sim's runtime: what an effect does to it, as
/// opposed to what the model does to its link.
///
/// Shared with the link, not a replacement for it: a restart must leave the
/// effects layer holding the same link it borrowed, or every effect that
/// ends in one would hand the roster a different port. The channel is the
/// link's channel too, which is what makes the exclusive borrow mean
/// something — the pump is paused, so an io built here is the only reader
/// for as long as the conversation runs.
pub trait SimRuntimeControl {
    /// Throw the runtime away and start a fresh one.
    fn restart(&self) -> DeviceTransportFuture<Result<(), String>>;

    /// Replace the hardware manifest the NEXT runtime wears.
    fn set_hardware_manifest(&self, manifest_json: String);

    /// An `lpa-client` io on this runtime's protocol channel, for the
    /// exclusive-borrow conversations. `tap` receives every whole line the
    /// io drains, so the fold keeps hearing the sim while a conversation
    /// owns the channel.
    fn client_io(&self, tap: Option<LensLineTap>) -> Result<Box<dyn lpa_client::ClientIo>, String>;
}

/// A powered-on sim's live backing.
pub struct SimBacking {
    /// The closed link, handed to the effects layer by the next discovery.
    pub link: GrantedLink,
    pub control: Rc<dyn SimRuntimeControl>,
}

/// Where a running sim comes from on this platform.
pub trait SimLinkSource {
    /// Start `session`'s runtime backing. The link inside is CLOSED: powering
    /// a sim on is minting its port, and opening it stays the model's
    /// decision, exactly as it is for a granted serial port.
    fn open(&self, session: &SimSession) -> Result<SimBacking, String>;
}

/// The sims this tab has powered on.
pub struct SimDeviceTransport {
    source: Rc<dyn SimLinkSource>,
    /// By uid, so power on/off and endpoint routing agree by construction.
    powered: RefCell<BTreeMap<String, PoweredSim>>,
}

struct PoweredSim {
    control: Rc<dyn SimRuntimeControl>,
    /// The link, until a discovery hands it to the effects layer. `None`
    /// afterwards: a sim is not re-discoverable while it is already routed,
    /// and minting a second link for one runtime would give the roster two
    /// cards for one device.
    link: Option<GrantedLink>,
}

impl SimDeviceTransport {
    /// A transport serving sims from `source`.
    pub fn new(source: Rc<dyn SimLinkSource>) -> Self {
        Self {
            source,
            powered: RefCell::new(BTreeMap::new()),
        }
    }

    /// Power a sim on: start its runtime backing and offer its link to the
    /// next granted-port sweep.
    ///
    /// Idempotent by uid — powering on something already on is not an error
    /// and must not mint a second runtime.
    pub fn power_on(&self, session: SimSession) -> Result<(), String> {
        if self.powered.borrow().contains_key(&session.uid) {
            return Ok(());
        }
        let backing = self.source.open(&session)?;
        self.powered.borrow_mut().insert(
            session.uid,
            PoweredSim {
                control: backing.control,
                link: Some(backing.link),
            },
        );
        Ok(())
    }

    /// Power a sim off: drop the backing, which is what stops the runtime.
    /// `false` when it was not on — the goal state, not an error.
    pub fn power_off(&self, uid: &str) -> bool {
        self.powered.borrow_mut().remove(uid).is_some()
    }

    /// Whether this sim is running right now.
    pub fn is_powered(&self, uid: &str) -> bool {
        self.powered.borrow().contains_key(uid)
    }

    /// The uids of every running sim.
    pub fn powered_uids(&self) -> Vec<String> {
        self.powered.borrow().keys().cloned().collect()
    }

    /// The runtime control behind a link's endpoint, when it is one of ours.
    fn control_at(&self, info: &LinkInfo) -> Option<Rc<dyn SimRuntimeControl>> {
        let uid = uid_from_sim_endpoint(&info.endpoint.0)?;
        self.powered
            .borrow()
            .get(uid)
            .map(|powered| Rc::clone(&powered.control))
    }
}

impl DeviceTransport for SimDeviceTransport {
    fn label(&self) -> &'static str {
        "sim"
    }

    fn discover_granted(&self) -> DeviceTransportFuture<Result<Vec<GrantedLink>, String>> {
        let links: Vec<GrantedLink> = self
            .powered
            .borrow_mut()
            .values_mut()
            .filter_map(|powered| powered.link.take())
            .collect();
        Box::pin(core::future::ready(Ok(links)))
    }

    fn request_grant(&self) -> DeviceTransportFuture<Result<Option<GrantedLink>, String>> {
        // Not "no sims available" (`Ok(None)`, which reads as a dismissed
        // chooser) but a refusal with the way in: a sim is created, and the
        // browser's port chooser has nothing to say about one.
        Box::pin(core::future::ready(Err(
            "a sim is created from the Devices page, not from the browser's port chooser"
                .to_string(),
        )))
    }

    fn revoke_grant(&self, info: LinkInfo) -> DeviceTransportFuture<Result<(), String>> {
        // Handing a sim's "grant" back is powering it off: there is no
        // permission to return, only a runtime to stop.
        if let Some(uid) = uid_from_sim_endpoint(&info.endpoint.0) {
            self.power_off(uid);
        }
        Box::pin(core::future::ready(Ok(())))
    }

    fn run_effect(
        &self,
        info: LinkInfo,
        call: DeviceEffectCall,
        progress: DeviceEffectProgress,
    ) -> DeviceTransportFuture<Result<DeviceEffectFacts, String>> {
        let Some(control) = self.control_at(&info) else {
            return Box::pin(core::future::ready(Err(
                "this sim is not running any more".to_string()
            )));
        };
        let io = match call {
            DeviceEffectCall::PushProject { .. } | DeviceEffectCall::RemoveProject { .. } => {
                match control.client_io(None) {
                    Ok(io) => Some(io),
                    Err(error) => return Box::pin(core::future::ready(Err(error))),
                }
            }
            _ => None,
        };
        Box::pin(async move {
            match call {
                DeviceEffectCall::FlashFirmware { .. } => {
                    progress("Restarting the sim".to_string(), Some(50));
                    control.restart().await?;
                    progress("The sim restarted".to_string(), Some(100));
                    Ok(DeviceEffectFacts {
                        summary: "a sim runs the build it was started with".to_string(),
                        // Deliberately absent: no preflight read anything out
                        // of efuse, and no tool named a chip. The fold renders
                        // what was never reported as absent, which is the
                        // honest card.
                        ..Default::default()
                    })
                }
                DeviceEffectCall::EraseFlash => {
                    progress("Restarting the sim".to_string(), Some(50));
                    control.restart().await?;
                    Ok(DeviceEffectFacts {
                        summary: "a sim's storage is memory; the restart cleared it".to_string(),
                        ..Default::default()
                    })
                }
                DeviceEffectCall::WriteHardwareManifest { manifest_json } => {
                    control.set_hardware_manifest(manifest_json);
                    progress("Restarting the sim".to_string(), Some(50));
                    control.restart().await?;
                    Ok(DeviceEffectFacts {
                        summary: "the sim restarted wearing the new manifest".to_string(),
                        ..Default::default()
                    })
                }
                // The push and the removal are the REAL conversations, on
                // the sim's own channel: `lpa-client`'s, the same functions
                // the serial provider runs below its own seam. Nothing about
                // the stop/write/load order or the hash check is special-
                // cased for a sim, which is what makes a green push here
                // mean the same thing it means on a board.
                DeviceEffectCall::PushProject {
                    files,
                    expected_hash,
                    fallback_storage_id,
                } => {
                    let io = io.ok_or_else(|| "the sim has no channel".to_string())?;
                    let mut client = lpa_client::LpClient::new(io);
                    let mut report = |label: String, percent: Option<u8>| progress(label, percent);
                    let report = lpa_client::push_project(
                        &mut client,
                        &files,
                        &expected_hash,
                        &fallback_storage_id,
                        &mut report,
                    )
                    .await
                    .map_err(|error| error.to_string())?;
                    Ok(DeviceEffectFacts {
                        summary: format!("project sent to {}", report.storage_id),
                        ..Default::default()
                    })
                }
                DeviceEffectCall::RemoveProject {
                    fallback_storage_id,
                } => {
                    let io = io.ok_or_else(|| "the sim has no channel".to_string())?;
                    let mut client = lpa_client::LpClient::new(io);
                    let mut report = |label: String, percent: Option<u8>| progress(label, percent);
                    let report =
                        lpa_client::remove_project(&mut client, &fallback_storage_id, &mut report)
                            .await
                            .map_err(|error| error.to_string())?;
                    Ok(DeviceEffectFacts {
                        summary: match report.was_loaded {
                            true => format!("removed {}", report.storage_id),
                            false => format!(
                                "the sim reported nothing loaded; cleared {}",
                                report.storage_id
                            ),
                        },
                        ..Default::default()
                    })
                }
            }
        })
    }

    fn lens_client_io(
        &self,
        info: LinkInfo,
        tap: LensLineTap,
    ) -> Result<Box<dyn lpa_client::ClientIo>, String> {
        self.control_at(&info)
            .ok_or_else(|| "this sim is not running any more".to_string())?
            .client_io(Some(tap))
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::super::sim_record::sim_link_info;
    use super::*;

    /// A source that mints nothing but a countable, closed link.
    #[derive(Default)]
    struct CountingSource {
        opened: Rc<Cell<usize>>,
        restarts: Rc<Cell<usize>>,
        manifests: Rc<RefCell<Vec<String>>>,
    }

    struct CountingControl {
        restarts: Rc<Cell<usize>>,
        manifests: Rc<RefCell<Vec<String>>>,
    }

    impl SimRuntimeControl for CountingControl {
        fn restart(&self) -> DeviceTransportFuture<Result<(), String>> {
            self.restarts.set(self.restarts.get() + 1);
            Box::pin(core::future::ready(Ok(())))
        }

        fn set_hardware_manifest(&self, manifest_json: String) {
            self.manifests.borrow_mut().push(manifest_json);
        }

        fn client_io(
            &self,
            _tap: Option<LensLineTap>,
        ) -> Result<Box<dyn lpa_client::ClientIo>, String> {
            Err("this control has no channel".to_string())
        }
    }

    /// A `Link` that does nothing: these rows are about power and routing,
    /// not about the wire (the e2e bench drives the wire).
    struct SilentLink(LinkInfo);

    impl lpa_devices::link::Link for SilentLink {
        fn info(&self) -> &LinkInfo {
            &self.0
        }

        fn submit(&mut self, _command: lpa_devices::link::LinkCommand) {}

        fn poll_event(&mut self) -> Option<lpa_devices::link::LinkEvent> {
            None
        }
    }

    impl SimLinkSource for CountingSource {
        fn open(&self, session: &SimSession) -> Result<SimBacking, String> {
            self.opened.set(self.opened.get() + 1);
            let info = sim_link_info(&session.uid, &session.display_name);
            Ok(SimBacking {
                link: GrantedLink {
                    link: Box::new(SilentLink(info.clone())),
                    info,
                },
                control: Rc::new(CountingControl {
                    restarts: Rc::clone(&self.restarts),
                    manifests: Rc::clone(&self.manifests),
                }),
            })
        }
    }

    fn session(uid: &str) -> SimSession {
        SimSession {
            uid: uid.to_string(),
            target: "lightplayer/desktop".to_string(),
            display_name: "Desktop".to_string(),
            base_mac: "12:22:33:44:55:66".to_string(),
        }
    }

    fn block_on<F: core::future::Future>(future: F) -> F::Output {
        use core::task::{Context, Poll};
        use std::sync::Arc;
        use std::task::Wake;

        struct Noop;
        impl Wake for Noop {
            fn wake(self: Arc<Self>) {}
        }
        let waker = core::task::Waker::from(Arc::new(Noop));
        let mut cx = Context::from_waker(&waker);
        let mut future = core::pin::pin!(future);
        for _ in 0..1_000 {
            if let Poll::Ready(output) = future.as_mut().poll(&mut cx) {
                return output;
            }
        }
        panic!("a sim transport future did not complete");
    }

    /// A sim is made, not discovered: it appears once, is handed over once,
    /// and powering an already-running one on again mints nothing.
    #[test]
    fn a_powered_sim_is_discovered_exactly_once() {
        let opened = Rc::new(Cell::new(0));
        let transport = SimDeviceTransport::new(Rc::new(CountingSource {
            opened: Rc::clone(&opened),
            ..Default::default()
        }));

        assert!(block_on(transport.discover_granted()).unwrap().is_empty());

        transport.power_on(session("dev1")).unwrap();
        transport.power_on(session("dev1")).unwrap();
        assert_eq!(opened.get(), 1, "powering on twice is not two runtimes");
        assert!(transport.is_powered("dev1"));
        assert_eq!(transport.powered_uids(), vec!["dev1".to_string()]);

        let granted = block_on(transport.discover_granted()).unwrap();
        assert_eq!(granted.len(), 1);
        assert_eq!(granted[0].info.endpoint.0, "sim:dev1");
        assert!(
            block_on(transport.discover_granted()).unwrap().is_empty(),
            "an already-routed sim must not mint a second card"
        );
    }

    #[test]
    fn powering_off_stops_the_runtime_and_is_idempotent() {
        let transport = SimDeviceTransport::new(Rc::new(CountingSource::default()));
        transport.power_on(session("dev1")).unwrap();

        assert!(transport.power_off("dev1"));
        assert!(!transport.is_powered("dev1"));
        assert!(!transport.power_off("dev1"), "off is the goal state");
    }

    /// Revoking a sim's grant is powering it off — there is no browser
    /// permission to hand back, only a runtime to stop.
    #[test]
    fn revoking_a_sims_grant_powers_it_off() {
        let transport = SimDeviceTransport::new(Rc::new(CountingSource::default()));
        transport.power_on(session("dev1")).unwrap();

        block_on(transport.revoke_grant(sim_link_info("dev1", "Desktop"))).unwrap();

        assert!(!transport.is_powered("dev1"));
    }

    /// The chooser has nothing to say about a sim, and says so rather than
    /// answering like a dismissed dialog.
    #[test]
    fn the_chooser_is_refused_with_the_way_in() {
        let transport = SimDeviceTransport::new(Rc::new(CountingSource::default()));

        let answer = block_on(transport.request_grant());
        let error = match answer {
            Err(error) => error,
            Ok(_) => panic!("the chooser has nothing to say about a sim"),
        };

        assert!(error.contains("Devices page"), "{error}");
    }

    /// Flash on a sim writes nothing, restarts the runtime, and reports no
    /// facts it did not learn.
    #[test]
    fn flashing_a_sim_restarts_it_and_invents_no_facts() {
        let restarts = Rc::new(Cell::new(0));
        let transport = SimDeviceTransport::new(Rc::new(CountingSource {
            restarts: Rc::clone(&restarts),
            ..Default::default()
        }));
        transport.power_on(session("dev1")).unwrap();

        let facts = block_on(transport.run_effect(
            sim_link_info("dev1", "Desktop"),
            DeviceEffectCall::FlashFirmware {
                build_id: "esp32c6-4mb".to_string(),
            },
            Rc::new(|_, _| {}),
        ))
        .expect("a sim flash succeeds");

        assert_eq!(facts.summary, "a sim runs the build it was started with");
        assert_eq!(facts.probed_mac, None, "no preflight read anything");
        assert_eq!(facts.chip_name, None, "no tool named a chip");
        assert_eq!(restarts.get(), 1, "the card sees a fresh boot");
    }

    /// The erase says what it actually did rather than borrowing silicon's
    /// story about a flash that got wiped.
    #[test]
    fn erasing_a_sim_says_its_storage_was_memory() {
        let restarts = Rc::new(Cell::new(0));
        let transport = SimDeviceTransport::new(Rc::new(CountingSource {
            restarts: Rc::clone(&restarts),
            ..Default::default()
        }));
        transport.power_on(session("dev1")).unwrap();

        let facts = block_on(transport.run_effect(
            sim_link_info("dev1", "Desktop"),
            DeviceEffectCall::EraseFlash,
            Rc::new(|_, _| {}),
        ))
        .expect("a sim erase succeeds");

        assert!(facts.summary.contains("memory"), "{}", facts.summary);
        assert_eq!(restarts.get(), 1);
    }

    /// A manifest write is effective next boot, so the sim takes it and
    /// restarts — the same promise the verb makes on silicon.
    #[test]
    fn a_manifest_write_is_worn_by_the_next_runtime() {
        let manifests = Rc::new(RefCell::new(Vec::new()));
        let restarts = Rc::new(Cell::new(0));
        let transport = SimDeviceTransport::new(Rc::new(CountingSource {
            manifests: Rc::clone(&manifests),
            restarts: Rc::clone(&restarts),
            ..Default::default()
        }));
        transport.power_on(session("dev1")).unwrap();

        block_on(transport.run_effect(
            sim_link_info("dev1", "Desktop"),
            DeviceEffectCall::WriteHardwareManifest {
                manifest_json: "{\"id\":\"x\"}".to_string(),
            },
            Rc::new(|_, _| {}),
        ))
        .expect("a sim manifest write succeeds");

        assert_eq!(manifests.borrow().as_slice(), ["{\"id\":\"x\"}"]);
        assert_eq!(
            restarts.get(),
            1,
            "and the next runtime is the one wearing it"
        );
    }

    /// An effect aimed at a sim that has been powered off ends honestly
    /// rather than pretending — the same race the serial path calls "the
    /// port is gone".
    #[test]
    fn an_effect_on_a_stopped_sim_fails_with_the_reason() {
        let transport = SimDeviceTransport::new(Rc::new(CountingSource::default()));

        let error = block_on(transport.run_effect(
            sim_link_info("dev1", "Desktop"),
            DeviceEffectCall::EraseFlash,
            Rc::new(|_, _| {}),
        ))
        .expect_err("nothing is running");

        assert!(error.contains("not running"), "{error}");
    }
}
