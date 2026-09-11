//! [`DeviceTransport`] over the emulated boards this tab has powered on.
//!
//! [`SimDeviceTransport`](super::sim_transport::SimDeviceTransport)'s twin,
//! and deliberately line for line: an emu is made rather than discovered,
//! powering it on is what puts a link behind it, and the
//! [`DeviceTransport`] trait is the same either way — which is the point.
//! The effects layer, the roster and the card cannot tell an emu from a sim
//! or from a board at the end of a wire.
//!
//! # What is different, and it is only one thing
//!
//! A sim runs the DESKTOP firmware wearing the target's hardware manifest.
//! An emu runs the **target's own firmware image** on an emulated SoC. So
//! the two verbs a sim has nothing honest to do — flash and erase — are
//! real here: there is a flash chip, it is a byte array the page can
//! address, and writing it means what it says. The other three verbs are
//! the same real `lpa-client` conversations they are everywhere.
//!
//! | effect | on an emu |
//! |---|---|
//! | Flash firmware | the packaged build is fetched and written into the emulated chip, and the board reboots into it. No ROM downloader: mode A writes the chip directly (D5/D24). |
//! | Factory reset (erase) | the chip is erased and the board reset — a blank chip, which is what a blank chip is. |
//! | Write the board manifest | the REAL `/hardware.json` write over the app protocol, chunked, after the board says it is ready — the serial arm, not the sim's restart. |
//! | Push / remove a project | the REAL `lpa-client` conversation over the board's own wire. |
//!
//! **No flash reports a probed MAC or a chip name**, for the same reason
//! the sim's does not: nothing read an efuse and no tool named a chip.
//! Studio minted this board's identity and the emulator wears it, so
//! claiming a preflight read it out would be a fact about something that
//! never happened.
//!
//! # The platform lives behind one seam
//!
//! What a running emu actually IS — a Worker holding the C6 in the browser,
//! a counting double in the host tests — arrives through [`EmuLinkSource`].
//! That is what lets the whole of this file, and therefore power on/off,
//! the effect vocabulary and the borrow discipline, be `just test`-covered
//! on the host while the Worker half stays wasm-only.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use lpa_devices::link::LinkInfo;
use lpa_link::providers::browser_serial_esp32_options::BrowserSerialEsp32Options;
use lpc_model::AsLpPath;

use super::device_transport::{
    DeviceEffectCall, DeviceEffectFacts, DeviceEffectProgress, DeviceTransport,
    DeviceTransportFuture, GrantedLink, LensLineTap,
};
use super::sim_record::uid_from_emu_endpoint;

/// Where the board runtime manifest lives on a device — the same path the
/// serial arm writes (`browser_transport.rs`), because it is the same
/// firmware reading it at the same moment in its boot.
const DEVICE_HARDWARE_MANIFEST_PATH: &str = "/hardware.json";

/// One emulated board, as the transport needs to know it.
///
/// Everything here comes from the device's registry row and its sidecar;
/// the transport reads none of them itself (sans-IO), it is told.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmuSession {
    /// The derived uid — the registry key, the emu's endpoint, and the key
    /// its persisted flash image is stored under.
    pub uid: String,
    /// The hardware target this emu runs (a board id). Not a shape it
    /// wears: it decides which firmware build the chip is born flashed
    /// with, and which SoC module the page loads.
    pub target: String,
    /// What the card calls it, for the link's label.
    pub display_name: String,
    /// The minted base MAC the board wears in efuse and reports in its
    /// hello.
    pub base_mac: String,
}

/// A handle on ONE running emu: what an effect does to it, as opposed to
/// what the model does to its link.
///
/// Shared with the link, not a replacement for it. The wire is the link's
/// wire, which is what makes the exclusive borrow mean something — the pump
/// is paused, so an io built here is the only reader for as long as the
/// conversation runs.
pub trait EmuRuntimeControl {
    /// Fetch a packaged build, write it into the emulated chip and reboot
    /// into it. Answers the build's display name, so the card's summary can
    /// say what was written rather than repeat the id it was asked for.
    fn flash_package(&self, manifest_url: String) -> DeviceTransportFuture<Result<String, String>>;

