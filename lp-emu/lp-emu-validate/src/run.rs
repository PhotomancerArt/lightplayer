//! The runner: `list`, `replay`, `run`, `record`.
//!
//! `lp-cli validate` is the front door (vision Q1); this is the room behind it.
//! Every command is a plain function taking plain arguments and returning the
//! text it wants printed, so the CLI layer is argument parsing and nothing
//! else, and so every command is testable without a process.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::config::ValidateConfig;
use crate::configuration::{Availability, Configuration, ConfigurationKind};
use crate::driver::{RunRequest, default_out_dir, driver_for};
use crate::grade::FieldClass;
use crate::header::{InbandHeader, TranscriptHeader};
use crate::payload::{ALL_PAYLOADS, Link, Sentinel, find_payload};
use crate::replay::{ReplayOptions, ReplayReport, replay};
use crate::transcript::{Transcript, sidecar_path};

/// Where committed transcripts live, relative to the repository root.
pub const TRANSCRIPTS_DIR: &str = "lp-emu/transcripts";

/// `validate list` — payloads, sets, configurations, transcripts.
pub fn list(cfg: &ValidateConfig, repo_root: &Path) -> Result<String> {
    let mut s = String::new();

    let _ = writeln!(s, "payloads");
    for p in ALL_PAYLOADS {
        let sentinel = match p.sentinel {
            Sentinel::Done(m) => format!("done `{m}`"),
            Sentinel::Ready(m) => format!("ready `{m}`"),
            Sentinel::State(m) => format!("state `{m}`"),
        };
        let _ = writeln!(
            s,
            "  {:<24} {:<40} link={:<15} features={} {}",
            p.name,
            p.display_name,
            p.link.slug(),
            p.firmware_features.join(","),
            sentinel
        );
        if let Some(why) = p.emulator_only {
            let _ = writeln!(s, "  {:<24} emulator-only: {why}", "");
        }
    }

    let _ = writeln!(s, "\nsets");
    for set in &cfg.sets {
        let _ = writeln!(s, "  {:<24} {}", set.name, set.payloads.join(", "));
        let _ = writeln!(s, "  {:<24} {}", "", set.description);
    }

    let _ = writeln!(s, "\nconfigurations");
    for entry in &cfg.configurations {
        let parsed = entry.parsed()?;
        let _ = writeln!(
            s,
            "  {:<30} chip={:<9} {}",
            entry.name,
            entry.chip,
            parsed.availability()
        );
        let mut trust: Vec<String> = Vec::new();
        for class in FieldClass::ALL {
            if *class == FieldClass::Structural {
                continue;
            }
            trust.push(format!("{}={}", class.slug(), entry.trust.grade(*class)));
        }
        let _ = writeln!(s, "  {:<30} {}", "", trust.join(" "));
    }

    let transcripts = committed_transcripts(repo_root)?;
    let _ = writeln!(s, "\ntranscripts ({TRANSCRIPTS_DIR})");
    if transcripts.is_empty() {
        let _ = writeln!(s, "  (none)");
    }
    for path in &transcripts {
        let rel = path
            .strip_prefix(repo_root)
            .unwrap_or(path)
            .display()
            .to_string();
        match Transcript::load(path) {
            Ok(t) => {
                let _ = writeln!(
                    s,
                    "  {:<28} {:<22} {:<6} {}",
                    t.header.configuration,
                    t.header.payload,
                    format!("{} ln", t.lines.len()),
                    rel
                );
            }
            Err(e) => {
                let _ = writeln!(s, "  {rel}\n      UNREADABLE: {e:#}");
            }
        }
    }

    Ok(s)
}

/// Every `.txt` under the transcripts directory, sorted.
pub fn committed_transcripts(repo_root: &Path) -> Result<Vec<PathBuf>> {
    let root = repo_root.join(TRANSCRIPTS_DIR);
    let mut out = Vec::new();
    if !root.exists() {
        return Ok(out);
    }
    walk(&root, &mut out)?;
    out.retain(|p| p.extension().is_some_and(|e| e == "txt"));
    out.sort();
    Ok(out)
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let path = entry?.path();
        if path.is_dir() {
            walk(&path, out)?;
        } else {
            out.push(path);
        }
    }
    Ok(())
}

