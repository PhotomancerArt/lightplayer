//! Over-the-air updates over the EMULATED LAN, scenario by scenario (OTA
//! Wi-Fi plan P4; `just test-emu-c6-ota-lan`).
//!
//! The board is the shipped split image booted from the reset vector out of
//! its own writable flash (`emu serve`, `kind=rom-up`, a reset reboots it),
//! on a virtual LAN (`--lan`, the network seam `net=lan`) with **no USB
//! cable at all** (`--usb-host absent`): its own station joins, its own LAN
//! endpoint and secure link serve, in core-only too. The host is `lp-cli link
//! capture lan:<forward> --ota-offer …`, a process of its own — the OTA host
//! over `lan:`, reconnecting across the update's resets — and the test holds
//! the board's power (`power-cycle`) and its DHCP (`renumber`) on the
//! door's control channel.
//!
//! Every assertion is on what the board said on its link (its channel-3
//! manifests and refusals, as the host's console writes them), on the
//! door's account of it (`GET /boards`: reboots), or on the host's own end
//! (`done: …`, the engine it kept). With no USB host there is no board
//! console to read: what the board says over the LAN is what a house board
//! says. Nothing asserts on time; every wait is a wall-clock safety net.
//!
//! The images (named by `LP_OTA_LAN_IMAGES`, each a `scripts/ota/build-image.sh`
//! output): `x` (app version `a0a0a0a0`) and `y` (`b1b1b1b1`). The board
//! starts from `x` with the fixture's network saved, put there over USB
//! first (`emu run --host-link --request networkAdd`), which is the one step
//! that has a cable. Test values only (`lp-ota-net`). Figures it prints are
//! `lp-emu:esp32c6:t1+net=lan`. Not in CI (DM26).

mod support;

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use lpa_client::HostSpecifier;
use lpa_client::transport_lan::{LanError, LanLink, LanOptions, LanTarget};
use lpc_firmware_release::OtaManifest;
use support::{Serve, scratch};

const SSID: &str = "lp-ota-net";
/// The locked board's two passwords (made-up test values).
const EDIT_PASSWORD: &str = "lamp-edit-test";
const PLAY_PASSWORD: &str = "lamp-play-test";
const PASSWORD: &str = "ota-test-pass-7";
const FIXTURE: &str = r#"# emu_ota_lan.rs: made-up test values only.
[[access_point]]
name = "lp-ota-net"
password = "ota-test-pass-7"
signal_dbm = -50
"#;

/// The wall-clock net on one whole update (three resets, three rejoins).
const UPDATE_NET: Duration = Duration::from_secs(480);
/// The id of the one board.
const BOARD: &str = "c6";

// --- 1, 4: X -> Y over the LAN with a backup; the trial confirmed by the LAN ------

#[test]
#[ignore = "needs the OTA images; `just test-emu-c6-ota-lan`"]
fn l1_x_to_y_over_the_lan_with_a_backup_and_the_trial_confirmed_there() {
    let Some((x, y)) = images() else { return };
    let dir = scratch();
    let board = LanBoard::start(&prepared(&x, &dir), &dir);
    let cache = dir.join("cache");
    let mut host = Host::start(&board, &y, &cache, &dir, "l1");
    let console = host.wait_done(UPDATE_NET);
    assert!(console.contains("done: UpToDate"), "{}", tail(&console));
    // The backup read back X's engine before anything moved.
    let kept = std::fs::read(cache.join(format!("{}.bin", x.manifest.engine.sha256)))
        .expect("the backup landed in the cache");
    assert_eq!(kept, x.engine(), "the read-back is X's engine.bin");
    // X running first; Y on trial, confirmed by the only host it had — the
    // LAN's (there is no cable) — and then Y running.
    let boards = manifests(&console);
    assert!(
        boards
            .first()
            .is_some_and(|b| b.contains(&x.build()) && b.contains("Running")),
        "{boards:#?}"
    );
    assert!(
        boards
            .iter()
            .any(|b| b.contains(&y.build()) && b.contains("OnTrial")),
        "the trial core spoke on the LAN: {boards:#?}"
    );
    assert!(
        boards
            .last()
            .is_some_and(|b| b.contains(&y.build()) && b.contains("Running")),
        "{boards:#?}"
    );
    assert!(board.reboots() >= 3, "three resets: {}", board.reboots());
    report("L1/L4", &console);
}

// --- 2: power cuts in the core piece and in the engine piece -----------------------

