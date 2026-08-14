//! The one URL grammar for project share links: `/p/<slug>-<uid>`, or bare
//! `/p/<uid>` when the slug is empty (vision D1/D9/D10/D11 — settled).
//!
//! The slug is lowercase `[a-z0-9-]`, cosmetic, and mutable; the uid is a
//! [`PrefixedUid`] naming a project — authoritative, forever, and (for a
//! shared project) the whole of the access token. Every parser
//! in the tree delegates here instead of re-implementing the split: the
//! cloud server's page-plane (`lp-cloud-server::page::share_path`), the
//! Studio router (`lpa-studio-web::router`), and `lpa-cloud-client`'s
//! `ProjectLink`. A second implementation drifted before this module
//! existed — pre-#384 one of them split on the last `prj_`, which stopped
//! matching once uids dropped the underscore separator.

use alloc::format;
use alloc::string::{String, ToString};

use lpc_history::{PrefixedUid, UidPrefix};
use unicode_normalization::UnicodeNormalization;

/// Cap on a slug's length (D11): [`slugify`] never produces more, and
/// `validate_slug` in `lp-cloud-domain::cloud_service` (the only other
/// place a length limit applies — a client-supplied slug at `PublishProject`
/// time) enforces the same number so a hand-typed slug can never exceed
/// what generation itself would ever produce. Long enough to keep a name
/// recognizable, short enough that a `<slug>-<uid>` address stays a single
/// readable line.
pub const MAX_SLUG_CHARS: usize = 48;

/// Build a URL-safe slug from a project's display name (D11).
///
/// NFKD-normalizes, strips combining marks (accents fold off: `é` → `e`),
/// drops apostrophes outright (`'` `'` `'` — so `Yona's` → `yonas`, not
/// `yona-s`), lowercases, and maps every remaining run of non-`[a-z0-9]`
/// characters to a single `-`. A name with nothing left to keep
/// (emoji-only, CJK-only) yields the empty string — callers fall back to
/// the bare-uid path rather than invent a fake slug.
pub fn slugify(name: &str) -> String {
    let folded: String = name
        .nfkd()
        .filter(|c| !unicode_normalization::char::is_combining_mark(*c))
        .filter(|c| !matches!(c, '\'' | '\u{2019}' | '\u{2018}'))
        .collect::<String>()
        .to_lowercase();

    let mut slug = String::with_capacity(folded.len());
    let mut last_was_dash = false;
    for c in folded.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c);
            last_was_dash = false;
        } else if !last_was_dash {
            slug.push('-');
            last_was_dash = true;
        }
    }

    let trimmed = slug.trim_matches('-');
    let capped = if trimmed.len() > MAX_SLUG_CHARS {
        trimmed[..MAX_SLUG_CHARS].trim_end_matches('-')
    } else {
        trimmed
    };
    capped.to_string()
}

/// The project uid named by a share path or full URL, or `None` if the
/// final segment does not name one.
///
/// Only the last `/`-separated segment is examined — `/p/zook-dome-prj…`
/// and a full `https://…/p/zook-dome-prj…` both resolve the same way, and a
/// path with nothing after the last slash resolves to nothing rather than
/// to a guess.
pub fn parse_path(path_or_url: &str) -> Option<PrefixedUid> {
    let segment = path_or_url
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|segment| !segment.is_empty())?;
    parse_segment(segment)
}

/// The project uid named by one path segment (`slug-prj…`, bare `prj…`, or
/// junk-decorated variants of either), or `None`.
///
/// See [`split_segment`] for the rule; this keeps only the uid half of it.
pub fn parse_segment(segment: &str) -> Option<PrefixedUid> {
    split_segment(segment).map(|(_slug, uid)| uid)
}

