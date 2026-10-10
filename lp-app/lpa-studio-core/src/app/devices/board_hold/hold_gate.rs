//! [`UsbHoldGate`]: the ports this tab does not open because another tab's
//! claims account for them, and which board a held port is.
//!
//! Web Serial names a port by its vendor and product only, so a tab cannot
//! tell which of its granted ports is which board before it opens one. What
//! it CAN count is how many boards of each kind other tabs say they hold.
//! The gate reads its ports against those claims:
//!
//! - **The sweep.** The granted ports a sweep has not attached yet are
//!   grouped by vendor and product. A group no larger than the claims for
//!   its pair is attached and never opened ([`gate_group`]): every port in
//!   it is accounted for. A larger group is opened as today, and the OS
//!   refuses the held one.
//! - **A refused open.** A port whose open the OS refused is read the same
//!   way afterwards ([`reads_as_held`]): when this tab's ports of the pair
//!   that are not open number no more than the claims, each is another
//!   tab's.
//! - **Association.** When exactly one claim names the pair and exactly one
//!   of this tab's ports of the pair is held, that port IS the claimed
//!   board ([`associate`]), and the model merges it onto the board's own
//!   card. In every other count the claims are carried by the boards' facts
//!   alone.
//!
//! The controller keeps the claims current ([`UsbHoldGate::set_claims`]);
//! the effects layer reads the gate when a sweep attaches ports and when
//! the model asks a port to open. Shared ([`SharedUsbHoldGate`]) because the
//! sweep runs in a spawned future that cannot borrow the effects layer.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use lpa_devices::identity::MacAddress;
use lpa_devices::link::{LinkId, LinkInfo};

use super::hold_key::UsbPair;

/// The gate, shared between the effects layer and its sweep futures.
pub type SharedUsbHoldGate = Rc<RefCell<UsbHoldGate>>;

/// Other tabs' USB claims, and this tab's ports held because of them.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct UsbHoldGate {
    /// Other tabs' USB holds, by pair: each claimed board's MAC.
    claims: BTreeMap<UsbPair, Vec<MacAddress>>,
    /// Ports attached and never opened: the claims accounted for them when
    /// the sweep found them.
    gated: BTreeMap<LinkId, UsbPair>,
    /// Ports whose open the OS refused, read as another tab's afterwards.
    read_held: BTreeMap<LinkId, UsbPair>,
}

impl UsbHoldGate {
    /// Replace the claims with what the hold book says now.
    pub fn set_claims(&mut self, claims: BTreeMap<UsbPair, Vec<MacAddress>>) {
        self.claims = claims;
    }

    /// How many boards of this kind other tabs hold.
    pub fn claims_for(&self, pair: UsbPair) -> usize {
        self.claims.get(&pair).map_or(0, Vec::len)
    }

    /// Which of a sweep's new ports to gate, in the order given: each
    /// port's pair (`None` for a port that is not a USB serial port, which
    /// is never gated), grouped and read against the claims.
    pub fn gate_decisions(&self, pairs: &[Option<UsbPair>]) -> Vec<bool> {
        let mut groups: BTreeMap<UsbPair, usize> = BTreeMap::new();
        for pair in pairs.iter().flatten() {
            *groups.entry(*pair).or_default() += 1;
        }
        pairs
            .iter()
            .map(|pair| pair.is_some_and(|pair| gate_group(groups[&pair], self.claims_for(pair))))
            .collect()
    }

    /// `link` is attached and must never be opened while it stays gated.
    pub fn gate(&mut self, link: LinkId, pair: UsbPair) {
        self.gated.insert(link, pair);
    }

    /// Whether the model's open of `link` is to be answered with
    /// `Event::LinkHeld` instead of opening the port.
    pub fn gates(&self, link: LinkId) -> bool {
        self.gated.contains_key(&link)
    }

    /// `link`'s refused open was read as another tab's hold.
    pub fn mark_read_held(&mut self, link: LinkId, pair: UsbPair) {
        self.read_held.insert(link, pair);
    }

    /// Whether `link` is held because of the claims: gated, or read as held.
    pub fn holds(&self, link: LinkId) -> bool {
        self.gated.contains_key(&link) || self.read_held.contains_key(&link)
    }