    /// Erase the whole chip.
    fn erase(&self) -> DeviceTransportFuture<Result<(), String>>;

    /// Reset the chip. NOT a replug: the link does not re-enumerate, so the
    /// effects layer keeps the link it borrowed.
    fn reset(&self) -> DeviceTransportFuture<Result<(), String>>;

    /// How fast the board runs against wall time, as the page last reported
    /// it (D8/D25). `None` until the first report — absent is absent, and
    /// the band drops the tail rather than guessing.
    fn dilation(&self) -> Option<f64> {
        None
    }

    /// Whether the runtime is still coming up: the Worker, the module fetch
    /// and the cold ROM boot.
    ///
    /// The fold cannot see this — a link that is attached and not open
    /// looks the same whether a boot is in flight behind it or nothing is —
    /// and a cold boot takes seconds, so the studio asks here before it
    /// decides an emu has given up.
    fn is_starting(&self) -> bool {
        false
    }

    /// An `lpa-client` io on this board's wire, for the exclusive-borrow
    /// conversations. `tap` receives every whole line the io drains, so the
    /// fold keeps hearing the board while a conversation owns the wire.
    fn client_io(&self, tap: Option<LensLineTap>) -> Result<Box<dyn lpa_client::ClientIo>, String>;
}

/// A powered-on emu's live backing.
pub struct EmuBacking {
    /// The closed link, handed to the effects layer by the next discovery.
    pub link: GrantedLink,
    pub control: Rc<dyn EmuRuntimeControl>,
}

/// Where a running emu comes from on this platform.
pub trait EmuLinkSource {
    /// Start `session`'s runtime backing. The link inside is CLOSED:
    /// powering an emu on is minting its port, and opening it stays the
    /// model's decision, exactly as it is for a granted serial port.
    fn open(&self, session: &EmuSession) -> Result<EmuBacking, String>;

    /// Forget everything this platform persists for `uid` — the 4 MiB flash
    /// image (D15). A source that persists nothing has nothing to do, which
    /// is why this has a default.
    fn forget(&self, _uid: &str) -> DeviceTransportFuture<Result<(), String>> {
        Box::pin(core::future::ready(Ok(())))
    }
}

/// The emus this tab has powered on.
pub struct EmuDeviceTransport {
    source: Rc<dyn EmuLinkSource>,
    /// Where packaged firmware lives. The SAME options the browser serial
    /// provider is built with (`web_app.rs` hands it `Default::default()`),
    /// so a flash on an emu and a flash on a board fetch the same manifest
    /// — one derivation, not two.
    firmware: BrowserSerialEsp32Options,
    /// By uid, so power on/off and endpoint routing agree by construction.
    powered: RefCell<BTreeMap<String, PoweredEmu>>,
}

struct PoweredEmu {
    control: Rc<dyn EmuRuntimeControl>,
    /// The link, until a discovery hands it to the effects layer. `None`
    /// afterwards: an emu is not re-discoverable while it is already
    /// routed, and minting a second link for one board would give the
    /// roster two cards for one device.
    link: Option<GrantedLink>,
}

impl EmuDeviceTransport {
    /// A transport serving emus from `source`.
    pub fn new(source: Rc<dyn EmuLinkSource>) -> Self {
        Self {
            source,
            firmware: BrowserSerialEsp32Options::default(),
            powered: RefCell::new(BTreeMap::new()),
        }
    }

    /// Point the flash arm at a different packaged-firmware base. Only for
    /// a build that also moved the serial provider's: the two must name the
    /// same files or a board and an emu would be flashed with different
    /// bytes under one build id.
    pub fn with_firmware_options(mut self, firmware: BrowserSerialEsp32Options) -> Self {
        self.firmware = firmware;
        self
    }

    /// Power an emu on: start its runtime backing and offer its link to the
    /// next granted-port sweep.
    ///
    /// Idempotent by uid — powering on something already on is not an error
    /// and must not mint a second board.
    pub fn power_on(&self, session: EmuSession) -> Result<(), String> {
        if self.powered.borrow().contains_key(&session.uid) {
            return Ok(());
        }
        let backing = self.source.open(&session)?;
        self.powered.borrow_mut().insert(
            session.uid,
            PoweredEmu {
                control: backing.control,
                link: Some(backing.link),
            },
        );
        Ok(())
    }