#[test]
#[ignore = "needs the OTA images; `just test-emu-c6-ota-lan`"]
fn l2_power_cuts_in_the_core_and_the_engine_converge_with_no_cable() {
    let Some((x, y)) = images() else { return };
    let dir = scratch();
    let board = LanBoard::start(&prepared(&x, &dir), &dir);
    let mut host = Host::start(&board, &y, &dir.join("cache"), &dir, "l2");
    let mut control = board.serve.control(BOARD);
    // In the core piece: the board is in core-only, taking chunks.
    host.wait_line("(Core ", UPDATE_NET);
    std::thread::sleep(Duration::from_secs(3));
    let cut1 = control.cmd("power-cycle");
    eprintln!("emu_ota_lan l2: cut in the core piece: {cut1}");
    // In the engine piece (the host's `Finishing` stage: the trial core
    // confirmed, its engine moving).
    host.wait_line("stage Finishing", UPDATE_NET);
    std::thread::sleep(Duration::from_secs(2));
    let cut2 = control.cmd("power-cycle");
    eprintln!("emu_ota_lan l2: cut in the engine piece: {cut2}");
    let console = host.wait_done(UPDATE_NET);
    assert!(console.contains("done: UpToDate"), "{}", tail(&console));
    assert!(
        manifests(&console)
            .last()
            .is_some_and(|b| b.contains(&y.build()) && b.contains("Running"))
    );
    assert_eq!(board.power_cycles(), 2);
    report("L2", &console);
}

// --- 3: an engine-less board heals over the LAN ------------------------------------

#[test]
#[ignore = "needs the OTA images; `just test-emu-c6-ota-lan`"]
fn l3_an_engine_less_board_heals_over_the_lan_with_no_login() {
    let Some((x, _)) = images() else { return };
    let dir = scratch();
    let flash = prepared(&x, &dir);
    erase_engine_header(&flash, &x);
    let board = LanBoard::start(&flash, &dir);
    let cache = dir.join("cache");
    std::fs::create_dir_all(&cache).unwrap();
    std::fs::write(
        cache.join(format!("{}.bin", x.manifest.engine.sha256)),
        x.engine(),
    )
    .unwrap();
    let mut host = Host::start(&board, &x, &cache, &dir, "l3");
    let console = host.wait_done(UPDATE_NET);
    let boards = manifests(&console);
    assert!(
        boards.first().is_some_and(|b| b.contains("NeedsEngine")),
        "{boards:#?}"
    );
    assert!(console.contains("done: "), "{}", tail(&console));
    assert!(
        boards
            .last()
            .is_some_and(|b| b.contains(&x.build()) && b.contains("Running")),
        "{boards:#?}"
    );
    report("L3", &console);
}

// --- 5, 6: the address moves mid-update; a second client is told busy -------------

#[test]
#[ignore = "needs the OTA images; `just test-emu-c6-ota-lan`"]
fn l5_l6_a_moved_address_and_a_second_client_do_not_stop_the_update() {
    let Some((x, y)) = images() else { return };
    let dir = scratch();
    let board = LanBoard::start(&prepared(&x, &dir), &dir);
    let mut host = Host::start(&board, &y, &dir.join("cache"), &dir, "l5");
    let mut control = board.serve.control(BOARD);
    host.wait_line("(Core ", UPDATE_NET);
    // L6: a second client while the update holds the one LAN slot.
    let second = LanLink::open(&board.target().endpoint(), &LanOptions::default());
    assert!(
        matches!(second, Err(LanError::Busy { .. })),
        "a second client is told busy: {:?}",
        second.err()
    );
    // L5: the board's next lease is another address (after the core's
    // reset it rejoins and is renumbered).
    let renumbered = control.cmd("renumber");
    assert!(renumbered.starts_with("ok renumber"), "{renumbered}");
    let before = board.address();
    let console = host.wait_done(UPDATE_NET);
    assert!(console.contains("done: UpToDate"), "{}", tail(&console));
    let after = board.address();
    eprintln!("emu_ota_lan l5: address {before:?} → {after:?}");
    assert_ne!(before, after, "the board was renumbered");
    report("L5/L6", &console);
}

// --- 7: access over the LAN: the key decides -------------------------------------

