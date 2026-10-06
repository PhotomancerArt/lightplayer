//! [`OtaHost`]: `lpa-update`'s `UpdateDriver` on one board's link, with its
//! effects resolved the way `lp-cli` can.
//!
//! The caller owns the link (an emulated board's `WireLinkPort` in
//! `emu run --host-link`, a serial port's in `link capture`) and tells this
//! what happened on it: the link came up or went down, a channel-3 message
//! arrived, time passed. This answers with channel-3 messages to send
//! ([`OtaHost::next_outgoing`]) and console lines to print, each prefixed
//! [`OTA_LINE_PREFIX`]. The board's own words are its console; these lines
//! are the host's, one per decision, stage and end.
//!
//! Effects: the engine cache is `--ota-cache`'s directory (`<sha256>.bin`);
//! the store is offline (lp-cli has no store client yet); the read-back the
//! driver does itself. Credentials are `--ota-password`, passed to the
//! driver each time and never logged.

use std::collections::VecDeque;
use std::path::PathBuf;

use anyhow::Result;
use lpa_update::decide::{SourceEffect, SourceResult, StoreAnswer};
use lpa_update::{
    Credential, Decision, DriverConfig, DriverEffect, Finish, ServeConfig, ServeCounters, Stage,
    UpdateDriver, UpdateIntent,
};
use lpc_firmware_release::OtaManifest;
use lpc_update::{BoardManifest, BoardMessage, sha256_to_hex};

use super::ota_args::{CorruptAt, OtaArgs};
use super::ota_offer_dir::load_offer;

/// Every line this host prints starts with this.
pub const OTA_LINE_PREFIX: &str = "[host-ota]";

/// The update host on one board's link. See the module docs.
pub struct OtaHost {
    driver: UpdateDriver,
    /// The offered release, as its `ota-manifest.json` says.
    pub release: OtaManifest,
    cache: Option<PathBuf>,
    credentials: Vec<Credential>,
    corrupt: Option<CorruptAt>,
    cut_after: Option<u32>,
    /// `R`s the board sent, across links.
    pub requests: u32,
    /// Every board manifest the board sent, in order.
    pub manifests: Vec<BoardManifest>,
    /// Every refusal the board sent, as its reason letter.
    pub refusals: Vec<u8>,
    /// How the driver ended, when it has.
    pub finish: Option<Finish>,
    outbox: VecDeque<Vec<u8>>,
    lines: Vec<String>,
    stage: Option<Stage>,
    cut: bool,
}

impl OtaHost {
    /// The host `args` describes, or `None` without `--ota-offer`.
    pub fn from_args(args: &OtaArgs) -> Result<Option<Self>> {
        let Some(dir) = &args.ota_offer else {
            return Ok(None);
        };
        let (release, build) = load_offer(dir, args.ota_no_z)?;
        let config = DriverConfig {
            serve: ServeConfig {
                ahead: args.ota_ahead.unwrap_or(ServeConfig::USB.ahead).max(1),
            },
            // An offer on the command line is the press: install it. With
            // `--ota-heal-only` nothing is pressed, and an offered update
            // waits for a go that never comes.
            intent: if args.ota_heal_only {
                UpdateIntent::Auto
            } else {
                UpdateIntent::Install {
                    allow_downgrade: false,
                }
            },
            ..DriverConfig::default()
        };
        let mut host = Self {
            driver: UpdateDriver::new(build, config),
            release,
            cache: args.ota_cache.clone(),
            credentials: args
                .ota_password
                .iter()
                .map(|p| Credential::Password(p.as_bytes().to_vec()))
                .collect(),
            corrupt: args.corrupt_at()?,
            cut_after: args.ota_cut_after,
            requests: 0,
            manifests: Vec::new(),
            refusals: Vec::new(),
            finish: None,
            outbox: VecDeque::new(),
            lines: Vec::new(),
            stage: None,
            cut: false,
        };
        host.line(format!(
            "offering {} ({}): core {} B, engine {} B{}",
            host.release.build_id(),
            host.release.target,
            host.release.core.length,
            host.release.engine.length,
            if args.ota_no_z { ", raw only" } else { "" }
        ));
        Ok(Some(host))
    }

