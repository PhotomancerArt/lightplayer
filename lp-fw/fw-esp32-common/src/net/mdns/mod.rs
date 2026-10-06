//! mDNS and DNS-SD for the board's own name, sans-IO.

pub mod mdns_announce;
pub mod mdns_answer;
pub mod mdns_name;
pub mod mdns_query;

pub use mdns_announce::MdnsAnnounce;
pub use mdns_answer::{MdnsIdentity, build_answer, build_goodbye, effective_instance};
pub use mdns_name::{mdns_host, mdns_label};
pub use mdns_query::{MdnsQuery, parse_query};
