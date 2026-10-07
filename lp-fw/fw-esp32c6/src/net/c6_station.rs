//! The station the station task drives on this image: the radio's, or the
//! network seam's, behind one type (the switch shape's station half; the
//! frame half is `esp_frame_device::C6FrameDevice`).

use alloc::boxed::Box;
use alloc::vec::Vec;

use esp_radio::wifi::WifiController;
use fw_esp32_common::net::{ConnectOutcome, SeamStation, StationControl};
use lpc_wire::HeardNetwork;

use super::esp_station::EspStation;

/// [`StationControl`] over whichever station `net::net_bringup` chose.
pub enum C6Station {
    /// esp-radio's station (silicon, and an emulator with no network seam).
    Radio(EspStation),
    /// The network seam's station (an emulated board that engaged `net=lan`).
    /// Boxed, so the silicon arm's task storage (static RAM) does not grow
    /// by the seam station's size.
    Seam {
        station: Box<SeamStation>,
        /// Held for the boot, not driven: it runs ESP-NOW, and dropping it
        /// would stop Wi-Fi and ESP-NOW both.
        _controller: WifiController<'static>,
    },
}

impl C6Station {
    /// Put the radio back on ESP-NOW's channel after a scan or a leave
    /// (`EspStation::restore_espnow_channel`). The seam's station never moves
    /// the radio's channel, so on that arm there is nothing to put back.
    pub fn restore_espnow_channel(&mut self) {
        if let Self::Radio(station) = self {
            station.restore_espnow_channel();
        }
    }
}

impl StationControl for C6Station {
    async fn scan(&mut self) -> Option<Vec<HeardNetwork>> {
        match self {
            Self::Radio(station) => station.scan().await,
            Self::Seam { station, .. } => station.scan().await,
        }
    }

    async fn connect(&mut self, ssid: &str, password: &str) -> ConnectOutcome {
        match self {
            Self::Radio(station) => station.connect(ssid, password).await,
            Self::Seam { station, .. } => station.connect(ssid, password).await,
        }
    }

    async fn disconnect(&mut self) {
        match self {
            Self::Radio(station) => station.disconnect().await,
            Self::Seam { station, .. } => station.disconnect().await,
        }
    }

    async fn wait_link_lost(&mut self) {
        match self {
            Self::Radio(station) => station.wait_link_lost().await,
            Self::Seam { station, .. } => station.wait_link_lost().await,
        }
    }

    fn rssi(&self) -> Option<i8> {
        match self {
            Self::Radio(station) => station.rssi(),
            Self::Seam { station, .. } => station.rssi(),
        }
    }
}