    /// A link to the board came up.
    pub fn link_up(&mut self, now_ms: u64) {
        self.driver.link_up(now_ms);
        self.pump();
    }

    /// The link went down, or the board reset.
    pub fn link_down(&mut self, now_ms: u64) {
        self.driver.link_down(now_ms);
        self.pump();
    }

    /// One channel-3 message from the board.
    pub fn on_board(&mut self, now_ms: u64, bytes: &[u8]) {
        match BoardMessage::decode(bytes) {
            Ok(BoardMessage::Manifest(json)) => {
                if let Ok(m) = BoardManifest::from_json(json) {
                    self.line(format!(
                        "board: {} {:?}{}",
                        m.build_id,
                        m.state,
                        m.transfer
                            .map(|t| format!(" ({:?} {}/{} B)", t.kind, t.done, t.total))
                            .unwrap_or_default()
                    ));
                    self.manifests.push(m);
                }
            }
            Ok(BoardMessage::Refusal(r)) => {
                let encoded = r.encode();
                self.refusals.push(encoded.get(1).copied().unwrap_or(0));
                self.line(format!("board refused: {r:?}"));
            }
            Ok(BoardMessage::Request(_)) => self.requests += 1,
            _ => {}
        }
        let creds = core::mem::take(&mut self.credentials);
        self.driver.on_board(now_ms, bytes, &creds);
        self.credentials = creds;
        self.pump();
        if self.cut_after.is_some_and(|n| self.requests >= n) {
            self.cut = true;
        }
    }

    /// Time passed (a login backoff may be over).
    pub fn tick(&mut self, now_ms: u64) {
        self.driver.tick(now_ms);
        self.pump();
    }

    /// The next channel-3 message to send, if any (left queued until
    /// [`Self::sent`]).
    pub fn next_outgoing(&self) -> Option<&[u8]> {
        self.outbox.front().map(Vec::as_slice)
    }

    /// The message [`Self::next_outgoing`] gave was queued on the link.
    pub fn sent(&mut self) {
        self.outbox.pop_front();
    }

    /// Lines for the console since the last call.
    pub fn take_lines(&mut self) -> Vec<String> {
        core::mem::take(&mut self.lines)
    }

    /// `--ota-cut-after`'s request has been answered: cut the power now.
    pub fn cut_due(&self) -> bool {
        self.cut
    }

    /// Whether the driver has ended.
    pub fn done(&self) -> bool {
        self.finish.is_some()
    }

    /// Bytes served, across links.
    pub fn served(&self) -> ServeCounters {
        self.driver.served()
    }

    /// One line for a run's report.
    pub fn summary(&self) -> String {
        let c = self.served();
        format!(
            "{} request(s); D {} chunk(s) / {} B; Z {} chunk(s) / {} B; {} duplicate(s) / {} B; \
             {} refusal(s); {}",
            self.requests,
            c.chunks_raw,
            c.bytes_raw,
            c.chunks_encoded,
            c.bytes_encoded,
            c.chunks_duplicate,
            c.bytes_duplicate,
            self.refusals.len(),
            match &self.finish {
                Some(f) => format!("ended {f:?}"),
                None => "not ended".to_string(),
            }
        )
    }

    // ---- Effects ----------------------------------------------------------

    fn pump(&mut self) {
        loop {
            let effects = self.driver.take_effects();
            if effects.is_empty() {
                return;
            }
            for effect in effects {
                self.perform(effect);
            }
        }
    }