/// `validate replay <transcript> --against <transcript>`.
pub fn replay_files(
    left: &Path,
    right: &Path,
    options: ReplayOptions,
) -> Result<(ReplayReport, String)> {
    let l = Transcript::load(left)?;
    let r = Transcript::load(right)?;
    let report = replay(&l, &r, options)?;
    let text = report.render();
    Ok((report, text))
}

/// `validate replay <transcript> --against <configuration>`: replay against
/// the committed transcript of the same payload and chip on that configuration.
pub fn replay_against_configuration(
    left: &Path,
    configuration: &str,
    repo_root: &Path,
    options: ReplayOptions,
) -> Result<(ReplayReport, String)> {
    let l = Transcript::load(left)?;
    let want = Configuration::parse(configuration)?;
    let mut candidates = Vec::new();
    for path in committed_transcripts(repo_root)? {
        let Ok(t) = Transcript::load(&path) else {
            continue;
        };
        if t.header.payload == l.header.payload
            && t.header.chip == l.header.chip
            && t.header.configuration == want.name()
        {
            candidates.push(path);
        }
    }
    match candidates.len() {
        0 => bail!(
            "no committed transcript of payload `{}` on chip `{}` for configuration `{}`",
            l.header.payload,
            l.header.chip,
            want.name()
        ),
        1 => replay_files(left, &candidates[0], options),
        n => bail!(
            "{n} committed transcripts match configuration `{}` for payload `{}`; \
             name one explicitly:\n  {}",
            want.name(),
            l.header.payload,
            candidates
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join("\n  ")
        ),
    }
}

/// What the operator chose for one `run` or `record`.
///
/// A struct rather than five positional arguments because the two that matter
/// most are easy to swap: `port` is silicon's and `image` is the emulators',
/// and a runner that mixed them up would flash a board with somebody else's
/// ELF, or replay a transcript of an image nobody can name.
#[derive(Clone, Copy, Debug)]
pub struct RunOptions<'a> {
    /// Serial port, silicon only.
    pub port: Option<&'a str>,
    /// Already-built images, emulated configurations only. The reference
    /// images `scripts/emu/build-reference-image.sh` produces are what this is
    /// for.
    pub images: &'a ImageOverrides,
    /// Seconds to wait for the payload's sentinel. **Emulated** seconds on an
    /// emulated configuration, host seconds on silicon.
    pub timeout_secs: u64,
    /// Force the payload's effective link on an `lp-emu:*` configuration
    /// (`--link real|spike`; M1 P1, DD8). `None` is the default: the
    /// payload's own [`crate::payload::Payload::link`] decides, as it always
    /// has. Refused on any configuration that is not `lp-emu:*` — the
    /// override is `RunRequest::effective_link`'s seam, and only the
    /// `lp-emu:*` driver reads it.
    pub link_override: Option<Link>,
}

/// No `--image` at all: build what the plan says to build.
static NO_IMAGES: ImageOverrides = ImageOverrides {
    entries: Vec::new(),
};

impl Default for RunOptions<'_> {
    fn default() -> Self {
        Self {
            port: None,
            images: &NO_IMAGES,
            // The CLI's default, so a `RunOptions::default()` in a test is the
            // same run an operator would get.
            timeout_secs: 120,
            link_override: None,
        }
    }
}

/// `--image [<payload>=]<path>`, repeatable.
///
/// A set is several payloads and a reference image is built per feature set,
/// so one path cannot serve a set: `emu-m3` runs the compile harness on
/// `d6cfaa205-harness` and the shipped-image walk on
/// `d6cfaa205-boot-idle-memfs`. A bare path is the fallback for every payload
/// that has no entry of its own, which is what a one-payload set wants.
#[derive(Clone, Debug, Default)]
pub struct ImageOverrides {
    entries: Vec<(Option<String>, PathBuf)>,
}