    /// The board a held `link` is, by the association rule: exactly one
    /// claim for its pair and exactly one held port of that pair.
    pub fn association(&self, link: LinkId) -> Option<MacAddress> {
        let pair = self
            .gated
            .get(&link)
            .or_else(|| self.read_held.get(&link))?;
        associate(
            self.claims.get(pair).map_or(&[], Vec::as_slice),
            self.held_count(*pair),
        )
    }

    /// How many of this tab's ports of `pair` are held (gated or read).
    pub fn held_count(&self, pair: UsbPair) -> usize {
        self.held_links(pair).len()
    }

    /// This tab's ports of `pair` that are held (gated or read).
    pub fn held_links(&self, pair: UsbPair) -> Vec<LinkId> {
        self.gated
            .iter()
            .chain(self.read_held.iter())
            .filter(|(_, held)| **held == pair)
            .map(|(link, _)| *link)
            .collect()
    }

    /// Take every port of `pair` out of the gate (a holder of that kind let
    /// go, or this tab is taking a board of that kind over). Opening them
    /// is someone else's decision: nothing here opens a port. Returns them.
    pub fn release_pair(&mut self, pair: UsbPair) -> Vec<LinkId> {
        let links = self.held_links(pair);
        self.gated.retain(|_, held| *held != pair);
        self.read_held.retain(|_, held| *held != pair);
        links
    }

    /// Take `link` alone out of the gate: a take-over here opens it again.
    pub fn let_out(&mut self, link: LinkId) {
        self.gated.remove(&link);
        self.read_held.remove(&link);
    }

    /// Forget ports the model no longer routes.
    pub fn retain_links(&mut self, keep: impl Fn(LinkId) -> bool) {
        self.gated.retain(|link, _| keep(*link));
        self.read_held.retain(|link, _| keep(*link));
    }
}

/// The pair a link's port can be held by, or `None` for a link no other tab
/// can hold by its port: Bluetooth, the LAN, the relay, a sim, an in-tab
/// emulated board, or a port that reports no vendor and product.
pub fn usb_pair_of(info: &LinkInfo) -> Option<UsbPair> {
    let endpoint = &info.endpoint;
    if endpoint.is_network()
        || endpoint.is_bluetooth()
        || crate::uid_from_sim_endpoint(&endpoint.0).is_some()
        || crate::uid_from_emu_endpoint(&endpoint.0).is_some()
    {
        return None;
    }
    info.usb.map(UsbPair::from)
}

/// A group of `ports` not yet attached, of one kind, is gated when the
/// claims for that kind account for every one of them.
pub fn gate_group(ports: usize, claims: usize) -> bool {
    ports > 0 && ports <= claims
}

/// A tab's ports of one kind that are not open (refused or gated) read as
/// other tabs' holds when the claims for that kind account for every one
/// of them.
pub fn reads_as_held(not_open: usize, claims: usize) -> bool {
    not_open > 0 && not_open <= claims
}

