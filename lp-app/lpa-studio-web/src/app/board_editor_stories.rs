//! Stories for the board display-def editor (`lpa-board-editor`).
//!
//! Coverage per the M4 milestone: the form sections, the live preview with
//! its mode/anatomy controls, and the lint panel — clean on a checked-in def
//! and loud on a seeded-broken one. The full-page story is the feel-gate
//! surface (`#/boards/edit`).

use dioxus::prelude::*;
use lpa_board_editor::view::lint_panel::LintPanel;
use lpa_board_editor::view::pin_table::PinRailEditor;
use lpa_board_editor::view::preview_pane::PreviewPane;
use lpa_board_editor::{BoardEditor, EditorDoc, RailTarget};
use lpa_studio_web_story_macros::story;

/// A doc loaded from the embedded catalog, exactly as the picker loads it.
fn checked_in_doc(board_id: &str) -> EditorDoc {
    let (id, source) = lpa_boards::DISPLAY_MANIFEST_SOURCES
        .iter()
        .find(|(id, _)| *id == board_id)
        .unwrap_or_else(|| panic!("board {board_id} in the embedded catalog"));
    EditorDoc::from_source(id, source).expect("checked-in defs parse")
}

/// A def seeded with the error classes the lint panel exists to catch.
fn broken_doc() -> EditorDoc {
    let mut doc = checked_in_doc("seeed/xiao-esp32-c6");
    doc.edit(|board| {
        board.price_usd = 0.0;
        board.purchase_urls.clear();
        // Duplicate gpio: D1 gets D0's gpio 0.
        board.hw.left[1].gpio = Some(0);
        // IO-prefixed label disagreeing with its gpio.
        board.hw.right[3].label = "IO21".into();
        // A usb cap on a plain io role.
        board.hw.right[4].caps.push(lpa_boards::PinCap {
            text: "USB_D+".into(),
            kind: lpa_boards::CapKind::Usb,
        });
    });
    doc
}

#[story(
    description = "The whole editor on a checked-in def (XIAO ESP32-C6): identity + drawing + pin tables on the left, live preview and lint on the right. The feel-gate surface at #/boards/edit."
)]
pub(crate) fn editor_loaded_xiao() -> Element {
    let doc = use_signal(|| checked_in_doc("seeed/xiao-esp32-c6"));
    rsx! {
        div { style: "max-width: 1400px;",
            BoardEditor { doc }
        }
    }
}

#[story(
    description = "The pin table editor on the DOM-Z-102's screw terminals and left rail: label / role / gpio / capability chips per row, reorder within the rail, typed cap add."
)]
pub(crate) fn pin_tables_dom_z_102() -> Element {
    let doc = use_signal(|| checked_in_doc("domraem/dom-z-102"));
    rsx! {
        div { style: "max-width: 760px; display: flex; flex-direction: column; gap: 14px;",
            PinRailEditor { doc, target: RailTarget::Terminals }
            PinRailEditor { doc, target: RailTarget::Left }
        }
    }
}

#[story(
    description = "The live preview pane (C6 devkit): BoardDiagram re-rendered from the editing doc, mode switcher, pitch toggle, anatomy overlay toggle."
)]
pub(crate) fn preview_pane_c6() -> Element {
    let doc = use_signal(|| checked_in_doc("espressif/esp32-c6-devkitc-1"));
    rsx! {
        div { style: "max-width: 560px;",
            PreviewPane { doc }
        }
    }
}

#[story(
    description = "Lint on a clean checked-in def: no errors, the discovery-eligibility summary, and the unverified-usb_bridge reminder only where it applies."
)]
pub(crate) fn lint_clean() -> Element {
    let doc = use_signal(|| checked_in_doc("espressif/esp32-c6-devkitc-1"));
    rsx! {
        div { style: "max-width: 460px;",
            LintPanel { doc }
        }
    }
}

#[story(
    description = "Lint on a seeded-broken def: duplicate gpio, IO-prefixed label disagreeing with its gpio, usb cap on a non-usb role, zeroed price, no purchase links."
)]
pub(crate) fn lint_seeded_errors() -> Element {
    let doc = use_signal(broken_doc);
    rsx! {
        div { style: "max-width: 460px;",
            LintPanel { doc }
        }
    }
}