impl ImageOverrides {
    /// Parse `<payload>=<path>` and bare `<path>` specs, refusing a payload
    /// name nobody knows — a typo there would otherwise silently build the
    /// image instead of using the pinned one, and the transcript would be of
    /// a different image than its header says.
    pub fn parse(specs: &[String]) -> Result<Self> {
        let mut entries: Vec<(Option<String>, PathBuf)> = Vec::new();
        for spec in specs {
            let entry = match spec.split_once('=') {
                Some((name, path)) => {
                    find_payload(name).with_context(|| {
                        format!("in --image `{spec}` (write `<payload>=<path>` or just a path)")
                    })?;
                    (Some(name.to_string()), PathBuf::from(path))
                }
                None => (None, PathBuf::from(spec)),
            };
            if entries.iter().any(|(n, _)| *n == entry.0) {
                bail!(
                    "--image names {} twice",
                    entry.0.as_deref().unwrap_or("the default image")
                );
            }
            entries.push(entry);
        }
        Ok(Self { entries })
    }

    pub fn for_payload(&self, payload: &str) -> Option<&Path> {
        self.entries
            .iter()
            .find(|(name, _)| name.as_deref() == Some(payload))
            .or_else(|| self.entries.iter().find(|(name, _)| name.is_none()))
            .map(|(_, path)| path.as_path())
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// `validate run <set> --config <name>` — plan, then (unless dry) execute.
pub fn run_set(
    cfg: &ValidateConfig,
    set: &str,
    configuration: &str,
    opts: &RunOptions<'_>,
    repo_root: &Path,
    dry_run: bool,
) -> Result<String> {
    let entry = cfg.configuration(configuration)?;
    let config = entry.parsed()?;
    check_link_override(opts, &config)?;
    let payloads = cfg.payloads_in(set)?;
    let driver = driver_for(&config);
    let out_dir = default_out_dir();

    let mut s = format!(
        "set `{set}` on `{}` ({})\n",
        config.name(),
        driver.availability()
    );
    if driver.availability() != Availability::Available {
        let _ = writeln!(
            s,
            "\nNothing was run. `{}` is {}.",
            config.name(),
            driver.availability()
        );
        for payload in payloads {
            let plan = driver.plan(&request(payload, entry, &config, opts, repo_root, &out_dir))?;
            s.push('\n');
            s.push_str(&plan.render());
        }
        return Ok(s);
    }

    for payload in payloads {
        let req = request(payload, entry, &config, opts, repo_root, &out_dir);
        let plan = driver.plan(&req)?;
        s.push('\n');
        s.push_str(&plan.render());
        if !dry_run {
            let dir = repo_root.join(&out_dir);
            std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
            let capture = driver.execute(&plan)?;
            let _ = writeln!(s, "  captured {}", capture.display());
        }
    }
    if dry_run {
        let _ = writeln!(s, "\n(dry run: nothing was executed)");
    }
    Ok(s)
}

/// The facts only the operator knows at record time.
///
/// The date and the commit end up in the transcript's filename, so they are
/// stated rather than sniffed: a filename derived from `git rev-parse` in the
/// runner would silently be wrong the moment the image under test is older
/// than the checkout, which is exactly what happened in the spike (the desk ran
/// `d6cfaa2051ae` while HEAD said `ec4a95aacc58`).
#[derive(Clone, Debug)]
pub struct RecordProvenance<'a> {
    pub date: &'a str,
    pub firmware_commit: &'a str,
    /// Was the image built from a dirty tree? Stated, like the commit: the
    /// reference images are a commit plus a staged cherry-pick, so `true` is
    /// the honest answer for them and the hello frame says so too.
    pub firmware_dirty: Option<bool>,
}

/// `validate record <set> --config <name>` — run, then write each capture into
/// its committed location with the header filled in.
/// Let the image have the last word on which features it was built with.
///
/// A plan asks for the payload's features — `esp32c6,test_shader_compile_incremental`
/// — and Cargo then resolves that against the package's *defaults*, so the
/// image that actually boots carries `default,esp_radio,lpfs,server,…` as well.
/// Writing the request into the sidecar therefore stated something the image
/// contradicts on its very first line, and `agrees_with_inband` refused every
/// such capture. It had never fired before G3 sitting 1 (2026-09-07) because
/// the only images anyone had recorded were the pinned reference ones, built
/// at `d6cfaa205` — before the payloads printed an in-band header at all.
///
/// So: when the payload printed a header, the sidecar takes its resolved
/// feature list, and what the plan asked for is checked as a **subset** of it.
/// That is the invariant worth having — it still catches a capture taken from
/// an image built for a different payload — while the exact list becomes a
/// fact reported by the firmware rather than a guess reconstructed by the host.
/// `firmware_commit` is deliberately *not* adopted: a disagreement there means
/// the flashed image is older than the tree, which is the operator's to resolve.
fn adopt_inband_features(mut header: TranscriptHeader, body: &str) -> Result<TranscriptHeader> {
    let Some(inband) = body
        .split('\n')
        .find_map(|line| InbandHeader::from_line(line).transpose())
        .transpose()?
    else {
        return Ok(header);
    };
    let resolved: Vec<&str> = inband
        .firmware_features
        .split(',')
        .map(str::trim)
        .filter(|f| !f.is_empty())
        .collect();
    let missing: Vec<&str> = header
        .firmware_features
        .iter()
        .map(String::as_str)
        .filter(|f| !resolved.contains(f))
        .collect();
    if !missing.is_empty() {
        bail!(
            "the image that ran is not the image the plan asked for: it reports features \
             `{}`, which is missing `{}`. Re-build and re-flash rather than recording this.",
            inband.firmware_features,
            missing.join(", ")
        );
    }
    header.firmware_features = resolved.into_iter().map(str::to_string).collect();
    Ok(header)
}

/// Where a recording lands, and which recording of its stem it is.
///
/// The committed stem is `<configuration>-<date>-<short-commit>`. It names
/// the firmware and the day, and **not** the machine that ran it or the host
/// script it ran — both of which can change under one firmware commit, and
/// both did: M5 P3 re-recorded M4's `upload-walk` (a new emulator, a new
/// pacing in the script) and had nowhere to put the new capture but where the
/// old one was (DD51). So a second recording of a stem gets an additive
/// suffix — `<stem>-r2.txt`, then `-r3`, … — beside the first, with its
/// sidecar and pin capture named the same way, and the sidecar's `note` says
/// which recording it is and why the name did not change. Nothing is ever
/// overwritten and every earlier file keeps its name, which is what keeps
/// every test that names a transcript valid.
///
/// Why a suffix, and not the emulator's commit in the name: a silicon capture
/// has no emulator and collides the same way (two sittings on one commit in
/// one day), the sidecar already carries the tool versions, the source
/// command and the image digest — the provenance a name could only
/// abbreviate — and the stem stays what `TranscriptHeader::file_stem` says,
/// so nothing that parses one has to learn a second grammar.
fn additive_destination(
    header: &mut TranscriptHeader,
    repo_root: &Path,
) -> Result<(std::path::PathBuf, Option<std::path::PathBuf>, u32)> {
    let base = repo_root
        .join(TRANSCRIPTS_DIR)
        .join(header.relative_path()?);
    let stem = header.file_stem()?;
    let has_pins = header.pins.is_some();
    let candidate = |n: u32| {
        let name = if n == 1 {
            format!("{stem}.txt")
        } else {
            format!("{stem}-r{n}.txt")
        };
        let dest = base.with_file_name(&name);
        let pins = has_pins.then(|| dest.with_file_name(format!("{name}.pins.jsonl")));
        (dest, pins)
    };
    let taken = |dest: &Path, pins: &Option<std::path::PathBuf>| {
        dest.exists() || sidecar_path(dest).exists() || pins.as_ref().is_some_and(|p| p.exists())
    };
    let mut n = 1;
    let (mut dest, mut pins) = candidate(n);
    while taken(&dest, &pins) {
        n += 1;
        (dest, pins) = candidate(n);
    }
    if let Some(p) = &pins {
        header.pins = Some(
            p.file_name()
                .expect("a file name")
                .to_string_lossy()
                .into_owned(),
        );
    }
    if n > 1 {
        let first = candidate(1).0;
        let sentence = format!(
            "recording {n} of stem `{stem}`: {} already exists and a transcript is never \
             overwritten (DD51). The stem names the firmware commit, not the machine or the \
             host script that produced this file; `tools`, `source` and `firmware_sha256` \
             here say which those were.",
            first.file_name().expect("a file name").to_string_lossy()
        );
        header.note = Some(match header.note.take() {
            Some(note) => format!("{note} {sentence}"),
            None => sentence,
        });
    }
    Ok((dest, pins, n))
}

pub fn record_set(
    cfg: &ValidateConfig,
    set: &str,
    configuration: &str,
    opts: &RunOptions<'_>,
    repo_root: &Path,
    provenance: &RecordProvenance<'_>,
    dry_run: bool,
) -> Result<String> {
    let RecordProvenance {
        date,
        firmware_commit,
        ..
    } = *provenance;
    let entry = cfg.configuration(configuration)?;
    let config = entry.parsed()?;
    check_link_override(opts, &config)?;
    let payloads = cfg.payloads_in(set)?;
    let driver = driver_for(&config);
    if driver.availability() != Availability::Available {
        bail!(
            "cannot record on `{}`: it is {}",
            config.name(),
            driver.availability()
        );
    }
    let out_dir = default_out_dir();
    let mut s = String::new();

    for payload in payloads {
        let req = request(payload, entry, &config, opts, repo_root, &out_dir);
        let plan = driver.plan(&req)?;
        let header = TranscriptHeader {
            schema: crate::header::HEADER_SCHEMA,
            payload: payload.name.to_string(),
            chip: entry.chip.clone(),
            configuration: config.name(),
            date: date.to_string(),
            firmware_commit: firmware_commit.to_string(),
            firmware_features: req.features().iter().map(|f| (*f).to_string()).collect(),
            firmware_dirty: provenance.firmware_dirty,
            firmware_sha256: None,
            silicon_rev: entry.silicon_rev.clone(),
            board: entry.board.clone(),
            mac: entry.mac.clone(),
            // What ran it. The driver fills this: silicon's tools are
            // espflash's, an emulator's are its own commit and the ROM it
            // loaded, and only the driver knows which.
            tools: plan.tools.clone(),
            source: Some(
                plan.steps
                    .iter()
                    .map(|st| st.shell())
                    .collect::<Vec<_>>()
                    .join(" && "),
            ),
            capture: Some(format!(
                "lp-cli validate record {set} --config {configuration}"
            )),
            note: if plan.notes.is_empty() {
                None
            } else {
                Some(plan.notes.join(" "))
            },
            // The companion file's name, when this configuration can observe
            // a pad and this payload makes a claim about one. A file name,
            // not a path: it is resolved against the transcript's own
            // directory (E3, the additive widening).
            pins: None,
            trust: entry.trust.clone(),
        };
        let mut header = header;
        if plan.pins.is_some() {
            header.pins = Some(header.pins_file_name()?);
        }
        header.validate()?;
        let (dest, pins_dest, recording) = additive_destination(&mut header, repo_root)?;
        let _ = writeln!(s, "{}", plan.render());
        if recording > 1 {
            let _ = writeln!(
                s,
                "  recording {recording} of this stem: the earlier files stay where they are \
                 (DD51); the sidecar's note says so"
            );
        }
        let _ = writeln!(s, "  would write {}", dest.display());
        let _ = writeln!(s, "           + {}", sidecar_path(&dest).display());
        if let Some(p) = &pins_dest {
            let _ = writeln!(s, "           + {}", p.display());
        }
        if dry_run {
            continue;
        }
        std::fs::create_dir_all(repo_root.join(&out_dir))?;
        let capture = repo_root.join(driver.execute(&plan)?);
        let body = std::fs::read_to_string(&capture)
            .with_context(|| format!("reading capture {}", capture.display()))?;
        let mut header = adopt_inband_features(header, &body)?;
        // Which BYTES ran, not only which commit (L4). Hashed after the plan
        // executed, because a plan that builds its own image has nothing to
        // hash before it does.
        header.firmware_sha256 = plan
            .image
            .as_ref()
            .and_then(|image| crate::driver::sha256_file(&plan.cwd.join(image)));
        // Parse before committing: a capture that does not parse is not a
        // transcript, and writing it would make the tree lie.
        let parsed = Transcript::from_parts(header.clone(), &body)?;
        if parsed.sentinel_line().is_none() {
            bail!(
                "capture {} never reached payload `{}`'s sentinel `{}`; not recording it",
                capture.display(),
                payload.name,
                payload.sentinel.marker()
            );
        }
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // `additive_destination` chose a name nothing holds; this is the race
        // guard for a recording that started in between, and the rule it
        // states is the one that never changes.
        for existing in [Some(&dest), pins_dest.as_ref()].into_iter().flatten() {
            if existing.exists() {
                bail!(
                    "{} already exists. Transcripts are never edited or overwritten — \
                     a new capture is a new file, and a differing one is a regression \
                     to investigate.",
                    existing.display()
                );
            }
        }
        // The pin capture, before the transcript: a transcript whose sidecar
        // names a companion that is not there is worse than no companion, so
        // the file lands first and its absence stops the recording.
        if let (Some(src), Some(dst)) = (&plan.pins, &pins_dest) {
            let src = repo_root.join(src);
            let pins = std::fs::read_to_string(&src).with_context(|| {
                format!(
                    "reading the pin capture {} — payload `{}` declares one, so a recording \
                     without it would claim a pad nobody observed",
                    src.display(),
                    payload.name
                )
            })?;
            if pins.trim().is_empty() {
                bail!(
                    "the pin capture {} is empty: payload `{}` claims a pad and this run \
                     decoded no frame on one. Not recording it.",
                    src.display(),
                    payload.name
                );
            }
            std::fs::write(dst, &pins)?;
        }
        std::fs::write(&dest, &body)?;
        std::fs::write(sidecar_path(&dest), header.to_json()?)?;
        let _ = writeln!(s, "  wrote {}", dest.display());
        if let Some(p) = &pins_dest {
            let _ = writeln!(s, "  wrote {}", p.display());
        }
    }
    if dry_run {
        let _ = writeln!(s, "(dry run: nothing was executed or written)");
    }
    Ok(s)
}

fn request(
    payload: &'static crate::payload::Payload,
    entry: &crate::config::ConfigurationEntry,
    config: &Configuration,
    opts: &RunOptions<'_>,
    repo_root: &Path,
    out_dir: &Path,
) -> RunRequest {
    RunRequest {
        payload,
        configuration: config.clone(),
        port: opts.port.map(str::to_string),
        timeout_secs: opts.timeout_secs,
        repo_root: repo_root.to_path_buf(),
        out_dir: out_dir.to_path_buf(),
        image: opts.images.for_payload(payload.name).map(Path::to_path_buf),
        link_override: opts.link_override,
        identity: entry.identity(),
    }
}

/// `--link` is `lp-emu:*` only: it is `RunRequest::effective_link`'s seam,
/// and no other driver reads `link_override`. Silicon has one link because a
/// board does; `esp-emu:*` gets `spike_uart0_link` unconditionally regardless
/// of what a payload or an override says (`RunRequest::features`'s own
/// doc). Refusing it elsewhere means a mistyped `--link real` on the wrong
/// configuration fails loudly rather than being silently ignored.
fn check_link_override(opts: &RunOptions<'_>, config: &Configuration) -> Result<()> {
    if opts.link_override.is_some() && config.kind != ConfigurationKind::LpEmu {
        bail!(
            "--link only applies to `lp-emu:*` configurations; `{}` is `{}`",
            config.name(),
            config.kind
        );
    }
    Ok(())
}

/// Resolve `find_payload` for the CLI layer without re-exporting the module.
pub fn payload_named(name: &str) -> Result<&'static crate::payload::Payload> {
    find_payload(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_names_payloads_sets_and_configurations() {
        let cfg = ValidateConfig::embedded();
        let out = list(&cfg, Path::new("/nonexistent-repo-root")).unwrap();
        assert!(out.contains("shader-compile-stress"), "{out}");
        assert!(out.contains("gpio-calibrate"), "{out}");
        assert!(out.contains("compile-parity"), "{out}");
        assert!(out.contains("silicon:esp32c6"), "{out}");
        assert!(out.contains("lp-emu:esp32c6:t1"), "{out}");
        assert!(out.contains("timing=modeled"), "{out}");
        assert!(out.contains("(none)"), "{out}");
    }

    #[test]
    fn run_on_the_lp_emu_configuration_plans_a_machine_run() {
        let cfg = ValidateConfig::embedded();
        let out = run_set(
            &cfg,
            "compile-parity",
            "lp-emu:esp32c6:t1",
            &RunOptions {
                timeout_secs: 60,
                ..RunOptions::default()
            },
            Path::new("/repo"),
            true,
        )
        .unwrap();
        // M3 P7: it is available, and the plan is the whole protocol.
        assert!(out.contains("available"), "{out}");
        assert!(out.contains("cargo run -q -p lp-emu-esp32c6"), "{out}");
        assert!(out.contains("dry run"), "{out}");
    }

    /// DD51: the stem names the firmware, and a second recording of it must
    /// land beside the first rather than on top of it.
    #[test]
    fn a_second_recording_of_a_stem_gets_an_additive_suffix() {
        let tmp = std::env::temp_dir().join(format!("lp-emu-run-stem-{}", std::process::id()));
        let dir = tmp.join(TRANSCRIPTS_DIR).join("esp32c6/rmt-chase");
        std::fs::create_dir_all(&dir).unwrap();
        let mut header = TranscriptHeader {
            schema: crate::header::HEADER_SCHEMA,
            payload: "rmt-chase".into(),
            chip: "esp32c6".into(),
            configuration: "lp-emu:esp32c6:t1".into(),
            date: "2026-09-07".into(),
            firmware_commit: "c0d62e360".into(),
            firmware_features: vec!["esp32c6".into()],
            firmware_dirty: None,
            firmware_sha256: None,
            silicon_rev: None,
            board: None,
            mac: None,
            tools: Default::default(),
            source: None,
            capture: None,
            note: Some("a plan note.".into()),
            pins: None,
            trust: Default::default(),
        };
        header.pins = Some(header.pins_file_name().unwrap());
        let stem = "lp-emu-esp32c6-t1-2026-09-07-c0d62e360";

        // Nothing there yet: the plain stem, recording 1, note untouched.
        let mut h1 = header.clone();
        let (d1, p1, n1) = additive_destination(&mut h1, &tmp).unwrap();
        assert_eq!(n1, 1);
        assert_eq!(d1, dir.join(format!("{stem}.txt")));
        assert_eq!(
            p1.as_deref(),
            Some(dir.join(format!("{stem}.txt.pins.jsonl")).as_path())
        );
        assert_eq!(h1.note.as_deref(), Some("a plan note."));

        // The first recording exists: the second is `-r2`, all three files.
        std::fs::write(&d1, "captured").unwrap();
        let mut h2 = header.clone();
        let (d2, p2, n2) = additive_destination(&mut h2, &tmp).unwrap();
        assert_eq!(n2, 2);
        assert_eq!(d2, dir.join(format!("{stem}-r2.txt")));
        assert_eq!(
            h2.pins.as_deref(),
            Some(format!("{stem}-r2.txt.pins.jsonl").as_str())
        );
        assert_eq!(
            p2.as_deref(),
            Some(dir.join(format!("{stem}-r2.txt.pins.jsonl")).as_path())
        );
        let note = h2.note.unwrap();
        assert!(
            note.starts_with("a plan note. recording 2 of stem"),
            "{note}"
        );
        assert!(
            note.contains(&format!("{stem}.txt already exists")),
            "{note}"
        );

        // A stray companion at `-r2` counts as taken too: `-r3`.
        std::fs::write(p2.unwrap(), "frames").unwrap();
        let mut h3 = header.clone();
        let (d3, _, n3) = additive_destination(&mut h3, &tmp).unwrap();
        assert_eq!(n3, 3);
        assert_eq!(d3, dir.join(format!("{stem}-r3.txt")));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn recording_the_emu_m3_set_names_both_committed_destinations() {
        let cfg = ValidateConfig::embedded();
        let images = ImageOverrides::parse(&[
            "shader-compile-stress=target/emu-ref/d6cfaa205-harness/fw-esp32c6".to_string(),
            "boot-idle=target/emu-ref/d6cfaa205-boot-idle-memfs/fw-esp32c6".to_string(),
        ])
        .unwrap();
        let out = record_set(
            &cfg,
            "emu-m3",
            "lp-emu:esp32c6:t1",
            &RunOptions {
                images: &images,
                timeout_secs: 6,
                ..RunOptions::default()
            },
            Path::new("/repo"),
            &RecordProvenance {
                date: "2026-09-06",
                firmware_commit: "d6cfaa2051ae",
                firmware_dirty: Some(true),
            },
            true,
        )
        .unwrap();
        assert!(
            out.contains(
                "/repo/lp-emu/transcripts/esp32c6/boot-idle/\
                 lp-emu-esp32c6-t1-2026-09-06-d6cfaa205.txt"
            ),
            "{out}"
        );
        assert!(
            out.contains(
                "/repo/lp-emu/transcripts/esp32c6/shader-compile-stress/\
                 lp-emu-esp32c6-t1-2026-09-06-d6cfaa205.txt"
            ),
            "{out}"
        );
        assert!(out.contains("--efuse-mac a0:f2:62:87:b4:8c"), "{out}");
    }

    #[test]
    fn dry_run_prints_the_commands_and_the_destination() {
        let cfg = ValidateConfig::embedded();
        let out = record_set(
            &cfg,
            "compile-parity",
            "esp-emu:0.42.0",
            &RunOptions {
                timeout_secs: 90,
                ..RunOptions::default()
            },
            Path::new("/repo"),
            &RecordProvenance {
                date: "2026-09-06",
                firmware_commit: "d6cfaa2051ae",
                firmware_dirty: None,
            },
            true,
        )
        .unwrap();
        assert!(out.contains("save-image"), "{out}");
        assert!(
            out.contains(
                "/repo/lp-emu/transcripts/esp32c6/shader-compile-stress/\
                 esp-emu-0.42.0-2026-09-06-d6cfaa205.txt"
            ),
            "{out}"
        );
        assert!(out.contains(".meta.json"), "{out}");
        assert!(out.contains("dry run"), "{out}");
    }

    #[test]
    fn dry_run_of_a_silicon_set_still_refuses_without_a_port() {
        let cfg = ValidateConfig::embedded();
        let err = format!(
            "{:#}",
            run_set(
                &cfg,
                "compile-parity",
                "silicon:esp32c6",
                &RunOptions {
                    timeout_secs: 90,
                    ..RunOptions::default()
                },
                Path::new("/repo"),
                true,
            )
            .unwrap_err()
        );
        assert!(err.contains("--port"), "{err}");
    }

    #[test]
    fn unknown_set_and_configuration_list_the_known_ones() {
        let cfg = ValidateConfig::embedded();
        let err = cfg.set("nope").unwrap_err().to_string();
        assert!(err.contains("compile-parity"), "{err}");
        let err = cfg.configuration("nope").unwrap_err().to_string();
        assert!(err.contains("esp-emu:0.42.0"), "{err}");
    }
}
