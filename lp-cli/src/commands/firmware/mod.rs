pub mod args;
pub mod build;
pub mod build_def;
pub mod distribution_manifest;
pub mod list;
pub mod package;
pub mod show;

pub use args::FirmwareCli;

pub fn handle_firmware(cli: FirmwareCli) -> anyhow::Result<()> {
    match cli.subcommand {
        args::FirmwareSubcommand::Show(args) => show::handle_show(args),
        args::FirmwareSubcommand::List(args) => list::handle_list(args),
        args::FirmwareSubcommand::Build(args) => build::handle_build(args),
        args::FirmwareSubcommand::Package(args) => package::handle_package(args),
    }
}
