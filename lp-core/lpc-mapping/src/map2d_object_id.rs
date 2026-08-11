//! Stable per-object identity for mapping documents (format 3).
//!
//! An id is a **sticky slug**: assigned once — derived from the object's
//! authored `name` at assignment time — and never rewritten by rename.
//! Patch documents address objects by id path (`/sector/2`, see
//! [`crate::PatchDoc`]), and the editor's document-in/edits-out boundary
//! cannot reach sibling patch files to fix the references a rewrite would
//! break, so a renamed object simply keeps its id. A diverged id is a
//! visible property, not an error.

use alloc::collections::BTreeSet;
use alloc::format;
use alloc::string::{String, ToString};

use serde::{Deserialize, Deserializer, Serialize};

use crate::map2d_doc::Map2dDoc;

/// Longest id, in bytes (the charset is ASCII, so bytes = chars).
pub const MAP2D_OBJECT_ID_MAX_LEN: usize = 24;

/// A stable object id: a lowercase slug, `[a-z][a-z0-9_-]*`, at most
/// [`MAP2D_OBJECT_ID_MAX_LEN`] bytes.
///
/// The mandatory leading letter is load-bearing: patch paths interleave id
/// segments with integer repeat-instance segments (`/sector/2`), so an id
/// that could parse as an integer would make the grammar ambiguous.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct Map2dObjectId(String);

impl Map2dObjectId {
    /// Validate and wrap an id. The error string names the violated rule.
    pub fn new(id: &str) -> Result<Self, String> {
        if id.is_empty() {
            return Err("object id is empty".to_string());
        }
        if id.len() > MAP2D_OBJECT_ID_MAX_LEN {
            return Err(format!(
                "object id {id:?} is longer than {MAP2D_OBJECT_ID_MAX_LEN} bytes"
            ));
        }
        let mut chars = id.chars();
        let first = chars.next().expect("non-empty checked above");
        if !first.is_ascii_lowercase() {
            return Err(format!(
                "object id {id:?} must start with a lowercase letter"
            ));
        }
        if !chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-') {
            return Err(format!(
                "object id {id:?} may only contain [a-z0-9_-] after the first letter"
            ));
        }
        Ok(Self(id.to_string()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Derive a slug from an authored object name: lowercased, runs of
    /// non-slug characters collapsed to one `-`, leading non-letters
    /// dropped (the grammar requires a letter first), truncated to fit.
    /// `None` when nothing slug-shaped survives (an unnamed object).
    #[must_use]
    pub fn slugify(name: &str) -> Option<Self> {
        let mut slug = String::new();
        let mut pending_dash = false;
        for c in name.chars() {
            let c = c.to_ascii_lowercase();
            let keep = c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-';
            if slug.is_empty() {
                // The first kept character must be a letter.
                if c.is_ascii_lowercase() {
                    slug.push(c);
                }
                continue;
            }
            if keep {
                if pending_dash {
                    slug.push('-');
                    pending_dash = false;
                }
                slug.push(c);
            } else {
                pending_dash = true;
            }
        }
        slug.truncate(MAP2D_OBJECT_ID_MAX_LEN);
        while slug.ends_with('-') {
            slug.pop();
        }
        Self::new(&slug).ok()
    }
}

impl core::fmt::Display for Map2dObjectId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Map2dObjectId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Self::new(&raw).map_err(serde::de::Error::custom)
    }
}

/// Assign ids to every object that lacks one. Returns whether the document
/// changed.
///
/// Ids derive from the authored `name` ([`Map2dObjectId::slugify`]),
/// uniquified with a numeric suffix on collision; unnameable objects fall
/// back to `obj-<wiring-index+1>`. Objects that already carry an id are
/// never touched — this is the "sticky" in sticky slug. Callers invoke this
/// deliberately (entering patch mode, an explicit editor action); nothing
/// runs it spontaneously, so documents without ids stay format ≤ 2.
pub fn ensure_object_ids(doc: &mut Map2dDoc) -> bool {
    let mut taken: BTreeSet<String> = doc
        .objects
        .iter()
        .filter_map(|object| object.id.as_ref())
        .map(|id| id.as_str().to_string())
        .collect();
    let mut changed = false;
    for index in 0..doc.objects.len() {
        if doc.objects[index].id.is_some() {
            continue;
        }
        let base =
            Map2dObjectId::slugify(&doc.objects[index].name).unwrap_or_else(|| fallback_id(index));
        let id = uniquify(&base, &taken);
        taken.insert(id.as_str().to_string());
        doc.objects[index].id = Some(id);
        changed = true;
    }
    changed
}

/// `obj-<n>` for an object whose name yields no slug; `n` is the 1-based
/// wiring index, which reads naturally against the rail.
fn fallback_id(index: usize) -> Map2dObjectId {
    Map2dObjectId::new(&format!("obj-{}", index + 1)).expect("fallback id is a valid slug")
}

