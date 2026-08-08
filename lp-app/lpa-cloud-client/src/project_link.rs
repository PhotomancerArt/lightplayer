//! The share link: a slug for humans, a uid for the machine.

use alloc::string::{String, ToString};
use core::fmt;
use core::str::FromStr;

use lpc_cloud_api::share_link;
use lpc_history::PrefixedUid;

/// A project's canonical share address, `/p/<slug>-<uid>`.
///
/// The uid is the whole of the identity **and** the whole of the access
/// token: 95 bits of keyspace, so holding the link *is* the permission on a
/// `Visibility::Link` project. The slug is decoration for the address bar and
/// carries no meaning — two links with different slugs and the same uid are
/// the same project, and parsing deliberately ignores the slug rather than
/// validating it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectLink {
    /// Human-readable decoration. May be empty.
    pub slug: String,
    /// The project uid — identity and link token both.
    pub uid: PrefixedUid,
}

impl ProjectLink {
    /// A link for a project.
    pub fn new(slug: impl Into<String>, uid: PrefixedUid) -> Self {
        Self {
            slug: slug.into(),
            uid,
        }
    }

    /// The path form, ready to append to an origin.
    pub fn path(&self) -> String {
        self.to_string()
    }
}

impl fmt::Display for ProjectLink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&share_link::canonical_path(&self.slug, self.uid))
    }
}

/// Why a string is not a share link: the shared grammar
/// ([`lpc_cloud_api::share_link`]) found no project uid in it, whether
/// because nothing parsed as a uid at all or because a uid parsed but
/// named something other than a project.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectLinkParseError;

impl fmt::Display for ProjectLinkParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("no project uid in link")
    }
}

/// Accepts anything that *contains* the canonical tail: a full URL, a path,
/// `slug-prj…`, or a bare `prj…`, case-folded and junk-trimmed per the
/// shared grammar (D10) — delegates entirely to
/// [`lpc_cloud_api::share_link::split_segment`] rather than re-deriving the
/// fold/trim/split rule.
impl FromStr for ProjectLink {
    type Err = ProjectLinkParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let segment = s.trim_end_matches('/').rsplit('/').next().unwrap_or(s);
        let (slug, uid) = share_link::split_segment(segment).ok_or(ProjectLinkParseError)?;
        Ok(Self { slug, uid })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_history::UidPrefix;

    #[test]
    fn round_trips_through_the_canonical_path() {
        let link = ProjectLink::new("zook-dome", project());
        assert_eq!(link.path(), alloc::format!("/p/zook-dome-{}", project()));
        assert_eq!(link.path().parse::<ProjectLink>().unwrap(), link);
    }

    /// A hyphenated slug is the normal case, and the uid still comes off
    /// cleanly.
    #[test]
    fn parses_a_full_url_and_a_bare_uid_alike() {
        let uid = project();
        for form in [
            alloc::format!("https://lightplayer.app/p/zook-dome-{uid}"),
            alloc::format!("/p/zook-dome-{uid}"),
            alloc::format!("zook-dome-{uid}"),
        ] {
            assert_eq!(form.parse::<ProjectLink>().unwrap().uid, uid);
            assert_eq!(form.parse::<ProjectLink>().unwrap().slug, "zook-dome");
        }
        let bare: ProjectLink = uid.to_string().parse().unwrap();
        assert_eq!(bare.uid, uid);
        assert!(bare.slug.is_empty());
        assert_eq!(bare.path(), alloc::format!("/p/{uid}"));
    }

    #[test]
    fn refuses_a_uid_that_is_not_a_project() {
        let device = PrefixedUid::mint(UidPrefix::Device, &[1u8; 16]);
        assert_eq!(
            device.to_string().parse::<ProjectLink>(),
            Err(ProjectLinkParseError)
        );
        assert_eq!(
            "/p/not-a-link".parse::<ProjectLink>(),
            Err(ProjectLinkParseError)
        );
    }

    /// D10: the whole URL survives case mangling, and trailing sentence
    /// punctuation is junk, not part of the link — same rule as
    /// `lp-cloud-server`'s `page::share_path` and the Studio router.
    #[test]
    fn survives_case_mangling_and_trailing_punctuation() {
        let uid = project();
        let mangled = alloc::format!("https://lightplayer.app/p/zook-dome-{uid}").to_uppercase();
        assert_eq!(mangled.parse::<ProjectLink>().unwrap().uid, uid);

        let decorated = alloc::format!("see /p/zook-dome-{uid}).");
        assert_eq!(decorated.parse::<ProjectLink>().unwrap().uid, uid);
    }

    fn project() -> PrefixedUid {
        PrefixedUid::mint(UidPrefix::Project, &[7u8; 16])
    }
}
