//! Drivers: how a payload actually gets run on a configuration.
//!
//! Every driver produces a **plan** first — the exact commands, in order, with
//! their environment — and only then executes it. `--dry-run` prints the plan
//! and stops, which is what makes a desk protocol reviewable before a board is
//! plugged in (G3) and what makes this file's claims checkable by a test.
//!
//! The silicon driver does not reinvent the port discipline that the spike and
//! the device-scenarios runner arrived at the hard way. It shells out to
//! `scripts/emu/desk-espflash-step.sh`, which runs espflash in the
//! **foreground** under `script(1)` with a `SIG_DFL` exec shim, polls the
//! capture for the payload's sentinel, sends SIGINT **to that pid only**, and
//! post-checks with `lsof`/`pgrep`. Every clause there is a sitting that broke:
//! a backgrounded espflash dies silently mid-write; a `&` child of a
//! non-interactive bash inherits `SIGINT = SIG_IGN` and can only be freed by
//! TERM/KILL, which wedges a native-USB port; and signalling espflash by
//! pattern has killed the wrong lane on a two-board desk.
//!
//! One payload is watched differently, and the difference is the payload
//! (`Payload::capture`, M6 P1b). `usb-negative-control` asks what the device
//! did while **nobody** was reading it, so its plan is three steps rather than
//! one: flash with no monitor and let the port go back to closed, wait, then
//! open a non-resetting reader. A monitor at the flash would answer a
//! different question, and answer it every time.
//!
//! `lp-emu:*` needs none of that discipline and has none of it: there is no
//! board, so its plan is two commands — build the image, run the machine with
//! UART0 pointed at a file — and every decision that shapes the run is a flag
//! on the second one. It landed in M3 P7 and nothing else in this crate
//! changed to accept it, which is what the seam was for.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::configuration::{Availability, Configuration, ConfigurationKind};
use crate::payload::{BootPath, Capture, Link, Payload, Sentinel};

/// Build constants, kept equal to the `justfile`'s variables of the same name.
pub const RV32_TARGET: &str = "riscv32imac-unknown-none-elf";
pub const FW_ESP32C6_PROFILE: &str = "release-esp32";
pub const C6_FLASH_SIZE: &str = "4mb";
pub const C6_PARTITIONS: &str = "lp-fw/fw-esp32c6/partitions.csv";
/// The firmware package's directory, and the only place its build may run
/// from. `lp-fw/fw-esp32c6/.cargo/config.toml` carries `-Tlinkall.x`,
/// `-Zbuild-std` and the flash-budget flags, and Cargo resolves a config from
/// the **current directory** upward — never from `--manifest-path`. Building
/// this package from the repository root therefore links without a linker
/// script and dies on every symbol in `__EXTERNAL_INTERRUPTS` (`undefined
/// symbol: GPIO`, `WIFI_MAC`, …). The `justfile` has always `cd`-ed here
/// (`build-fw-esp32c6`), and so does
/// `scripts/emu/build-reference-image.sh`; this constant is that same rule for
/// the runner's plans. Found at G3 sitting 1, 2026-09-07.
pub const FW_ESP32C6_DIR: &str = "lp-fw/fw-esp32c6";
pub const DESK_STEP_SCRIPT: &str = "scripts/emu/desk-espflash-step.sh";

/// Builds the whole 4 MiB flash part a `BootPath::RomUp` payload boots from:
/// the second-stage bootloader at `0x0`, the partition table at `0x8000`,
/// the app in `factory`. It is `espflash save-image --merge` with this
/// repository's partition table and a pinned espflash, because the
/// bootloader inside it comes out of espflash's bundled resources.
pub const MERGED_IMAGE_SCRIPT: &str = "scripts/emu/build-merged-image.sh";
/// The flash half of a [`Capture::FlashThenOpenAfter`] run: the same port
/// discipline as `DESK_STEP_SCRIPT`, and no monitor.
pub const DESK_FLASH_NO_MONITOR_SCRIPT: &str = "scripts/emu/desk-flash-no-monitor.sh";
/// The non-resetting reader (`os.open` + raw termios, `HUPCL` cleared, DTR and
/// RTS untouched) — the same open Studio and lp-cli make.
pub const TTY_CAPTURE_SCRIPT: &str = "scripts/emu/tty-capture.py";

/// The environment variable naming the `esp-emu` binary (spike report §1, §10).
pub const ESP_EMU_ENV: &str = "LP_ESP_EMU";

#[derive(Clone, Debug)]
pub struct RunRequest {
    pub payload: &'static Payload,
    pub configuration: Configuration,
    /// Serial port, silicon only. Never defaulted: a runner that picks
    /// `candidates[0]` eventually flashes the wrong board.
    pub port: Option<String>,
    pub timeout_secs: u64,
    /// Repository root. Every path a plan prints is relative to it, and it is
    /// the directory the steps run in — so a plan reads the same in a
    /// transcript's sidecar as it does on a terminal, whoever's checkout it
    /// came from.
    pub repo_root: PathBuf,
    /// Where captures and intermediate images go, relative to `repo_root`.
    pub out_dir: PathBuf,
    /// An already-built image to run instead of building one.
    ///
    /// The reason this exists is provenance, not convenience: the committed
    /// transcripts are at firmware `d6cfaa205` with `spike_uart0_link` applied
    /// as a dirty tree, which is not what a checkout builds today.
    /// `scripts/emu/build-reference-image.sh` reproduces that tree, and
    /// `--image` is how the runner is pointed at what it produced. Only the
    /// emulated configurations accept it — an image nobody can put on a board
    /// is not a silicon run.
    pub image: Option<PathBuf>,
    /// The chip identity the configuration reports, from `validate.toml`.
    ///
    /// Silicon reads its own eFuse; an emulator has to be told. Ours is told
    /// the desk board's MAC and revision so the identity fields in a hello
    /// frame compare equal to silicon's transcripts instead of differing for
    /// a reason that has nothing to do with the model.
    pub identity: Identity,
}

/// The eFuse identity an emulated configuration is given.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Identity {
    pub mac: Option<String>,
    /// As the chip reports it: `v0.2` or `0.2`.
    pub silicon_rev: Option<String>,
    pub board: Option<String>,
}

impl Identity {
    /// `v0.2` -> `0.2`: the machine's `--efuse-rev` takes wafer
    /// `major.minor`, the header records the chip's own spelling.
    pub fn efuse_rev(&self) -> Option<&str> {
        self.silicon_rev
            .as_deref()
            .map(|r| r.strip_prefix('v').unwrap_or(r))
    }
}

impl RunRequest {
    pub fn emulated(&self) -> bool {
        matches!(
            self.configuration.kind,
            ConfigurationKind::EspEmu | ConfigurationKind::LpEmu
        )
    }

