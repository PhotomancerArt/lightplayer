//! What an ESP-IDF app image says it is: the application description at
//! the start of its first segment.
//!
//! Written from ESP-IDF's documented image format, not from its source: an
//! app image starts with a 24-byte header (magic `0xE9`), then its first
//! segment's 8-byte header, and an ESP-IDF build places
//! `esp_app_desc_t` first in that segment — so at image offset `0x20`. The
//! description opens with the magic word `0xABCD5432` (little-endian), a
//! `u32` secure version and two reserved `u32`s, then `version[32]` and
//! `project_name[32]`, each NUL-terminated.
//!
//! It is how Studio names somebody else's firmware before replacing it
//! ("This board is running …"). An Arduino/PlatformIO build usually reports
//! the framework's own project (`arduino-lib-builder`), which the display
//! layer words as an Arduino sketch: the raw strings stay raw here.

/// Image offset of the application description.
pub const APP_DESC_OFFSET: usize = 0x20;
/// Bytes to read from the start of an app partition to see its header and
/// description.
pub const APP_HEADER_READ_LEN: u32 = 0x120;

const IMAGE_MAGIC: u8 = 0xE9;
const APP_DESC_MAGIC: u32 = 0xABCD_5432;
const VERSION_AT: usize = APP_DESC_OFFSET + 16;
const PROJECT_NAME_AT: usize = VERSION_AT + 32;
const FIELD_LEN: usize = 32;

/// What an app image's description names.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppDescriptor {
    /// `project_name`, or `None` when empty.
    pub project_name: Option<String>,
    /// `version`, or `None` when empty.
    pub version: Option<String>,
}

/// What the start of an app partition holds.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AppImage {
    /// Erased flash: no image was ever written there.
    Erased,
    /// An image with an application description.
    Described(AppDescriptor),
    /// An image header with no description (not an ESP-IDF build, or an
    /// older one), or bytes that are not an image at all.
    Undescribed,
}

impl AppImage {
    /// Read the first bytes of an app partition (at least
    /// [`APP_HEADER_READ_LEN`] for a description to be found).
    pub fn parse(bytes: &[u8]) -> Self {
        if bytes.is_empty() || bytes.iter().all(|&b| b == 0xFF) {
            return Self::Erased;
        }
        if bytes[0] != IMAGE_MAGIC {
            return Self::Undescribed;
        }
        let Some(magic) = bytes.get(APP_DESC_OFFSET..APP_DESC_OFFSET + 4) else {
            return Self::Undescribed;
        };
        if u32::from_le_bytes([magic[0], magic[1], magic[2], magic[3]]) != APP_DESC_MAGIC {
            return Self::Undescribed;
        }
        let (Some(version), Some(project_name)) = (
            bytes.get(VERSION_AT..VERSION_AT + FIELD_LEN),
            bytes.get(PROJECT_NAME_AT..PROJECT_NAME_AT + FIELD_LEN),
        ) else {
            return Self::Undescribed;
        };
        Self::Described(AppDescriptor {
            project_name: c_string(project_name),
            version: c_string(version),
        })
    }
}

/// A NUL-terminated field as text: lossy, trimmed, `None` when empty.
fn c_string(field: &[u8]) -> Option<String> {
    let end = field.iter().position(|&b| b == 0).unwrap_or(field.len());
    let text = String::from_utf8_lossy(&field[..end]);
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn an_esp_idf_image_names_its_project_and_version() {
        let image = image_with_description("WLED", "0.15.0");
        assert_eq!(
            AppImage::parse(&image),
            AppImage::Described(AppDescriptor {
                project_name: Some("WLED".into()),
                version: Some("0.15.0".into()),
            })
        );
    }

    #[test]
    fn erased_flash_is_no_image() {
        assert_eq!(AppImage::parse(&[0xFF; 0x120]), AppImage::Erased);
        assert_eq!(AppImage::parse(&[]), AppImage::Erased);
    }

    #[test]
    fn an_image_without_the_description_magic_is_undescribed() {
        let mut image = image_with_description("x", "y");
        image[APP_DESC_OFFSET] = 0;
        assert_eq!(AppImage::parse(&image), AppImage::Undescribed);
        let mut not_an_image = image_with_description("x", "y");
        not_an_image[0] = 0x00;
        assert_eq!(AppImage::parse(&not_an_image), AppImage::Undescribed);
        assert_eq!(AppImage::parse(&image[..0x22]), AppImage::Undescribed);
    }

    #[test]
    fn empty_fields_are_none_and_garbage_is_lossy() {
        let image = image_with_description("", "\u{1}v1 ");
        assert_eq!(
            AppImage::parse(&image),
            AppImage::Described(AppDescriptor {
                project_name: None,
                version: Some("\u{1}v1".into()),
            })
        );
    }

    /// The first `APP_HEADER_READ_LEN` bytes of an app image whose
    /// description names `project` at `version`. Shared with the resident
    /// classifier's tests.
    pub(crate) fn image_with_description(project: &str, version: &str) -> Vec<u8> {
        let mut image = vec![0u8; APP_HEADER_READ_LEN as usize];
        image[0] = IMAGE_MAGIC;
        image[APP_DESC_OFFSET..APP_DESC_OFFSET + 4].copy_from_slice(&APP_DESC_MAGIC.to_le_bytes());
        image[VERSION_AT..VERSION_AT + version.len()].copy_from_slice(version.as_bytes());
        image[PROJECT_NAME_AT..PROJECT_NAME_AT + project.len()].copy_from_slice(project.as_bytes());
        image
    }
}
