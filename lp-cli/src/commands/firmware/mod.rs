pub mod args;
mod bootloader_guard;
pub mod build;
pub mod build_def;
pub mod distribution_manifest;
pub mod list;
mod ota_files;
#[cfg(test)]
pub(crate) mod ota_fixture;
pub mod package;
mod release_assets;
mod release_check;
pub mod show;
mod split_package;

pub use args::FirmwareCli;

pub fn handle_firmware(cli: FirmwareCli) -> anyhow::Result<()> {
    match cli.subcommand {
        args::FirmwareSubcommand::Show(args) => show::handle_show(args),
        args::FirmwareSubcommand::List(args) => list::handle_list(args),
        args::FirmwareSubcommand::Build(args) => build::handle_build(args),
        args::FirmwareSubcommand::Package(args) => package::handle_package(args),
        args::FirmwareSubcommand::ReleaseAssets(args) => {
            release_assets::handle_release_assets(args)
        }
        args::FirmwareSubcommand::ReleaseCheck(args) => release_check::handle_release_check(args),
    }
}