    /// The cargo feature list for this payload on this configuration.
    ///
    /// `spike_uart0_link` moves the host link onto UART0, and it is added for
    /// exactly two reasons. `esp-emu:*` gets it unconditionally: that machine
    /// asserts SOF for ever and reports EP1 free for ever, so the firmware
    /// serves into the void believing a host is there (spike report §4) — its
    /// USB is a lie, and the workaround is the only honest way to hear it.
    /// `lp-emu:*` gets it only for a payload whose [`Link`] says
    /// `Uart0Spike`, which since M6 means only the payloads whose committed
    /// transcripts are of that image. Adding it on silicon would change the
    /// image under test; adding it to a USB-link payload on our own machine
    /// would defeat the comparison the milestone exists for (DD30).
    pub fn features(&self) -> Vec<&'static str> {
        let mut f = vec!["esp32c6"];
        f.extend_from_slice(self.payload.features_for(self.emulated()));
        let spike = match self.configuration.kind {
            ConfigurationKind::EspEmu => true,
            ConfigurationKind::LpEmu => self.payload.link == Link::Uart0Spike,
            ConfigurationKind::Silicon => false,
        };
        if spike {
            f.push("spike_uart0_link");
        }
        f
    }

    /// How long the run is given, in emulated seconds: the payload's own
    /// figure when its subject is a timeline, and the runner's otherwise.
    ///
    /// A scenario is a schedule — a port that opens at eight seconds, a cable
    /// out at six — and a run shorter than its own schedule records the
    /// beginning of a story. One `--timeout-secs` across a whole set cannot
    /// express that, so the payload that knows says.
    pub fn run_secs(&self) -> u64 {
        self.payload.run_secs.unwrap_or(self.timeout_secs)
    }

    pub fn capture_path(&self) -> PathBuf {
        self.out_dir.join(format!("{}.cap", self.payload.name))
    }

    /// Where the bytes the guest handed over that no host took are written —
    /// an observation beside the capture, never part of it.
    pub fn tried_path(&self) -> PathBuf {
        self.out_dir.join(format!("{}.tried", self.payload.name))
    }

    /// Where the payload's `--usb-script` is written, when it has one.
    pub fn usb_script_path(&self) -> PathBuf {
        self.out_dir
            .join(format!("{}.usbscript", self.payload.name))
    }

    /// Where the pin capture goes for a payload that has one: the decoded
    /// frames, one JSON line each, beside the console capture rather than
    /// inside it.
    pub fn pin_capture_path(&self) -> PathBuf {
        self.out_dir
            .join(format!("{}.pins.jsonl", self.payload.name))
    }
}

#[derive(Clone, Debug)]
pub struct PlanStep {
    pub describe: String,
    pub command: Vec<String>,
    pub env: Vec<(String, String)>,
    /// Run this step somewhere other than the plan's `cwd`, relative to it.
    /// Only the firmware build needs it, and it needs it absolutely: see
    /// [`FW_ESP32C6_DIR`].
    pub cwd: Option<String>,
    /// Why this step is shaped this way, when the shape is load-bearing.
    pub note: Option<String>,
    /// Send this step's stdout to a file, relative to the plan's `cwd`.
    ///
    /// Two steps need it and both are honest uses. A payload whose subject is
    /// machine state has no console output to capture, so its transcript IS
    /// the machine's `--probe` report on stdout. And a scenario's host script
    /// is content rather than a command, so it is written by a step whose
    /// whole text then lands in the sidecar's `source` — a script referenced
    /// by path alone would be a provenance hole.
    pub stdout_to: Option<String>,
}

impl PlanStep {
    fn new(describe: impl Into<String>, command: Vec<String>) -> Self {
        Self {
            describe: describe.into(),
            command,
            env: Vec::new(),
            cwd: None,
            note: None,
            stdout_to: None,
        }
    }

    fn to_file(mut self, path: &Path) -> Self {
        self.stdout_to = Some(path.display().to_string());
        self
    }

    fn with_env(mut self, k: &str, v: impl Into<String>) -> Self {
        self.env.push((k.to_string(), v.into()));
        self
    }

    fn in_dir(mut self, dir: &str) -> Self {
        self.cwd = Some(dir.to_string());
        self
    }

    fn with_note(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }

    pub fn shell(&self) -> String {
        let env = self
            .env
            .iter()
            .map(|(k, v)| format!("{k}={}", shell_quote(v)))
            .collect::<Vec<_>>();
        let argv = self.command.iter().map(|a| shell_quote(a));
        let line = env.into_iter().chain(argv).collect::<Vec<_>>().join(" ");
        // A printed plan is meant to be pasted from the repository root, so a
        // step that runs elsewhere has to say so in the line itself.
        let line = match &self.stdout_to {
            Some(path) => format!("{line} > {}", shell_quote(path)),
            None => line,
        };
        match &self.cwd {
            Some(dir) => format!("(cd {} && {line})", shell_quote(dir)),
            None => line,
        }
    }
}

#[derive(Clone, Debug)]
pub struct RunPlan {
    pub configuration: String,
    pub payload: &'static str,
    pub availability: Availability,
    pub steps: Vec<PlanStep>,
    /// Where the transcript body will be after the plan runs, relative to
    /// `cwd`.
    pub capture: PathBuf,
    /// Where the **pin capture** will be, relative to `cwd`, for a payload
    /// that declares one and a configuration that can observe a pad. `None`
    /// everywhere else, including on silicon, where a pad is only observable
    /// with an instrument nobody has attached.
    pub pins: Option<PathBuf>,
    /// The directory the steps run in: the repository root.
    pub cwd: PathBuf,
    pub warnings: Vec<String>,
    /// Facts about the plan that are not warnings: which pinned image is
    /// being run, and by what recipe. They reach the transcript's sidecar as
    /// its `note`, because a reader six months from now needs them more than
    /// the operator does today.
    pub notes: Vec<String>,
    /// Tool name -> version, for the sidecar's `tools` map.
    pub tools: BTreeMap<String, String>,
    /// The image this plan runs, relative to `cwd` — the ELF a machine loads
    /// or the merged binary a chip is flashed with. The recorder hashes it
    /// into the sidecar's `firmware_sha256`, so a transcript says which BYTES
    /// produced it and not only which commit (L4).
    pub image: Option<PathBuf>,
}

impl RunPlan {
    pub fn render(&self) -> String {
        let mut s = format!(
            "plan: payload `{}` on configuration `{}` ({})\n",
            self.payload, self.configuration, self.availability
        );
        for w in &self.warnings {
            s.push_str(&format!("  ! {w}\n"));
        }
        for n in &self.notes {
            s.push_str(&format!("  # {n}\n"));
        }
        for (i, step) in self.steps.iter().enumerate() {
            s.push_str(&format!("\n  {}. {}\n", i + 1, step.describe));
            if let Some(note) = &step.note {
                s.push_str(&format!("     # {note}\n"));
            }
            s.push_str(&format!("     $ {}\n", step.shell()));
        }
        s.push_str(&format!("\n  capture -> {}\n", self.capture.display()));
        s
    }
}

pub trait ConfigurationDriver {
    fn kind(&self) -> ConfigurationKind;
    fn availability(&self) -> Availability;

    /// The exact commands, in order. Must not touch a port or a process.
    fn plan(&self, req: &RunRequest) -> Result<RunPlan>;

    /// Run the plan. The default refuses, which is the right answer for every
    /// configuration that has no machine behind it yet.
    fn execute(&self, plan: &RunPlan) -> Result<PathBuf> {
        bail!(
            "configuration `{}` is {} — nothing to execute yet",
            plan.configuration,
            plan.availability
        )
    }
}

/// A board on a port.
pub struct SiliconDriver;

impl ConfigurationDriver for SiliconDriver {
    fn kind(&self) -> ConfigurationKind {
        ConfigurationKind::Silicon
    }

    fn availability(&self) -> Availability {
        Availability::Available
    }

