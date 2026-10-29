//! Shared by the frozen sampling copy's proof (`v12_sampling_live`) and the
//! look report (`look_report`): the repo's projects read as plain numbers,
//! and today's live engine recorded frame by frame.

#![allow(dead_code, reason = "each test binary uses a different subset")]

pub mod live_streams;
pub mod project_tree;
pub mod recording_graphics;
pub mod scratch_copy;
