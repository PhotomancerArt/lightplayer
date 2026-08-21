//! The patch verbs' UI-side helpers and shared story fixtures. (The
//! interim full-page `/patch` surface is gone — R5 re-housed patching as
//! the workbench Patching view; `verb_ui` is what it left behind.)

#[cfg(feature = "stories")]
pub(crate) mod patch_story_fixtures;
pub(crate) mod verb_ui;