    fn plan(&self, req: &RunRequest) -> Result<RunPlan> {
        if let Some(why) = req.payload.emulator_only {
            bail!(
                "payload `{}` cannot be recorded on silicon: {why}.",
                req.payload.name
            );
        }
        let port = req.port.as_deref().context(
            "a silicon run needs an explicit --port. The resolver is \
             `cargo run -q -p lp-cli -- fwcheck port --chip esp32c6`; this runner will \
             not pick a port for you, because a runner that grabs candidates[0] \
             eventually flashes the wrong board.",
        )?;
        let capture = req.capture_path();
        let mut notes: Vec<String> = Vec::new();
        // One build step, whichever way the board is then watched: the
        // negative control flashes the same image as everyone else, and it is
        // built from the firmware's own directory for the reason
        // `FW_ESP32C6_DIR` gives.
        let mut steps = Vec::new();
        let elf = match &req.image {
            Some(path) => {
                // A pinned image on silicon was refused outright until M6 P5,
                // and the reason was good: an ELF from nowhere makes the
                // sidecar's `firmware_features` a guess. What changed is that
                // the milestone's central claim needs one. DD30 arbitrates two
                // machines on the *same bytes*, and the tree a desk agent
                // happens to be standing in is not the commit the emulator
                // side is pinned to — so either both sides run the reference
                // image or neither comparison is a comparison.
                //
                // The guess is closed rather than accepted:
                // `build-reference-image.sh` names its output directory
                // `<commit>-<feature slug>`, so the features are checkable
                // against the ones this payload asks for, and the check is
                // below. Anything outside `target/emu-ref/` is still refused.
                let dir = path
                    .parent()
                    .and_then(|p| p.file_name())
                    .and_then(|n| n.to_str())
                    .unwrap_or_default()
                    .to_string();
                let under_ref = path.components().any(|c| c.as_os_str() == "emu-ref");
                let slug = reference_image_slug(&req.features());
                if !under_ref || !dir.ends_with(&slug) {
                    bail!(
                        "--image {} is not a reference image for this payload. A silicon run \
                         flashes what it built from the tree it is in; the one exception is an \
                         image `scripts/emu/build-reference-image.sh` produced, whose directory \
                         is named `<commit>-<features>` so the sidecar's `firmware_features` is \
                         checkable rather than a guess. Expected a path under `target/emu-ref/` \
                         in a directory ending `{slug}`, got `{dir}`.",
                        path.display(),
                    );
                }
                notes.push(format!(
                    "flashing the pinned reference image {} rather than building one: this \
                     payload's claim is a comparison against an emulated run of the SAME bytes \
                     (DD30), and the tree this runner stands in is not that commit. \
                     `scripts/emu/build-reference-image.sh {} <commit> none` builds it in a \
                     detached worktree; the directory name is the provenance, and the runner \
                     checks its feature half.",
                    path.display(),
                    req.features().join(","),
                ));
                path.display().to_string()
            }
            None => {
                steps.push(
                    PlanStep::new(
                        "build the payload image",
                        vec![
                            "cargo".into(),
                            "build".into(),
                            "--target".into(),
                            RV32_TARGET.into(),
                            "--profile".into(),
                            FW_ESP32C6_PROFILE.into(),
                            "--features".into(),
                            req.features().join(","),
                        ],
                    )
                    .in_dir(FW_ESP32C6_DIR),
                );
                format!("target/{RV32_TARGET}/{FW_ESP32C6_PROFILE}/fw-esp32c6")
            }
        };

        // The image this plan flashes, kept before the command line consumes
        // it: the recorder hashes it into the sidecar's `firmware_sha256`
        // (L4), which is what makes DD30's "the SAME bytes" checkable rather
        // than asserted — the directory name above says which features, this
        // says which bytes.
        let image = PathBuf::from(&elf);

        // A payload whose subject starts on a blank part has to be given one.
        // The emulated side gets it for free (`--flash` is blank unless a file
        // is named); the board keeps whatever the last sitting left on it, and
        // a leftover `startup_project` would be auto-loaded before the first
        // heartbeat — a different measurement wearing the same name.
        if req.payload.fresh_chip {
            steps.push(
                PlanStep::new(
                    "erase the flash chip",
                    vec![
                        DESK_FLASH_NO_MONITOR_SCRIPT.into(),
                        "--".into(),
                        "erase-flash".into(),
                        "--chip".into(),
                        "esp32c6".into(),
                        "--port".into(),
                        port.into(),
                    ],
                )
                .with_env("PORT_DEV", port)
                .with_note(
                    "the same foreground-under-script(1) port discipline as the write, for the \
                     same reasons, and it releases the port when it is done. This erases the \
                     filesystem AND the boot-control record, so the boot that follows is the \
                     board's first: `bootCount 1`, no `startup_project`, `[FS] Mount failed … \
                     formatting` — which is what the emulated twin's blank chip produces.",
                ),
            );
        }

        // The espflash argument list, up to but not including `--monitor`.
        let flash_args = |elf: String| -> Vec<String> {
            vec![
                "flash".into(),
                "--chip".into(),
                "esp32c6".into(),
                "--port".into(),
                port.into(),
                "--partition-table".into(),
                C6_PARTITIONS.into(),
                "--flash-size".into(),
                C6_FLASH_SIZE.into(),
                "--after".into(),
                "hard-reset".into(),
                elf,
            ]
        };

        match req.payload.capture {
            Capture::Monitor => {
                let mut command = vec![
                    DESK_STEP_SCRIPT.into(),
                    capture.display().to_string(),
                    req.payload.sentinel.marker().into(),
                    req.timeout_secs.to_string(),
                    "--".into(),
                ];
                let mut args = flash_args(elf);
                // `--monitor` goes before the ELF, where espflash wants it.
                args.insert(args.len() - 1, "--monitor".into());
                command.extend(args);
                steps.push(
                    PlanStep::new(
                        "flash and monitor in the foreground, stop at the sentinel",
                        command,
                    )
                    .with_env("PORT_DEV", port)
                    .with_note(
                        "the script pre-checks lsof/pgrep, runs espflash in the foreground under \
                     script(1) with a SIG_DFL exec shim, SIGINTs that pid only, and post-checks \
                     that the port is free. Never run two of these at once.",
                    ),
                );
            }
            Capture::FlashThenOpenAfter(open_after_secs) => {
                let mut command: Vec<String> =
                    vec![DESK_FLASH_NO_MONITOR_SCRIPT.into(), "--".into()];
                command.extend(flash_args(elf));
                steps.push(
                    PlanStep::new("flash in the foreground and RELEASE the port", command)
                        .with_env("PORT_DEV", port)
                        .with_note(
                            "no --monitor, on purpose: espflash exits the moment the write is \
                             done and the port goes back to closed. The same pre-check and \
                             post-check as the monitored step, and the post-check is what \
                             proves the wait that follows really is a wait with nobody reading.",
                        ),
                );
                steps.push(
                    PlanStep::new(
                        format!("wait {open_after_secs} s with the port closed"),
                        vec!["sleep".into(), open_after_secs.to_string()],
                    )
                    .with_note(
                        "THIS is the measurement. The board is enumerated and nothing is \
                         draining it, so its protocol writes time out, the monitor latches, \
                         and both log lines about it are dropped by that latch. What survives \
                         is the pair of timestamps in the next heartbeat.",
                    ),
                );
                let mut open: Vec<String> = vec![
                    TTY_CAPTURE_SCRIPT.into(),
                    "--dev".into(),
                    port.into(),
                    "--out".into(),
                    capture.display().to_string(),
                    "--seconds".into(),
                    req.timeout_secs.to_string(),
                ];
                if let Some(marker) = req.payload.sentinel.exit_on() {
                    open.push("--until".into());
                    open.push(marker.into());
                }
                steps.push(
                    PlanStep::new("open a non-resetting reader", open).with_note(
                        "os.open + raw termios with HUPCL cleared and DTR/RTS untouched: opening \
                     the port is the only line-state change, which is the same one Studio and \
                     lp-cli make. espflash --monitor would assert the reset dance instead and \
                     the board would boot again with a reader already attached — the very \
                     thing this payload must not do.",
                    ),
                );
            }
        }

        Ok(RunPlan {
            configuration: req.configuration.name(),
            payload: req.payload.name,
            availability: Availability::Available,
            steps,
            capture,
            pins: None,
            cwd: req.repo_root.clone(),
            warnings: vec![
                "never open this port while Studio holds it — check `just hardware-list` \
                 and close the browser tab first"
                    .into(),
            ],
            notes,
            tools: BTreeMap::new(),
            image: Some(image),
        })
    }

