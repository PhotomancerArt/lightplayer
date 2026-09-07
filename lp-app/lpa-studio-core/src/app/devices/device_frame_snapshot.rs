//! The persisted last frame: a remembered board keeps its picture across
//! reloads.
//!
//! [`DeviceFrameFeed`](super::device_frame_feed::DeviceFrameFeed) keeps a
//! board's newest composed frame in memory, and the honest-device-preview
//! ADR (2026-08-06) left the rest as a follow-up: after a page reload an
//! offline board's tile fell back to its sentence, because the only copy
//! of the picture died with the tab. This module is that follow-up's
//! on-disk half — one sidecar per device uid in the library store,
//! `/device-frames/<uid>.json`, holding the composed frame (bytes + the
//! board's display layout) and the moment it was captured.
//!
//! # Posture: a cache, not user data
//!
//! The AGENTS.md persisted-format rule is honoured the additive way.
//! Nothing that already exists changes shape: the registry row is
//! untouched, and this file is NEW, keyed by uid, with its own
//! [`DEVICE_FRAME_SNAPSHOT_VERSION`]. Absence is the default — a board
//! without a sidecar simply has no picture yet — and an unreadable or
//! foreign-version sidecar reads as absent too (`debug!`, never a user-
//! facing error, never a migration): the next fed frame overwrites it. A
//! snapshot is memory of what the board did, and losing one costs a
//! sentence on a tile, not a project.
//!
//! # What the file holds
//!
//! The camelCase JSON mirror of a [`UiControlProductPreview`] plus
//! `capturedAt` (the studio's epoch-seconds clock at the frame's revision
//! move — the same stamp the feed ages the live pill from, so a rehydrated
//! frame ages honestly: "last frame · 3 h ago" means three hours). The
//! sample bytes ride as standard base64; the display layout serializes in
//! `lpc-model`'s packed span form, which is what keeps a 2,000-lamp sidecar
//! in the tens of kilobytes.

use std::rc::Rc;

use base64::Engine as _;
use lpc_model::{ControlDisplayLayout, ControlExtent, ControlSampleLayout};
use lpfs::{AsLpPath, FsError, LpFs};
use serde::{Deserialize, Serialize};

use crate::{UiControlProductPreview, UiControlSampleFormat};

/// Where the sidecars live inside the library store, beside `/registry.json`.
pub const DEVICE_FRAMES_DIR: &str = "/device-frames";

/// The sidecar's own format version. Bump on any change to the bytes a
/// reader could misread; an older reader treats a newer file as absent.
pub const DEVICE_FRAME_SNAPSHOT_VERSION: u32 = 1;

/// The write rate limit per device: a fed board publishes many frames a
/// second, and the store is a locked, flushed transaction each time. One
/// write per ten seconds keeps a remembered board's picture within ten
/// seconds of the last thing it did, at a cost nobody can see.
pub const DEVICE_FRAME_SNAPSHOT_INTERVAL_SECS: f64 = 10.0;

/// `/device-frames/<uid>.json`.
pub fn snapshot_path(uid: &str) -> String {
    format!("{DEVICE_FRAMES_DIR}/{uid}.json")
}

/// The on-disk shape. Private: callers hand in and get back the
/// [`UiControlProductPreview`] the tile draws.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredDeviceFrame {
    version: u32,
    /// Epoch seconds (the studio clock) when this frame's revision moved.
    captured_at: f64,
    revision: i64,
    extent: ControlExtent,
    /// The native sample format's name — `"u16"` is the only one today.
    sample_format: String,
    sample_layout: ControlSampleLayout,
    /// Absent when the board declined its layout (over the wire budget):
    /// bytes without geometry, exactly as the live feed carries them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    display_layout: Option<ControlDisplayLayout>,
    /// Standard base64 of the native sample bytes.
    bytes: String,
}

const SAMPLE_FORMAT_U16: &str = "u16";

fn sample_format_name(format: UiControlSampleFormat) -> &'static str {
    match format {
        UiControlSampleFormat::U16 => SAMPLE_FORMAT_U16,
    }
}

fn sample_format_from_name(name: &str) -> Option<UiControlSampleFormat> {
    match name {
        SAMPLE_FORMAT_U16 => Some(UiControlSampleFormat::U16),
        _ => None,
    }
}

