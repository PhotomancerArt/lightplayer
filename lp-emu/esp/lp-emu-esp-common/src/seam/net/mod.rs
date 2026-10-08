//! The virtual LAN: the medium under the network seam (`net=lan`).
//!
//! A small home network several emulated boards join: access points a board
//! can hear and join ([`virtual_access_point`]), one Ethernet segment that
//! carries their frames ([`virtual_lan`]), a router on it that answers ARP
//! and hands out addresses over DHCP ([`lan_gateway`],
//! [`lan_dhcp_server`]), a host TCP port forwarded to each board
//! ([`lan_port_forward`]), a host-side participant tests use to ask the
//! boards things ([`lan_probe`]), and the handle every board's machine, the
//! host and a runner share one LAN through, with who drives it on whose clock
//! ([`shared_lan`]).
//!
//! It knows Ethernet frames and a few protocols, not LightPlayer: nothing
//! here reads the firmware's link, its server or its names. MIT, inside the
//! `lp-emu/` fence. **It never pretends to be a radio**: no airtime, no
//! fading, no retransmission, no coexistence; a signal strength is a
//! configured number.
//!
//! Every LAN is a value, so several can exist in one process, and its time
//! is the guests' (guest cycles), never the host's clock. The one exception
//! is the host edge of a port forward, which reads and writes a host socket
//! whenever the medium is driven, like the USB door — and the run's pace
//! ([`lan_pace`]): unset, a host connected through a forward holds a
//! self-driven board's guest clock to the host's ([`lan_host_pace`]), so the
//! board's timers and the host's agree on how long a round trip takes;
//! `realtime` holds it there for the whole run, and `max` never does.

pub mod lan_dhcp_server;
pub mod lan_dns;
pub mod lan_dns_server;
pub mod lan_frame;
pub mod lan_gateway;
pub mod lan_host_pace;
pub mod lan_pace;
pub mod lan_port_forward;
pub mod lan_probe;
pub mod lan_stack;
pub mod lan_station;
pub mod lan_uplink;
pub mod shared_lan;
pub mod virtual_access_point;
pub mod virtual_lan;

#[cfg(test)]
mod lan_test_board;

pub use lan_dhcp_server::{DhcpServer, Lease};
pub use lan_dns::{DnsAnswer, DnsData};
pub use lan_gateway::LanGateway;
pub use lan_host_pace::HostPace;
pub use lan_pace::{PACE_LABEL_MARKER, Pace};
pub use lan_port_forward::{ForwardCounters, LanPortForward};
pub use lan_probe::{LanProbe, ProbeConn, ProbeId};
pub use lan_station::{LanStation, StationEvent};
pub use lan_uplink::{LanUplink, UPLINK_IP};
pub use shared_lan::{HOST_PACE_STEP_US, LanDriver, NET_SEAM, SharedLan, net_endpoint};
pub use virtual_access_point::{JoinOutcome, ScanRecord, VirtualAccessPoint};
pub use virtual_lan::{FrameRecord, LanConfig, LanCounters, LanPort, VirtualLan, net_pacer_config};