    fn execute(&self, plan: &RunPlan) -> Result<PathBuf> {
        ensure_no_usbmodem_port_held()?;
        run_steps(plan)?;
        Ok(plan.capture.clone())
    }
}

/// Espressif's binary emulator, the C6's second oracle.
pub struct EspEmuDriver;

impl ConfigurationDriver for EspEmuDriver {
    fn kind(&self) -> ConfigurationKind {
        ConfigurationKind::EspEmu
    }

    fn availability(&self) -> Availability {
        Availability::Available
    }

    fn plan(&self, req: &RunRequest) -> Result<RunPlan> {
        if let Some(why) = req.payload.emulator_only {
            // Emulator-only does not mean *this* emulator: reading a static
            // out of the guest is how these payloads answer, and esp-emu has
            // no such door.
            bail!(
                "payload `{}` reads state out of the guest, which this configuration cannot do \
                 ({why}).",
                req.payload.name
            );
        }
        if req.payload.host_plan.is_some() {
            bail!(
                "payload `{}` makes a claim about the USB host, and this configuration's USB \
                 model asserts SOF for ever and reports EP1 free for ever (spike report §4): it \
                 would answer every question the same way whatever the host did. \
                 `validate.toml` grades its `usb-serial-jtag` class `modeled` for that reason.",
                req.payload.name
            );
        }
        let binary = std::env::var(ESP_EMU_ENV).unwrap_or_else(|_| "esp-emu".into());
        let elf = format!("target/{RV32_TARGET}/{FW_ESP32C6_PROFILE}/fw-esp32c6");
        let image = req.out_dir.join(format!("{}.bin", req.payload.name));
        let capture = req.capture_path();

        let mut emu = vec![
            binary.clone(),
            "--chip".into(),
            "esp32c6".into(),
            "--firmware".into(),
            image.display().to_string(),
            "--timeout".into(),
            format!("{}s", req.timeout_secs),
            "--log-color".into(),
            "never".into(),
        ];
        if let Some(marker) = req.payload.sentinel.exit_on() {
            emu.push("--exit-on".into());
            emu.push(marker.into());
        }

        let steps = vec![
            PlanStep::new(
                "build the payload image (UART0 host link)",
                vec![
                    "cargo".into(),
                    "build".into(),
                    "--target".into(),
                    RV32_TARGET.into(),
                    "--profile".into(),
                    FW_ESP32C6_PROFILE.into(),
                    "--features".into(),
                    req.features().join(","),
                ],
            )
            .in_dir(FW_ESP32C6_DIR)
            .with_note(
                "spike_uart0_link is on because the emulator has no USB host: the shipped \
                 image's USB-Serial-JTAG link cannot be served there (spike report §4, §5.1)",
            ),
            PlanStep::new(
                "merge a flashable image",
                vec![
                    "espflash".into(),
                    "save-image".into(),
                    "--chip".into(),
                    "esp32c6".into(),
                    "--flash-size".into(),
                    C6_FLASH_SIZE.into(),
                    "--merge".into(),
                    "--partition-table".into(),
                    C6_PARTITIONS.into(),
                    elf,
                    image.display().to_string(),
                ],
            ),
            PlanStep::new("run the emulator, capturing stdout", emu).with_note(
                "the binary comes from $LP_ESP_EMU; install it per spike report §10 \
                 (checksum-verified release asset, outside the repo)",
            ),
        ];

        Ok(RunPlan {
            configuration: req.configuration.name(),
            payload: req.payload.name,
            availability: Availability::Available,
            steps,
            capture,
            pins: None,
            cwd: req.repo_root.clone(),
            warnings: if std::env::var(ESP_EMU_ENV).is_err() {
                vec![format!(
                    "{ESP_EMU_ENV} is not set; the plan assumes `esp-emu` is on PATH"
                )]
            } else {
                Vec::new()
            },
            notes: Vec::new(),
            tools: BTreeMap::new(),
            // The merged binary, not the ELF: it is what this emulator loads.
            image: Some(image),
        })
    }

    fn execute(&self, plan: &RunPlan) -> Result<PathBuf> {
        run_steps(plan)?;
        Ok(plan.capture.clone())
    }
}

/// Our own machine (`lp-emu/esp/lp-emu-esp32c6`), M3 onward.
///
/// Two steps and no ceremony: build the payload image (or take a pinned one),
/// then run it with UART0 pointed at a file. Everything that decides what the
/// run *is* — the time grade, the eFuse identity, the strict bus, the
/// sentinel, the emulated timeout — is on that one command line, so the plan
/// a `--dry-run` prints is the whole protocol. There is no port, no reset
/// dance and no `lsof` pre-check, because there is no board.
///
/// Nothing else in this crate knows which driver produced a transcript; the
/// configuration name in the header is the only difference.
pub struct LpEmuDriver;

/// The emulator binary's package, and the `just` front door for a human.
pub const LP_EMU_C6_PACKAGE: &str = "lp-emu-esp32c6";
/// The vendored mask ROM's checksum file, read into the sidecar's `tools`.
pub const C6_ROM_SHA256SUMS: &str = "lp-emu/esp/roms/SHA256SUMS";
pub const C6_ROM_ELF: &str = "esp32c6_rev0_rom.elf";

/// How much host time the emulated timeout is allowed to cost before the
/// wall-clock safety net fires.
///
/// The net can end a run, never change one (the machine's rule): guest time is
/// the scheduler's, and this is only here so a wedged run on a desk or in CI
/// stops instead of burning a core all night. M3 P6 measured about 3x wall for
/// emulated on this machine (5.5 s of guest time in ~15 s), so 20x is generous
/// by a factor of six and still bounded.
pub const WALL_TIMEOUT_FACTOR: u64 = 20;

impl ConfigurationDriver for LpEmuDriver {
    fn kind(&self) -> ConfigurationKind {
        ConfigurationKind::LpEmu
    }

    fn availability(&self) -> Availability {
        Availability::Available
    }