/// The board a held port is: the one claim, when there is exactly one
/// claim and exactly one held port of the kind. Otherwise no port is
/// named (two of a kind cannot be told apart before they open).
pub fn associate(claims: &[MacAddress], held_ports: usize) -> Option<MacAddress> {
    match (claims, held_ports) {
        ([only], 1) => Some(only.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use lpa_devices::identity::EndpointKey;
    use lpa_devices::link::UsbIds;

    use super::*;

    #[test]
    fn a_group_the_claims_account_for_is_gated_and_a_larger_one_is_not() {
        let mut gate = UsbHoldGate::default();
        gate.set_claims(claims(&[(C6, &[A]), (BRIDGE, &[B, D])]));

        // One C6 port, one C6 claim: gated. Two bridges, two claims: both.
        assert_eq!(
            gate.gate_decisions(&[Some(C6), Some(BRIDGE), Some(BRIDGE)]),
            vec![true, true, true]
        );
        // Two C6 ports, one claim: open them; the OS refuses the held one.
        assert_eq!(
            gate.gate_decisions(&[Some(C6), Some(C6), None]),
            vec![false, false, false]
        );
        // A kind nobody claims is never gated.
        let other = UsbPair {
            vendor: 0x10c4,
            product: 0xea60,
        };
        assert_eq!(gate.gate_decisions(&[Some(other)]), vec![false]);
    }

    #[test]
    fn one_claim_and_one_held_port_name_the_board() {
        let mut gate = UsbHoldGate::default();
        gate.set_claims(claims(&[(C6, &[A])]));
        gate.gate(LinkId(1), C6);

        assert_eq!(gate.association(LinkId(1)), Some(mac(A)));
        assert!(gate.gates(LinkId(1)));

        // A second held port of the kind: no port is named any more.
        gate.mark_read_held(LinkId(2), C6);
        assert_eq!(gate.association(LinkId(1)), None);
        assert_eq!(gate.association(LinkId(2)), None);
        assert!(gate.holds(LinkId(2)) && !gate.gates(LinkId(2)));

        // Two claims, two ports: still nothing named.
        gate.set_claims(claims(&[(C6, &[A, B])]));
        assert_eq!(gate.association(LinkId(1)), None);
        assert_eq!(gate.association(LinkId(9)), None, "not held at all");
    }

    #[test]
    fn releasing_a_kind_lets_its_ports_go_and_opens_nothing() {
        let mut gate = UsbHoldGate::default();
        gate.gate(LinkId(1), C6);
        gate.mark_read_held(LinkId(2), C6);
        gate.gate(LinkId(3), BRIDGE);

        let mut released = gate.release_pair(C6);
        released.sort();
        assert_eq!(released, vec![LinkId(1), LinkId(2)]);
        assert!(!gate.holds(LinkId(1)) && !gate.holds(LinkId(2)));
        assert!(gate.gates(LinkId(3)));

        gate.retain_links(|link| link != LinkId(3));
        assert!(!gate.gates(LinkId(3)));

        gate.gate(LinkId(4), C6);
        gate.mark_read_held(LinkId(5), C6);
        gate.let_out(LinkId(4));
        assert!(
            !gate.holds(LinkId(4)) && gate.holds(LinkId(5)),
            "one port, not its kind"
        );
    }

    #[test]
    fn a_refused_port_reads_as_held_only_when_the_claims_account_for_it() {
        assert!(reads_as_held(1, 1));
        assert!(reads_as_held(2, 2));
        assert!(!reads_as_held(2, 1), "one of them is another app's");
        assert!(!reads_as_held(1, 0), "nobody claimed it");
        assert!(!reads_as_held(0, 3));
        assert!(gate_group(1, 1) && !gate_group(0, 1) && !gate_group(2, 1));
    }

    #[test]
    fn only_a_usb_serial_port_has_a_pair() {
        let usb = |endpoint: &str, ids: Option<UsbIds>| LinkInfo {
            endpoint: EndpointKey(endpoint.to_string()),
            usb: ids,
            ..LinkInfo::default()
        };
        let c6 = Some(UsbIds {
            vendor: 0x303a,
            product: 0x1001,
        });
        assert_eq!(
            usb_pair_of(&usb("browser-serial-esp32-port-1", c6)),
            Some(C6)
        );
        assert_eq!(usb_pair_of(&usb("browser-serial-esp32-port-1", None)), None);
        assert_eq!(usb_pair_of(&usb("ble:QkxF", c6)), None);
        assert_eq!(usb_pair_of(&usb("lan:ws://192.168.1.4/link", c6)), None);
        assert_eq!(usb_pair_of(&usb("relay:a0f26287b48c", c6)), None);
        assert_eq!(usb_pair_of(&usb(&crate::sim_endpoint("dev1").0, c6)), None);
        assert_eq!(usb_pair_of(&usb(&crate::emu_endpoint("dev1").0, c6)), None);
    }

    const C6: UsbPair = UsbPair {
        vendor: 0x303a,
        product: 0x1001,
    };
    const BRIDGE: UsbPair = UsbPair {
        vendor: 0x1a86,
        product: 0x7523,
    };
    const A: u8 = 0x0a;
    const B: u8 = 0x0b;
    const D: u8 = 0x0d;

    fn mac(n: u8) -> MacAddress {
        MacAddress(format!("a0:f2:62:87:b4:{n:02x}"))
    }

    fn claims(by_pair: &[(UsbPair, &[u8])]) -> BTreeMap<UsbPair, Vec<MacAddress>> {
        by_pair
            .iter()
            .map(|(pair, macs)| (*pair, macs.iter().map(|n| mac(*n)).collect()))
            .collect()
    }
}
