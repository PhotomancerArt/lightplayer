//! The transports the comparison runs over: a pipe model, the link preset
//! that goes with it, and the shape its faults take at a given rate.

use crate::sim::pipe::{Faults, PipeModel};
use crate::{Framing, LinkConfig};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transport {
    /// USB-Serial-JTAG / CDC: a byte stream in 64-byte packets, ~1 ms.
    Usb,
    /// BLE NUS used as a byte stream: COBS frames cut into notifications.
    BleStream,
    /// BLE NUS with one frame per notification (datagram framing).
    Ble,
    /// UDP on a LAN: datagrams, loss, reordering, duplication.
    Udp,
    /// WebSocket / TCP: reliable and ordered.
    Ws,
}

impl Transport {
    pub const ALL: [Transport; 5] = [
        Transport::Usb,
        Transport::BleStream,
        Transport::Ble,
        Transport::Udp,
        Transport::Ws,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Transport::Usb => "USB",
            Transport::BleStream => "BLE-stream",
            Transport::Ble => "BLE",
            Transport::Udp => "UDP",
            Transport::Ws => "WS",
        }
    }

    pub fn pipe(self) -> PipeModel {
        let ble = PipeModel {
            name: "ble",
            packet: 244,
            datagram: true,
            byte_ns: 0,
            latency: 2_000,
            jitter: 0,
            ordered: true,
            conn_events: Some((30_000, 4)),
            queue_packets: 8,
        };
        match self {
            Transport::Usb => PipeModel {
                name: "usb",
                packet: 64,
                datagram: false,
                // ~256 KB/s.
                byte_ns: 3_906,
                latency: 1_000,
                jitter: 0,
                ordered: true,
                conn_events: None,
                queue_packets: 8,
            },
            Transport::BleStream => PipeModel {
                name: "ble-stream",
                datagram: false,
                ..ble
            },
            Transport::Ble => ble,
            Transport::Udp => PipeModel {
                name: "udp",
                packet: 1_400,
                datagram: true,
                byte_ns: 1_000,
                latency: 2_000,
                jitter: 3_000,
                ordered: false,
                conn_events: None,
                queue_packets: 16,
            },
            Transport::Ws => PipeModel {
                name: "ws",
                packet: 65_536,
                datagram: true,
                byte_ns: 1_000,
                latency: 10_000,
                jitter: 0,
                ordered: true,
                conn_events: None,
                queue_packets: 16,
            },
        }
    }

    pub fn link_config(self) -> LinkConfig {
        match self {
            Transport::Usb => LinkConfig::usb(),
            Transport::BleStream => LinkConfig {
                framing: Framing::Stream,
                max_payload: 480,
                ..LinkConfig::ble()
            },
            Transport::Ble => LinkConfig::ble(),
            Transport::Udp => LinkConfig::udp(),
            Transport::Ws => LinkConfig::ws(),
        }
    }

    /// The fault mix at rate `p`, shaped like what each transport does:
    /// USB tears the tail (and sometimes the middle) of a write; BLE loses
    /// whole notifications at our edges; UDP loses, duplicates and delays;
    /// WS loses nothing.
    pub fn faults(self, p: f64) -> Faults {
        match self {
            Transport::Usb => Faults {
                drop_tail: p,
                drop_packet: p / 4.0,
                drop_span: p / 8.0,
                corrupt: p / 100.0,
                ..Faults::none()
            },
            Transport::BleStream | Transport::Ble => Faults {
                drop_packet: p,
                ..Faults::none()
            },
            Transport::Udp => Faults {
                drop_packet: p,
                duplicate: p / 4.0,
                spike: p / 10.0,
                spike_len: 100_000,
                ..Faults::none()
            },
            Transport::Ws => Faults::none(),
        }
    }
}
