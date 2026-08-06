//! Sessions at the edge.
//!
//! The split is deliberate and matches P03's: the *record* — minting,
//! hashing, expiry, resolution — belongs to
//! [`CloudService`](lp_cloud_domain::CloudService), and this module owns only
//! what a domain has no business knowing: the cookie header it arrives in
//! ([`session_cookie`]), the OAuth round trip that establishes who somebody
//! is ([`google_auth`]), and the password-free local login that stands in for
//! it on a laptop ([`dev_auth`]).
//!
//! Both logins end the same way — `upsert_user` then `open_session` — which
//! is why the dev one was worth keeping after the real one landed: it
//! exercises the same two domain calls with no network in the way.

pub mod dev_auth;
pub mod google_auth;
pub mod session_cookie;
