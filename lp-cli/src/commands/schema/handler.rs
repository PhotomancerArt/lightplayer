use anyhow::Result;

use super::args::{SchemaCli, SchemaSubcommand};
use super::generate;

pub fn handle_schema(cli: SchemaCli) -> Result<()> {
    match cli.subcommand {
        SchemaSubcommand::Gen(args) => generate::handle_gen(args),
    }
}
