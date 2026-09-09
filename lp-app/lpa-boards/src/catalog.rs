//! The checked-in board catalog, embedded so wasm consumers need no fs.
//!
//! Sources live next to the runtime manifests:
//! `lp-core/lpc-hardware/boards/<vendor>/<product>.display.json`.
//! Adding a board = adding the file there and listing it here; the drift
//! tests fail if this list and the directory disagree.

use std::sync::OnceLock;

use crate::display_manifest::BoardDisplayFile;

/// `(board_id, json_source)` for every checked-in display sidecar.
pub const DISPLAY_MANIFEST_SOURCES: &[(&str, &str)] = &[
    (
        "espressif/esp32-c6-devkitc-1",
        include_str!(
            "../../../lp-core/lpc-hardware/boards/espressif/esp32-c6-devkitc-1.display.json"
        ),
    ),
    (
        "espressif/esp32-s3-devkitc-1",
        include_str!(
            "../../../lp-core/lpc-hardware/boards/espressif/esp32-s3-devkitc-1.display.json"
        ),
    ),
    (
        "espressif/esp32-devkitc-v4",
        include_str!(
            "../../../lp-core/lpc-hardware/boards/espressif/esp32-devkitc-v4.display.json"
        ),
    ),
    (
        "seeed/xiao-esp32-c6",
        include_str!("../../../lp-core/lpc-hardware/boards/seeed/xiao-esp32-c6.display.json"),
    ),
    (
        "seeed/xiao-esp32-s3-plus",
        include_str!("../../../lp-core/lpc-hardware/boards/seeed/xiao-esp32-s3-plus.display.json"),
    ),
    (
        "quinled/dig-uno",
        include_str!("../../../lp-core/lpc-hardware/boards/quinled/dig-uno.display.json"),
    ),
    (
        "quinled/dig2go",
        include_str!("../../../lp-core/lpc-hardware/boards/quinled/dig2go.display.json"),
    ),
    (
        "domraem/dom-z-102",
        include_str!("../../../lp-core/lpc-hardware/boards/domraem/dom-z-102.display.json"),
    ),
    (
        "lightplayer/desktop",
        include_str!("../../../lp-core/lpc-hardware/boards/lightplayer/desktop.display.json"),
    ),
];

/// Every checked-in board, parsed once. Panics on malformed embedded data —
/// the tests and the schema gate keep that impossible at HEAD.
pub fn all_boards() -> &'static [BoardDisplayFile] {
    static BOARDS: OnceLock<Vec<BoardDisplayFile>> = OnceLock::new();
    BOARDS.get_or_init(|| {
        DISPLAY_MANIFEST_SOURCES
            .iter()
            .map(|(id, source)| {
                let board = BoardDisplayFile::read_json(source)
                    .unwrap_or_else(|error| panic!("embedded display manifest {id}: {error}"));
                assert_eq!(
                    &board.board_id, id,
                    "embedded display manifest listed under the wrong id"
                );
                board
            })
            .collect()
    })
}

pub fn board_by_id(board_id: &str) -> Option<&'static BoardDisplayFile> {
    all_boards().iter().find(|board| board.board_id == board_id)
}

/// The `family` of boards that are not hardware — a computer running the
/// desktop firmware.
pub const DESKTOP_FAMILY: &str = "desktop";

/// Every board the **Boards page** shows: [`all_boards`] minus the desktop
/// family, because Desktop is a target, not a board anyone can buy, and the
/// Boards page is a shopping page (vision D42).
///
/// The catalog itself keeps it — the device picker and a project's Hardware
/// row both need Desktop to be a board like any other, and so does the sim
/// that wears its manifest. This is presentation, and the one place it is
/// decided.
pub fn purchasable_boards() -> impl Iterator<Item = &'static BoardDisplayFile> {
    all_boards()
        .iter()
        .filter(|board| board.family != DESKTOP_FAMILY)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_embedded_boards_parse_and_validate() {
        let boards = all_boards();
        assert_eq!(boards.len(), DISPLAY_MANIFEST_SOURCES.len());
    }

    /// Desktop is IN the catalog — the picker, the Hardware row and the
    /// sim's manifest all need it to be a board like any other — and off
    /// the Boards page, which is a shopping page and has nothing to sell.
    #[test]
    fn the_desktop_target_is_in_the_catalog_but_not_on_the_boards_page() {
        assert!(
            board_by_id("lightplayer/desktop").is_some(),
            "the catalog keeps Desktop"
        );
        assert!(
            !purchasable_boards().any(|board| board.board_id == "lightplayer/desktop"),
            "the Boards page does not offer Desktop"
        );
        assert_eq!(
            purchasable_boards().count(),
            all_boards().len() - 1,
            "Desktop is the only entry the Boards page skips"
        );
    }

    #[test]
    fn board_ids_are_unique() {
        let boards = all_boards();
        for board in boards {
            assert_eq!(
                boards
                    .iter()
                    .filter(|other| other.board_id == board.board_id)
                    .count(),
                1
            );
        }
    }
}
