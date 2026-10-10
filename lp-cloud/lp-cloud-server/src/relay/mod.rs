//! The cloud relay: boards on Wi-Fi reachable through lightplayer.app.
//!
//! Two socket legs meet here:
//!
//! - **The device leg**, `ws://…/relay/device` ([`device_leg`]): one socket
//!   per board, plain HTTP (the C6 cannot afford TLS), speaking
//!   `lpc-relay`'s framing. The board proves which accounts it holds keys
//!   for, and stays registered while its socket lives.
//! - **The browser leg**, `wss://…/relay/board/{id}` ([`browser_leg`]): one
//!   socket per session, bare lp-link frames, joined to a route on the
//!   board.
//!
//! The relay is a pipe. Every session is the same Noise-sealed lp-link a
//! LAN connection is; the relay holds no key and reads no frame. Who may
//! open a session is [`route_admission`]'s decision (an interim rule behind
//! one interface, until the cloud has per-board access settings); what a
//! session may *do* is
//! the board's, which sees a relay link as `LinkTrust::Relayed` (its
//! "Anyone" setting never applies).
//!
//! **Presence lives in memory** ([`relay_hub`]): one machine, and a deploy
//! drops every board, which comes back within its backoff (the legs are
//! closed "going away" first, so boards take the short one). No table, no
//! migration for presence; the store is touched only to look up the
//! accounts a registering board names, and the session of an opening
//! browser. **So do pictures** ([`picture_cache`]): each protocol 2 board's
//! last picture, kept after it leaves, gone at a deploy.
//!
//! The device leg speaks two relay protocols (`lpc-relay`): fielded cores
//! speak protocol 1, which the hub accepts forever and **never sends a
//! frame it does not have** (the hub's one send path checks); protocol 2
//! adds the board's firmware, its project and its pictures.
//!
//! Decision record: `docs/adr/2026-10-06-cloud-relay.md`.

pub mod browser_leg;
pub mod client_address;
pub mod device_leg;
pub mod leg_pump;
pub mod picture_cache;
pub mod relay_hub;
pub mod relay_registry;
pub mod route_admission;
pub mod visitor_rate_limit;

pub use relay_registry::RelayRegistry;
