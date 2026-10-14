//! `lp-cli hardware tree`: a tree-store partition, read only (plan
//! `lp2025/2026-10-08-1017-tree-store-device-round`, M8). Sits beside
//! `hardware lpfs` and reads its partition the same way: a raw image (an
//! `lpfs save`'s `raw-lpfs-*.bin`, or a whole chip), or a board's port over
//! the bootloader.
//!
//! | file | command |
//! |---|---|
//! | [`inspect`] | sectors, roots, the chosen root's tree, live and garbage bytes |
//! | [`check`] | the store's fsck: exits 2 on any inconsistency |
//! | [`extract`] | the committed files into a directory |
//! | [`tree_source`] | shared: where the bytes come from |
//!
//! Every decision is `lp_tree_store::StoreImage`'s (feature `inspect`): the
//! format is read by the store's own decoders, never reimplemented here.
//! Nothing here writes a board.

pub mod check;
pub mod extract;
pub mod inspect;
pub mod tree_source;

use anyhow::Result;

use super::args::{TreeArgs, TreeCommand};

pub fn handle_tree(args: TreeArgs) -> Result<()> {
    match args.command {
        TreeCommand::Inspect(args) => inspect::handle_inspect(args),
        TreeCommand::Check(args) => check::handle_check(args),
        TreeCommand::Extract(args) => extract::handle_extract(args),
    }
}

#[cfg(test)]
mod hardware_tree_tests;
