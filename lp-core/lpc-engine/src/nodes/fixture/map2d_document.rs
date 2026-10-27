//! A fixture's `*.map2d.json`, read by its format: format 6 is one
//! fixture's parts and patching ([`Mapping`], `lpc_mapping::map2d6`), which
//! the fixture patches itself (`parts_patch.rs`); formats 1 to 5 are the
//! older layout document ([`Map2dDoc`]), which a project at format 12 holds
//! only by a hand edit or an older writer, and which the fixture refuses by
//! name (`fixture_node::OLDER_MAPPING_REFUSAL`, mapping rebuild M3).
//!
//! **Today's reader reads first, unchanged.** A format 1 to 5 document costs
//! exactly what it did before format 6 existed (no second peek: one was 1
//! allocation and 8 B per fixture at load on the device, measured by the
//! engine heap ratchet), and today's words for "not a map2d document" stand.
//! Only a document today's reader refuses as newer goes on to the format-6
//! reader, which reads 6 and refuses anything newer naming 6.

use alloc::string::{String, ToString};

use lpc_mapping::map2d6::{self, MAP2D6_FORMAT};
use lpc_mapping::part::Mapping;
use lpc_mapping::{Map2dDoc, Map2dError};

/// A map2d document, read.
pub(crate) enum Map2dDocument {
    /// Formats 1 to 5: an older layout document, refused in a format-12
    /// project.
    Layout,
    /// Format 6: parts and patching.
    Parts(Mapping),
}

/// Read a map2d document by its format; `Err` is the reader's refusal, in
/// its own words.
pub(crate) fn read_map2d_document(text: &str) -> Result<Map2dDocument, String> {
    match Map2dDoc::from_json(text) {
        Ok(_) => Ok(Map2dDocument::Layout),
        Err(Map2dError::UnsupportedFormat { found, .. }) if found >= MAP2D6_FORMAT => {
            map2d6::read(text)
                .map(Map2dDocument::Parts)
                .map_err(|error| error.to_string())
        }
        Err(error) => Err(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A format-5 document reads as today's layout.
    #[test]
    fn a_format_5_document_reads_as_todays_layout() {
        let text = r#"{ "format": 5, "objects": [] }"#;
        assert!(matches!(
            read_map2d_document(text),
            Ok(Map2dDocument::Layout)
        ));
    }

    /// A format-6 document reads as parts and patching.
    #[test]
    fn a_format_6_document_reads_as_parts() {
        let text = r#"{ "format": 6, "parts": [] }"#;
        assert!(matches!(
            read_map2d_document(text),
            Ok(Map2dDocument::Parts(_))
        ));
    }

    /// A document newer than this build reads is refused, naming 6.
    #[test]
    fn a_newer_document_is_refused_naming_6() {
        let Err(error) = read_map2d_document(r#"{ "format": 7, "parts": [] }"#) else {
            panic!("format 7 read");
        };
        assert_eq!(
            error,
            "unsupported map2d format 7 (this build reads up to 6)"
        );
    }

    /// Text that isn't a map2d document keeps today's reader's words.
    #[test]
    fn text_that_isnt_a_document_keeps_todays_words() {
        let ours = read_map2d_document("not json").err();
        let todays = Map2dDoc::from_json("not json")
            .err()
            .map(|error| error.to_string());
        assert!(ours.is_some());
        assert_eq!(ours, todays);
    }
}
