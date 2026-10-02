//! `lp-cli link …`: the link lab's entry point.

use anyhow::Result;

use super::args::{LinkCli, LinkSubcommand};

/// Run `lp-cli link …`.
pub fn handle_link(cli: LinkCli) -> Result<()> {
    match cli.subcommand {
        LinkSubcommand::Lab(args) => super::lab_cmd::lab(&args),
        LinkSubcommand::Capture(args) => super::capture::capture(&args),
    }
}
