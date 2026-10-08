//! Two shipped C6 boards on one virtual LAN, in lockstep — the CI cell for
//! Wi-Fi in the emulator (plan `lp2025/2026-10-05-1903-wifi-link-c6`, P12):
//! the real `fw-esp32c6` image, its network seam answered by a virtual LAN
//! (`net=lan`), and nothing timed by the host.
//!
//! The firmware joins only a network it has saved, and saving one is a wire
//! request over the board's USB link (`lp-cli wifi add`), which the MIT
//! emulator crate cannot speak (the `lp-emu/` fence). So each board is
//! **prepared** first, alone, over its USB link with the product's own client
//! (`lpa-client` over `lp-cli`'s in-process link host), on a LAN of its own
//! with the same access points:
//!
//! - a board with nothing saved boots, engages the seam, plugs it in instead
//!   of the radio (`[wifi] network: the emulator's network seam`) and never
//!   scans;
//! - the fixture's network is added; the board's own status then says
//!   `connected`, with the LAN's address and its `lp-xxxx.local`;
//! - its flash, which now holds the saved network, is kept.
//!
//! Then the **pair**: both prepared flashes on two machines held by the
//! lockstep runner, one runner-driven LAN between them
//! (`Lockstep::with_medium`). Each board takes an address over DHCP, a
//! [`LanProbe`](lp_emu_esp_common::seam::net::LanProbe) on the segment
//! resolves each board's `lp-xxxx.local` to it, and the same run twice gives
//! the same frame log, cycle for cycle.
//!
//! A third board says why a join failed: a refused password ends
//! `failed { wrongPassword }` (and the network's last attempt says so), and a
//! name not in range `failed { notFound }`.
//!
//! **Deterministic.** Every wait is counted in guest time (emulated
//! microseconds and cycles), never the host's; nothing asserts how long
//! anything took. Test values only (`lp-lockstep-net` / `staple-battery-9`).
//! In lp-cli because the link host is a product crate. `#[ignore]`d: it needs
//! a built `fw-esp32c6` ELF (`LP_EMU_BUILD_FW=1`, or `LP_CI_IMAGES`); `just
//! test-emu-c6-cli` runs it. Figures it prints are `lp-emu:esp32c6:t1+net=lan`.

use std::future::Future;
use std::net::Ipv4Addr;
use std::path::Path;
use std::pin::pin;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};

use lp_cli::commands::emu::link_host::{C6Board, EmuLinkHost};
use lp_emu_esp_common::ParticipantId;
use lp_emu_esp_common::seam::net::lan_dns::TYPE_A;
use lp_emu_esp_common::seam::net::{
    FrameRecord, LanConfig, LanDriver, LanPort, SharedLan, VirtualAccessPoint, VirtualLan,
    net_endpoint,
};
use lp_emu_esp32c6::flash::FlashBacking;
use lp_emu_esp32c6::loader::EfuseIdentity;
use lp_emu_esp32c6::lockstep::Lockstep;
use lp_emu_esp32c6::machine::{AppSource, Esp32C6Builder, StopCondition, TimeGrade, UsbHost};
use lp_emu_esp32c6::memmap;
use lp_emu_esp32c6::test_support::{FwImage, fw_esp32c6_image};
use lpa_client::LpClient;
use lpc_wire::WifiPassword;
use lpc_wire::server::{LastAttempt, NetworkStatus, StationFailure, StationState};

const SSID: &str = "lp-lockstep-net";
const PASSWORD: &str = "staple-battery-9";
/// A second network in range, open and weaker: a scan hears two.
const OPEN_SSID: &str = "lp-lockstep-cafe";
/// A name no access point carries.
const ABSENT_SSID: &str = "lp-not-in-range";

/// The two boards' eFuse MACs, and so their station MACs and their names
/// (`lp-` and the MAC's last two bytes, `fw_esp32_common::net::mdns`).
const MACS: [[u8; 6]; 2] = [
    [0xa0, 0xf2, 0x62, 0x87, 0xb4, 0x8c],
    [0xa0, 0xf2, 0x62, 0x85, 0xa8, 0x7c],
];
const NAMES: [&str; 2] = ["lp-b48c.local", "lp-a87c.local"];
/// The third board, the one whose joins fail.
const FAILING_MAC: [u8; 6] = [0xa0, 0xf2, 0x62, 0x80, 0x01, 0x02];

