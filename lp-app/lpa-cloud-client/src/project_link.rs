//! The share link: a slug for humans, a uid for the machine.

use alloc::string::{String, ToString};
use core::fmt;
use core::str::FromStr;

use lpc_history::{PrefixedUid, UidParseError, UidPrefix};

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
        if self.slug.is_empty() {
            write!(f, "/p/{}", self.uid)
        } else {
            write!(f, "/p/{}-{}", self.slug, self.uid)
        }
    }
}

/// Why a string is not a share link.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectLinkParseError {
    /// Nothing in the string parses as a uid.
    MissingUid(UidParseError),
    /// A uid parsed, but it names something other than a project.
    NotAProject(UidPrefix),
}

impl fmt::Display for ProjectLinkParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProjectLinkParseError::MissingUid(e) => write!(f, "no project uid in link: {e}"),
            ProjectLinkParseError::NotAProject(prefix) => {
                write!(f, "link names a {prefix} uid, not a project")
            }
        }
    }
}

/// Accepts anything that *contains* the canonical tail: a full URL, a path,
/// `slug-prj_…`, or a bare `prj_…`. The last `-` separates slug from uid,
/// which is unambiguous because a uid body is base-62 and its prefix has no
/// hyphen.
impl FromStr for ProjectLink {
    type Err = ProjectLinkParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let tail = s.trim_end_matches('/').rsplit('/').next().unwrap_or(s);
        let (slug, uid) = match tail.rsplit_once('-') {
            Some((slug, uid)) => (slug, uid),
            None => ("", tail),
        };
        let uid: PrefixedUid = uid.parse().map_err(ProjectLinkParseError::MissingUid)?;
        if uid.prefix() != UidPrefix::Project {
            return Err(ProjectLinkParseError::NotAProject(uid.prefix()));
        }
        Ok(Self {
            slug: slug.to_string(),
            uid,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            Err(ProjectLinkParseError::NotAProject(UidPrefix::Device))
        );
        assert!(matches!(
            "/p/not-a-link".parse::<ProjectLink>(),
            Err(ProjectLinkParseError::MissingUid(_))
        ));
    }

    fn project() -> PrefixedUid {
        PrefixedUid::mint(UidPrefix::Project, &[7u8; 16])
    }
}
