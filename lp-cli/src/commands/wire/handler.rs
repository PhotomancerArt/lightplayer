//! `lp-cli wire unpack`: a capture of a board's link, as lines.

use std::io::Write;

use anyhow::{Context, Result};
use lpc_wire::lp_link::sniffer::Direction;

use super::args::{UnpackArgs, WireCli, WireSubcommand};
use super::line_unpack::unpack_line_stream;
use super::link_unpack::unpack_link_stream;
use super::tap_unpack::unpack_tap;
use super::unpack_report::UnpackReport;

/// Run `lp-cli wire …`.
pub fn handle_wire(cli: WireCli) -> Result<()> {
    match cli.subcommand {
        WireSubcommand::Unpack(args) => handle_unpack(&args),
    }
}

fn handle_unpack(args: &UnpackArgs) -> Result<()> {
    let stdin = std::io::stdin().lock();
    let mut stdout = std::io::stdout().lock();
    let mut stderr = std::io::stderr().lock();
    let mut report = UnpackReport::new(args.sizes);
    if args.tap {
        unpack_tap(stdin, &mut stdout, &mut report, &mut stderr)?;
    } else if args.lines {
        unpack_line_stream(stdin, &mut stdout, &mut report, &mut stderr)?;
    } else {
        let dir = match args.from_host {
            true => Direction::HostToBoard,
            false => Direction::BoardToHost,
        };
        unpack_link_stream(stdin, dir, &mut stdout, &mut report, &mut stderr)?;
    }
    stdout.flush().context("writing stdout")?;
    report.finish(&mut stderr)?;
    Ok(())
}
