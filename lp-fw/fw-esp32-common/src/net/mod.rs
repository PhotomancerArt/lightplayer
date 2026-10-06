//! The Wi-Fi station, chip-free (feature `wifi`; Wi-Fi roadmap M6).
//!
//! Everything about the station that is not a radio fact lives here and is
//! host-tested: the join policy ([`StationPolicy`], a sans-IO state machine
//! with its backoff and join rule), what the policy knows of the network
//! file ([`StationSettings`], no passwords), and the scan answer the
//! server's probe reads ([`ScanCache`]).
//!
//! It also holds **the boundary the network seam plugs into** (plan MD4):
//! [`StationControl`] (scan, connect, disconnect, link loss) and
//! [`NetFrameDevice`] (the frame device under embassy-net). The C6 puts
//! esp-radio behind both (`fw-esp32c6/src/net/`); the network seam (PR C)
//! puts the emulator's virtual LAN behind them again, and nothing above
//! the boundary can tell. Each item's doc is the contract an emulator
//! answer is held to.

pub mod join_choice;
pub mod mdns;
pub mod net_frame_device;
pub mod radio_rule;
pub mod scan_cache;
pub mod station_backoff;
pub mod station_board;
pub mod station_control;
pub mod station_policy;
pub mod station_settings;

pub use join_choice::{JoinChoice, choose};
pub use net_frame_device::{NetFrameDevice, Unplugged};
pub use scan_cache::ScanCache;
pub use station_backoff::StationBackoff;
pub use station_board::StationBoard;
pub use station_control::{ConnectOutcome, StationControl};
pub use station_policy::{StationAction, StationEvent, StationPolicy};
pub use station_settings::{SavedNetwork, StationSettings, secret_tag};