    /// Power an emu off: drop the backing, which is what ends the Worker.
    /// `false` when it was not on — the goal state, not an error.
    pub fn power_off(&self, uid: &str) -> bool {
        self.powered.borrow_mut().remove(uid).is_some()
    }

    /// Whether this emu is running right now.
    pub fn is_powered(&self, uid: &str) -> bool {
        self.powered.borrow().contains_key(uid)
    }

    /// The uids of every running emu.
    pub fn powered_uids(&self) -> Vec<String> {
        self.powered.borrow().keys().cloned().collect()
    }

    /// How fast this emu runs against wall time, when it is running and has
    /// reported (see [`EmuRuntimeControl::dilation`]).
    pub fn dilation(&self, uid: &str) -> Option<f64> {
        self.powered.borrow().get(uid)?.control.dilation()
    }

    /// Whether this emu is still coming up (see
    /// [`EmuRuntimeControl::is_starting`]). `false` for one that is not
    /// powered on.
    pub fn is_starting(&self, uid: &str) -> bool {
        self.powered
            .borrow()
            .get(uid)
            .is_some_and(|powered| powered.control.is_starting())
    }

    /// Forget an emu: stop it, then take its persisted flash image away.
    ///
    /// Both, and in that order. The sidecar is the library's to delete
    /// (`delete_sim_record`); the 4 MiB image is the page's, and a Forget
    /// that took only the sidecar would leave megabytes behind that nothing
    /// can name again (Q7).
    pub fn forget(&self, uid: &str) -> DeviceTransportFuture<Result<(), String>> {
        self.power_off(uid);
        self.source.forget(uid)
    }

    /// The runtime control behind a link's endpoint, when it is one of ours.
    fn control_at(&self, info: &LinkInfo) -> Option<Rc<dyn EmuRuntimeControl>> {
        let uid = uid_from_emu_endpoint(&info.endpoint.0)?;
        self.powered
            .borrow()
            .get(uid)
            .map(|powered| Rc::clone(&powered.control))
    }
}

