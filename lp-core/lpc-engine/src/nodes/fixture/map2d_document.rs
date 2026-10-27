//! A fixture's `*.map2d.json`, read by its format: formats 1 to 5 are the
//! layout document today's path resolves ([`Map2dDoc`]); format 6 is one
//! fixture's parts and patching ([`Mapping`], `lpc_mapping::map2d6`), which
//! the fixture patches itself (`parts_patch.rs`). Both kinds live side by
//! side until project format 12 moves every project to format 6 (mapping
//! rebuild M3).
//!
//! The format is peeked once and the document goes to the reader that owns
//! it. Anything from 6 up goes to the format-6 reader, so a newer document
//! is refused naming the newest this build reads (6); a document whose
//! format can't be peeked goes to today's reader, whose words for "not a
//! map2d document" stand unchanged.

use alloc::string::{String, ToString};

use lpc_mapping::Map2dDoc;
use lpc_mapping::map2d6::{self, MAP2D6_FORMAT};
use lpc_mapping::part::Mapping;

/// A map2d document, read.
pub(crate) enum Map2dDocument {
    /// Formats 1 to 5: today's layout, resolved by today's path.
    Layout(Map2dDoc),
    /// Format 6: parts and patching.
    Parts(Mapping),
}

/// Read a map2d document by its format; `Err` is the reader's refusal, in
/// its own words.
pub(crate) fn read_map2d_document(text: &str) -> Result<Map2dDocument, String> {
    match map2d6::peek_format(text) {
        Ok(format) if format >= MAP2D6_FORMAT => map2d6::read(text)
            .map(Map2dDocument::Parts)
            .map_err(|error| error.to_string()),
        _ => Map2dDoc::from_json(text)
            .map(Map2dDocument::Layout)
            .map_err(|error| error.to_string()),
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
            Ok(Map2dDocument::Layout(_))
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