/// Encode a composed frame captured at `captured_at` as sidecar bytes.
pub fn encode(frame: &UiControlProductPreview, captured_at: f64) -> Vec<u8> {
    let stored = StoredDeviceFrame {
        version: DEVICE_FRAME_SNAPSHOT_VERSION,
        captured_at,
        revision: frame.revision,
        extent: frame.extent,
        sample_format: sample_format_name(frame.sample_format).to_string(),
        sample_layout: frame.sample_layout.clone(),
        display_layout: frame.display_layout.as_deref().cloned(),
        bytes: base64::engine::general_purpose::STANDARD.encode(&frame.bytes),
    };
    // A struct of plain serde types cannot fail to serialize.
    serde_json::to_vec(&stored).unwrap_or_default()
}

/// Decode sidecar bytes into the frame and its capture stamp, or `None`
/// for anything this reader should not trust: unparsable JSON, a foreign
/// version, an unknown sample format, or a byte count that disagrees with
/// the extent (a U16 sample is two bytes).
pub fn decode(bytes: &[u8]) -> Option<(UiControlProductPreview, f64)> {
    let stored: StoredDeviceFrame = match serde_json::from_slice(bytes) {
        Ok(stored) => stored,
        Err(error) => {
            log::debug!("device frame snapshot unreadable: {error}");
            return None;
        }
    };
    if stored.version != DEVICE_FRAME_SNAPSHOT_VERSION {
        log::debug!(
            "device frame snapshot version {} is not {DEVICE_FRAME_SNAPSHOT_VERSION}; ignored",
            stored.version
        );
        return None;
    }
    let sample_format = sample_format_from_name(&stored.sample_format)?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&stored.bytes)
        .ok()?;
    let expected_len = match sample_format {
        UiControlSampleFormat::U16 => stored.extent.sample_count() as usize * 2,
    };
    if bytes.len() != expected_len {
        log::debug!(
            "device frame snapshot carries {} bytes for a {}-sample extent; ignored",
            bytes.len(),
            stored.extent.sample_count()
        );
        return None;
    }
    let frame = UiControlProductPreview {
        revision: stored.revision,
        extent: stored.extent,
        sample_format,
        sample_layout: stored.sample_layout,
        display_layout: stored.display_layout.map(Rc::new),
        bytes: Rc::from(bytes),
    };
    Some((frame, stored.captured_at))
}

/// Write `uid`'s sidecar (already encoded — the host stays codec-free).
pub fn write_snapshot(fs: &dyn LpFs, uid: &str, bytes: &[u8]) -> Result<(), FsError> {
    fs.write_file(snapshot_path(uid).as_path(), bytes)
}

/// Read and decode `uid`'s sidecar. `None` when there is none, or when
/// what is there should not be trusted (see [`decode`]).
pub fn read_snapshot(fs: &dyn LpFs, uid: &str) -> Option<(UiControlProductPreview, f64)> {
    match fs.read_file(snapshot_path(uid).as_path()) {
        Ok(bytes) => decode(&bytes),
        Err(FsError::NotFound(_)) => None,
        Err(error) => {
            log::debug!("device frame snapshot for {uid} unreadable: {error}");
            None
        }
    }
}

