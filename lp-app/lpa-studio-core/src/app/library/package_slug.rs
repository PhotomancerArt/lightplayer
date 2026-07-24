//! Human-friendly slugs — THE user-facing project identifier.
//!
//! The slug names the package directory, titles the gallery card and the
//! editor, rides the URL (`#/sim/<slug>`), and names exports. The
//! `prj_…` uid stays the true identity underneath (history, device
//! associations, renames all key off it) — a rename changes the slug and
//! moves the directory without touching identity.
//!
//! New packages get **date-based slugs**: `2026-07-09-1421-basic` (local
//! date, time, source label). The wall-clock stamp is injected
//! ([`LibraryStore`](super::LibraryStore) construction) per the sans-IO
//! discipline — core never reads a clock or a timezone.

/// Slugify a display name: lowercase ASCII alphanumerics and hyphens.
pub fn slugify(name: &str) -> String {
    let mut slug = String::new();
    let mut last_hyphen = true; // suppress leading hyphens
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
            last_hyphen = false;
        } else if !last_hyphen {
            slug.push('-');
            last_hyphen = true;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        "project".to_string()
    } else {
        slug
    }
}

/// First of `slug`, `slug-2`, `slug-3`, … not present in `taken`.
pub fn unique_slug(name: &str, taken: &[String]) -> String {
    let base = slugify(name);
    if !taken.iter().any(|t| t == &base) {
        return base;
    }
    for i in 2.. {
        let candidate = format!("{base}-{i}");
        if !taken.iter().any(|t| t == &candidate) {
            return candidate;
        }
    }
    unreachable!("unbounded suffix search")
}

/// The date-based slug for a new package: `<stamp>-<label>`, uniqued.
/// `stamp` is the injected local `YYYY-MM-DD-HHMM`; `label` is the source
/// name (example name, zip stem, duplicate source) with any date-time
/// prefix of its own stripped so dates never stack.
pub fn dated_slug(stamp: &str, label: &str, taken: &[String]) -> String {
    let slugified = slugify(label);
    let label = strip_date_prefix(&slugified);
    unique_slug(&format!("{stamp}-{label}"), taken)
}

/// Strip a leading `YYYY-MM-DD-` or `YYYY-MM-DD-HHMM-` from an
/// already-slugified name (re-importing/duplicating a dated package must
/// not stack stamps). Returns `"project"` when nothing remains.
pub fn strip_date_prefix(slug: &str) -> &str {
    let rest = strip_one_date_prefix(slug);
    if rest.is_empty() { "project" } else { rest }
}

fn strip_one_date_prefix(slug: &str) -> &str {
    let bytes = slug.as_bytes();
    let digits = |range: core::ops::Range<usize>| {
        bytes.len() > range.end - 1 && bytes[range].iter().all(u8::is_ascii_digit)
    };
    let dash_at = |i: usize| bytes.get(i) == Some(&b'-');
    // YYYY-MM-DD
    if !(bytes.len() >= 10
        && digits(0..4)
        && dash_at(4)
        && digits(5..7)
        && dash_at(7)
        && digits(8..10))
    {
        return slug;
    }
    // optional -HHMM
    if bytes.len() >= 15 && dash_at(10) && digits(11..15) {
        return slug.get(16..).unwrap_or("");
    }
    if dash_at(10) {
        return slug.get(11..).unwrap_or("");
    }
    // the whole slug is just a date
    if bytes.len() == 10 {
        return "";
    }
    slug
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugifies_names() {
        assert_eq!(slugify("Fyeah Sign!"), "fyeah-sign");
        assert_eq!(slugify("  Luna's Porch  "), "luna-s-porch");
        assert_eq!(slugify("电"), "project");
    }

    #[test]
    fn suffixes_collisions() {
        let taken = vec!["basic".to_string(), "basic-2".to_string()];
        assert_eq!(unique_slug("Basic", &taken), "basic-3");
        assert_eq!(unique_slug("Other", &taken), "other");
    }

    #[test]
    fn dated_slugs_stamp_and_unique() {
        let taken = vec!["2026-07-09-1421-basic".to_string()];
        assert_eq!(
            dated_slug("2026-07-09-1421", "Basic", &taken),
            "2026-07-09-1421-basic-2"
        );
        assert_eq!(
            dated_slug("2026-07-09-1500", "Basic", &taken),
            "2026-07-09-1500-basic"
        );
    }

    #[test]
    fn date_prefixes_never_stack() {
        assert_eq!(strip_date_prefix("2026-07-08-1851-basic"), "basic");
        assert_eq!(strip_date_prefix("2026-07-08-basic"), "basic");
        assert_eq!(strip_date_prefix("basic"), "basic");
        assert_eq!(strip_date_prefix("2026-07-08-1851"), "project");
        assert_eq!(strip_date_prefix("2026-07-08"), "project");
        // date-like tails inside the label survive
        assert_eq!(
            strip_date_prefix("party-2026-07-04-mix"),
            "party-2026-07-04-mix"
        );
        assert_eq!(
            dated_slug("2026-07-09-1500", "2026-07-08-1851-basic", &[]),
            "2026-07-09-1500-basic"
        );
    }
}
