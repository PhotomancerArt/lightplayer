//! A smoltcp TCP/IP stack standing on the segment: the gateway's and a
//! probe's.
//!
//! The stack's device is two queues: frames the segment delivered, waiting
//! to be read, and frames the stack wrote, waiting for the segment to carry.
//! Its clock is **guest time**: a cycle count converted at the chip's rate
//! (the rate is the caller's; this crate holds no clock rate), never the
//! host's. Its random seed is fixed from its MAC, so its TCP sequence numbers
//! and ports are the same on every run.

use std::collections::VecDeque;
use std::net::Ipv4Addr;

use lp_emu_core::sched::Cycles;
use smoltcp::iface::{Config, Interface, SocketSet};
use smoltcp::phy::{Device, DeviceCapabilities, Medium, RxToken, TxToken};
use smoltcp::time::Instant;
use smoltcp::wire::{EthernetAddress, HardwareAddress, IpCidr, Ipv4Cidr};

use super::lan_frame::MAX_FRAME_LEN;

/// A TCP/IP stack on the segment.
pub struct LanStack {
    pub iface: Interface,
    pub sockets: SocketSet<'static>,
    device: QueueDevice,
    cycles_per_us: u64,
    last: Instant,
}

impl LanStack {
    /// A stack with MAC `mac` and no address yet, its time at `now`.
    pub fn new(mac: [u8; 6], cycles_per_us: u64, now: Cycles) -> Self {
        assert!(cycles_per_us > 0, "a stack needs a clock rate");
        let mut device = QueueDevice::default();
        let mut config = Config::new(HardwareAddress::Ethernet(EthernetAddress(mac)));
        config.random_seed =
            u64::from_le_bytes([mac[0], mac[1], mac[2], mac[3], mac[4], mac[5], 0x4c, 0x41]);
        let start = instant(now, cycles_per_us);
        let iface = Interface::new(config, &mut device, start);
        Self {
            iface,
            sockets: SocketSet::new(Vec::new()),
            device,
            cycles_per_us,
            last: start,
        }
    }

    /// Take `ip`/24 as the stack's address, with `gateway` as its default
    /// route when given.
    pub fn set_address(&mut self, ip: Ipv4Addr, prefix: u8, gateway: Option<Ipv4Addr>) {
        self.iface.update_ip_addrs(|addrs| {
            addrs.clear();
            addrs
                .push(IpCidr::Ipv4(Ipv4Cidr::new(ip, prefix)))
                .expect("one address fits");
        });
        if let Some(gw) = gateway {
            self.iface
                .routes_mut()
                .add_default_ipv4_route(gw)
                .expect("one route fits");
        }
    }

    /// Hold `ip`/`prefix` too, beside the addresses it has (the gateway's
    /// uplink address). `Err` when the stack holds as many as it can.
    pub fn add_address(&mut self, ip: Ipv4Addr, prefix: u8) -> Result<(), ()> {
        let mut added = Err(());
        self.iface.update_ip_addrs(|addrs| {
            added = addrs
                .push(IpCidr::Ipv4(Ipv4Cidr::new(ip, prefix)))
                .map_err(|_| ());
        });
        added
    }

    /// A frame the segment delivered.
    pub fn push_frame(&mut self, frame: Vec<u8>) {
        self.device.rx.push_back(frame);
    }

    /// Run the stack at guest cycle `now` (never behind its last run): read
    /// every delivered frame, run its timers, and hand back what it sent.
    pub fn poll(&mut self, now: Cycles) -> Vec<Vec<u8>> {
        let at = self.instant(now);
        self.iface.poll(at, &mut self.device, &mut self.sockets);
        std::mem::take(&mut self.device.tx)
    }

    /// The earliest guest cycle the stack wants running again (a timer, a
    /// retransmission), or `None` for nothing scheduled.
    pub fn poll_at(&mut self, now: Cycles) -> Option<Cycles> {
        let at = self.instant(now);
        self.iface
            .poll_at(at, &self.sockets)
            .map(|due| (due.total_micros().max(0) as u64).saturating_mul(self.cycles_per_us))
    }

    /// Guest cycles as the stack's clock, held monotonic.
    fn instant(&mut self, now: Cycles) -> Instant {
        let at = instant(now, self.cycles_per_us).max(self.last);
        self.last = at;
        at
    }
}

fn instant(now: Cycles, cycles_per_us: u64) -> Instant {
    Instant::from_micros(i64::try_from(now / cycles_per_us).unwrap_or(i64::MAX))
}

/// The stack's device: the segment's frames in, the stack's frames out.
#[derive(Default)]
struct QueueDevice {
    rx: VecDeque<Vec<u8>>,
    tx: Vec<Vec<u8>>,
}

impl Device for QueueDevice {
    type RxToken<'a> = QueueRx;
    type TxToken<'a> = QueueTx<'a>;

    fn receive(&mut self, _at: Instant) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        let frame = self.rx.pop_front()?;
        Some((QueueRx(frame), QueueTx(&mut self.tx)))
    }

    fn transmit(&mut self, _at: Instant) -> Option<Self::TxToken<'_>> {
        Some(QueueTx(&mut self.tx))
    }

    fn capabilities(&self) -> DeviceCapabilities {
        let mut caps = DeviceCapabilities::default();
        caps.medium = Medium::Ethernet;
        caps.max_transmission_unit = MAX_FRAME_LEN;
        caps
    }
}

struct QueueRx(Vec<u8>);

impl RxToken for QueueRx {
    fn consume<R, F>(self, f: F) -> R
    where
        F: FnOnce(&[u8]) -> R,
    {
        f(&self.0)
    }
}

struct QueueTx<'a>(&'a mut Vec<Vec<u8>>);

impl TxToken for QueueTx<'_> {
    fn consume<R, F>(self, len: usize, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        let mut frame = vec![0u8; len];
        let r = f(&mut frame);
        self.0.push(frame);
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_clock_is_guest_cycles_at_the_given_rate_and_never_runs_back() {
        let mut s = LanStack::new([2, 0, 0, 0, 0, 1], 160, 0);
        assert_eq!(s.instant(160_000), Instant::from_millis(1));
        assert_eq!(s.instant(80), Instant::from_millis(1), "held monotonic");
        assert_eq!(s.instant(320_000), Instant::from_millis(2));
    }

    #[test]
    fn an_idle_stack_with_an_address_sends_nothing() {
        let mut s = LanStack::new([2, 0, 0, 0, 0, 1], 160, 0);
        s.set_address(Ipv4Addr::new(192, 168, 4, 1), 24, None);
        assert!(s.poll(1_000).is_empty());
        assert_eq!(s.poll_at(1_000), None);
    }
}
