//! Drivers: how a payload actually gets run on a configuration.
//!
//! Every driver produces a **plan** first — the exact commands, in order, with
//! their environment — and only then executes it. `--dry-run` prints the plan
//! and stops, which is what makes a desk protocol reviewable before a board is
//! plugged in (G3) and what makes this file's claims checkable by a test.
//!
//! The silicon driver does not reinvent the port discipline that the spike and
//! the device-scenarios runner arrived at the hard way. It shells out to
//! `scripts/spike/esp-emu/desk-espflash-step.sh`, which runs espflash in the
//! **foreground** under `script(1)` with a `SIG_DFL` exec shim, polls the
//! capture for the payload's sentinel, sends SIGINT **to that pid only**, and
//! post-checks with `lsof`/`pgrep`. Every clause there is a sitting that broke:
//! a backgrounded espflash dies silently mid-write; a `&` child of a
//! non-interactive bash inherits `SIGINT = SIG_IGN` and can only be freed by
//! TERM/KILL, which wedges a native-USB port; and signalling espflash by
//! pattern has killed the wrong lane on a two-board desk.
//!
//! `lp-emu:*` has no driver yet. It is listed, it says which milestone will
//! bring it, and the trait below is the seam M3 implements.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::configuration::{Availability, Configuration, ConfigurationKind};
use crate::payload::{Payload, Sentinel};

/// Build constants, kept equal to the `justfile`'s variables of the same name.
pub const RV32_TARGET: &str = "riscv32imac-unknown-none-elf";
pub const FW_ESP32C6_PROFILE: &str = "release-esp32";
pub const C6_FLASH_SIZE: &str = "4mb";
pub const C6_PARTITIONS: &str = "lp-fw/fw-esp32c6/partitions.csv";
pub const FW_ESP32C6_MANIFEST: &str = "lp-fw/fw-esp32c6/Cargo.toml";
pub const DESK_STEP_SCRIPT: &str = "scripts/spike/esp-emu/desk-espflash-step.sh";

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
    /// Repository root; every path in a plan is relative to it.
    pub repo_root: PathBuf,
    /// Where captures and intermediate images go.
    pub out_dir: PathBuf,
}

impl RunRequest {
    /// The cargo feature list for this payload on this configuration.
    ///
    /// `spike_uart0_link` is added for `esp-emu:*` and only there: the
    /// emulator has no USB host, so the host link has to be UART0 (spike
    /// report §5.1). Adding it on silicon would change the image under test.
    pub fn features(&self) -> Vec<&'static str> {
        let mut f = vec!["esp32c6", self.payload.firmware_feature];
        if self.configuration.kind == ConfigurationKind::EspEmu {
            f.push("spike_uart0_link");
        }
        f
    }

    pub fn capture_path(&self) -> PathBuf {
        self.out_dir.join(format!("{}.cap", self.payload.name))
    }
}

#[derive(Clone, Debug)]
pub struct PlanStep {
    pub describe: String,
    pub command: Vec<String>,
    pub env: Vec<(String, String)>,
    /// Why this step is shaped this way, when the shape is load-bearing.
    pub note: Option<String>,
}

impl PlanStep {
    fn new(describe: impl Into<String>, command: Vec<String>) -> Self {
        Self {
            describe: describe.into(),
            command,
            env: Vec::new(),
            note: None,
        }
    }

    fn with_env(mut self, k: &str, v: impl Into<String>) -> Self {
        self.env.push((k.to_string(), v.into()));
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
        env.into_iter().chain(argv).collect::<Vec<_>>().join(" ")
    }
}

