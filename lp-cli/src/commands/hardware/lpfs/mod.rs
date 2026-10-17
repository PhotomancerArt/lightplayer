//! `lp-cli hardware lpfs`: a board's filesystem across partition layouts
//! (plan `lp2025/2026-10-01-1843-c6-repartition`, P04).
//!
//! | file | command |
//! |---|---|
//! | [`report`] | how full a board (or an image, or a project dir) is, and whether it fits the target layout |
//! | [`save`] | raw region + table + backup ZIP, read only |
//! | [`migrate`] | inspect → backup → plan → write, one bootloader session |
//! | [`restore`] | a backup ZIP's files back onto a board |
//! | [`preflight`] | the both-directions layout check the flash recipes run |
//! | [`fixture`] | a 4 MiB emulator chip with a given layout and tree (hidden) |
//! | [`lpfs_target`] | shared: the target layout, the package, the backup store |
//!
//! Every decision is `lpa_link::layout_migration`'s; this module reads
//! ports and files and prints.

pub mod fixture;
pub mod lpfs_target;
pub mod migrate;
pub mod preflight;
pub mod report;
pub mod restore;
pub mod save;

use anyhow::Result;

use super::args::{LpfsArgs, LpfsCommand};

/// What lp-cli says about a board whose hello reports `fs: refused` (an
/// `fs-tree` build that found a newer or damaged store header). Never a
/// migration or a restore: those would write over the store it kept.
pub const REFUSED_STORE: &str = "the board refused its file store: a newer or damaged store \
     header; its files are kept — read them with `lp-cli hardware tree extract`";

/// What lp-cli says about a board whose hello reports `fs: refused` from a
/// build WITHOUT `fs.tree`: a littlefs core that found its files in a tree
/// store, and kept them rather than format it.
pub const TREE_STORE_HELD: &str = "the board's files are in the tree store, a newer filesystem \
     than its firmware reads; they are kept, untouched — install a firmware that reads the tree \
     store";

/// The words for a refused board's hello, by whether its build reads the
/// tree store.
pub fn refused_store_words(hello: &lpc_wire::ServerHello) -> &'static str {
    if hello.build.features.contains(&lpc_model::LpFeature::FsTree) {
        REFUSED_STORE
    } else {
        TREE_STORE_HELD
    }
}

pub fn handle_lpfs(args: LpfsArgs) -> Result<()> {
    match args.command {
        LpfsCommand::Report(args) => report::handle_report(args),
        LpfsCommand::Save(args) => save::handle_save(args),
        LpfsCommand::Migrate(args) => migrate::handle_migrate(args),
        LpfsCommand::Restore(args) => restore::handle_restore(args),
        LpfsCommand::Preflight(args) => preflight::handle_preflight(args),
        LpfsCommand::Fixture(args) => fixture::handle_fixture(args),
    }
}