/// `base`, else `base-2`, `base-3`, … — the suffix trimmed into the length
/// cap by shortening the base, never the counter.
fn uniquify(base: &Map2dObjectId, taken: &BTreeSet<String>) -> Map2dObjectId {
    if !taken.contains(base.as_str()) {
        return base.clone();
    }
    for counter in 2u32.. {
        let suffix = format!("-{counter}");
        let mut stem = base.as_str().to_string();
        stem.truncate(MAP2D_OBJECT_ID_MAX_LEN.saturating_sub(suffix.len()));
        while stem.ends_with('-') {
            stem.pop();
        }
        let candidate = format!("{stem}{suffix}");
        if !taken.contains(candidate.as_str()) {
            return Map2dObjectId::new(&candidate).expect("uniquified id keeps the slug charset");
        }
    }
    unreachable!("u32 counter space exceeds any document's object count")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map2d_doc::{Map2dObject, Map2dShape, PathShape};
    use alloc::vec;
    use alloc::vec::Vec;

    #[test]
    fn accepts_slugs_and_refuses_everything_else() {
        assert!(Map2dObjectId::new("sector").is_ok());
        assert!(Map2dObjectId::new("a").is_ok());
        assert!(Map2dObjectId::new("box-5_x2").is_ok());
        assert!(Map2dObjectId::new("").is_err());
        assert!(Map2dObjectId::new("Sector").is_err());
        assert!(
            Map2dObjectId::new("2sector").is_err(),
            "leading digit would collide with instance segments"
        );
        assert!(Map2dObjectId::new("-sector").is_err());
        assert!(Map2dObjectId::new("sec tor").is_err());
        assert!(Map2dObjectId::new("sec/tor").is_err());
        assert!(Map2dObjectId::new(&"a".repeat(MAP2D_OBJECT_ID_MAX_LEN)).is_ok());
        assert!(Map2dObjectId::new(&"a".repeat(MAP2D_OBJECT_ID_MAX_LEN + 1)).is_err());
    }

    #[test]
    fn slugify_lowercases_collapses_and_leads_with_a_letter() {
        assert_eq!(Map2dObjectId::slugify("Sector").unwrap().as_str(), "sector");
        assert_eq!(
            Map2dObjectId::slugify("Left  Door (front)")
                .unwrap()
                .as_str(),
            "left-door-front"
        );
        assert_eq!(
            Map2dObjectId::slugify("12 rings").unwrap().as_str(),
            "rings"
        );
        assert_eq!(Map2dObjectId::slugify("grid 3").unwrap().as_str(), "grid-3");
        assert_eq!(Map2dObjectId::slugify(""), None);
        assert_eq!(Map2dObjectId::slugify("123"), None);
        assert_eq!(Map2dObjectId::slugify("!!!"), None);
        // Truncation lands inside the cap and never ends on a dash.
        let long = Map2dObjectId::slugify("a very long object name that keeps going").unwrap();
        assert!(long.as_str().len() <= MAP2D_OBJECT_ID_MAX_LEN);
        assert!(!long.as_str().ends_with('-'));
    }

    #[test]
    fn ensure_assigns_sticky_unique_ids_and_skips_existing() {
        let mut doc = doc_with_names(&["Sector", "sector", "", "Sector"]);
        doc.objects[1].id = Some(Map2dObjectId::new("kept").unwrap());
        assert!(ensure_object_ids(&mut doc));
        let ids: Vec<&str> = doc
            .objects
            .iter()
            .map(|object| object.id.as_ref().unwrap().as_str())
            .collect();
        assert_eq!(ids, vec!["sector", "kept", "obj-3", "sector-2"]);
        // Idempotent: a second pass changes nothing.
        assert!(!ensure_object_ids(&mut doc));
    }

    #[test]
    fn uniquify_shortens_the_stem_never_the_counter() {
        let base = Map2dObjectId::new(&"a".repeat(MAP2D_OBJECT_ID_MAX_LEN)).unwrap();
        let mut taken = BTreeSet::new();
        taken.insert(base.as_str().to_string());
        let next = uniquify(&base, &taken);
        assert!(next.as_str().len() <= MAP2D_OBJECT_ID_MAX_LEN);
        assert!(next.as_str().ends_with("-2"));
    }

    fn doc_with_names(names: &[&str]) -> Map2dDoc {
        let mut doc = Map2dDoc::new();
        for name in names {
            doc.objects.push(Map2dObject {
                name: name.to_string(),
                id: None,
                stride: None,
                shape: Map2dShape::Path(PathShape {
                    points: vec![[0.0, 0.0], [10.0, 0.0]],
                    count: 2,
                    reversed: false,
                    gaps: Vec::new(),
                }),
            });
        }
        doc
    }
}
