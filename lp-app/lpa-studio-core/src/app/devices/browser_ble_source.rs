//! [`BleLinkSource`] over the page's Web Bluetooth (wasm only): what a
//! Bluetooth device actually is in the browser.
//!
//! The thinnest join, like `browser_emu_source.rs`: `lpa-link`'s
//! `browser_ble` owns the devices, the bounded connect, the paced writes and
//! the reconnect loop; `BrowserBleLink` turns a session into the model's
//! link; `BleClientIo` is the borrowing conversation's io. What is left here
//! is keeping ONE shared wire per session, so the link and a borrowing
//! conversation drain the same `LineSplitter` (`ble_wire.rs` says why).
//!
//! ⚠️ **wasm-only, so `just test` never sees it.** The transport it plugs
//! into is host-covered through `ble_transport.rs`'s double; the JS below it
//! is pinned by `lpa-link/tests/browser_ble_conformance.rs`.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use lpa_link::device_link::browser_ble::BrowserBleLink;
use lpa_link::providers::browser_ble::{self as ble, BleClientIo, BleDevice, BleTapLine, BleWire};

use super::ble_transport::BleLinkSource;
use super::device_transport::{DeviceTransportFuture, GrantedLink, LensLineTap, LensTapEvent};
use super::sim_record::ble_link_info;

/// Bluetooth devices, as this page holds them.
#[derive(Default)]
pub struct BrowserBleSource {
    /// One wire per JS session, by the browser's device id.
    wires: Rc<RefCell<BTreeMap<String, Rc<BleWire>>>>,
}

impl BrowserBleSource {
    pub fn new() -> Self {
        Self::default()
    }

    fn granted(
        wires: &Rc<RefCell<BTreeMap<String, Rc<BleWire>>>>,
        device: &BleDevice,
    ) -> GrantedLink {
        let wire = {
            let mut wires = wires.borrow_mut();
            let wire = wires
                .entry(device.device_id.clone())
                .or_insert_with(|| Rc::new(BleWire::new(device.session)));
            // A forgotten-then-re-picked device has a NEW session; the wire
            // follows it rather than speaking to a deleted one.
            if wire.session() != device.session {
                *wire = Rc::new(BleWire::new(device.session));
            }
            Rc::clone(wire)
        };
        let info = ble_link_info(&device.device_id, &device.name);
        GrantedLink {
            link: Box::new(BrowserBleLink::new(wire, info.clone())),
            info,
        }
    }
}

impl BleLinkSource for BrowserBleSource {
    fn present(&self) -> Vec<GrantedLink> {
        ble::present_devices()
            .iter()
            .map(|device| Self::granted(&self.wires, device))
            .collect()
    }

    fn restore(&self) {
        // Idempotent per page on the JS side; the answer arrives as a
        // presence edge, so nothing here waits for it.
        wasm_bindgen_futures::spawn_local(ble::restore_granted_devices());
    }

    fn request(&self) -> DeviceTransportFuture<Result<Option<GrantedLink>, String>> {
        let wires = Rc::clone(&self.wires);
        Box::pin(async move {
            let device = ble::request_device().await?;
            Ok(device.map(|device| Self::granted(&wires, &device)))
        })
    }

    fn forget(&self, device_id: &str) -> DeviceTransportFuture<Result<(), String>> {
        let wire = self.wires.borrow_mut().remove(device_id);
        Box::pin(async move {
            if let Some(wire) = wire {
                // Best effort, as every revoke is: a browser without
                // `BluetoothDevice.forget()` keeps the permission, and says
                // so by answering false — which is a log line, not a failure.
                if !ble::forget(wire.session()).await {
                    log::debug!("bluetooth permission not revoked (no forget() here)");
                }
            }
            Ok(())
        })
    }

    fn client_io(
        &self,
        device_id: &str,
        tap: Option<LensLineTap>,
    ) -> Result<Box<dyn lpa_client::ClientIo>, String> {
        let wire = self
            .wires
            .borrow()
            .get(device_id)
            .cloned()
            .ok_or_else(|| "this Bluetooth device is not connected".to_string())?;
        // The studio's tap vocabulary is its own; `lpa-link` stays
        // independent of it, so the join is one match.
        let tap: Option<Rc<dyn Fn(BleTapLine)>> = tap.map(|tap| {
            Rc::new(move |line: BleTapLine| {
                tap(match line {
                    BleTapLine::Line(line) => LensTapEvent::Line(line),
                    BleTapLine::PortError(error) => LensTapEvent::PortError(error),
                })
            }) as Rc<dyn Fn(BleTapLine)>
        });
        Ok(Box::new(BleClientIo::new(wire, tap)))
    }
}
