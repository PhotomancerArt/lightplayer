//! GLSL preprocessor (no_std + `alloc` build for bare-metal naga `glsl-in`).
#![cfg_attr(not(test), no_std)]
extern crate alloc;

extern crate unicode_xid;

#[allow(clippy::match_like_matches_macro)]
mod lexer;
pub mod pp;
pub mod token;

#[cfg(test)]
mod lexer_tests;
#[cfg(test)]
mod pp_tests;
