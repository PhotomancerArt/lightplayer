//! mDNS and DNS-SD for the board's own name, sans-IO.

pub mod mdns_name;

pub use mdns_name::{mdns_host, mdns_label};
