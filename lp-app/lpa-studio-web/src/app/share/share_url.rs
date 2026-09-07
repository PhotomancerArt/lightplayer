//! The share link, in the three pieces the URL hero paints it in.
//!
//! The address bar IS the share link (vision D1/D13), so the panel's hero
//! is that same string — origin dimmed, the readable slug in the heading
//! colour, the uid dimmed again, because the uid is the part that never
//! changes and the slug is the part a human reads. One type holds the
//! pieces so the copied text and the painted text can never drift.

use lpc_cloud_api::share_link;
use lpc_history::PrefixedUid;

/// One project's canonical link, split for display.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShareUrl {
    /// `lightplayer.app` — no scheme: the hero is a link to read, not to
    /// parse, and `https://` is noise in a 348px card. The COPIED string
    /// keeps its scheme (see [`ShareUrl::absolute`]).
    pub origin: String,
    /// The cosmetic half. Empty for a project whose name slugifies to
    /// nothing (emoji-only, CJK-only) — then the address is the bare uid.
    pub slug: String,
    /// The load-bearing half: `prj…`. `None` for an **embedded example**,
    /// whose canonical address is the bare `/p/<slug>` — its transient
    /// session carries a RAM-minted uid that must never reach a URL
    /// (`UiStudioView::open_transient_example`). Not a new grammar: it is
    /// the `StudioRoute::Example` address the router already serves.
    pub uid: Option<PrefixedUid>,
}

impl ShareUrl {
    /// `/p/<slug>-<uid>`, `/p/<uid>` when there is no slug, and the bare
    /// `/p/<slug>` for a uid-less example — the one grammar, from the one
    /// module that owns it.
    pub fn path(&self) -> String {
        match self.uid {
            Some(uid) => share_link::canonical_path(&self.slug, uid),
            None => format!("/p/{}", self.slug),
        }
    }

    /// The whole thing, as it goes on the clipboard. The origin gets its
    /// scheme back here: a link pasted into a chat app without one is a
    /// link that may not resolve.
    pub fn absolute(&self) -> String {
        let path = self.path();
        match self.origin.as_str() {
            "" => path,
            origin if origin.contains("://") => format!("{origin}{path}"),
            origin => format!("https://{origin}{path}"),
        }
    }

    /// The uid's own segment as the hero paints it: `-prj…` after the slug,
    /// the bare uid when there is no slug to hang it off, and nothing at
    /// all for an example (the slug IS the whole address).
    pub fn uid_segment(&self) -> String {
        let Some(uid) = self.uid else {
            return String::new();
        };
        if self.slug.is_empty() {
            uid.to_string()
        } else {
            format!("-{uid}")
        }
    }
}

/// A project's canonical absolute link from loose parts — the card and
/// detail surfaces carry the uid as a string and only know a name/slug.
/// The uid is the load-bearing half; the slug is cosmetic and the address
/// bar heals it on open, so a dated library slug here is fine.
pub fn project_link_absolute(name: &str, uid: &str) -> String {
    let slug = share_link::slugify(name);
    match uid.parse::<PrefixedUid>() {
        Ok(uid) => ShareUrl {
            origin: current_origin(),
            slug,
            uid: Some(uid),
        }
        .absolute(),
        // A malformed uid never happens in practice (these come off view
        // models), but a lenient fallback beats a panic in a click handler.
        Err(_) => {
            let path = if slug.is_empty() {
                format!("/p/{uid}")
            } else {
                format!("/p/{slug}-{uid}")
            };
            let origin = current_origin();
            if origin.is_empty() {
                path
            } else {
                format!("https://{origin}{path}")
            }
        }
    }
}

/// This page's origin, scheme stripped, for the hero's dim prefix.
///
/// Empty when there is no window (host builds, the story book's own
/// fixtures pass an origin explicitly) — the hero then paints the path
/// alone, which is honest rather than a guess at somebody's deployment.
#[cfg(target_arch = "wasm32")]
pub fn current_origin() -> String {
    web_sys::window()
        .and_then(|window| window.location().host().ok())
        .unwrap_or_default()
}

#[cfg(not(target_arch = "wasm32"))]
pub fn current_origin() -> String {
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_history::UidPrefix;

    fn uid() -> PrefixedUid {
        PrefixedUid::mint(UidPrefix::Project, &[7u8; 16])
    }

    #[test]
    fn a_slugged_link_reads_as_the_address_bar_shows_it() {
        let url = ShareUrl {
            origin: "lightplayer.app".to_string(),
            slug: "radiance-dome".to_string(),
            uid: Some(uid()),
        };
        assert_eq!(url.path(), format!("/p/radiance-dome-{}", uid()));
        assert_eq!(url.uid_segment(), format!("-{}", uid()));
        assert_eq!(
            url.absolute(),
            format!("https://lightplayer.app/p/radiance-dome-{}", uid())
        );
    }

    /// A name with nothing to keep gets the bare-uid URL — never a fake
    /// slug, and never a dangling hyphen.
    #[test]
    fn a_slugless_link_is_the_bare_uid() {
        let url = ShareUrl {
            origin: "lightplayer.app".to_string(),
            slug: String::new(),
            uid: Some(uid()),
        };
        assert_eq!(url.path(), format!("/p/{}", uid()));
        assert_eq!(url.uid_segment(), uid().to_string());
    }

    /// An embedded example's address is the bare `/p/<slug>`: its transient
    /// uid is RAM-minted and must never reach a URL, so the hero paints no
    /// uid segment at all and the copied link carries none either.
    #[test]
    fn an_example_link_is_the_bare_slug() {
        let url = ShareUrl {
            origin: "lightplayer.app".to_string(),
            slug: "small-dome".to_string(),
            uid: None,
        };
        assert_eq!(url.path(), "/p/small-dome");
        assert_eq!(url.uid_segment(), "");
        assert_eq!(url.absolute(), "https://lightplayer.app/p/small-dome");
    }

    /// The dev server is `127.0.0.1:2820` over http; an origin that
    /// already carries its scheme must not be given a second one.
    #[test]
    fn an_explicit_scheme_survives() {
        let url = ShareUrl {
            origin: "http://127.0.0.1:2820".to_string(),
            slug: "zook-dome".to_string(),
            uid: Some(uid()),
        };
        assert_eq!(
            url.absolute(),
            format!("http://127.0.0.1:2820/p/zook-dome-{}", uid())
        );
    }

    /// No window, no origin: the path alone is honest.
    #[test]
    fn no_origin_copies_the_path() {
        let url = ShareUrl {
            origin: String::new(),
            slug: "zook-dome".to_string(),
            uid: Some(uid()),
        };
        assert_eq!(url.absolute(), url.path());
    }
}
