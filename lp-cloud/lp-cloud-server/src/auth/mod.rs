//! Sessions at the edge.
//!
//! The split is deliberate and matches P03's: the *record* — minting,
//! hashing, expiry, resolution — belongs to
//! [`CloudService`](lp_cloud_domain::CloudService), and this module owns only
//! what a domain has no business knowing: the cookie header it arrives in
//! ([`session_cookie`]) and the password-free local login that stands in for
//! Google until P08 ([`dev_auth`]).

pub mod dev_auth;
pub mod session_cookie;
