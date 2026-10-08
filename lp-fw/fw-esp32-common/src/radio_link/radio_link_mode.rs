//! What this boot's radio links are for, decided once per boot before any of
//! them opens.
//!
//! A board either **serves** its radio links (the engine runs, the link mux
//! carries the wire) or takes an **update** over them (core-only: no engine,
//! the core serves the over-the-air update protocol on channel 3). The two
//! want different lp-link configurations ([`super::radio_link_config`]): an
//! update link advertises a wider receive window in its SYN, and the SYN is
//! the link's first frame, so the mode must be known before the link opens.
//!
//! The boot knows the mode only after the radio is up (a split image decides
//! engine or core-only after its radios and links, so that a core dying in
//! that bring-up still counts against its trial). So the decision is a
//! one-shot on the port ([`super::RadioLinkPort::decide_mode`]): a slot
//! refuses to open a link before it ([`super::OpenRefused::ModeUndecided`]),
//! and the radio side waits for it ([`super::RadioLinkPort::wait_for_mode`]) before
//! opening one. Every boot that starts a radio decides — a plain image at
//! once (`Serve`), a split image when it chooses the engine (`Serve`) or
//! core-only (`Update`) — so nothing waits forever.

/// What a boot's radio links carry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RadioLinkMode {
    /// The engine runs: the link mux serves the wire on each link (the
    /// board's cut of `LinkConfig::ble()`).
    Serve,
    /// Core-only: the links carry the update protocol, and receive with the
    /// widest window lp-link allows ([`UPDATE_RX_WINDOW`]).
    Update,
}

/// The receive window a radio link advertises in update mode: lp-link's
/// selective-repeat maximum. An update link is window-bound (a window of
/// frames per round trip, and a browser host's round trip is long), and
/// core-only has the heap — no engine, no project. The host keeps its own
/// send window at 16 with 4 chunks ahead (S5c: loss rises sharply above 16
/// in flight on Mac Chrome), so this is room, not a target.
pub const UPDATE_RX_WINDOW: u8 = 32;

/// The receive window a **LAN** link advertises in update mode (OTA Wi-Fi
/// plan D7): 8 frames of 1 KiB in flight where serve mode keeps 2, so a
/// slower Wi-Fi round trip still keeps up with the flash (~85 KB/s of
/// compressed chunks). The slots cost ~8 KB, and the LAN endpoint's socket
/// buffer grows to match, on core-only's free heap.
///
/// Measured on silicon (OTA Wi-Fi plan P6, 2026-10-07: FC6 fixture-c6 on
/// the desk's test access point, lp-cli on a Mac on the same network,
/// two runs each): the core and engine pieces together took **38.0 s at a
/// window of 2, 34.0–35.5 s at 8, 33.6 s at 16**. 8 is the smallest window
/// within ~10 % of the best. The flash, not the window, paces an update on
/// a ~15 ms round trip; the window is room for a slower network. Core-only's
/// heap free at a piece's commit, the inflate window held: 105,400 B (a
/// window-16 core-only taking the core), 122,512 B (a window-8 trial core
/// taking the engine).
pub const LAN_UPDATE_RX_WINDOW: u8 = 8;

impl RadioLinkMode {
    /// The mode as the port stores it (0 is "not decided").
    pub(crate) const fn code(self) -> u8 {
        match self {
            Self::Serve => 1,
            Self::Update => 2,
        }
    }

    /// The mode a stored code names; `None` while undecided.
    pub(crate) const fn from_code(code: u8) -> Option<Self> {
        match code {
            1 => Some(Self::Serve),
            2 => Some(Self::Update),
            _ => None,
        }
    }
}
