pub mod args;
pub mod show;

pub use args::FirmwareCli;

pub fn handle_firmware(cli: FirmwareCli) -> anyhow::Result<()> {
    match cli.subcommand {
        args::FirmwareSubcommand::Show(show_args) => show::handle_show(show_args),
    }
}
