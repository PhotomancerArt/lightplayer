//! The home page's "Connect a board" section: three square buttons (USB ·
//! Bluetooth · Network) and "start a board here" under them.
//!
//! Every button is an offer core publishes (`devices/connect-usb`,
//! `devices/connect-ble`, `devices/connect-wifi-address`,
//! `devices/new-sim`); this section draws them and constructs no action.
//! The web's part is what is about *this browser*: the way forward under a
//! transport it cannot drive (a link out, an address to copy), and the
//! Network square's inline address row.

pub(crate) mod connect_board_section;
pub(crate) mod connect_square;
pub(crate) mod transport_offer;

pub(crate) use connect_board_section::ConnectBoardSection;
pub(crate) use transport_offer::TransportOffer;
