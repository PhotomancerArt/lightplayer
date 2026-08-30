//! Base UI building blocks.
//!
//! These components should stay independent of `lpa-studio-core`. They are
//! generic controls and display primitives that Studio could plausibly get
//! from a design-system package.

pub mod code_editor;
#[cfg(feature = "stories")]
pub(crate) mod code_editor_stories;
pub mod code_figure;
#[cfg(feature = "stories")]
pub(crate) mod code_figure_stories;
pub mod detail_popover;
#[cfg(feature = "stories")]
pub(crate) mod detail_popover_stories;
pub mod field_row;
pub mod gradient_strip;
#[cfg(feature = "stories")]
pub(crate) mod gradient_strip_stories;
pub mod help_link;
pub mod icon;
pub mod icon_menu;
#[cfg(feature = "stories")]
pub(crate) mod icon_menu_stories;
pub mod inline_button;
#[cfg(feature = "stories")]
pub(crate) mod inline_button_stories;
pub mod interaction_light;
pub mod keyboard;
pub mod logo_mark;
#[cfg(feature = "stories")]
pub(crate) mod logo_mark_stories;
pub mod markdown_text;
#[cfg(feature = "stories")]
pub(crate) mod markdown_text_stories;
pub mod option_cards;
pub mod outline;
pub mod popover;
#[cfg(feature = "stories")]
pub(crate) mod popover_stories;
pub mod reveal_on_focus;
pub mod tabs;
pub mod toast;

pub use code_editor::{
    CodeEditor, CodeEditorCompletion, CodeEditorCompletionKind, CodeEditorDiagnostic,
    CodeEditorLanguage,
};
pub use code_figure::{CodeFigure, CodeHighlight, CodeHighlightTone};
pub use detail_popover::{
    DetailPopover, DetailSection, DetailSectionTint, detail_popover_card_class,
    detail_popover_section_class,
};
pub use field_row::FieldRow;
pub use gradient_strip::GradientStripCanvas;
pub use help_link::HelpLink;
pub use icon::{NodeKindIcon, StudioIcon, StudioIconName, action_icon_name, node_kind_icon};
pub use icon_menu::{IconActionButton, IconMenuButton, IconMenuTone, IconMenuVisualState};
pub use inline_button::{
    INLINE_ICON_SIZE, INLINE_TEXT_ICON_SIZE, InlineButton, InlineButtonTone,
    inline_icon_button_class, inline_text_button_class,
};
pub use interaction_light::{
    conic_spinner_class, focus_ring_class, ir_ring_class, iridescent_fill_class,
    iridescent_fill_static_class, row_edge_class,
};
pub use keyboard::Platform;
pub use logo_mark::{LogoLockup, LogoMark, LogoStacked};
pub use markdown_text::{MarkdownDocs, MarkdownText};
pub use option_cards::{
    OPTION_CARD_CHECK_CLASS, OptionCard, OptionCards, option_card_class, option_card_grid_class,
};
pub use popover::{IconPopoverButton, PopoverButton, PopoverCloseHandle, PopoverPlacement};
pub use reveal_on_focus::{reveal_selected_pane, use_reveal_on_focus};
pub use tabs::{TabItem, Tabs};
pub use toast::{ToastHost, ToastMessage, ToastTone, Toasts, use_toast_provider};