#[derive(Clone, Debug)]
pub struct RunPlan {
    pub configuration: String,
    pub payload: &'static str,
    pub availability: Availability,
    pub steps: Vec<PlanStep>,
    /// Where the transcript body will be after the plan runs.
    pub capture: PathBuf,
    pub warnings: Vec<String>,
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
        let port = req.port.as_deref().context(
            "a silicon run needs an explicit --port. The resolver is \
             `cargo run -q -p lp-cli -- fwcheck port --chip esp32c6`; this runner will \
             not pick a port for you, because a runner that grabs candidates[0] \
             eventually flashes the wrong board.",
        )?;
        let elf = format!("target/{RV32_TARGET}/{FW_ESP32C6_PROFILE}/fw-esp32c6");
        let capture = req.capture_path();
        let steps = vec![
            PlanStep::new(
                "build the payload image",
                vec![
                    "cargo".into(),
                    "build".into(),
                    "--manifest-path".into(),
                    FW_ESP32C6_MANIFEST.into(),
                    "--target".into(),
                    RV32_TARGET.into(),
                    "--profile".into(),
                    FW_ESP32C6_PROFILE.into(),
                    "--features".into(),
                    req.features().join(","),
                ],
            ),
            PlanStep::new(
                "flash and monitor in the foreground, stop at the sentinel",
                vec![
                    DESK_STEP_SCRIPT.into(),
                    capture.display().to_string(),
                    req.payload.sentinel.marker().into(),
                    req.timeout_secs.to_string(),
                    "--".into(),
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
                    "--monitor".into(),
                    elf,
                ],
            )
            .with_env("PORT_DEV", port)
            .with_note(
                "the script pre-checks lsof/pgrep, runs espflash in the foreground under \
                 script(1) with a SIG_DFL exec shim, SIGINTs that pid only, and post-checks \
                 that the port is free. Never run two of these at once.",
            ),
        ];
        Ok(RunPlan {
            configuration: req.configuration.name(),
            payload: req.payload.name,
            availability: Availability::Available,
            steps,
            capture,
            warnings: vec![
                "never open this port while Studio holds it — check `just hardware-list` \
                 and close the browser tab first"
                    .into(),
            ],
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
        if let Sentinel::Done(marker) = req.payload.sentinel {
            emu.push("--exit-on".into());
            emu.push(marker.into());
        }

        let steps = vec![
            PlanStep::new(
                "build the payload image (UART0 host link)",
                vec![
                    "cargo".into(),
                    "build".into(),
                    "--manifest-path".into(),
                    FW_ESP32C6_MANIFEST.into(),
                    "--target".into(),
                    RV32_TARGET.into(),
                    "--profile".into(),
                    FW_ESP32C6_PROFILE.into(),
                    "--features".into(),
                    req.features().join(","),
                ],
            )
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
            warnings: if std::env::var(ESP_EMU_ENV).is_err() {
                vec![format!(
                    "{ESP_EMU_ENV} is not set; the plan assumes `esp-emu` is on PATH"
                )]
            } else {
                Vec::new()
            },
        })
    }

    fn execute(&self, plan: &RunPlan) -> Result<PathBuf> {
        run_steps(plan)?;
        Ok(plan.capture.clone())
    }
}

/// Our own machine. The seam, and nothing behind it yet.
///
/// M3 replaces this with a driver over the C6 machine. What it has to provide
/// is exactly what the other two do: a plan whose steps are reproducible from
/// the command line, and an execution that leaves a capture file. Nothing in
/// the rest of this crate knows which driver produced a transcript — the
/// configuration name in the header is the only difference.
pub struct LpEmuDriver;

impl ConfigurationDriver for LpEmuDriver {
    fn kind(&self) -> ConfigurationKind {
        ConfigurationKind::LpEmu
    }

    fn availability(&self) -> Availability {
        Availability::UnavailableUntil("M3")
    }

    fn plan(&self, req: &RunRequest) -> Result<RunPlan> {
        Ok(RunPlan {
            configuration: req.configuration.name(),
            payload: req.payload.name,
            availability: self.availability(),
            steps: Vec::new(),
            capture: req.capture_path(),
            warnings: vec![
                "the lp-emu machine arrives in M3 (esp-emulator plan one). Until then this \
                 configuration exists as a name and a seam: implement ConfigurationDriver \
                 for it and nothing else in this crate changes."
                    .into(),
            ],
        })
    }
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
        cmd.args(args);
        for (k, v) in &step.env {
            cmd.env(k, v);
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

/// Where a driver's scratch output goes by default.
pub fn default_out_dir(repo_root: &Path) -> PathBuf {
    repo_root.join("target/validate")
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
            out_dir: PathBuf::from("/repo/target/validate"),
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
    fn lp_emu_is_a_seam_not_a_driver() {
        let req = request("lp-emu:esp32c6:t1", "shader-compile-stress", None);
        let plan = LpEmuDriver.plan(&req).unwrap();
        assert!(plan.steps.is_empty());
        assert_eq!(plan.availability, Availability::UnavailableUntil("M3"));
        let err = LpEmuDriver.execute(&plan).unwrap_err().to_string();
        assert!(err.contains("unavailable until M3"), "{err}");
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
