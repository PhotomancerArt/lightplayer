//! `lp-emu-validate` prints the link host's command lines, and `lp-cli`
//! parses them. Only `lp-cli` can see both, so the contract is checked here.
//!
//! The runner lives inside the `lp-emu/` MIT fence and cannot import the host
//! (lp-link, lpc-wire): `lp-cli validate` hands it two command prefixes and it
//! appends flags in `lp-cli emu run --host-link`'s and `lp-cli link
//! capture`'s vocabulary (`LinkHost`'s doc). A flag the runner spells and the
//! host does not take would fail a recording at run time, after a firmware
//! build; this fails it at `cargo test`. If it fails, fix whichever side is
//! wrong — do not relax the assertion.

use std::path::PathBuf;

use clap::Parser;
use lp_cli::commands::emu::args::{EmuCli, EmuCommand};
use lp_cli::commands::link::LinkCli;
use lp_cli::commands::link::args::LinkSubcommand;
use lp_cli::commands::validate::handler::{RECORDING_LINK_NONCE, link_host};
use lp_emu_validate::ValidateConfig;
use lp_emu_validate::driver::{LinkHost, RunPlan, RunRequest, default_out_dir, driver_for};
use lp_emu_validate::find_payload;

/// The shipped-image payloads a hosted plan can be made for, on each
/// configuration that runs them.
const HOSTED: &[(&str, &str)] = &[
    ("lp-emu:esp32c6:t1", "boot-idle"),
    ("lp-emu:esp32c6:t2", "boot-idle-flash"),
    ("lp-emu:esp32c6:t1", "rom-up-boot"),
    ("lp-emu:esp32c6:t1", "usb-detach-reattach"),
    ("lp-emu:esp32c6:t1", "usb-negative-control"),
    // A capability seam's composite: the plan states `--seams net=lan`.
    ("lp-emu:esp32c6:t1+net=lan", "boot-idle"),
    ("silicon:esp32c6", "boot-idle"),
    ("silicon:esp32c6", "boot-idle-flash"),
    ("silicon:esp32c6", "usb-negative-control"),
];

#[test]
fn every_hosted_emulated_run_parses_as_lp_cli_emu_run() {
    let host = host();
    for (config, payload) in HOSTED.iter().filter(|(c, _)| c.starts_with("lp-emu:")) {
        let plan = plan(config, payload, &host);
        let run = &plan.steps.last().expect("a run step").command;
        let args = tail_after(run, &host.emulated, "emu run");
        let cli = Wrapped::try_parse_from(
            std::iter::once("lp-cli-emu".to_string())
                .chain(["run".to_string()])
                .chain(run[host.emulated.len() - HOST_FLAGS..].iter().cloned()),
        )
        .unwrap_or_else(|e| panic!("{payload} on {config}: {e}\n  {}", args.join(" ")));
        let EmuCommand::Run(parsed) = cli.emu.command else {
            panic!("{payload} on {config}: not `emu run`");
        };
        assert!(parsed.host_link, "{payload} on {config}: not hosted");
        assert!(parsed.json_replies, "{payload} on {config}: packs replies");
        assert_eq!(
            parsed.link_nonce,
            Some(u32::from_str_radix(RECORDING_LINK_NONCE, 16).unwrap()),
            "{payload} on {config}: a recording's nonce is fixed"
        );
        assert_eq!(
            parsed.console,
            Some(plan.capture.clone()),
            "{payload} on {config}: the console is the capture"
        );
        assert!(parsed.strict_bus, "{payload} on {config}");
        // The configuration is the whole seam request: its atoms, strictly,
        // or `none` — never the machine's capability defaults behind the
        // label (`RunRequest::seams`).
        let want = config.split_once('+').map_or("none", |(_, atoms)| atoms);
        assert_eq!(
            parsed.seams.as_deref(),
            Some(want),
            "{payload} on {config}: the seams the label names"
        );
        assert_eq!(parsed.seams_prefer, None, "{payload} on {config}");
    }
}

#[test]
fn every_hosted_silicon_capture_parses_as_lp_cli_link_capture() {
    let host = host();
    for (config, payload) in HOSTED.iter().filter(|(c, _)| c.starts_with("silicon:")) {
        let plan = plan(config, payload, &host);
        let open = &plan.steps.last().expect("an open step").command;
        tail_after(open, &host.port, "link capture");
        let cli = LinkCli::try_parse_from(
            std::iter::once("link".to_string()).chain(open[host.port.len() - 2..].iter().cloned()),
        )
        .unwrap_or_else(|e| panic!("{payload} on {config}: {e}\n  {}", open.join(" ")));
        let LinkSubcommand::Capture(parsed) = cli.subcommand else {
            panic!("{payload} on {config}: not `link capture`");
        };
        assert_eq!(parsed.target, "/dev/cu.usbmodem-parity");
        assert_eq!(parsed.console, plan.capture);
        assert!(parsed.json_replies, "{payload} on {config}: packs replies");
        assert!(
            parsed.exit_on.is_some(),
            "{payload} on {config}: no sentinel"
        );
    }
}

#[test]
fn raw_supplies_no_host() {
    assert_eq!(link_host("raw"), None);
}

/// `emu run --host-link --json-replies --link-nonce <n>`: the flags the
/// host's own prefix carries after `emu run`, which the parse above keeps.
const HOST_FLAGS: usize = 4;

#[derive(Parser)]
struct Wrapped {
    #[command(flatten)]
    emu: EmuCli,
}

fn host() -> LinkHost {
    link_host("lp-link").expect("lp-link supplies a host")
}

fn plan(config: &str, payload: &str, host: &LinkHost) -> RunPlan {
    let cfg = ValidateConfig::embedded();
    let entry = cfg.configuration(config).unwrap();
    let configuration = entry.parsed().unwrap();
    let req = RunRequest {
        payload: find_payload(payload).unwrap(),
        configuration: configuration.clone(),
        port: config
            .starts_with("silicon:")
            .then(|| "/dev/cu.usbmodem-parity".to_string()),
        timeout_secs: 30,
        repo_root: PathBuf::from("/repo"),
        out_dir: default_out_dir(),
        image: None,
        link_override: None,
        identity: entry.identity(),
        chip: entry.chip.clone(),
        link_host: Some(host.clone()),
        seams: entry
            .seams
            .iter()
            .map(|s| format!("{}={}", s.seam, s.implementation))
            .collect(),
    };
    assert!(
        req.hosted().unwrap().is_some(),
        "{payload} on {config} is not hosted"
    );
    driver_for(&configuration)
        .plan(&req)
        .unwrap_or_else(|e| panic!("{payload} on {config}: {e:#}"))
}

/// The command must start with the host's prefix, and the prefix must be
/// `cargo run … lp-cli … -- <subcommand>`.
fn tail_after(command: &[String], prefix: &[String], subcommand: &str) -> Vec<String> {
    assert_eq!(
        &command[..prefix.len()],
        prefix,
        "the step does not start with the host's prefix"
    );
    let joined = prefix.join(" ");
    assert!(
        joined.contains(&format!("-p lp-cli --release -- {subcommand}")),
        "{joined}"
    );
    command[prefix.len()..].to_vec()
}