impl DeviceTransport for EmuDeviceTransport {
    fn label(&self) -> &'static str {
        "emu"
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
        // Not "no emus available" (`Ok(None)`, which reads as a dismissed
        // chooser) but a refusal with the way in: an emu is created, and
        // the browser's port chooser has nothing to say about one.
        Box::pin(core::future::ready(Err(
            "an emu is created from the Devices page, not from the browser's port chooser"
                .to_string(),
        )))
    }

    fn revoke_grant(&self, _info: LinkInfo) -> DeviceTransportFuture<Result<(), String>> {
        // Nothing to hand back, and nothing to stop — the sim transport's
        // documented reason applies here unchanged. The fold revokes a
        // grant when it dismisses a provisional pending link, which is
        // exactly what ADOPTING a runtime into its record does, so a revoke
        // that powered off would stop the board Studio had just started.
        // The runtime's lifetime belongs to power on/off alone (PD8, Q15).
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
                "this emu is not running any more".to_string()
            )));
        };
        // Built before the future, like the sim's: an io is a borrow of the
        // wire, and a failure to take it is the effect's failure, not a
        // step inside it.
        let io = match call {
            DeviceEffectCall::PushProject { .. }
            | DeviceEffectCall::RemoveProject { .. }
            | DeviceEffectCall::WriteHardwareManifest { .. } => match control.client_io(None) {
                Ok(io) => Some(io),
                Err(error) => return Box::pin(core::future::ready(Err(error))),
            },
            _ => None,
        };
        let manifest_url = match &call {
            DeviceEffectCall::FlashFirmware { build_id } => {
                Some(self.firmware.firmware_manifest_path(build_id))
            }
            _ => None,
        };
        Box::pin(async move {
            match call {
                DeviceEffectCall::FlashFirmware { .. } => {
                    let manifest_url =
                        manifest_url.ok_or_else(|| "no build was named".to_string())?;
                    progress("Writing the emulated flash".to_string(), Some(10));
                    let display_name = control.flash_package(manifest_url).await?;
                    progress("The board rebooted".to_string(), Some(100));
                    Ok(DeviceEffectFacts {
                        summary: format!("wrote {display_name} into the emulated flash"),
                        // Deliberately absent, as on a sim: no preflight
                        // read anything out of efuse and no tool named a
                        // chip. The fold renders what was never reported as
                        // absent, which is the honest card.
                        ..Default::default()
                    })
                }
                DeviceEffectCall::EraseFlash => {
                    progress("Erasing the emulated flash".to_string(), Some(50));
                    control.erase().await?;
                    control.reset().await?;
                    Ok(DeviceEffectFacts {
                        summary: "emulated flash erased".to_string(),
                        ..Default::default()
                    })
                }
                // The REAL write, not the sim's restart: this board has a
                // filesystem and a loader that reads `/hardware.json` at
                // boot, so the verb's promise ("effective next boot") is
                // kept the way it is kept on silicon. The wait and the
                // chunking are the serial arm's, for the serial arm's
                // reasons — a board formatting its littlefs does not answer
                // writes, and one big frame OOMs a decode.
                DeviceEffectCall::WriteHardwareManifest { manifest_json } => {
                    let io = io.ok_or_else(|| "the emu has no channel".to_string())?;
                    let mut client = lpa_client::LpClient::new(io).on_borrowed_wire();
                    let mut report = |label: String, percent: Option<u8>| progress(label, percent);
                    lpa_client::wait_until_ready(
                        &mut client,
                        lpa_client::READY_ATTEMPTS,
                        &mut report,
                    )
                    .await
                    .map_err(|error| {
                        format!("the board never became ready to write to: {error}")
                    })?;
                    lpa_client::write_file_in_chunks(
                        &mut client,
                        DEVICE_HARDWARE_MANIFEST_PATH.as_path(),
                        manifest_json.as_bytes(),
                        lpa_client::MANIFEST_CHUNK_BYTES,
                        &mut report,
                    )
                    .await
                    .map_err(|error| format!("device file write failed: {error}"))?;
                    Ok(DeviceEffectFacts {
                        summary: "board manifest written".to_string(),
                        ..Default::default()
                    })
                }
                // The push and the removal are the REAL conversations, on
                // the board's own wire: `lpa-client`'s, the same functions
                // the serial provider runs below its own seam. Nothing
                // about the stop/write/load order or the hash check is
                // special-cased, which is what makes a green push here mean
                // the same thing it means on a board.
                DeviceEffectCall::PushProject {
                    files,
                    expected_hash,
                    fallback_storage_id,
                } => {
                    let io = io.ok_or_else(|| "the emu has no channel".to_string())?;
                    let mut client = lpa_client::LpClient::new(io).on_borrowed_wire();
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
                    let io = io.ok_or_else(|| "the emu has no channel".to_string())?;
                    let mut client = lpa_client::LpClient::new(io).on_borrowed_wire();
                    let mut report = |label: String, percent: Option<u8>| progress(label, percent);
                    let report =
                        lpa_client::remove_project(&mut client, &fallback_storage_id, &mut report)
                            .await
                            .map_err(|error| error.to_string())?;
                    Ok(DeviceEffectFacts {
                        summary: match report.was_loaded {
                            true => format!("removed {}", report.storage_id),
                            // Under-claim, as on a board: it had already
                            // stopped reporting the project, so "removed"
                            // would be a claim about something never seen.
                            false => format!(
                                "the board reported nothing loaded; cleared {}",
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
            .ok_or_else(|| "this emu is not running any more".to_string())?
            .client_io(Some(tap))
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use lpc_wire::{ClientMessage, ClientRequest, TransportError, WireServerMessage};

    use super::super::sim_record::emu_link_info;
    use super::*;

    /// An emu is made, not discovered: it appears once, is handed over
    /// once, and powering an already-running one on again mints nothing.
    #[test]
    fn a_powered_emu_is_discovered_exactly_once() {
        let opened = Rc::new(Cell::new(0));
        let transport = EmuDeviceTransport::new(Rc::new(CountingSource {
            opened: Rc::clone(&opened),
            ..Default::default()
        }));

        assert!(block_on(transport.discover_granted()).unwrap().is_empty());

        transport.power_on(session("dev1")).unwrap();
        transport.power_on(session("dev1")).unwrap();
        assert_eq!(opened.get(), 1, "powering on twice is not two boards");
        assert!(transport.is_powered("dev1"));
        assert_eq!(transport.powered_uids(), vec!["dev1".to_string()]);

        let granted = block_on(transport.discover_granted()).unwrap();
        assert_eq!(granted.len(), 1);
        assert_eq!(granted[0].info.endpoint.0, "emu:dev1");
        assert!(
            block_on(transport.discover_granted()).unwrap().is_empty(),
            "an already-routed emu must not mint a second card"
        );
    }

    #[test]
    fn powering_off_stops_the_runtime_and_is_idempotent() {
        let transport = EmuDeviceTransport::new(Rc::new(CountingSource::default()));
        transport.power_on(session("dev1")).unwrap();

        assert!(transport.power_off("dev1"));
        assert!(!transport.is_powered("dev1"));
        assert!(!transport.power_off("dev1"), "off is the goal state");
    }

    /// Revoking an emu's grant leaves the board ALONE, for the reason the
    /// sim's does: the fold revokes when it dismisses a provisional pending
    /// link, which is exactly what adopting a runtime into its record does.
    #[test]
    fn revoking_an_emus_grant_leaves_the_board_running() {
        let transport = EmuDeviceTransport::new(Rc::new(CountingSource::default()));
        transport.power_on(session("dev1")).unwrap();

        block_on(transport.revoke_grant(emu_link_info("dev1", "XIAO ESP32-C6"))).unwrap();

        assert!(
            transport.is_powered("dev1"),
            "adopting an emu must not stop it"
        );
    }

    #[test]
    fn the_chooser_is_refused_with_the_way_in() {
        let transport = EmuDeviceTransport::new(Rc::new(CountingSource::default()));

        let answer = block_on(transport.request_grant());
        let error = match answer {
            Err(error) => error,
            Ok(_) => panic!("the chooser has nothing to say about an emu"),
        };

        assert!(error.contains("Devices page"), "{error}");
    }

    /// The flash arm fetches the SAME manifest the esptool path would, by
    /// the same derivation, and reports no facts it did not learn.
    #[test]
    fn flashing_an_emu_writes_the_packaged_build_and_invents_no_facts() {
        let flashed = Rc::new(RefCell::new(Vec::new()));
        let transport = EmuDeviceTransport::new(Rc::new(CountingSource {
            flashed: Rc::clone(&flashed),
            ..Default::default()
        }));
        transport.power_on(session("dev1")).unwrap();

        let facts = block_on(transport.run_effect(
            emu_link_info("dev1", "XIAO ESP32-C6"),
            DeviceEffectCall::FlashFirmware {
                build_id: "esp32c6-4mb".to_string(),
            },
            Rc::new(|_, _| {}),
        ))
        .expect("an emu flash succeeds");

        assert_eq!(
            flashed.borrow().as_slice(),
            [BrowserSerialEsp32Options::default().firmware_manifest_path("esp32c6-4mb")],
            "one derivation of the manifest URL, shared with the serial flash"
        );
        assert_eq!(
            facts.summary,
            "wrote LightPlayer C6 into the emulated flash"
        );
        assert_eq!(facts.probed_mac, None, "no preflight read anything");
        assert_eq!(facts.chip_name, None, "no tool named a chip");
    }

    /// Erase wipes the chip and reboots into what that leaves: a blank one.
    #[test]
    fn erasing_an_emu_erases_the_chip_and_resets() {
        let erases = Rc::new(Cell::new(0));
        let resets = Rc::new(Cell::new(0));
        let transport = EmuDeviceTransport::new(Rc::new(CountingSource {
            erases: Rc::clone(&erases),
            resets: Rc::clone(&resets),
            ..Default::default()
        }));
        transport.power_on(session("dev1")).unwrap();

        let facts = block_on(transport.run_effect(
            emu_link_info("dev1", "XIAO ESP32-C6"),
            DeviceEffectCall::EraseFlash,
            Rc::new(|_, _| {}),
        ))
        .expect("an emu erase succeeds");

        assert_eq!(facts.summary, "emulated flash erased");
        assert_eq!(erases.get(), 1);
        assert_eq!(resets.get(), 1, "and the card sees the blank chip boot");
    }

    /// The manifest write is the REAL conversation over the board's own
    /// wire — the serial arm — not the sim's restart. The io records what
    /// it was asked, so the test can say which conversation actually ran.
    #[test]
    fn a_manifest_write_goes_over_the_client_io() {
        let asked = Rc::new(RefCell::new(Vec::new()));
        let resets = Rc::new(Cell::new(0));
        let transport = EmuDeviceTransport::new(Rc::new(CountingSource {
            asked: Rc::clone(&asked),
            resets: Rc::clone(&resets),
            ..Default::default()
        }));
        transport.power_on(session("dev1")).unwrap();

        block_on(transport.run_effect(
            emu_link_info("dev1", "XIAO ESP32-C6"),
            DeviceEffectCall::WriteHardwareManifest {
                manifest_json: "{\"id\":\"x\"}".to_string(),
            },
            Rc::new(|_, _| {}),
        ))
        .expect("an emu manifest write succeeds");

        assert_eq!(
            asked.borrow().as_slice(),
            [
                "listLoadedProjects".to_string(),
                "write /hardware.json {\"id\":\"x\"}".to_string()
            ],
            "ready first, then the write — the serial arm's order"
        );
        assert_eq!(
            resets.get(),
            0,
            "a manifest write is effective next boot; it does not reboot the board"
        );
    }

    /// An effect aimed at an emu that has been powered off ends honestly
    /// rather than pretending — the same race the serial path calls "the
    /// port is gone".
    #[test]
    fn an_effect_on_a_stopped_emu_fails_with_the_reason() {
        let transport = EmuDeviceTransport::new(Rc::new(CountingSource::default()));

        let error = block_on(transport.run_effect(
            emu_link_info("dev1", "XIAO ESP32-C6"),
            DeviceEffectCall::EraseFlash,
            Rc::new(|_, _| {}),
        ))
        .expect_err("nothing is running");

        assert!(error.contains("not running"), "{error}");
    }

    /// Forget stops the board AND takes its persisted image away. Both, or
    /// megabytes stay behind that nothing can name again (Q7).
    #[test]
    fn forget_stops_the_board_and_deletes_its_image() {
        let forgotten = Rc::new(RefCell::new(Vec::new()));
        let transport = EmuDeviceTransport::new(Rc::new(CountingSource {
            forgotten: Rc::clone(&forgotten),
            ..Default::default()
        }));
        transport.power_on(session("dev1")).unwrap();

        block_on(transport.forget("dev1")).unwrap();

        assert!(!transport.is_powered("dev1"));
        assert_eq!(forgotten.borrow().as_slice(), ["dev1".to_string()]);
    }

    // --- the doubles -----------------------------------------------------

    /// A source that mints nothing but a countable, closed link.
    #[derive(Default)]
    struct CountingSource {
        opened: Rc<Cell<usize>>,
        erases: Rc<Cell<usize>>,
        resets: Rc<Cell<usize>>,
        flashed: Rc<RefCell<Vec<String>>>,
        forgotten: Rc<RefCell<Vec<String>>>,
        asked: Rc<RefCell<Vec<String>>>,
    }

    struct CountingControl {
        erases: Rc<Cell<usize>>,
        resets: Rc<Cell<usize>>,
        flashed: Rc<RefCell<Vec<String>>>,
        asked: Rc<RefCell<Vec<String>>>,
    }

    impl EmuRuntimeControl for CountingControl {
        fn flash_package(
            &self,
            manifest_url: String,
        ) -> DeviceTransportFuture<Result<String, String>> {
            self.flashed.borrow_mut().push(manifest_url);
            Box::pin(core::future::ready(Ok("LightPlayer C6".to_string())))
        }

        fn erase(&self) -> DeviceTransportFuture<Result<(), String>> {
            self.erases.set(self.erases.get() + 1);
            Box::pin(core::future::ready(Ok(())))
        }

        fn reset(&self) -> DeviceTransportFuture<Result<(), String>> {
            self.resets.set(self.resets.get() + 1);
            Box::pin(core::future::ready(Ok(())))
        }

        fn client_io(
            &self,
            _tap: Option<LensLineTap>,
        ) -> Result<Box<dyn lpa_client::ClientIo>, String> {
            Ok(Box::new(CountingIo {
                asked: Rc::clone(&self.asked),
                answer: None,
            }))
        }
    }

    /// An `lpa-client` io that records what it was asked and answers each
    /// request with the smallest successful reply. Enough to tell "the
    /// conversation ran, in this order" from "something else happened".
    struct CountingIo {
        asked: Rc<RefCell<Vec<String>>>,
        answer: Option<WireServerMessage>,
    }

    impl lpa_client::ClientIo for CountingIo {
        fn send<'a, 'async_trait>(
            &'a mut self,
            msg: ClientMessage,
        ) -> std::pin::Pin<
            Box<dyn core::future::Future<Output = Result<(), TransportError>> + 'async_trait>,
        >
        where
            'a: 'async_trait,
            Self: 'async_trait,
        {
            use lpc_wire::server::ServerMsgBody;
            use lpc_wire::server::{FsRequest, FsResponse};
            let (note, body) = match msg.msg {
                ClientRequest::ListLoadedProjects => (
                    "listLoadedProjects".to_string(),
                    ServerMsgBody::ListLoadedProjects {
                        projects: Vec::new(),
                    },
                ),
                ClientRequest::Filesystem(FsRequest::Write { path, data }) => (
                    format!("write {} {}", path.as_str(), String::from_utf8_lossy(&data)),
                    ServerMsgBody::Filesystem(FsResponse::Write { path, error: None }),
                ),
                // Anything else is a conversation this double was not
                // written for: recorded by name, and answered with a reply
                // the client will refuse, so a wrong conversation fails
                // loudly instead of passing on a shrug.
                other => (format!("{other:?}"), ServerMsgBody::UnloadProject),
            };
            self.asked.borrow_mut().push(note);
            self.answer = Some(WireServerMessage::new(msg.id, body));
            Box::pin(async { Ok(()) })
        }

        fn receive<'a, 'async_trait>(
            &'a mut self,
        ) -> std::pin::Pin<
            Box<
                dyn core::future::Future<Output = Result<WireServerMessage, TransportError>>
                    + 'async_trait,
            >,
        >
        where
            'a: 'async_trait,
            Self: 'async_trait,
        {
            let answer = self.answer.take();
            Box::pin(async move {
                answer.ok_or_else(|| TransportError::Other("nothing was asked".to_string()))
            })
        }

        fn close<'a, 'async_trait>(
            &'a mut self,
        ) -> std::pin::Pin<
            Box<dyn core::future::Future<Output = Result<(), TransportError>> + 'async_trait>,
        >
        where
            'a: 'async_trait,
            Self: 'async_trait,
        {
            Box::pin(async { Ok(()) })
        }
    }

    /// A `Link` that does nothing: these rows are about power, routing and
    /// the verbs, not about the wire.
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

    impl EmuLinkSource for CountingSource {
        fn open(&self, session: &EmuSession) -> Result<EmuBacking, String> {
            self.opened.set(self.opened.get() + 1);
            let info = emu_link_info(&session.uid, &session.display_name);
            Ok(EmuBacking {
                link: GrantedLink {
                    link: Box::new(SilentLink(info.clone())),
                    info,
                },
                control: Rc::new(CountingControl {
                    erases: Rc::clone(&self.erases),
                    resets: Rc::clone(&self.resets),
                    flashed: Rc::clone(&self.flashed),
                    asked: Rc::clone(&self.asked),
                }),
            })
        }

        fn forget(&self, uid: &str) -> DeviceTransportFuture<Result<(), String>> {
            self.forgotten.borrow_mut().push(uid.to_string());
            Box::pin(core::future::ready(Ok(())))
        }
    }

    fn session(uid: &str) -> EmuSession {
        EmuSession {
            uid: uid.to_string(),
            target: "seeed/xiao-esp32-c6".to_string(),
            display_name: "XIAO ESP32-C6".to_string(),
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
        panic!("an emu transport future did not complete");
    }
}
