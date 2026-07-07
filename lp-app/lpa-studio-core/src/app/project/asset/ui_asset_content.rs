//! Effective-content DTO for one asset artifact's editor view.

/// Effective editor content for an asset artifact, resolved by
/// `ProjectController::asset_content`: the un-acked buffered body first, else
/// the overlay mirror's `ReplaceBody` bytes, else the base file body fetched
/// through the server filesystem (cached until a commit or overlay clear
/// invalidates it).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiAssetContent {
    /// The resolved body, decoded for display.
    pub body: UiAssetContentBody,
    /// True when the content derives from a pending edit (buffer or overlay
    /// mirror) rather than the saved base file.
    pub dirty: bool,
    /// Overlay mirror revision the content was resolved at — the edit-state
    /// generation. Together with [`Self::dirty`] this is the editor's resync
    /// marker: every apply/revert/save ack advances it.
    pub revision: i64,
}

impl UiAssetContent {
    /// Decode `bytes` for display: UTF-8 text when valid, else the
    /// binary/read-only signal — never a lossy conversion.
    pub fn from_bytes(bytes: &[u8], dirty: bool, revision: i64) -> Self {
        let body = match core::str::from_utf8(bytes) {
            Ok(text) => UiAssetContentBody::Text {
                text: text.to_string(),
            },
            Err(_) => UiAssetContentBody::Binary { len: bytes.len() },
        };
        Self {
            body,
            dirty,
            revision,
        }
    }

    /// The text content, when the body is valid UTF-8.
    pub fn text(&self) -> Option<&str> {
        match &self.body {
            UiAssetContentBody::Text { text } => Some(text),
            UiAssetContentBody::Binary { .. } | UiAssetContentBody::Deleted => None,
        }
    }
}

/// The decoded body of a [`UiAssetContent`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UiAssetContentBody {
    /// Valid UTF-8 content, editable as text.
    Text {
        /// The full decoded body.
        text: String,
    },
    /// Non-UTF-8 content: read-only in the editor, never decoded lossily.
    Binary {
        /// Raw body length in bytes.
        len: usize,
    },
    /// The overlay deletes the artifact body — there is no effective content.
    Deleted,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_bytes_decode_to_text() {
        let content = UiAssetContent::from_bytes(b"void main() {}", true, 7);

        assert_eq!(content.text(), Some("void main() {}"));
        assert!(content.dirty);
        assert_eq!(content.revision, 7);
    }

    #[test]
    fn non_utf8_bytes_signal_binary_without_lossy_decode() {
        let content = UiAssetContent::from_bytes(&[0xff, 0xfe, 0x00], false, 0);

        assert_eq!(content.body, UiAssetContentBody::Binary { len: 3 });
        assert_eq!(content.text(), None);
        assert!(!content.dirty);
    }
}