/// How long a board with nothing saved runs before it is checked for a scan,
/// in emulated microseconds (past its first heartbeat).
const IDLE_US: u64 = 6_000_000;
/// How long a board may take to reach a station state, in emulated
/// microseconds.
const STATION_BUDGET_US: u64 = 60_000_000;
/// The pause between two status reads while a board joins.
const STATUS_GAP_US: u64 = 250_000;
/// The pair's budget for both addresses, then for both names, in emulated
/// microseconds; and its step while it waits.
const PAIR_ADDRESS_BUDGET_US: u64 = 30_000_000;
const PAIR_NAME_BUDGET_US: u64 = 5_000_000;
const PAIR_STEP_US: u64 = 100_000;
/// How long the pair runs after both leases before the probe asks: past
/// each board's two announcements (at the address, and 1 s later).
const PAIR_SETTLE_US: u64 = 3_000_000;

#[test]
#[ignore = "needs a built fw-esp32c6 ELF; `just test-emu-c6-cli` runs it"]
fn two_boards_in_lockstep_take_addresses_and_answer_their_names_the_same_way_twice() {
    let Some(elf) = image() else { return };
    let flashes = [0, 1].map(|i| prepare(&elf, i));

    let first = pair_run(&elf, &flashes);
    let second = pair_run(&elf, &flashes);

    assert_ne!(first.ips[0], first.ips[1], "two leases");
    assert_eq!(
        first.resolved,
        [Some(first.ips[0]), Some(first.ips[1])],
        "the probe resolved each board's name to that board's lease"
    );
    for i in 0..2 {
        let me = LanPort::Board(net_endpoint(ParticipantId(i)));
        assert!(
            first.log.iter().any(|r| r.from == me),
            "board {i} put frames on the LAN"
        );
    }
    assert_eq!(first.ips, second.ips, "the same leases twice");
    assert_eq!(first.cycles, second.cycles, "at the same guest cycles");
    assert_eq!(
        first.log.len(),
        second.log.len(),
        "the same number of frames"
    );
    assert!(
        first.log == second.log,
        "the same run twice, the same frame log"
    );
    eprintln!(
        "emu_lan_lockstep: pair leases {} and {}, both bound by {:.2} s emulated and both names \
         answered by {:.2} s; {} frames (lp-emu:esp32c6:t1+net=lan)",
        first.ips[0],
        first.ips[1],
        seconds(first.cycles[0]),
        seconds(first.cycles[1]),
        first.log.len()
    );
}

#[test]
#[ignore = "needs a built fw-esp32c6 ELF; `just test-emu-c6-cli` runs it"]
fn a_refused_password_ends_wrong_password_and_a_name_not_in_range_ends_not_found() {
    let Some(elf) = image() else { return };
    let lan = SharedLan::new(fixture_lan(), LanDriver::SelfDriven);
    let mut board = Hosted::new(solo_host(&elf, &lan, FAILING_MAC, None));
    board.ask(|c| block_on(c.hello())).expect("hello");

    board
        .ask(|c| {
            block_on(c.network_add(
                String::from(SSID),
                WifiPassword::new("not-the-password"),
                None,
            ))
        })
        .expect("add with a wrong password");
    let status =
        board.wait_station(|s| matches!(s, StationState::Failed { ssid, .. } if ssid == SSID));
    assert_eq!(
        status.station,
        StationState::Failed {
            ssid: String::from(SSID),
            reason: StationFailure::WrongPassword,
        }
    );
    let saved = status.networks.iter().find(|n| n.ssid == SSID).unwrap();
    assert_eq!(saved.last, Some(LastAttempt::WrongPassword));

    board
        .ask(|c| {
            block_on(c.network_add(
                String::from(ABSENT_SSID),
                WifiPassword::new("any-password-1"),
                None,
            ))
        })
        .expect("add a network not in range");
    let status = board
        .wait_station(|s| matches!(s, StationState::Failed { ssid, .. } if ssid == ABSENT_SSID));
    assert_eq!(
        status.station,
        StationState::Failed {
            ssid: String::from(ABSENT_SSID),
            reason: StationFailure::NotFound,
        }
    );
    let saved = status
        .networks
        .iter()
        .find(|n| n.ssid == ABSENT_SSID)
        .unwrap();
    assert_eq!(saved.last, Some(LastAttempt::NotFound));
    assert!(
        lan.address(net_endpoint(ParticipantId(0))).is_none(),
        "no lease for a board that never joined"
    );
    assert_eq!(board.host.link_errors, 0, "a clean link");
    echo_wifi_lines("failing board", &board.host);
}

