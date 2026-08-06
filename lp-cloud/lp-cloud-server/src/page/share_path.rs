//! Reading a project uid out of a share URL.
//!
//! A share URL is `/p/<slug>-prj_<16 base62>` (D24). The uid is the whole of
//! the identity — it is the link token, 95 bits of it — and the slug in
//! front of it is **cosmetic**: it exists so a pasted link says what it
//! points at, and it is ignored here. Renaming a project therefore never
//! breaks a link that is already in somebody's chat history, and the same
//! project reached through two different slugs is the same project.
//!
//! This is the server half of the rule; `lpa-studio-web`'s router parses the
//! same shape client-side (P09).

use lpc_history::{PrefixedUid, UidPrefix};

/// The uid a share path points at, or `None` if there is not one in it.
///
/// Only the last segment is examined: `/p/zook-dome-prj_XXXXXXXXXXXXXXXX`
/// and `/p/anything/else/prj_XXXXXXXXXXXXXXXX` both resolve, and a path with
/// no uid resolves to nothing rather than to a guess.
pub fn project_uid(share_path: &str) -> Option<PrefixedUid> {
    let segment = share_path
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|segment| !segment.is_empty())?;

    // The slug may itself contain '-', and a base62 uid body may contain
    // anything alphanumeric, so the split point is the LAST `prj_`. Parsing
    // is strict about the 16-character body, which is what makes trailing
    // junk a miss rather than a truncation.
    let start = segment.rfind("prj_")?;
    let uid: PrefixedUid = segment[start..].parse().ok()?;
    (uid.prefix() == UidPrefix::Project).then_some(uid)
}

/// The canonical share path for a project: cosmetic slug, load-bearing uid.
pub fn canonical(slug: &str, uid: PrefixedUid) -> String {
    if slug.is_empty() {
        format!("/p/{uid}")
    } else {
        format!("/p/{slug}-{uid}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_uid_out_of_a_decorated_path() {
        let uid = PrefixedUid::mint(UidPrefix::Project, &[7u8; 16]);
        for path in [
            format!("zook-dome-{uid}"),
            format!("{uid}"),
            format!("/{uid}"),
            format!("a-very-long-project-name-{uid}/"),
        ] {
            assert_eq!(project_uid(&path), Some(uid), "for {path}");
        }
    }

    /// The slug is decoration, so two different slugs on one uid are one
    /// project — the property that makes renaming safe.
    #[test]
    fn the_slug_is_ignored_entirely() {
        let uid = PrefixedUid::mint(UidPrefix::Project, &[3u8; 16]);
        assert_eq!(
            project_uid(&format!("old-name-{uid}")),
            project_uid(&format!("new-name-{uid}"))
        );
    }

    #[test]
    fn a_path_without_a_uid_resolves_to_nothing() {
        for path in [
            "",
            "/",
            "zook-dome",
            "prj_tooshort",
            "prj_0000000000000000extra",
            "usr_0000000000000000",
        ] {
            assert_eq!(project_uid(path), None, "for {path:?}");
        }
    }

    #[test]
    fn canonical_round_trips_through_the_parser() {
        let uid = PrefixedUid::mint(UidPrefix::Project, &[9u8; 16]);
        let path = canonical("zook-dome", uid);
        assert_eq!(path, format!("/p/zook-dome-{uid}"));
        assert_eq!(project_uid(&path), Some(uid));
        assert_eq!(project_uid(&canonical("", uid)), Some(uid));
    }
}
