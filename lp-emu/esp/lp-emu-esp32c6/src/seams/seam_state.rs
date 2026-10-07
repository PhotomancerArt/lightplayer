//! What one machine holds about seams: the request, the current chip
//! start's resolution, the arm sites and the counters.
//!
//! Per machine, never static, so several machines in one process share
//! nothing.

use lp_emu_core::sched::Cycles;
use lp_emu_esp_common::ParticipantId;
use lp_emu_esp_common::seam::net::{Pace, SharedLan};
use lp_emu_esp_common::seam::{
    ArmSite, Engaged, PacerConfig, ScanResult, SeamEndpoint, SeamImpl, SeamRequest, SiteKind,
    WakePacer,
};

use super::seam_wake_stats::WakeStats;

/// One arm site and what the machine knows about it now.
#[derive(Clone, Debug)]
pub struct SeamSite {
    pub site: ArmSite,
    /// The flash offset it was last planted from (the live MMU's answer at
    /// that moment).
    pub paddr: Option<u32>,
    /// The patch is in the window now.
    pub armed: bool,
    /// Planted at least once this chip start (so the next plant is a re-arm).
    pub ever_armed: bool,
}

impl SeamSite {
    pub fn new(site: ArmSite) -> Self {
        Self {
            site,
            paddr: None,
            armed: false,
            ever_armed: false,
        }
    }

    pub fn is_code(&self) -> bool {
        self.site.kind == SiteKind::Code
    }
}

/// The seam state of one machine.
#[derive(Clone, Debug, Default)]
pub struct SeamState {
    /// What the run asked for. Empty is today's machine.
    pub request: SeamRequest,
    /// This chip start's scan (`None` until one ran, and always `None` on an
    /// empty request).
    pub scan: Option<ScanResult>,
    /// What engaged this chip start, against which table.
    pub engaged: Option<Engaged>,
    pub sites: Vec<SeamSite>,
    /// Calls answered (the snapshot's `seam_calls`).
    pub calls: u64,
    /// Patches planted, first arms and re-arms alike (`seam_arms`).
    pub arms_planted: u64,
    /// Of those, the ones after a fill had erased an earlier patch.
    pub rearms: u64,
    /// How many times the flash was scanned (zero on a seam-off run).
    pub scans: u64,
    /// Chip starts seen with a non-empty request.
    pub starts: u64,
    /// Waiting for the hart to first run from the flash window.
    pub waiting_for_app: bool,
    /// The cycle this chip start's app was first seen running.
    pub app_started_at: Option<Cycles>,
    /// The cycle of this chip start's first plant.
    pub first_arm_at: Option<Cycles>,
    /// The hart is parked in a seam until an interrupt wakes it.
    pub parked: bool,
    /// Answers that parked (the rest found an interrupt already pending and
    /// returned at once, exactly as `wfi` would).
    pub parks: u64,
    /// Scheduler events skipped through while parked.
    pub park_events: u64,
    /// This chip start's announcement lines (`SEAM … engaged`, or `SEAM none
    /// engaged: …`), for `--seams-info`-style reporting and the tab.
    pub lines: Vec<String>,
    /// Lines not yet handed to a host ([`crate::machine::Esp32C6Machine::take_seam_lines`]).
    pub pending_lines: Vec<String>,
    /// Why nothing engaged this chip start (a soft request), if so.
    pub none_why: Option<String>,
    /// A strict request that cannot engage, found mid-run: the run loop ends
    /// with [`crate::machine::Outcome::Seam`].
    pub strict_error: Option<String>,
    /// This machine's name on a seam medium (`<board>/<seam>`); `0` unless a
    /// host that runs several machines says otherwise.
    pub board: ParticipantId,
    /// One per engaged capability seam, in engaged order; a guest names one
    /// by its index (`test_take`'s `endpoint` argument), and its bit in the
    /// pending word is `1 << index`.
    pub endpoints: Vec<SeamEndpoint>,
    /// Per endpoint, beside it.
    pub wake_stats: Vec<WakeStats>,
    /// The live table's wake pending word, 0 when the image has no wake.
    pub pending: u32,
    /// When to raise the wake (G0 rule (b)).
    pub pacer: WakePacer,
    /// The pacing knobs new endpoints and the pacer are made with.
    pub pacer_config: PacerConfig,
    /// The virtual LAN this board's network seam answers from: the one a
    /// host gave ([`crate::machine::Esp32C6Builder::lan`]), or the empty
    /// private one made the first time `net=lan` engaged. Kept across chip
    /// starts, so a restarted board is the same board on the same LAN.
    pub lan: Option<SharedLan>,
    /// This chip start's `<board>/net` endpoint, by index, once `net=lan`
    /// engaged.
    pub net_endpoint: Option<usize>,
    /// The machine drives its LAN itself at the top of its slices (a
    /// self-driven or wall-clock LAN, not a runner's).
    pub net_pumps: bool,
    /// The run's pace (`lp_emu_esp_common::seam::net::lan_pace`), set on its
    /// LAN when the network seam engages; `None` is the unset pace. Kept
    /// across chip starts, and in the label when set.
    pub pace: Option<Pace>,
}

impl SeamState {
    pub fn new(request: SeamRequest) -> Self {
        Self {
            request,
            ..Self::default()
        }
    }

    /// Anything engaged this chip start.
    pub fn engaged(&self) -> bool {
        self.engaged.is_some()
    }

    /// The engaged implementations, in atom order.
    pub fn engaged_impls(&self) -> &[&'static SeamImpl] {
        self.engaged.as_ref().map_or(&[], |e| e.engaged.as_slice())
    }

    /// The code site claiming `pc`: armed now, or planted earlier this chip
    /// start (an `ebreak` we planted must never reach the guest as its own).
    pub fn code_site_at(&self, pc: u32) -> Option<&SeamSite> {
        self.sites
            .iter()
            .find(|s| s.is_code() && s.site.vaddr == pc && (s.armed || s.ever_armed))
    }

    /// `base` plus the engaged atoms, then the pace when one was set
    /// (`…+net=lan@pace=realtime`); exactly `base` with none engaged and no
    /// pace set.
    pub fn label(&self, base: &str) -> String {
        let mut label = match &self.engaged {
            Some(e) => e.label(base),
            None => base.to_string(),
        };
        label.push_str(&Pace::label_suffix(self.pace));
        label
    }

    /// Forget the last chip start's resolution, keeping the request and the
    /// run-long counters.
    pub(crate) fn reset_for_start(&mut self) {
        self.scan = None;
        self.engaged = None;
        self.sites.clear();
        self.waiting_for_app = false;
        self.app_started_at = None;
        self.first_arm_at = None;
        self.parked = false;
        self.lines.clear();
        self.none_why = None;
        self.endpoints.clear();
        self.net_endpoint = None;
        self.net_pumps = false;
        self.wake_stats.clear();
        self.pending = 0;
        self.pacer = WakePacer::new(self.pacer_config);
    }

    pub(crate) fn announce(&mut self, line: String) {
        self.lines.push(line.clone());
        self.pending_lines.push(line);
    }
}