    fn perform(&mut self, effect: DriverEffect) {
        match effect {
            DriverEffect::Send(mut bytes) => {
                self.maybe_corrupt(&mut bytes);
                self.outbox.push_back(bytes);
            }
            DriverEffect::NeedCredentials => {
                self.line("the board asks for a login".to_string());
                let creds = core::mem::take(&mut self.credentials);
                self.driver.login_with(&creds);
                self.credentials = creds;
            }
            DriverEffect::Source(effect) => self.source(effect),
            DriverEffect::Progress { stage, done, total } => {
                if self.stage != Some(stage) {
                    self.stage = Some(stage);
                    self.line(format!("stage {stage:?} at {done}/{total} B"));
                }
            }
            DriverEffect::Decided(decision) => {
                self.line(format!("decided: {}", describe(&decision)));
            }
            DriverEffect::Done(finish) => {
                self.line(format!("done: {finish:?}"));
                self.finish = Some(finish);
            }
        }
    }

    fn source(&mut self, effect: SourceEffect) {
        match effect {
            SourceEffect::LookUpCache { sha } => {
                let bytes = self
                    .cache
                    .as_ref()
                    .and_then(|dir| std::fs::read(dir.join(cache_name(&sha))).ok());
                self.line(format!(
                    "engine cache: {} {}",
                    sha256_to_hex(&sha),
                    if bytes.is_some() { "hit" } else { "miss" }
                ));
                self.driver.source_result(SourceResult::Cache(bytes));
            }
            SourceEffect::FetchFromStore { .. } => {
                self.line("store: offline (lp-cli has no store client)".to_string());
                self.driver
                    .source_result(SourceResult::Store(StoreAnswer::Offline));
            }
            // The driver reads back over its own link.
            SourceEffect::ReadBack { .. } => {}
            SourceEffect::KeepInCache { sha, bytes } => {
                let Some(dir) = &self.cache else {
                    return;
                };
                let path = dir.join(cache_name(&sha));
                let written = std::fs::create_dir_all(dir)
                    .and_then(|()| std::fs::write(&path, &bytes))
                    .is_ok();
                self.line(format!(
                    "engine cache: kept {} ({} B){}",
                    sha256_to_hex(&sha),
                    bytes.len(),
                    if written {
                        ""
                    } else {
                        " — could not write it"
                    }
                ));
            }
        }
    }

    /// `--ota-corrupt`: the first `D`/`Z` for that chunk, damaged once.
    fn maybe_corrupt(&mut self, bytes: &mut Vec<u8>) {
        let Some(at) = self.corrupt else {
            return;
        };
        let (Some(&ty), Some(&kind)) = (bytes.first(), bytes.get(1)) else {
            return;
        };
        if !matches!(ty, b'D' | b'Z') || kind != at.kind.byte() || bytes.len() < 7 {
            return;
        }
        let off = u32::from_le_bytes([bytes[2], bytes[3], bytes[4], bytes[5]]);
        if off != at.off {
            return;
        }
        if ty == b'D' {
            let mid = 6 + (bytes.len() - 6) / 2;
            bytes[mid] ^= 0x5a;
        } else {
            let keep = 6 + (bytes.len() - 6) / 2;
            bytes.truncate(keep);
        }
        self.corrupt = None;
        self.line(format!(
            "corrupted the {} chunk {:?}@{off:#x} once",
            if ty == b'D' { "raw" } else { "encoded" },
            at.kind
        ));
    }

    fn line(&mut self, text: String) {
        self.lines.push(format!("{OTA_LINE_PREFIX} {text}"));
    }
}

/// `<sha256 hex>.bin`.
fn cache_name(sha: &[u8; 32]) -> String {
    format!("{}.bin", sha256_to_hex(sha))
}

/// A decision in one short phrase (no hashes).
fn describe(decision: &Decision) -> String {
    match decision {
        Decision::Heal { build_id, .. } => format!("Heal {build_id}"),
        Decision::Reinstall { build_id, .. } => format!("Reinstall {build_id}"),
        other => format!("{other:?}"),
    }
}