/// The `(slug, uid)` split of one path segment, if the segment names a
/// project — `None` otherwise. [`parse_segment`] is this with the slug
/// dropped; `lpa-cloud-client`'s `ProjectLink` uses the pair directly
/// rather than re-deriving the split (a second copy of this rule is
/// exactly what regressed pre-#384).
///
/// 1. Trim trailing ASCII punctuation a sentence glues on (`.,);:!?'"` and
///    the like — anything outside `[a-z0-9-]` once folded).
/// 2. ASCII-lowercase (D10: the whole URL survives case mangling; folding
///    happens here, not in [`PrefixedUid::from_str`], which is strict
///    lowercase) — used only to *classify* characters; the returned slug
///    keeps the segment's original casing, since it is cosmetic.
/// 3. Take the substring after the last `-` (or the whole segment if there
///    is none) — the slug may itself contain hyphens, but the uid alphabet
///    never does, so the last hyphen is always the split point.
/// 4. Parse that tail as a [`PrefixedUid`] and require a project prefix.
pub fn split_segment(segment: &str) -> Option<(String, PrefixedUid)> {
    let lowered = segment.to_ascii_lowercase();
    // ASCII case-folding never changes a string's byte length or its char
    // boundaries, so a length measured on `lowered` slices `segment` (the
    // original casing) safely too.
    let trimmed_len = lowered
        .trim_end_matches(|c: char| !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'))
        .len();
    let original_trimmed = &segment[..trimmed_len];
    let folded_trimmed = &lowered[..trimmed_len];

    let (slug, uid_str) = match folded_trimmed.rsplit_once('-') {
        Some((_, tail)) => {
            let split_at = trimmed_len - tail.len() - 1; // exclude the '-'
            (&original_trimmed[..split_at], tail)
        }
        None => ("", folded_trimmed),
    };

    let uid: PrefixedUid = uid_str.parse().ok()?;
    if uid.prefix() != UidPrefix::Project {
        return None;
    }
    Some((slug.to_string(), uid))
}

/// The canonical share path for a project: cosmetic slug in front,
/// load-bearing uid behind.
pub fn canonical_path(slug: &str, uid: PrefixedUid) -> String {
    if slug.is_empty() {
        format!("/p/{uid}")
    } else {
        format!("/p/{slug}-{uid}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_history::UID_BODY_LEN;

    // slugify -----------------------------------------------------------

    /// Pinned acceptance case (exact) — do not relitigate.
    #[test]
    fn slugify_pinned_acceptance_case() {
        assert_eq!(
            slugify(r#"Yona's "radiance dome" Doors"#),
            "yonas-radiance-dome-doors"
        );
    }

    #[test]
    fn slugify_examples() {
        assert_eq!(slugify("Ember — Field (v2)"), "ember-field-v2");
        assert_eq!(slugify("Crème brûlée nights"), "creme-brulee-nights");
    }

    #[test]
    fn slugify_emoji_only_is_empty() {
        assert_eq!(slugify("🎆🎇✨"), "");
    }

    #[test]
    fn slugify_cjk_only_is_empty() {
        assert_eq!(slugify("光の橋"), "");
    }

    #[test]
    fn slugify_caps_at_48_chars() {
        let name = "a".repeat(71);
        let slug = slugify(&name);
        assert_eq!(slug.len(), 48);
        assert!(!slug.ends_with('-'));
    }

    /// The 48-char cut can land exactly on a dash; the cap re-trims it
    /// rather than leaving a trailing `-`.
    #[test]
    fn slugify_re_trims_a_dash_landing_exactly_at_the_cut() {
        let name = format!("{} {}", "a".repeat(47), "b".repeat(10));
        let slug = slugify(&name);
        assert_eq!(slug, "a".repeat(47));
        assert!(!slug.ends_with('-'));
    }

    // parse ---------------------------------------------------------------

    fn uid() -> PrefixedUid {
        PrefixedUid::mint(UidPrefix::Project, &[7u8; 16])
    }

    #[test]
    fn parses_canonical_and_bare_uid_forms() {
        let u = uid();
        assert_eq!(parse_path(&format!("/p/zook-dome-{u}")), Some(u));
        assert_eq!(parse_path(&format!("/p/{u}")), Some(u));
    }

    /// The slug is decoration, so a renamed (stale) slug on the same uid
    /// resolves identically — the property that makes renaming safe.
    #[test]
    fn a_stale_slug_still_resolves_the_same_uid() {
        let u = uid();
        assert_eq!(
            parse_path(&format!("/p/old-name-{u}")),
            parse_path(&format!("/p/new-name-{u}"))
        );
    }

    #[test]
    fn a_slug_containing_hyphens_still_resolves() {
        let u = uid();
        assert_eq!(
            parse_path(&format!("/p/a-very-long-project-name-{u}")),
            Some(u)
        );
    }

    #[test]
    fn uppercase_whole_url_still_resolves() {
        let u = uid();
        let url = format!("https://lightplayer.app/p/zook-dome-{u}").to_uppercase();
        assert_eq!(parse_path(&url), Some(u));
    }

    #[test]
    fn trailing_sentence_punctuation_is_trimmed() {
        let u = uid();
        assert_eq!(parse_path(&format!("see /p/zook-dome-{u}).")), Some(u));
    }

    #[test]
    fn uid_with_excluded_confusable_letters_is_rejected() {
        // 'i' is a Crockford-excluded confusable, never in the alphabet.
        assert_eq!(parse_segment("prj000000000000000i"), None);
    }

    #[test]
    fn non_project_prefix_is_rejected() {
        let device = PrefixedUid::mint(UidPrefix::Device, &[1u8; 16]);
        assert_eq!(parse_segment(&device.to_string()), None);
    }

    #[test]
    fn wrong_length_body_is_rejected() {
        let short = format!("prj{}", "0".repeat(UID_BODY_LEN - 1));
        let long = format!("prj{}", "0".repeat(UID_BODY_LEN + 1));
        assert_eq!(parse_segment(&short), None);
        assert_eq!(parse_segment(&long), None);
    }

    #[test]
    fn a_segment_with_no_uid_resolves_to_nothing() {
        for segment in ["", "zook-dome", "usr0000000000000000"] {
            assert_eq!(parse_segment(segment), None, "for {segment:?}");
        }
    }

    #[test]
    fn round_trips_through_canonical_path() {
        let u = uid();
        for slug in ["zook-dome", ""] {
            assert_eq!(parse_path(&canonical_path(slug, u)), Some(u));
        }
    }

    // split_segment ---------------------------------------------------------

    #[test]
    fn split_segment_recovers_the_slug_half() {
        let u = uid();
        assert_eq!(
            split_segment(&format!("zook-dome-{u}")),
            Some(("zook-dome".to_string(), u))
        );
        assert_eq!(split_segment(&u.to_string()), Some((String::new(), u)));
    }

    /// The slug is cosmetic, so it keeps whatever casing the caller wrote —
    /// only the uid half is folded for classification and validation.
    #[test]
    fn split_segment_preserves_slug_casing_and_drops_trailing_junk() {
        let u = uid();
        let (slug, parsed) =
            split_segment(&format!("Zook-Dome-{}).", u.to_string().to_uppercase())).unwrap();
        assert_eq!(slug, "Zook-Dome");
        assert_eq!(parsed, u);
    }
}