/// The fixture's access points: the network the boards save, and an open one.
fn fixture_lan() -> VirtualLan {
    VirtualLan::new(LanConfig::new(memmap::CYCLES_PER_US))
        .with_access_point(VirtualAccessPoint::secured(SSID, PASSWORD, -45))
        .with_access_point(VirtualAccessPoint::open(OPEN_SSID, -80))
}

/// Board `index`, alone on a LAN of its own, over its USB link: check the
/// board with nothing saved, save the fixture's network, wait until the
/// board says it joined, and hand back its flash.
fn prepare(elf: &Path, index: usize) -> Vec<u8> {
    let lan = SharedLan::new(fixture_lan(), LanDriver::SelfDriven);
    let me = net_endpoint(ParticipantId(index));
    let mut board = Hosted::new(solo_host(
        elf,
        &lan,
        MACS[index],
        Some(ParticipantId(index)),
    ));
    let hello = board.ask(|c| block_on(c.hello())).expect("hello").value;
    assert_eq!(hello.proto, lpc_wire::WIRE_PROTO_VERSION);

    // Nothing saved: not connected, on the LAN, and it never scans.
    let status = board.status();
    assert!(status.networks.is_empty());
    assert_eq!(status.station, StationState::NotConnected);
    board.run_for(IDLE_US);
    assert!(
        lan.with(|l| l.station(me).is_some()),
        "the board is on its LAN"
    );
    assert!(
        lan.scan_results(me).is_empty() && !lan.has_event(me),
        "a board with nothing saved never scans"
    );
    assert!(lan.address(me).is_none(), "and holds no lease");
    let machine = &board.host.board.machine;
    let seams = &machine.seams().lines;
    assert!(
        seams.iter().any(|l| l.starts_with("SEAM net=lan engaged")),
        "{seams:?}"
    );
    assert!(machine.configuration_label().ends_with("+net=lan"));
    assert!(
        board
            .host
            .console()
            .iter()
            .any(|l| l.contains("[wifi] network: the emulator's network seam")),
        "the firmware plugged the seam in:\n{}",
        board.host.console().join("\n")
    );

    // Save the fixture's network; the board joins it.
    let added = board
        .ask(|c| block_on(c.network_add(String::from(SSID), WifiPassword::new(PASSWORD), None)))
        .expect("`wifi add` over USB")
        .value;
    assert_eq!(added.networks.len(), 1, "{added:?}");
    let status = board.wait_station(|s| {
        matches!(
            s,
            StationState::Connected { .. } | StationState::Failed { .. }
        )
    });
    let lease = match &status.station {
        StationState::Connected { ssid, ip, host, .. } => {
            let lease = lan
                .address(me)
                .expect("the LAN leased the board an address");
            assert_eq!(ssid, SSID);
            assert_eq!(ip, &lease.to_string(), "the board's address is its lease");
            assert_eq!(host, NAMES[index]);
            lease
        }
        other => panic!(
            "board {index} did not join: {other:?}\n{}",
            board.host.console().join("\n")
        ),
    };
    assert_eq!(board.host.link_errors, 0, "a clean link");
    echo_wifi_lines(&format!("board {index}"), &board.host);
    for line in &board.host.board.machine.seams().lines {
        eprintln!("emu_lan_lockstep: board {index}: {line}");
    }
    eprintln!(
        "emu_lan_lockstep: board {index} joined as {} at {lease} by {:.2} s emulated \
         (lp-emu:esp32c6:t1+net=lan)",
        NAMES[index],
        board.host.board_seconds()
    );
    let flash = board
        .host
        .board
        .machine
        .flash()
        .lock()
        .unwrap()
        .bytes()
        .to_vec();
    flash
}