/// Remove `uid`'s sidecar. A missing file is the goal state, not an error.
pub fn delete_snapshot(fs: &dyn LpFs, uid: &str) -> Result<(), FsError> {
    match fs.delete_file(snapshot_path(uid).as_path()) {
        Ok(()) | Err(FsError::NotFound(_)) => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use lpc_model::{
        ColorOrder, ControlLamp2d, ControlLayout2d, ControlSampleEncoding, ControlSampleSpan,
        Revision,
    };
    use lpfs::LpFsMemory;

    use super::*;

    fn frame(with_layout: bool) -> UiControlProductPreview {
        let layout = with_layout.then(|| {
            Rc::new(ControlDisplayLayout::Layout2d(ControlLayout2d::new(
                Revision::new(11),
                8,
                8,
                vec![ControlLamp2d {
                    lamp_index: 0,
                    sample_start: 0,
                    center: [0.5, 0.5],
                    radius: 0.05,
                }],
            )))
        });
        UiControlProductPreview {
            revision: 7,
            extent: ControlExtent::new(1, 3),
            sample_format: UiControlSampleFormat::U16,
            sample_layout: ControlSampleLayout {
                spans: vec![ControlSampleSpan {
                    row: 0,
                    start: 0,
                    len: 3,
                    encoding: ControlSampleEncoding::RgbPixels {
                        count: 1,
                        color_order: ColorOrder::Rgb,
                    },
                }],
            },
            display_layout: layout,
            bytes: Rc::from(vec![1, 0, 2, 0, 3, 0]),
        }
    }

    /// Equality up to the packed span form's quantization: the wire
    /// layout stores lamp centers at 16-bit precision, so a 0.5 comes
    /// back as 0.5000076 — the same picture on any slot.
    fn assert_same_picture(decoded: &UiControlProductPreview, original: &UiControlProductPreview) {
        assert_eq!(decoded.revision, original.revision);
        assert_eq!(decoded.extent, original.extent);
        assert_eq!(decoded.sample_format, original.sample_format);
        assert_eq!(decoded.sample_layout, original.sample_layout);
        assert_eq!(decoded.bytes, original.bytes);
        match (
            decoded.display_layout.as_deref(),
            original.display_layout.as_deref(),
        ) {
            (None, None) => {}
            (
                Some(ControlDisplayLayout::Layout2d(decoded)),
                Some(ControlDisplayLayout::Layout2d(original)),
            ) => {
                assert_eq!(decoded.revision, original.revision);
                assert_eq!(decoded.lamps.len(), original.lamps.len());
                for (a, b) in decoded.lamps.iter().zip(&original.lamps) {
                    assert_eq!(a.lamp_index, b.lamp_index);
                    assert_eq!(a.sample_start, b.sample_start);
                    assert!((a.center[0] - b.center[0]).abs() < 1e-3, "{a:?} vs {b:?}");
                    assert!((a.center[1] - b.center[1]).abs() < 1e-3, "{a:?} vs {b:?}");
                    assert!((a.radius - b.radius).abs() < 1e-3, "{a:?} vs {b:?}");
                }
            }
            (decoded, original) => panic!("layout presence differs: {decoded:?} vs {original:?}"),
        }
    }

    #[test]
    fn a_frame_round_trips_with_and_without_its_layout() {
        for with_layout in [true, false] {
            let original = frame(with_layout);
            let (decoded, at) = decode(&encode(&original, 1_800.5)).expect("decodes");
            assert_same_picture(&decoded, &original);
            assert_eq!(at, 1_800.5);
        }
    }

    #[test]
    fn the_file_is_camel_case_json_with_its_own_version() {
        let text = String::from_utf8(encode(&frame(false), 1.0)).unwrap();
        assert!(text.contains("\"version\":1"), "{text}");
        assert!(text.contains("\"capturedAt\":1.0"), "{text}");
        assert!(text.contains("\"sampleFormat\":\"u16\""), "{text}");
        assert!(
            !text.contains("displayLayout"),
            "absent layout is omitted: {text}"
        );
    }

    /// The cache posture: anything this reader should not trust is
    /// "no snapshot", never an error and never a migration.
    #[test]
    fn untrusted_bytes_read_as_absent() {
        assert!(decode(b"not json").is_none());
        assert!(decode(b"{}").is_none());

        let mut foreign: serde_json::Value =
            serde_json::from_slice(&encode(&frame(true), 1.0)).unwrap();
        foreign["version"] = serde_json::json!(DEVICE_FRAME_SNAPSHOT_VERSION + 1);
        assert!(decode(&serde_json::to_vec(&foreign).unwrap()).is_none());

        let mut odd_format =
            serde_json::from_slice::<serde_json::Value>(&encode(&frame(true), 1.0)).unwrap();
        odd_format["sampleFormat"] = serde_json::json!("f32");
        assert!(decode(&serde_json::to_vec(&odd_format).unwrap()).is_none());

        let mut short =
            serde_json::from_slice::<serde_json::Value>(&encode(&frame(true), 1.0)).unwrap();
        short["bytes"] = serde_json::json!("AQA=");
        assert!(decode(&serde_json::to_vec(&short).unwrap()).is_none());
    }

    #[test]
    fn the_store_helpers_key_by_uid_and_tolerate_absence() {
        let fs = LpFsMemory::new();
        assert_eq!(snapshot_path("dev1"), "/device-frames/dev1.json");
        assert!(read_snapshot(&fs, "dev1").is_none());
        delete_snapshot(&fs, "dev1").expect("deleting nothing is fine");

        write_snapshot(&fs, "dev1", &encode(&frame(true), 42.0)).unwrap();
        let (stored, at) = read_snapshot(&fs, "dev1").expect("reads back");
        assert_same_picture(&stored, &frame(true));
        assert_eq!(at, 42.0);
        assert!(read_snapshot(&fs, "dev2").is_none(), "keyed by uid");

        delete_snapshot(&fs, "dev1").unwrap();
        assert!(read_snapshot(&fs, "dev1").is_none());
    }
}
