//! Each saved network's id in an offer path:
//! `devices/<board>/wifi/forget/<slug>`.
//!
//! A network name may hold anything — a `.`, a `/`, an emoji — and an offer
//! path's verb segments hold none of those. So each name becomes a slug
//! (lower-case ASCII letters and digits, runs of anything else a `-`,
//! `network` when nothing is left), and a slug two names share gets `-2`,
//! `-3`, … in saved order. The slug is an id for the agent's path; the
//! offer's label names the network itself.

use crate::app::library::package_slug::slugify;

/// One slug per name in `ssids`, in the same order, no two alike.
pub fn wifi_network_slugs<'a>(ssids: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut taken: Vec<String> = Vec::new();
    for ssid in ssids {
        let base = if ssid.chars().any(|c| c.is_ascii_alphanumeric()) {
            slugify(ssid)
        } else {
            "network".to_string()
        };
        let mut slug = base.clone();
        let mut n = 2;
        while taken.contains(&slug) {
            slug = format!("{base}-{n}");
            n += 1;
        }
        taken.push(slug);
    }
    taken
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_become_path_safe_and_distinct() {
        assert_eq!(
            wifi_network_slugs([
                "Starlink Home",
                "cafe.guest/2G",
                "🌈🌈",
                "starlink-home",
                "☕"
            ]),
            [
                "starlink-home",
                "cafe-guest-2g",
                "network",
                "starlink-home-2",
                "network-2"
            ]
        );
        for slug in wifi_network_slugs(["a.b", "x/y", "  "]) {
            assert!(!slug.contains('.') && !slug.contains('/') && !slug.is_empty());
        }
    }
}
