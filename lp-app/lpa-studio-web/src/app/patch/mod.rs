//! The patch verbs' UI-side helpers, THE patch panel, and shared story
//! fixtures. (The interim full-page `/patch` surface is gone — R5 re-housed
//! patching as the workbench Patching view; `verb_ui` is what it left
//! behind, and pass 2's panel joined it here so its stories land on a
//! three-segment path the story build accepts.)

pub(crate) mod lamp_strip;
pub(crate) mod patch_panel;
#[cfg(feature = "stories")]
pub(crate) mod patch_panel_stories;
#[cfg(feature = "stories")]
pub(crate) mod patch_story_fixtures;
pub(crate) mod verb_ui;
