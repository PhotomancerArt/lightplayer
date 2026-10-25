//! The mDNS / DNS-SD responder on `lp-net` (plan P05, MD13).
//!
//! Answers for this board only: `lp-xxxx.local` (A, and NSEC for AAAA so a
//! dual-stack lookup does not wait out its 5 s), and the DNS-SD service
//! `_lightplayer._tcp.local` (PTR, SRV port 80, TXT `mac`/`proto`/`path`).
//! The codec is `fw_esp32_common::net::mdns` (sans-IO, oracle-tested); this
//! task is the socket around it: UDP 5353 joined to 224.0.0.251, started
//! when the station has an address, two announcements 1 s apart, again
//! when the address changes. A query from a port other than 5353 is a
//! legacy one-shot resolver and gets a unicast answer with its id and
//! question echoed (RFC 6762 §6.7); one with the QU bit set gets a unicast
//! answer too. Buffers are allocated once: at boot on a board with a
//! network saved, else at the first address (the LAN endpoint's rule);
//! nothing per query. No probing or conflict resolution (plan: future work).

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec;

use embassy_futures::select::{Either3, select3};
use embassy_net::udp::{PacketMetadata, UdpSocket};
use embassy_net::{IpAddress, IpEndpoint, Ipv4Address, Stack};
use embassy_time::Instant;
use fw_esp32_common::net::mdns::{
    MdnsAnnounce, MdnsIdentity, MdnsQuery, build_answer, parse_query,
};

/// mDNS's group and port.
const GROUP: Ipv4Address = Ipv4Address::new(224, 0, 0, 251);
const PORT: u16 = 5353;
/// The link's port (P04's WebSocket endpoint).
const LINK_PORT: u16 = 80;
/// A received query this long is enough: a question section is short.
const RX_BYTES: usize = 768;
/// Our answers are a handful of short records.
const TX_BYTES: usize = 512;

/// The responder's buffers: its socket's rings and one query and one
/// answer.
pub struct MdnsBuffers {
    rx_meta: &'static mut [PacketMetadata; 4],
    tx_meta: &'static mut [PacketMetadata; 4],
    rx_buf: &'static mut [u8],
    tx_buf: &'static mut [u8],
    packet: &'static mut [u8],
    out: &'static mut [u8],
}

impl MdnsBuffers {
    pub fn leak() -> Self {
        let leak =
            |len: usize| -> &'static mut [u8] { Box::leak(vec![0u8; len].into_boxed_slice()) };
        Self {
            rx_meta: Box::leak(Box::new([PacketMetadata::EMPTY; 4])),
            tx_meta: Box::leak(Box::new([PacketMetadata::EMPTY; 4])),
            rx_buf: leak(RX_BYTES),
            tx_buf: leak(TX_BYTES),
            packet: leak(RX_BYTES),
            out: leak(TX_BYTES),
        }
    }
}

/// The task. `label` is `lp-xxxx`; `mac` the base MAC (its TXT record).
/// `buffers` are the boot-time ones, if the board booted with a network.
#[embassy_executor::task]
pub async fn mdns_task(
    stack: Stack<'static>,
    label: String,
    mac: [u8; 6],
    buffers: Option<MdnsBuffers>,
) {
    let mut address = super::net_address::watch();
    let mut first = None;
    // Without boot-time buffers nothing is allocated until the station
    // first has an address: a board that never joins pays nothing for its
    // name.
    let buffers = match buffers {
        Some(buffers) => buffers,
        None => {
            first = Some(super::net_address::wait_up(&mut address).await);
            MdnsBuffers::leak()
        }
    };
    let MdnsBuffers {
        rx_meta,
        tx_meta,
        rx_buf,
        tx_buf,
        packet,
        out,
    } = buffers;
    let mut socket = UdpSocket::new(stack, rx_meta, rx_buf, tx_meta, tx_buf);
    if socket.bind(PORT).is_err() {
        log::warn!("[mdns] could not bind port {PORT}: no .local name");
        return;
    }
    let group = IpEndpoint::new(IpAddress::Ipv4(GROUP), PORT);
    // The board's name as a DNS-SD instance: its LAN label for now (the
    // codec falls back to it anyway).
    let mut identity = MdnsIdentity {
        label: label.clone(),
        instance: label,
        mac,
        proto: lpc_wire::WIRE_PROTO_VERSION,
        port: LINK_PORT,
        ipv4: [0; 4],
    };
    loop {
        identity.ipv4 = match first.take() {
            Some(ip) => ip,
            None => super::net_address::wait_up(&mut address).await,
        };
        if stack.join_multicast_group(IpAddress::Ipv4(GROUP)).is_err() {
            log::warn!("[mdns] could not join 224.0.0.251");
        }
        log::info!(
            "[mdns] answering for {}.local at {}.{}.{}.{}",
            identity.label,
            identity.ipv4[0],
            identity.ipv4[1],
            identity.ipv4[2],
            identity.ipv4[3]
        );
        let mut announce = MdnsAnnounce::new();
        announce.address_acquired(now_ms());
        loop {
            let next = announce.next_at(now_ms()).map(Instant::from_millis);
            match select3(
                socket.recv_from(packet),
                super::station_task::sleep_until(next),
                super::net_address::wait_down(&mut address),
            )
            .await
            {
                Either3::First(Ok((len, meta))) => {
                    let Some(query) =
                        parse_query(&packet[..len], &identity.label, &identity.instance)
                    else {
                        continue;
                    };
                    if query.is_empty() {
                        continue;
                    }
                    let legacy = meta.endpoint.port != PORT;
                    let to = if legacy || query.unicast_response {
                        meta.endpoint
                    } else {
                        group
                    };
                    let legacy_id = legacy.then_some(query.id);
                    if let Some(n) = build_answer(out, &identity, &query, legacy_id) {
                        let _ = socket.send_to(&out[..n], to).await;
                    }
                }
                Either3::First(Err(_)) => {}
                Either3::Second(()) => {
                    let all = MdnsQuery {
                        host_a: true,
                        service_ptr: true,
                        ..MdnsQuery::default()
                    };
                    if let Some(n) = build_answer(out, &identity, &all, None) {
                        let _ = socket.send_to(&out[..n], group).await;
                    }
                    announce.on_sent(now_ms());
                }
                Either3::Third(()) => {
                    log::info!("[mdns] address lost: silent until the next one");
                    break;
                }
            }
        }
    }
}

fn now_ms() -> u64 {
    Instant::now().as_millis()
}
