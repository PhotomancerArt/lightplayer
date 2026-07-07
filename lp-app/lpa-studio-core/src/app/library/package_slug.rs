//! Human-friendly directory slugs. Dirs are not identity — the uid is.

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
}