    fn plan(&self, req: &RunRequest) -> Result<RunPlan> {
        let grade = req.configuration.qualifier.as_deref().unwrap_or("t1");
        if !matches!(grade, "t1" | "t2") {
            bail!(
                "`{}`: `{grade}` is not a time grade. The machine has two — `t1` counts \
                 instructions, `t2` uses the measured per-class model — and neither is a \
                 claim about milliseconds on silicon (the vision's graded ladder).",
                req.configuration.name()
            );
        }
        if req.configuration.detail != "esp32c6" {
            bail!(
                "`{}`: the only lp-emu machine is the C6 today",
                req.configuration.name()
            );
        }

        let capture = req.capture_path();
        let built = format!("target/{RV32_TARGET}/{FW_ESP32C6_PROFILE}/fw-esp32c6");
        let mut steps = Vec::new();
        let mut notes = Vec::new();

        let usb = req.payload.link == Link::UsbSerialJtag;
        let elf = match &req.image {
            Some(path) => {
                notes.push(if usb {
                    format!(
                        "running the pinned image {} rather than building one. It is the \
                         shipped image over its own USB-Serial-JTAG link — no \
                         `spike_uart0_link`, no cherry-pick — so that it is the same bytes a \
                         silicon flash of the same commit puts on the board, which is what \
                         makes a memory comparison a comparison of two machines rather than of \
                         two link drivers (DD30). \
                         `scripts/emu/build-reference-image.sh {} <commit> none` builds it in a \
                         detached worktree at that commit.",
                        path.display(),
                        req.features().join(","),
                    )
                } else {
                    format!(
                        "running the pinned image {} rather than building one: the committed \
                         transcripts are at firmware {} with `spike_uart0_link` applied as a \
                         dirty tree, which is not what this checkout builds. \
                         `scripts/emu/build-reference-image.sh {}` reproduces that tree in a \
                         detached worktree and builds it there.",
                        path.display(),
                        REFERENCE_FIRMWARE_COMMIT,
                        req.features().join(","),
                    )
                });
                path.display().to_string()
            }
            None => {
                let mut build = PlanStep::new(
                    if usb {
                        "build the payload image (its own USB-Serial-JTAG link)"
                    } else {
                        "build the payload image (UART0 host link)"
                    },
                    vec![
                        "cargo".into(),
                        "build".into(),
                        "--target".into(),
                        RV32_TARGET.into(),
                        "--profile".into(),
                        FW_ESP32C6_PROFILE.into(),
                        "--features".into(),
                        req.features().join(","),
                    ],
                )
                .in_dir(FW_ESP32C6_DIR);
                build = build.with_note(if usb {
                    "no spike_uart0_link: this machine models the host's side of the \
                     USB-Serial-JTAG block (M6), so the shipped image runs on the link it \
                     ships with"
                } else {
                    "spike_uart0_link is on for the same reason it is on for esp-emu: this \
                     payload's committed transcripts are of that image, and a transcript is \
                     never re-baselined to suit a later idea"
                });
                steps.push(build);
                built
            }
        };

        // The ELF this plan runs, kept before the command line consumes it:
        // the recorder hashes it into the sidecar's `firmware_sha256` (L4).
        let image = PathBuf::from(&elf);
        // A payload whose subject is machine state has no console output at
        // all — with no cable the device says nothing, which is the finding —
        // so its transcript is the machine's own `--probe` report on stdout.
        let state_payload = matches!(req.payload.sentinel, Sentinel::State(_));
        let secs = req.run_secs();
        let mut emu: Vec<String> = vec![
            "cargo".into(),
            "run".into(),
            "-q".into(),
            "-p".into(),
            LP_EMU_C6_PACKAGE.into(),
            "--release".into(),
            "--".into(),
        ];
        // How the image is reached. `Direct` hands the machine the ELF;
        // `RomUp` hands it a whole merged flash part and lets the mask ROM
        // and the ESP-IDF bootloader do the loading, which needs a step
        // before this one to build that part (espflash is where the
        // second-stage bootloader comes from — there is nowhere else in the
        // repository to get that binary).
        match req.payload.boot {
            BootPath::Direct => {
                emu.push("--elf".into());
                emu.push(elf.clone());
            }
            BootPath::RomUp {
                reset_cause,
                strap,
            } => {
                let merged = image.with_file_name("merged.bin");
                steps.push(
                    PlanStep::new(
                        "build the merged flash image (the bytes a flasher writes)",
                        vec![
                            MERGED_IMAGE_SCRIPT.into(),
                            elf.clone(),
                            merged.display().to_string(),
                        ],
                    )
                    .with_note(
                        "espflash 3.3.0 exactly: it bundles the ESP-IDF second-stage \
                         bootloader, and a different espflash puts a different program in \
                         the image the ROM is about to run",
                    ),
                );
                emu.push("--merged".into());
                emu.push(merged.display().to_string());
                // Both are printed VERBATIM by the ROM's own banner
                // (`rst:0x%x` / `boot:0x%x`), so they are inputs to the
                // transcript, not decoration.
                emu.push("--reset-cause".into());
                emu.push(reset_cause.into());
                emu.push("--strap".into());
                emu.push(strap.into());
            }
        }
        emu.push("--time-grade".into());
        emu.push(grade.into());
        if usb {
            // The capture is the USB byte stream: the same bytes a reader on
            // the silicon port sees, and nothing else. What the guest handed
            // over that no host took is an observation and goes beside it.
            if !state_payload {
                emu.push("--usb-sj".into());
                emu.push(format!("file:{}", capture.display()));
            }
            emu.push("--usb-sj-tried".into());
            emu.push(format!("file:{}", req.tried_path().display()));
        } else {
            emu.push("--uart0".into());
            emu.push(format!("file:{}", capture.display()));
        }
        if let Some(plan) = &req.payload.host_plan {
            emu.push("--usb-host".into());
            emu.push(plan.host.into());
            if !plan.script.is_empty() {
                emu.push("--usb-script".into());
                emu.push(req.usb_script_path().display().to_string());
            }
        }
        if req.payload.pin_capture.is_on() {
            // The other half of the recording: what the pad carried, decoded
            // from the waveform by something that never spoke to the
            // firmware. It goes beside the console capture, never into it —
            // a transcript is the bytes a reader on the port saw, and a
            // decoded frame is not one of those.
            emu.push("--dump-frames".into());
            emu.push(format!("file:{}", req.pin_capture_path().display()));
        }
        for (symbol, ms) in req.payload.probes {
            emu.push("--probe".into());
            emu.push(format!("{symbol}@{ms}"));
        }
        emu.extend([
            // Emulated time, always: a run is the same run on a laptop and on
            // a loaded CI box.
            "--timeout".into(),
            format!("{secs}s"),
            "--wall-timeout".into(),
            (secs * WALL_TIMEOUT_FACTOR).to_string(),
            // An access nothing claims is a fault, not a zero. A transcript
            // recorded with the bus in permissive mode would be a transcript
            // of a machine quietly answering questions it cannot answer.
            "--strict-bus".into(),
        ]);
        if let Some(marker) = req.payload.sentinel.exit_on() {
            emu.push("--exit-on".into());
            emu.push(marker.into());
        }
        // The host half of a walk. Deterministic by construction: each
        // request waits for the answer to the one before it, and the wait is
        // resolved in guest cycles, so the transcript does not move with the
        // recorder's laptop. (On silicon this is the client on a port, which
        // is why the file is a *payload* field and not a machine flag.)
        //
        // Which flag carries it is the *link's* business, not the walk's: the
        // same file of `after "<line>"` steps replays on either, because the
        // needle is matched against whatever a host on that link received.
        // That is the whole of P5's answer to "should `host_script` and
        // `host_plan` be one field" — they should not. `host_script` is the
        // conversation an application had (a generated file of verbatim
        // client bytes, 12 KB of it, provenance in `walks/README.md`);
        // `host_plan` is the cable it had it over (three lines, written
        // inline so the sidecar carries the whole text). One is content
        // addressed by path, the other content itself, and merging them
        // would force a 12 KB blob into the registry or a file onto a
        // three-line schedule.
        if let Some(script) = req.payload.host_script {
            emu.push(
                if usb {
                    "--usb-script"
                } else {
                    "--uart0-script"
                }
                .into(),
            );
            emu.push(script.into());
        }
        if let Some(mac) = &req.identity.mac {
            emu.push("--efuse-mac".into());
            emu.push(mac.clone());
        }
        if let Some(rev) = req.identity.efuse_rev() {
            emu.push("--efuse-rev".into());
            emu.push(rev.to_string());
        }

        if let Some(plan) = &req.payload.host_plan
            && !plan.script.is_empty()
        {
            // Written by a step rather than behind the runner's back, so the
            // whole script text lands in the sidecar's `source`: a scenario
            // referenced only by a path is a scenario nobody can check.
            steps.push(
                PlanStep::new(
                    "write the host script (absolute emulated milliseconds)",
                    vec!["printf".into(), "%s".into(), plan.script.into()],
                )
                .to_file(&req.usb_script_path())
                .with_note(
                    "the deterministic twin of the control socket: a socket is host time and \
                     has no place in a transcript",
                ),
            );
        }

        let mut run = PlanStep::new(
            match (usb, state_payload) {
                (_, true) => "run the machine, its own report to the capture",
                (true, false) => "run the machine, the USB link to the capture",
                (false, false) => "run the machine, UART0 to the capture",
            },
            emu,
        )
        .with_note(
            "the eFuse identity comes from this configuration's entry in validate.toml, so the \
             hello frame's baseMac / chipRevision / eui64 read the same as the desk board's and \
             a replay against a silicon transcript compares chip identity rather than a \
             difference in who was told what",
        );
        if state_payload {
            run = run.to_file(&capture);
        }
        steps.push(run);

        Ok(RunPlan {
            configuration: req.configuration.name(),
            payload: req.payload.name,
            availability: Availability::Available,
            steps,
            capture,
            pins: req
                .payload
                .pin_capture
                .is_on()
                .then(|| req.pin_capture_path()),
            cwd: req.repo_root.clone(),
            warnings: Vec::new(),
            notes,
            tools: emulator_tools(&req.repo_root),
            image: Some(image),
        })
    }

