//! Patch source identity: a path into the mapping's object tree (D46).
//!
//! Grammar: `/<object-id>[/<instance>]*` — the leading segment is a stable
//! object id ([`crate::Map2dObjectId`], always starting with a letter), and
//! every following segment is an integer repeat-instance index (`/sector/2`
//! is instance 2 of the repeat object `sector`; `/panels/3/2` descends a
//! nested repeat). The charsets are disjoint by construction — an id can
//! never parse as an integer — so the grammar needs no escapes.
//!
//! Instance identity is *intrinsic* (rotation order `k`): an entry
//! addressing `/sector/2` means "instance 2, wherever its lamps currently
//! are". Lamp ranges re-derive from the mapping at resolve time, so the
//! dome that grows two lamps per strut next year keeps every patch entry
//! pointing at the right physical panel. Shrinking a repeat below `k`
//! dangles the entry — degrade and report, same as an unknown id.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::map2d_object_id::Map2dObjectId;

/// A parsed object path: the addressed object plus zero or more
/// repeat-instance steps.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MapObjectPath {
    pub id: Map2dObjectId,
    /// Instance indices, outermost repeat first. Empty = the whole object.
    pub instances: Vec<u32>,
}

impl MapObjectPath {
    /// The whole object, no instance steps.
    #[must_use]
    pub fn object(id: Map2dObjectId) -> Self {
        Self {
            id,
            instances: Vec::new(),
        }
    }

    /// Parse `/sector/2`-style text. The error string names the violated
    /// rule.
    pub fn parse(text: &str) -> Result<Self, String> {
        let Some(rest) = text.strip_prefix('/') else {
            return Err(format_error(text, "must start with '/'"));
        };
        let mut segments = rest.split('/');
        let id_segment = segments.next().unwrap_or_default();
        let id = Map2dObjectId::new(id_segment).map_err(|reason| format_error(text, &reason))?;
        let mut instances = Vec::new();
        for segment in segments {
            let instance: u32 = segment
                .parse()
                .map_err(|_| format_error(text, "instance segments must be integers"))?;
            // Refuse `/sector/007`: a non-canonical spelling would make two
            // texts name one instance and break byte-stable round-trips.
            if segment != instance.to_string() {
                return Err(format_error(
                    text,
                    "instance segments must be canonical integers",
                ));
            }
            instances.push(instance);
        }
        Ok(Self { id, instances })
    }

    /// The canonical text form (`/sector/2`).
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut text = String::new();
        text.push('/');
        text.push_str(self.id.as_str());
        for instance in &self.instances {
            text.push('/');
            text.push_str(&instance.to_string());
        }
        text
    }
}

impl core::fmt::Display for MapObjectPath {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.to_text())
    }
}

fn format_error(text: &str, reason: &str) -> String {
    alloc::format!("invalid object path {text:?}: {reason}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn parses_and_round_trips_the_grammar() {
        for (text, instances) in [
            ("/sector", vec![]),
            ("/sector/2", vec![2]),
            ("/panels/3/2", vec![3, 2]),
            ("/box-5_x/0", vec![0]),
        ] {
            let path = MapObjectPath::parse(text).unwrap();
            assert_eq!(path.instances, instances, "{text}");
            assert_eq!(path.to_text(), text);
        }
    }

    #[test]
    fn refuses_malformed_paths() {
        for text in [
            "",          // no leading slash
            "sector",    // no leading slash
            "/",         // empty id
            "/2sector",  // id must start with a letter
            "/2",        // an integer cannot be an id
            "/sector/x", // instance must be an integer
            "/sector/-1",
            "/sector/007", // non-canonical integer spelling
            "/sector/",    // empty trailing segment
            "/Sector/2",   // ids are lowercase
        ] {
            assert!(
                MapObjectPath::parse(text).is_err(),
                "{text:?} should refuse"
            );
        }
    }
}