/// What one run of the pair saw.
struct PairRun {
    ips: [Ipv4Addr; 2],
    resolved: [Option<Ipv4Addr>; 2],
    /// The guest cycle both boards held an address by, then both names
    /// resolved by.
    cycles: [u64; 2],
    log: Vec<FrameRecord>,
}

/// The two prepared boards on one runner-driven LAN, in lockstep.
fn pair_run(elf: &Path, flashes: &[Vec<u8>; 2]) -> PairRun {
    let lan = SharedLan::new(fixture_lan(), LanDriver::Runner);
    lan.with(|l| l.log_frames(true));
    let probe = lan.with(|l| l.add_probe());
    let boards = (0..2)
        .map(|i| {
            Esp32C6Builder::new()
                .app(AppSource::Path(elf.to_path_buf()))
                .time_grade(TimeGrade::T1)
                .usb_host(UsbHost::Attached { draining: true })
                .efuse(EfuseIdentity {
                    mac: MACS[i],
                    ..EfuseIdentity::default()
                })
                .flash(FlashBacking::Bytes(flashes[i].clone()))
                .lan(lan.clone(), ParticipantId(i))
                .build()
                .expect("the shipped image builds a machine")
        })
        .collect::<Vec<_>>();
    let mut pair = Lockstep::new(boards)
        .expect("two boards")
        .with_medium(Box::new(lan.clone()));
    let leases = || [0, 1].map(|i| lan.address(net_endpoint(ParticipantId(i))));

    let up = advance_until(&mut pair, PAIR_ADDRESS_BUDGET_US, || {
        leases().iter().all(Option::is_some)
    })
    .unwrap_or_else(|| panic!("both boards never held an address: {:?}", leases()));
    let ips = leases().map(Option::unwrap);
    for i in 0..2 {
        let m = pair.machine(ParticipantId(i)).unwrap();
        assert!(m.configuration_label().ends_with("+net=lan"));
    }

    // Past both boards' two announcements (at the address, then 1 s later),
    // so what the probe holds next is an answer to its own question.
    advance(&mut pair, PAIR_SETTLE_US);
    let asked = pair.cycles();
    lan.with(|l| {
        let probe = l.probe_mut(probe);
        probe.clear_answers();
        for name in NAMES {
            probe.query(name, TYPE_A);
        }
    });
    let resolved = || NAMES.map(|n| lan.with(|l| l.probe(probe).resolved(n)));
    let named = advance_until(&mut pair, PAIR_NAME_BUDGET_US, || {
        resolved().iter().all(Option::is_some)
    })
    .unwrap_or_else(|| panic!("the names never resolved: {:?}", resolved()));
    assert!(named > asked, "answered after it was asked");
    let resolved = resolved();
    PairRun {
        ips,
        resolved,
        cycles: [up, named],
        log: lan.with(|l| l.frame_log().to_vec()),
    }
}

/// Run the pair in steps until `done`, for at most `budget_us` more emulated
/// microseconds; the guest cycle it was done by.
fn advance_until(pair: &mut Lockstep, budget_us: u64, done: impl Fn() -> bool) -> Option<u64> {
    let end = pair.cycles() + budget_us * memmap::CYCLES_PER_US;
    while pair.cycles() < end {
        if done() {
            return Some(pair.cycles());
        }
        advance(pair, PAIR_STEP_US);
    }
    done().then(|| pair.cycles())
}

/// Run the pair `us` more emulated microseconds; every board must get there.
fn advance(pair: &mut Lockstep, us: u64) {
    let horizon = pair.cycles() + us * memmap::CYCLES_PER_US;
    let report = pair.run_until(horizon, &StopCondition::default());
    assert!(
        report.all_reached_the_horizon(),
        "a board stopped: {report:?}"
    );
}

