//! Where a board answers on its own network: an IPv4 address and a port.

use core::fmt;

/// A board's address on its LAN, as it reports it in its hello and in
/// [`RelayFrame::LanChanged`](crate::RelayFrame::LanChanged).
///
/// The hub passes it to the board's own accounts in `ListBoards`, so a
/// browser on the same network can move the session onto the LAN. It is the
/// board's word: nothing checks it, and a browser that dials it still runs
/// the same keyed handshake.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LanAddress {
    /// The board's IPv4 address.
    pub ip: [u8; 4],
    /// The port its LAN link listens on.
    pub port: u16,
}

/// `192.168.4.20:80`.
impl fmt::Display for LanAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [a, b, c, d] = self.ip;
        write!(f, "{a}.{b}.{c}.{d}:{}", self.port)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    #[test]
    fn displays_as_ip_and_port() {
        let lan = LanAddress {
            ip: [192, 168, 4, 20],
            port: 80,
        };
        assert_eq!(lan.to_string(), "192.168.4.20:80");
    }
}