    fn execute(&self, plan: &RunPlan) -> Result<PathBuf> {
        // The machine writes the capture itself; a stale one from an earlier
        // run would otherwise be appended to or, worse, left behind by a run
        // that produced nothing.
        for stale in [Some(&plan.capture), plan.pins.as_ref()]
            .into_iter()
            .flatten()
        {
            let path = plan.cwd.join(stale);
            if path.exists() {
                std::fs::remove_file(&path)
                    .with_context(|| format!("clearing {}", path.display()))?;
            }
        }
        run_steps(plan)?;
        Ok(plan.capture.clone())
    }
}

/// The directory-name slug `scripts/emu/build-reference-image.sh` gives a
/// build of these features.
///
/// Mirrored from that script's `case`, and the mirror is the point: it is what
/// lets a silicon run check that a pinned `--image` was built for the payload
/// being recorded instead of taking the operator's word for it. The two short
/// names are the M3 gate images; everything else is the feature list with
/// `,` -> `+`.
pub fn reference_image_slug(features: &[&str]) -> String {
    match features.join(",").as_str() {
        "test_shader_compile_incremental,esp32c6,spike_uart0_link" => "harness".to_string(),
        "esp32c6,server,radio,spike_uart0_link" => "boot-idle".to_string(),
        "esp32c6,server,radio,spike_uart0_link,memory_fs" => "boot-idle-memfs".to_string(),
        "esp32c6,server,radio,memory_fs" => "boot-idle-memfs-usb".to_string(),
        other => other.replace(',', "+"),
    }
}

/// The firmware commit the committed C6 transcripts and the spike report's
/// figures come from (`scripts/emu/build-reference-image.sh`).
pub const REFERENCE_FIRMWARE_COMMIT: &str = "d6cfaa2051ae";

/// What produced a `lp-emu:*` transcript, for the sidecar's `tools` map: this
/// workspace's version and commit, and the vendored ROM's checksum.
///
/// The emulator's own ELF is not identified by a hash on purpose. A build path
/// is compiled into it, so two checkouts of the same source produce different
/// bytes; the commit is what says which source ran, and the gates are what say
/// the machine still behaves the same.
fn emulator_tools(repo_root: &Path) -> BTreeMap<String, String> {
    let mut tools = BTreeMap::new();
    tools.insert(
        LP_EMU_C6_PACKAGE.to_string(),
        format!(
            "{} ({})",
            env!("CARGO_PKG_VERSION"),
            git_description(repo_root)
        ),
    );
    if let Some(sha) = rom_sha256(repo_root) {
        tools.insert("rom".to_string(), format!("{C6_ROM_ELF} sha256 {sha}"));
    }
    tools
}

/// `d6cfaa205` or `d6cfaa205+dirty`, from git. A dirty tree is not a reason to
/// refuse to record — it is a reason to say so in the header.
fn git_description(repo_root: &Path) -> String {
    let short = Command::new("git")
        .args([
            "-C",
            &repo_root.display().to_string(),
            "rev-parse",
            "--short=9",
            "HEAD",
        ])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty());
    let Some(short) = short else {
        return "unknown commit".to_string();
    };
    let dirty = Command::new("git")
        .args([
            "-C",
            &repo_root.display().to_string(),
            "status",
            "--porcelain",
        ])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .is_some_and(|o| !o.stdout.is_empty());
    if dirty {
        format!("{short}+dirty")
    } else {
        short
    }
}

/// The vendored ROM's sha256, read from the committed `SHA256SUMS` rather than
/// recomputed: that file is the checked artefact (`rom_vendoring.rs` re-derives
/// it in-process), and reading it here keeps one source of truth.
fn rom_sha256(repo_root: &Path) -> Option<String> {
    let text = std::fs::read_to_string(repo_root.join(C6_ROM_SHA256SUMS)).ok()?;
    text.lines()
        .find(|l| l.ends_with(C6_ROM_ELF))
        .and_then(|l| l.split_whitespace().next())
        .map(str::to_string)
}

/// The sha256 of a file, lower-case hex — `None` when it is not there.
///
/// Used for the sidecar's `firmware_sha256`: the image a recorded transcript
/// came from, by its bytes. Not an error when missing, because a `--dry-run`
/// records nothing and a plan that builds its image has none to hash until it
/// has run.
pub fn sha256_file(path: &Path) -> Option<String> {
    use sha2::{Digest, Sha256};
    let bytes = std::fs::read(path).ok()?;
    Some(
        Sha256::digest(&bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
    )
}

pub fn driver_for(config: &Configuration) -> Box<dyn ConfigurationDriver> {
    match config.kind {
        ConfigurationKind::Silicon => Box::new(SiliconDriver),
        ConfigurationKind::EspEmu => Box::new(EspEmuDriver),
        ConfigurationKind::LpEmu => Box::new(LpEmuDriver),
    }
}

/// The desk's first rule, in code: if anything holds a usbmodem port, stop.
pub fn ensure_no_usbmodem_port_held() -> Result<()> {
    let out = Command::new("lsof").arg("-n").output();
    let Ok(out) = out else {
        // No lsof is not a licence to proceed blindly, but it is not a reason
        // to refuse either — say so and let the desk script's own pre-check
        // (which will run next) be the gate.
        return Ok(());
    };
    let text = String::from_utf8_lossy(&out.stdout);
    let held: Vec<&str> = text
        .lines()
        .filter(|l| l.contains("/dev/cu.usbmodem") || l.contains("/dev/tty.usbmodem"))
        .collect();
    if held.is_empty() {
        Ok(())
    } else {
        bail!(
            "a usbmodem port is already held — close Studio's tab (or the other lane) first:\n{}",
            held.join("\n")
        )
    }
}

fn run_steps(plan: &RunPlan) -> Result<()> {
    for (i, step) in plan.steps.iter().enumerate() {
        let (program, args) = step
            .command
            .split_first()
            .with_context(|| format!("step {} has no command", i + 1))?;
        let mut cmd = Command::new(program);
        match &step.cwd {
            Some(dir) => cmd.current_dir(plan.cwd.join(dir)),
            None => cmd.current_dir(&plan.cwd),
        };
        cmd.args(args);
        for (k, v) in &step.env {
            cmd.env(k, v);
        }
        if let Some(path) = &step.stdout_to {
            let path = plan.cwd.join(path);
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)
                    .with_context(|| format!("creating {}", dir.display()))?;
            }
            let file = std::fs::File::create(&path)
                .with_context(|| format!("creating {}", path.display()))?;
            cmd.stdout(file);
        }
        let status = cmd
            .status()
            .with_context(|| format!("running step {}: {}", i + 1, step.shell()))?;
        if !status.success() {
            bail!("step {} failed ({status}): {}", i + 1, step.shell());
        }
    }
    Ok(())
}