/// Guest cycles as emulated seconds.
fn seconds(cycles: u64) -> f64 {
    cycles as f64 / (memmap::CYCLES_PER_US as f64 * 1e6)
}

/// A board alone on `lan` with the product's host end on its USB link.
fn solo_host(
    elf: &Path,
    lan: &SharedLan,
    mac: [u8; 6],
    board: Option<ParticipantId>,
) -> EmuLinkHost<C6Board> {
    let machine = Esp32C6Builder::new()
        .app(AppSource::Path(elf.to_path_buf()))
        .time_grade(TimeGrade::T1)
        .usb_host(UsbHost::Attached { draining: true })
        .usb_sj_queue_source()
        .efuse(EfuseIdentity {
            mac,
            ..EfuseIdentity::default()
        })
        .lan(lan.clone(), board.unwrap_or(ParticipantId(0)))
        .build()
        .expect("building the emulated C6");
    // A fixed nonce, so a prepared board's flash is the same every run.
    EmuLinkHost::new(
        C6Board::new(machine).expect("a hosted board"),
        0x1a40_5eed,
        true,
    )
}

/// A hosted board and the next request id its client may use: each ask is a
/// fresh client on the same link, in an id space no earlier ask used.
struct Hosted {
    host: EmuLinkHost<C6Board>,
    next_id: u64,
}

impl Hosted {
    fn new(host: EmuLinkHost<C6Board>) -> Self {
        Self { host, next_id: 1 }
    }

    /// One conversation with the board through the product's client.
    fn ask<T>(&mut self, f: impl FnOnce(&mut LpClient<&mut EmuLinkHost<C6Board>>) -> T) -> T {
        let mut client = LpClient::new(&mut self.host).with_request_ids_from(self.next_id);
        self.next_id += 1_000;
        f(&mut client)
    }

    fn status(&mut self) -> NetworkStatus {
        self.ask(|c| block_on(c.network_status()))
            .expect("`wifi status` over USB")
            .value
    }

    /// Run the board `us` more emulated microseconds.
    fn run_for(&mut self, us: u64) {
        let until = self.host.board.machine.micros() + us;
        self.host.run_until(until, None).expect("the board runs");
    }

    /// Read the board's status until `done` holds for its station, stepping
    /// [`STATUS_GAP_US`] between reads, for at most [`STATION_BUDGET_US`].
    fn wait_station(&mut self, done: impl Fn(&StationState) -> bool) -> NetworkStatus {
        let mut waited = 0;
        loop {
            let status = self.status();
            if done(&status.station) {
                return status;
            }
            assert!(
                waited < STATION_BUDGET_US,
                "no such station state within {STATION_BUDGET_US} emulated µs; last {:?}",
                status.station
            );
            self.run_for(STATUS_GAP_US);
            waited += STATUS_GAP_US;
        }
    }
}

/// The board's `[wifi]` and `[mdns]` lines, for the run's record.
fn echo_wifi_lines(who: &str, host: &EmuLinkHost<C6Board>) {
    for line in host
        .console()
        .iter()
        .filter(|l| l.contains("[wifi]") || l.contains("[mdns]"))
    {
        eprintln!("emu_lan_lockstep: {who}: {line}");
    }
}

fn image() -> Option<std::path::PathBuf> {
    match fw_esp32c6_image(&FwImage::SHIPPED) {
        Ok(path) => Some(path),
        Err(reason) => {
            eprintln!("emu_lan_lockstep: skipped — {reason}");
            None
        }
    }
}

/// Drive a future whose every await completes synchronously (the host steps
/// the board inside `receive`): tests are edges, and a null waker is enough.
fn block_on<F: Future>(future: F) -> F::Output {
    struct Noop;
    impl Wake for Noop {
        fn wake(self: Arc<Self>) {}
    }
    let waker = Waker::from(Arc::new(Noop));
    let mut context = Context::from_waker(&waker);
    let mut future = pin!(future);
    loop {
        if let Poll::Ready(output) = future.as_mut().poll(&mut context) {
            return output;
        }
    }
}
