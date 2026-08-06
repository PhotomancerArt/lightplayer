//! The page plane: the app itself, and the share URLs that unfurl.
//!
//! D26 makes this service the origin for `lightplayer.app`, which means the
//! root document always comes from here — that is what buys per-URL OG tags,
//! same-origin cookies, and deep links that answer 200 instead of 404.
//! Hashed asset *references* may point at a CDN later; the HTML never does.

pub mod cache_policy;
pub mod media_type;
pub mod og_inject;
pub mod page_route;
pub mod share_path;
pub mod static_site;
