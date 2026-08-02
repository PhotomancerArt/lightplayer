//! Embedded RUNTIME board manifests, for app-side consumers that write them
//! to devices (provisioning's `/hardware.json`, board-selection D4).
//!
//! These are the same checked-in `boards/<vendor>/<product>.json` files the
//! firmware compiles its per-target defaults from — embedded here so wasm
//! consumers need no fs, exactly like the display catalog. `lpc-hardware`
//! deliberately embeds only the per-target defaults (it links into
//! firmware); the full by-id lookup lives app-side. The drift tests keep
//! this list complete and byte-identical to the directory.

/// `(board_id, json_source)` for every checked-in RUNTIME manifest.
pub const RUNTIME_MANIFEST_SOURCES: &[(&str, &str)] = &[
    (
        "espressif/esp32-c6-devkitc-1",
        include_str!("../../../lp-core/lpc-hardware/boards/espressif/esp32-c6-devkitc-1.json"),
    ),
    (
        "espressif/esp32-s3-devkitc-1",
        include_str!("../../../lp-core/lpc-hardware/boards/espressif/esp32-s3-devkitc-1.json"),
    ),
    (
        "seeed/xiao-esp32-c6",
        include_str!("../../../lp-core/lpc-hardware/boards/seeed/xiao-esp32-c6.json"),
    ),
    (
        "seeed/xiao-esp32-s3-plus",
        include_str!("../../../lp-core/lpc-hardware/boards/seeed/xiao-esp32-s3-plus.json"),
    ),
    (
        "domraem/dom-z-102",
        include_str!("../../../lp-core/lpc-hardware/boards/domraem/dom-z-102.json"),
    ),
];

/// The checked-in runtime manifest JSON for `board_id`, verbatim — `None`
/// for display-only boards (catalog entry, no runtime manifest yet).
pub fn runtime_manifest_json(board_id: &str) -> Option<&'static str> {
    RUNTIME_MANIFEST_SOURCES
        .iter()
        .find(|(id, _)| *id == board_id)
        .map(|(_, source)| *source)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_hits_and_misses() {
        assert!(runtime_manifest_json("domraem/dom-z-102").is_some());
        // Display-only boards have no runtime manifest.
        assert!(runtime_manifest_json("quinled/dig-uno").is_none());
        assert!(runtime_manifest_json("nope/nope").is_none());
    }
}