fn shell_quote(s: &str) -> String {
    if !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./:=,+@".contains(c))
    {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', r"'\''"))
    }
}

/// Where a driver's scratch output goes by default, relative to the
/// repository root.
pub fn default_out_dir() -> PathBuf {
    PathBuf::from("target/validate")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::payload::find_payload;

    fn request(config: &str, payload: &str, port: Option<&str>) -> RunRequest {
        RunRequest {
            payload: find_payload(payload).unwrap(),
            configuration: Configuration::parse(config).unwrap(),
            port: port.map(str::to_string),
            timeout_secs: 120,
            repo_root: PathBuf::from("/repo"),
            out_dir: default_out_dir(),
            image: None,
            identity: Identity::default(),
        }
    }

    /// The desk board, as `validate.toml` carries it for `lp-emu:esp32c6:*`.
    fn desk_identity() -> Identity {
        Identity {
            mac: Some("a0:f2:62:87:b4:8c".into()),
            silicon_rev: Some("v0.2".into()),
            board: Some("seeed/xiao-esp32-c6".into()),
        }
    }

    #[test]
    fn silicon_refuses_without_a_port() {
        let req = request("silicon:esp32c6", "shader-compile-stress", None);
        let err = format!("{:#}", SiliconDriver.plan(&req).unwrap_err());
        assert!(err.contains("--port"), "{err}");
        assert!(err.contains("candidates[0]"), "{err}");
    }

    #[test]
    fn silicon_plan_uses_the_desk_step_script_and_the_sentinel() {
        let req = request(
            "silicon:esp32c6",
            "shader-compile-stress",
            Some("/dev/cu.usbmodem1433201"),
        );
        let plan = SiliconDriver.plan(&req).unwrap();
        let rendered = plan.render();
        assert!(rendered.contains(DESK_STEP_SCRIPT), "{rendered}");
        assert!(
            rendered.contains("'[inc-shader-compile] === DONE ==='"),
            "{rendered}"
        );
        assert!(rendered.contains("--after hard-reset"), "{rendered}");
        assert!(
            rendered.contains("PORT_DEV=/dev/cu.usbmodem1433201"),
            "{rendered}"
        );
        assert!(rendered.contains("Studio"), "{rendered}");
    }

    /// G1b-2: flash / wait / open, three steps after the build, no `--monitor`
    /// anywhere, and the open step is the non-resetting reader.
    #[test]
    fn the_negative_control_flashes_waits_then_opens_without_resetting() {
        let req = request(
            "silicon:esp32c6",
            "usb-negative-control",
            Some("/dev/cu.usbmodem1433201"),
        );
        let plan = SiliconDriver.plan(&req).unwrap();
        assert_eq!(plan.steps.len(), 4, "build, flash, wait, open");
        let rendered = plan.render();

        // The commands, not the prose that explains them — one of the notes
        // says the word `--monitor` precisely to say it is not there.
        let commands: Vec<String> = plan.steps.iter().map(|s| s.shell()).collect();
        assert!(
            !commands.iter().any(|c| c.contains("--monitor")),
            "a monitor at the flash answers a different question: {commands:?}"
        );
        assert!(
            rendered.contains(DESK_FLASH_NO_MONITOR_SCRIPT),
            "{rendered}"
        );
        assert!(rendered.contains("--after hard-reset"), "{rendered}");
        assert!(rendered.contains("sleep 8"), "{rendered}");
        assert!(rendered.contains(TTY_CAPTURE_SCRIPT), "{rendered}");
        assert!(
            rendered.contains("--dev /dev/cu.usbmodem1433201"),
            "{rendered}"
        );
        assert!(
            rendered.contains("--until '\"hostDrainingAgainMs\"'"),
            "{rendered}"
        );
        assert!(rendered.contains("--seconds 120"), "{rendered}");

        // In order, and the wait really is between the two.
        let flash = rendered.find(DESK_FLASH_NO_MONITOR_SCRIPT).unwrap();
        let wait = rendered.find("sleep 8").unwrap();
        let open = rendered.find(TTY_CAPTURE_SCRIPT).unwrap();
        assert!(flash < wait && wait < open, "{rendered}");

        // The shipped image, not the memfs variant.
        assert_eq!(req.features(), vec!["esp32c6", "server", "radio"]);
    }

    #[test]
    fn silicon_does_not_add_the_spike_uart_feature() {
        let req = request(
            "silicon:esp32c6",
            "shader-compile-stress",
            Some("/dev/cu.usbmodem1433201"),
        );
        assert_eq!(
            req.features(),
            vec!["esp32c6", "test_shader_compile_incremental"]
        );
    }

    #[test]
    fn esp_emu_adds_the_uart0_link_feature_and_exit_on() {
        let req = request("esp-emu:0.42.0", "shader-compile-stress", None);
        assert!(req.features().contains(&"spike_uart0_link"));
        let rendered = EspEmuDriver.plan(&req).unwrap().render();
        assert!(rendered.contains("--exit-on"), "{rendered}");
        assert!(rendered.contains("save-image"), "{rendered}");
        assert!(rendered.contains("--log-color never"), "{rendered}");
    }

    #[test]
    fn a_ready_payload_gets_no_exit_on() {
        let req = request("esp-emu:0.42.0", "gpio-calibrate", None);
        let rendered = EspEmuDriver.plan(&req).unwrap().render();
        assert!(!rendered.contains("--exit-on"), "{rendered}");
        assert!(rendered.contains("--timeout 120s"), "{rendered}");
    }

    #[test]
    fn lp_emu_builds_the_image_then_runs_the_machine() {
        let mut req = request("lp-emu:esp32c6:t1", "shader-compile-stress", None);
        req.identity = desk_identity();
        let plan = LpEmuDriver.plan(&req).unwrap();
        assert_eq!(plan.availability, Availability::Available);
        assert_eq!(plan.steps.len(), 2, "build, then run");
        let rendered = plan.render();
        assert!(
            rendered
                .contains("--features esp32c6,test_shader_compile_incremental,spike_uart0_link"),
            "{rendered}"
        );
        assert!(
            rendered.contains("cargo run -q -p lp-emu-esp32c6 --release"),
            "{rendered}"
        );
        assert!(rendered.contains("--time-grade t1"), "{rendered}");
        assert!(rendered.contains("--strict-bus"), "{rendered}");
        assert!(rendered.contains("--uart0 file:"), "{rendered}");
        assert!(
            rendered.contains("'[inc-shader-compile] === DONE ==='"),
            "{rendered}"
        );
        // Emulated time carries its unit; the wall clock is only a net.
        assert!(rendered.contains("--timeout 120s"), "{rendered}");
        assert!(rendered.contains("--wall-timeout 2400"), "{rendered}");
        // The identity the configuration was given, not the machine default.
        assert!(
            rendered.contains("--efuse-mac a0:f2:62:87:b4:8c"),
            "{rendered}"
        );
        assert!(rendered.contains("--efuse-rev 0.2"), "{rendered}");
    }

    /// The `boot-idle` payload is the shipped image, and since M6 it runs on
    /// the link the shipped image ships with: **no** `spike_uart0_link`, the
    /// capture off the USB byte stream, and a host attached and draining from
    /// the first byte — the state `espflash --monitor` puts a board in. That
    /// is the whole of DD30: the same bytes on both sides, or the comparison
    /// is of two link drivers.
    #[test]
    fn lp_emu_runs_a_pinned_image_for_the_shipped_image_payload() {
        let mut req = request("lp-emu:esp32c6:t2", "boot-idle", None);
        req.identity = desk_identity();
        req.image = Some(PathBuf::from(
            "target/emu-ref/735af98ae-boot-idle-memfs-usb/fw-esp32c6",
        ));
        req.timeout_secs = 6;
        assert_eq!(
            req.features(),
            vec!["esp32c6", "server", "radio", "memory_fs"],
            "the shipped image over its own link builds no spike feature"
        );
        let plan = LpEmuDriver.plan(&req).unwrap();
        assert_eq!(plan.steps.len(), 1, "a pinned image is not built here");
        let rendered = plan.render();
        assert!(rendered.contains("build-reference-image.sh"), "{rendered}");
        assert!(
            rendered.contains("--elf target/emu-ref/735af98ae-boot-idle-memfs-usb/fw-esp32c6"),
            "{rendered}"
        );
        assert!(rendered.contains("--time-grade t2"), "{rendered}");
        assert!(
            rendered.contains("'[stack] heartbeat: high-water'"),
            "{rendered}"
        );
        // The capture is the USB link; UART0 is not even opened.
        assert!(
            rendered.contains("--usb-sj file:target/validate/boot-idle.cap"),
            "{rendered}"
        );
        assert!(!rendered.contains("--uart0 "), "{rendered}");
        assert!(rendered.contains("--usb-host attached"), "{rendered}");
        // And what nobody took is kept beside the capture, never in it.
        assert!(
            rendered.contains("--usb-sj-tried file:target/validate/boot-idle.tried"),
            "{rendered}"
        );
    }

    /// A scenario is a schedule, and the plan writes it down before it runs
    /// it — so the sidecar's `source` carries the script itself rather than a
    /// path to a file nobody kept.
    #[test]
    fn a_scenario_payload_writes_its_host_script_as_a_step() {
        let mut req = request("lp-emu:esp32c6:t1", "usb-detach-reattach", None);
        req.identity = desk_identity();
        let plan = LpEmuDriver.plan(&req).unwrap();
        let rendered = plan.render();
        // build, write the script, run.
        assert_eq!(plan.steps.len(), 3, "{rendered}");
        assert!(
            rendered.contains("printf '%s' '6000  detach"),
            "the script's own text is in the plan: {rendered}"
        );
        assert!(
            rendered.contains("> target/validate/usb-detach-reattach.usbscript"),
            "{rendered}"
        );
        assert!(
            rendered.contains("--usb-script target/validate/usb-detach-reattach.usbscript"),
            "{rendered}"
        );
        // The payload's own schedule, not the runner's default.
        assert!(rendered.contains("--timeout 12s"), "{rendered}");
        assert!(rendered.contains("'\"uptime_ms\":10000'"), "{rendered}");
    }

    /// The negative control's emulator twin is a different image, and the
    /// plan says which and why in the same breath.
    #[test]
    fn the_negative_controls_emulator_twin_runs_the_memfs_variant() {
        let mut req = request("lp-emu:esp32c6:t1", "usb-negative-control", None);
        req.identity = desk_identity();
        assert_eq!(
            req.features(),
            vec!["esp32c6", "server", "radio", "memory_fs"]
        );
        let rendered = LpEmuDriver.plan(&req).unwrap().render();
        assert!(rendered.contains("--usb-host attached-idle"), "{rendered}");
        assert!(rendered.contains("printf '%s' '8000  open"), "{rendered}");
        assert!(rendered.contains("--timeout 12s"), "{rendered}");

        // On silicon it is the product's own flash-backed image, watched
        // after a wait.
        let mut sil = request("silicon:esp32c6", "usb-negative-control", None);
        sil.port = Some("/dev/cu.usbmodem1433201".into());
        assert_eq!(sil.features(), vec!["esp32c6", "server", "radio"]);
        let rendered = SiliconDriver.plan(&sil).unwrap().render();
        assert!(rendered.contains("desk-flash-no-monitor.sh"), "{rendered}");
        assert!(!rendered.contains("--usb-host"), "{rendered}");
    }

    /// A payload silicon cannot record is refused with the reason, not with
    /// an empty file.
    #[test]
    fn the_absent_host_payload_is_refused_everywhere_but_our_own_machine() {
        let mut sil = request("silicon:esp32c6", "usb-host-absent", None);
        sil.port = Some("/dev/cu.usbmodem1433201".into());
        let err = SiliconDriver.plan(&sil).unwrap_err().to_string();
        assert!(err.contains("records nothing"), "{err}");

        let esp = request("esp-emu:0.42.0", "usb-host-absent", None);
        let err = EspEmuDriver.plan(&esp).unwrap_err().to_string();
        assert!(err.contains("reads state out of the guest"), "{err}");

        // And on ours it is the machine's own report, with no `--exit-on`:
        // there is no console to match on.
        let mut req = request("lp-emu:esp32c6:t1", "usb-host-absent", None);
        req.identity = desk_identity();
        let plan = LpEmuDriver.plan(&req).unwrap();
        let rendered = plan.render();
        assert!(rendered.contains("--usb-host absent"), "{rendered}");
        assert!(!rendered.contains("--exit-on"), "{rendered}");
        assert!(!rendered.contains("--usb-sj file:"), "{rendered}");
        assert!(
            rendered.contains("> target/validate/usb-host-absent.cap"),
            "{rendered}"
        );
        assert!(
            rendered.contains(
                "--probe fw_esp32_common::serial::link_counters::NOT_DRAINING_COUNT@5000"
            ),
            "{rendered}"
        );
    }

    /// esp-emu's USB model would answer every host question the same way, so
    /// it is not asked.
    #[test]
    fn esp_emu_refuses_a_payload_that_asks_about_the_host() {
        let req = request("esp-emu:0.42.0", "usb-negative-control", None);
        let err = EspEmuDriver.plan(&req).unwrap_err().to_string();
        assert!(err.contains("asserts SOF for ever"), "{err}");
    }

    #[test]
    fn a_time_grade_that_is_not_a_rung_on_the_ladder_is_refused() {
        let req = request("lp-emu:esp32c6:t9", "boot-idle", None);
        let err = LpEmuDriver.plan(&req).unwrap_err().to_string();
        assert!(err.contains("t1"), "{err}");
        assert!(err.contains("t2"), "{err}");
    }

    #[test]
    fn a_pinned_image_is_not_a_silicon_run() {
        let mut req = request(
            "silicon:esp32c6",
            "boot-idle",
            Some("/dev/cu.usbmodem1433201"),
        );
        req.image = Some(PathBuf::from("target/emu-ref/x/fw-esp32c6"));
        let err = SiliconDriver.plan(&req).unwrap_err().to_string();
        assert!(err.contains("--image"), "{err}");
    }

    /// `v0.2` is how a chip spells it; `--efuse-rev` wants `0.2`.
    #[test]
    fn the_efuse_revision_drops_the_chips_v() {
        assert_eq!(desk_identity().efuse_rev(), Some("0.2"));
        assert_eq!(
            Identity {
                silicon_rev: Some("0.3".into()),
                ..Identity::default()
            }
            .efuse_rev(),
            Some("0.3")
        );
        assert_eq!(Identity::default().efuse_rev(), None);
    }

    #[test]
    fn shell_quoting_survives_a_sentinel_with_spaces() {
        let step = PlanStep::new("x", vec!["a".into(), "=== DONE ===".into()]);
        assert_eq!(step.shell(), "a '=== DONE ==='");
    }

    #[test]
    fn driver_for_matches_the_configuration_kind() {
        for (name, kind) in [
            ("silicon:b", ConfigurationKind::Silicon),
            ("esp-emu:0.42.0", ConfigurationKind::EspEmu),
            ("lp-emu:esp32c6:t1", ConfigurationKind::LpEmu),
        ] {
            let c = Configuration::parse(name).unwrap();
            assert_eq!(driver_for(&c).kind(), kind);
        }
    }
}