#[test]
#[ignore = "needs the OTA images; `just test-emu-c6-ota-lan`"]
fn l7_a_play_password_is_refused_a_core_and_an_edit_password_updates_through_core_only() {
    let Some((x, y)) = images() else { return };
    let dir = scratch();
    let flash = prepared_locked(&x, &dir);

    // Play: the engine's server grants the key play; the offer is refused
    // in words, before anything moves.
    let board = LanBoard::start(&flash, &dir.join("play"));
    let mut host =
        Host::start_with_password(&board, &y, &dir.join("cache"), &dir, "l7a", PLAY_PASSWORD);
    let console = host.wait_done(UPDATE_NET);
    assert!(
        console.contains("board refused: Access"),
        "{}",
        tail(&console)
    );
    assert!(
        console.contains("an update needs edit access"),
        "{}",
        tail(&console)
    );
    assert_eq!(board.reboots(), 0, "nothing moved");
    drop(board);

    // Edit: the whole update. After the first reset the board is in
    // core-only, which has no server: it answers the key lp-cli held from
    // the engine's session out of its own store, and takes the core on it.
    let board = LanBoard::start(&flash, &dir.join("edit"));
    let mut host =
        Host::start_with_password(&board, &y, &dir.join("cache"), &dir, "l7b", EDIT_PASSWORD);
    let console = host.wait_done(UPDATE_NET);
    assert!(console.contains("done: UpToDate"), "{}", tail(&console));
    let boards = manifests(&console);
    assert!(
        boards.iter().any(|b| b.contains("Updating (Core")),
        "core-only took the core on the held key: {boards:#?}"
    );
    assert!(
        boards
            .last()
            .is_some_and(|b| b.contains(&y.build()) && b.contains("Running")),
        "{boards:#?}"
    );
    report("L7", &console);
}

// --- 8: a trial core that hears from no host gives the board back --------------------

#[test]
#[ignore = "needs the OTA images; `just test-emu-c6-ota-lan`"]
fn l8_a_trial_core_with_no_host_rolls_back_and_heals_over_the_lan() {
    let Some((x, y)) = images() else { return };
    let dir = scratch();
    let board = LanBoard::start(&prepared(&x, &dir), &dir);
    let cache = dir.join("cache");
    let mut host = Host::start(&board, &y, &cache, &dir, "l8a");
    // The first reset hands over to core-only; the second is the core's
    // commit, into Y's trial core. Take the host away before it is back.
    let deadline = Instant::now() + UPDATE_NET;
    while board.reboots() < 2 {
        assert!(Instant::now() < deadline, "the core never committed");
        std::thread::sleep(Duration::from_millis(20));
    }
    host.stop();
    eprintln!("emu_ota_lan l8: host gone at the core's commit; waiting out the trial");
    // No host for the deadline: the trial core resets itself (warm) and the
    // loader rolls back to X, engine-less.
    let deadline = Instant::now() + UPDATE_NET;
    while board.reboots() < 3 {
        assert!(
            Instant::now() < deadline,
            "the trial never gave the board back"
        );
        std::thread::sleep(Duration::from_millis(200));
    }
    // X's engine heals it over the LAN, from the backup the first host kept.
    let mut heal = Host::start(&board, &x, &cache, &dir, "l8b");
    let console = heal.wait_done(UPDATE_NET);
    let boards = manifests(&console);
    assert!(
        boards
            .first()
            .is_some_and(|b| b.contains(&x.build()) && b.contains("NeedsEngine")),
        "X came back engine-less: {boards:#?}"
    );
    // The trial failed: the board refuses Y's build from now on (E3).
    let refuses = format!(
        "(refuses build {:#010x})",
        lpc_update::build_hash(y.build().as_bytes())
    );
    assert!(
        boards.first().is_some_and(|b| b.contains(&refuses)),
        "the rolled-back board names Y as refused ({refuses}): {boards:#?}"
    );
    assert!(
        boards
            .last()
            .is_some_and(|b| b.contains(&x.build()) && b.contains("Running")),
        "{boards:#?}"
    );
    report("L8", &console);
}

// ---- helpers ----

/// One test image (a `scripts/ota/build-image.sh` output).
struct Image {
    dir: PathBuf,
    manifest: OtaManifest,
}

impl Image {
    fn build(&self) -> String {
        self.manifest.build_id()
    }

    fn engine(&self) -> Vec<u8> {
        std::fs::read(self.dir.join("engine.bin")).unwrap()
    }

    fn engine_offset(&self) -> usize {
        let split: serde_json::Value =
            serde_json::from_slice(&std::fs::read(self.dir.join("split.json")).unwrap()).unwrap();
        split["engine"]["offset"].as_u64().unwrap() as usize
    }
}

fn images() -> Option<(Image, Image)> {
    let Ok(root) = std::env::var("LP_OTA_LAN_IMAGES") else {
        support::skip("emu_ota_lan", "LP_OTA_LAN_IMAGES is not set");
        return None;
    };
    let load = |name: &str| {
        let dir = Path::new(&root).join(name);
        let manifest: OtaManifest =
            serde_json::from_slice(&std::fs::read(dir.join("ota/ota-manifest.json")).ok()?).ok()?;
        Some(Image { dir, manifest })
    };
    Some((load("x")?, load("y")?))
}

