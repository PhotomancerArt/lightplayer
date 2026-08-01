//! Dioxus view components: thin over `editor_core`, styled by `lpb-ed-*`
//! classes owned by the consuming app's stylesheet.

pub mod board_editor;
pub mod board_editor_page;
pub mod drawing_form;
pub mod form_widgets;
pub mod identity_form;
pub mod lint_panel;
pub mod pin_table;
pub mod preview_pane;

pub use board_editor::BoardEditor;
pub use board_editor_page::BoardEditorPage;