/// X's merged image with the fixture's network saved: booted once over USB
/// with the product's own request, then its flash kept.
fn prepared(x: &Image, dir: &Path) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    let fixture = fixture(dir);
    let flash = dir.join("x-net.bin");
    std::fs::copy(x.dir.join("merged.bin"), &flash).unwrap();
    let request = format!(r#"{{"networkAdd":{{"ssid":"{SSID}","password":"{PASSWORD}"}}}}"#);
    let output = Command::new(env!("CARGO_BIN_EXE_lp-cli"))
        .args(["emu", "run", "--rom-up-flash"])
        .arg(&flash)
        .args(["--host-link", "--lan"])
        .arg(&fixture)
        .args(["--request", &request, "--exit-on", "[wifi] connected"])
        .args([
            "--reboot-on-reset",
            "--timeout",
            "120s",
            "--wall-timeout",
            "360",
        ])
        .output()
        .expect("lp-cli emu run");
    let said = String::from_utf8_lossy(&output.stderr);
    assert!(
        said.contains("stopped on --exit-on") && said.contains("flash image written back"),
        "the network was not saved:\n{said}"
    );
    flash
}

/// [`prepared`], locked: open to nobody, with a password at edit and one
/// at play (cheap iterations: a test's, never a person's).
fn prepared_locked(x: &Image, dir: &Path) -> PathBuf {
    use lpc_access::{OpenTo, SecretEntry, Tier};
    use lpc_wire::ClientRequest;
    std::fs::create_dir_all(dir).unwrap();
    let fixture = fixture(dir);
    let flash = dir.join("x-locked.bin");
    std::fs::copy(x.dir.join("merged.bin"), &flash).unwrap();
    let json = |request: &ClientRequest| lpc_wire::json::to_string(request).unwrap();
    let edit =
        SecretEntry::from_password("edit", Tier::Edit, EDIT_PASSWORD.as_bytes(), [0x21; 16], 4);
    let play =
        SecretEntry::from_password("play", Tier::Play, PLAY_PASSWORD.as_bytes(), [0x22; 16], 4);
    let requests = [
        json(&ClientRequest::AccessAdd { entry: edit }),
        json(&ClientRequest::AccessAdd { entry: play }),
        json(&ClientRequest::AccessSetSwitches {
            ble_enabled: None,
            open: Some(OpenTo::Nobody),
        }),
        format!(r#"{{"networkAdd":{{"ssid":"{SSID}","password":"{PASSWORD}"}}}}"#),
    ];
    let mut command = Command::new(env!("CARGO_BIN_EXE_lp-cli"));
    command
        .args(["emu", "run", "--rom-up-flash"])
        .arg(&flash)
        .args(["--host-link", "--lan"])
        .arg(&fixture);
    for request in &requests {
        command.args(["--request", request]);
    }
    let output = command
        .args(["--exit-on", "[wifi] connected"])
        .args([
            "--reboot-on-reset",
            "--timeout",
            "120s",
            "--wall-timeout",
            "360",
        ])
        .output()
        .expect("lp-cli emu run");
    let said = String::from_utf8_lossy(&output.stderr);
    assert!(
        said.contains("stopped on --exit-on") && said.contains("flash image written back"),
        "the board was not locked:\n{said}"
    );
    flash
}

fn fixture(dir: &Path) -> PathBuf {
    let path = dir.join("lan.toml");
    std::fs::write(&path, FIXTURE).unwrap();
    path
}

/// The engine's header sector erased: an engine-less board.
fn erase_engine_header(flash: &Path, img: &Image) {
    let mut bytes = std::fs::read(flash).unwrap();
    let at = img.engine_offset();
    bytes[at..at + 4096].fill(0xFF);
    std::fs::write(flash, bytes).unwrap();
}

/// The board under `emu serve`: on the LAN, no cable.
struct LanBoard {
    serve: Serve,
    forward: String,
}

impl LanBoard {
    fn start(flash: &Path, dir: &Path) -> Self {
        std::fs::create_dir_all(dir).unwrap();
        let fixture = fixture(dir);
        let lan = format!("home={}", fixture.display());
        let spec = format!("{BOARD}={},kind=rom-up,lan=home", flash.display());
        let serve = Serve::start_specs(
            &[spec],
            &["--lan", &lan, "--usb-host", "absent"],
            dir.join("serve"),
        );
        let forward = serve.board(BOARD)["forward"]
            .as_str()
            .expect("a forward")
            .to_string();
        let board = Self { serve, forward };
        board.wait_on_the_lan();
        board
    }

    /// The board joined its saved network and holds an address (a forward
    /// to a board with none is refused).
    fn wait_on_the_lan(&self) {
        let deadline = Instant::now() + UPDATE_NET;
        while self.address().is_none() {
            assert!(Instant::now() < deadline, "the board never joined its LAN");
            std::thread::sleep(Duration::from_millis(100));
        }
        // Its LAN endpoint listens once its address is up; give it a beat.
        std::thread::sleep(Duration::from_millis(500));
    }

    fn target(&self) -> LanTarget {
        LanTarget::from_specifier(&HostSpecifier::parse(&self.forward).unwrap()).unwrap()
    }

    fn reboots(&self) -> u64 {
        self.serve.board(BOARD)["reboots"].as_u64().unwrap_or(0)
    }

    fn power_cycles(&self) -> u64 {
        self.serve.board(BOARD)["power_cycles"]
            .as_u64()
            .unwrap_or(0)
    }

    fn address(&self) -> Option<String> {
        self.serve.board(BOARD)["address"]
            .as_str()
            .map(str::to_string)
    }
}

/// `lp-cli link capture lan:<forward> --ota-offer …`, a process of its own.
struct Host {
    child: Option<Child>,
    console: PathBuf,
}

impl Host {
    fn start(board: &LanBoard, offer: &Image, cache: &Path, dir: &Path, name: &str) -> Self {
        Self::spawn(board, offer, cache, dir, name, None)
    }

    /// With the board's password in `LP_PASSWORD` (never argv).
    fn start_with_password(
        board: &LanBoard,
        offer: &Image,
        cache: &Path,
        dir: &Path,
        name: &str,
        password: &str,
    ) -> Self {
        Self::spawn(board, offer, cache, dir, name, Some(password))
    }

    fn spawn(
        board: &LanBoard,
        offer: &Image,
        cache: &Path,
        dir: &Path,
        name: &str,
        password: Option<&str>,
    ) -> Self {
        let console = dir.join(format!("{name}.console.txt"));
        let log = std::fs::File::create(dir.join(format!("{name}.stderr.txt"))).unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_lp-cli"));
        match password {
            Some(password) => command.env("LP_PASSWORD", password),
            None => command.env_remove("LP_PASSWORD"),
        };
        let child = command
            .args(["link", "capture", &board.forward, "--console"])
            .arg(&console)
            .args([
                "--seconds",
                "560",
                "--json-replies",
                "--exit-on",
                "[host-ota] done",
            ])
            .arg("--ota-offer")
            .arg(offer.dir.join("ota"))
            .arg("--ota-cache")
            .arg(cache)
            .stdout(Stdio::null())
            .stderr(log)
            .spawn()
            .expect("lp-cli link capture");
        Self {
            child: Some(child),
            console,
        }
    }

    fn text(&self) -> String {
        std::fs::read(&self.console)
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .unwrap_or_default()
    }

    fn wait_line(&mut self, needle: &str, net: Duration) -> String {
        let deadline = Instant::now() + net;
        loop {
            let text = self.text();
            if let Some(line) = text.lines().find(|l| l.contains(needle)) {
                return line.to_string();
            }
            assert!(
                Instant::now() < deadline,
                "no `{needle}` from the host:\n{}",
                tail(&text)
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Wait for the host to end; its console.
    fn wait_done(&mut self, net: Duration) -> String {
        let deadline = Instant::now() + net;
        let child = self.child.as_mut().expect("running");
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                let text = self.text();
                assert!(
                    status.success(),
                    "the host failed ({status}):\n{}",
                    tail(&text)
                );
                self.child = None;
                return text;
            }
            assert!(
                Instant::now() < deadline,
                "the host never finished:\n{}",
                tail(&self.text())
            );
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        self.stop();
    }
}

/// The board's manifests, as the host's console wrote them
/// (`[host-ota] board: <build> <state>…`).
fn manifests(console: &str) -> Vec<String> {
    console
        .lines()
        .filter(|l| l.contains("[host-ota] board: "))
        .map(str::to_string)
        .collect()
}

fn tail(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(40)..].join("\n")
}

/// The scenario's one line for the record: the host's stage times and its
/// end, labelled with the configuration.
fn report(id: &str, console: &str) {
    for line in console.lines().filter(|l| {
        l.contains("[host-ota] stage")
            || l.contains("[host-ota] done")
            || l.contains("LAN link back")
    }) {
        eprintln!("emu_ota_lan {id} (lp-emu:esp32c6:t1+net=lan): {line}");
    }
}
