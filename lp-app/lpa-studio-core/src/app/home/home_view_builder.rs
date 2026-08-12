//! Building [`UiHomeView`] from hydrated library inputs, the registry,
//! and embedded examples.
//!
//! The device section is the D27 roster: ONE last-seen-sorted list where
//! live and remembered devices mix (live first naturally). Every card's
//! health derives through the M2 evidence function
//! ([`derive_roster_card_state`]); the D24 collapse is gone — a connected
//! device holding a known project keeps its device card AND the project
//! card carries the live chip (D28: one fact, two views).

use std::cell::RefCell;
use std::rc::Rc;

use lpa_link::DeviceState;
use lpc_history::EventKind;
use lpfs::LpFs;

use crate::UiIssue;
use crate::app::library::{LibraryStore, PackageMeta, PackageProvenance};
use crate::app::places::{DeviceContent, DeviceRegistry, DeviceSyncState, RegisteredDevice};
use crate::app::roster::{
    ConnectEvidence, RosterCardState, RosterEvidence, derive_roster_card_state,
};

use super::card_ui_state::CardUiState;
use super::embedded_example::embedded_examples;
use super::ui_device_card::{UiDeviceCard, UiDeviceProjectChip};
use super::ui_example_card::UiExampleCard;
use super::ui_home_view::UiHomeView;
use super::ui_package_card::{UiCardConnection, UiPackageCard};

/// The gallery's hydrated library data: built asynchronously from a host
/// catalog snapshot (`StudioController::refresh_library`) and cached —
/// `view()` never reads a live store.
#[derive(Debug, Clone, Default)]
pub struct HomeInputs {
    pub projects: Vec<UiPackageCard>,
    pub devices: Vec<UiDeviceCard>,
    /// The raw registry rows [`Self::devices`] was derived from.
    ///
    /// Kept because a card is a PRESENTATION and some registry facts are not
    /// presentation: the output face's board diagram needs the lens device's
    /// `board_id`, which no card carries. Read through
    /// `StudioController::lens_board_id`.
    pub registered: Vec<RegisteredDevice>,
    /// Listing failed — the gallery surfaces this instead of an empty
    /// library.
    pub issue: Option<UiIssue>,
}

/// Everything the runtime pool contributes to the roster (runtime-pool
/// P4): one evidence bundle per DEVICE session — each feeding its live
/// card through the M2 derivation exactly as the single-session shape did
/// — plus the SIM session's evidence while that session lives.
#[derive(Clone, Debug, Default)]
pub struct HomePoolEvidence {
    /// Per-DEVICE-session evidence — one entry per attached board, in
    /// install order, up to `DEVICE_SESSION_CAPACITY` (multi-device M3;
    /// the Vec was always the shape, capacity is policy). The connect
    /// flow's transient evidence (a connect in flight before any session
    /// exists) rides an entry of its own: evidence of work, not of a
    /// session.
    pub devices: Vec<HomeDeviceEvidence>,
    /// The live SIM session's evidence — present exactly while the
    /// session lives (D36: the sim card exists only while the session
    /// does; stop-sim removes both together).
    pub sim: Option<HomeSimEvidence>,
}

/// What the live SIM session contributes to its card (D36). The session's
/// existence IS the live status — there is no link state, no connect
/// ceremony, no registry entry (the sim is not a device, D22).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HomeSimEvidence {
    /// The project loaded on the sim (uid + display name), when one is —
    /// the card's chip and the project card's "Running in simulator"
    /// pairing key.
    pub project: Option<UiDeviceProjectChip>,
    /// The ▶ tab's live frame — the SIM ENGINE'S published output, read
    /// through the same feed as a device's (G1 ruling 3: the sim card
    /// never re-simulates; it shows what the simulated board actually
    /// output).
    pub frame: Option<crate::UiControlProductPreview>,
    /// Seconds since [`Self::frame`] arrived, stamped at build time.
    pub frame_age_secs: Option<f64>,
    /// The sim engine's reported fps, when known.
    pub fps: Option<f32>,
    /// The board the sim claims to be (vision D4), inherited from the
    /// project it runs — `vendor/product`, the registry's vocabulary.
    /// `None` = no board known, the ordinary default.
    pub board_id: Option<String>,
    /// The session's console tail (D42), oldest first.
    pub console_tail: Vec<crate::UiLogEntry>,
}

/// Everything one live DEVICE session contributes to the roster — the
/// evidence feeding [`derive_roster_card_state`] for its live card.
/// Absence of every field is honest evidence of absence (no live card).
#[derive(Clone, Debug, Default)]
pub struct HomeDeviceEvidence {
    /// The live session's pool identity (`RuntimeId` rendering), stable
    /// for the session's life — the card's anonymous-key fallback (M1 of
    /// the multi-board roadmap). `None` only for evidence that carries no
    /// session (a connect in flight before any session exists).
    pub session_key: Option<String>,
    /// Connect-as-pull result, once the pull landed.
    pub sync: Option<DeviceSyncState>,
    /// The hardware link's observable state, when a session exists.
    pub link: Option<DeviceState>,
    /// What the connect flow is doing right now.
    pub connect: ConnectEvidence,
    /// Transport label from the live connector class ("USB" for serial);
    /// `None` while the provider is still resolving.
    pub transport: Option<String>,
    /// The device copy's version number on its project line, looked up at
    /// absorb time via `ProjectHistory::version_number`.
    pub observed_version: Option<usize>,
    /// The local head's version number, for the "Push vN" affordance.
    pub head_version: Option<usize>,
    /// When the local head was last saved (§3c-3 drift copy).
    pub local_saved_at: Option<f64>,
    /// When we last pushed to this device (its registry association).
    pub pushed_at: Option<f64>,
    /// The remembered device a one-click reconnect targets: while the
    /// connect window is open and no identity has landed, the live
    /// evidence renders ON that card (uid + name adopted from the
    /// registry) instead of spawning a transient anonymous twin.
    pub pending_uid: Option<String>,
    /// The session's console tail (D42), oldest first.
    pub console_tail: Vec<crate::UiLogEntry>,
    /// A card-owned op flow is mid-flight on this device (model §2, I1).
    ///
    /// Load-bearing for the card's EXISTENCE, not just its overlay: an
    /// identity-less recovery board whose session lands `Gone` mid-op would
    /// otherwise derive Offline and be dropped — which erased the card at
    /// the exact moment its op was showing "unplug the board and plug it
    /// back in" (bench, 2026-07-31). An op in flight pins the card.
    pub op_in_flight: bool,
    /// The session's latest heartbeat-reported recovery status. Carries
    /// the safe-mode output clamp the card must surface (with its exit:
    /// a power cycle).
    pub recovery: Option<lpc_wire::server::RecoveryStatus>,
    /// Chip identity from the session snapshot (SYNC probe when one ran,
    /// else the passive boot banner) — the setup form's board picker leads
    /// with boards matching it (M5).
    pub detected_chip: Option<String>,
    /// The port as the app can name it ("ESP32 Serial (0x303a:0x1001) ·
    /// port-2"): the endpoint's human label captured at connect plus the
    /// grant's short id. Everything Web Serial exposes — the OS path never
    /// reaches the page. Rendered on the card's Technical tab (gate-1
    /// sitting, 2026-08-03: an unflashed card identified nothing).
    pub port_label: Option<String>,
    /// The newest frame this session's card feed pulled off the board
    /// (honest-device preview P2) — the ▶ tab's picture. Present only while
    /// the session object holds one; it survives the link going `Gone`.
    pub frame: Option<crate::UiControlProductPreview>,
    /// How old [`Self::frame`] is, measured at view build.
    pub frame_age_secs: Option<f64>,
    /// The card identity the feed was feeding — how the last frame finds
    /// its card once the live row yields to the remembered one.
    pub frame_card_key: Option<String>,
    /// The engine fps this session's latest heartbeat reported. `None`
    /// once the link is gone — a remembered rate is not a rate.
    pub fps: Option<f32>,
}

/// Hydrate [`HomeInputs`] from a library snapshot fs. `open_elsewhere`
/// marks the projects other tabs hold open (their cards get the badge
/// treatment and refuse structural actions kindly).
pub fn hydrate_home_inputs(fs: Rc<RefCell<dyn LpFs>>, open_elsewhere: &[String]) -> HomeInputs {
    let store = LibraryStore::read_only(fs);

    let registered = DeviceRegistry::new(store.fs_handle())
        .list()
        .unwrap_or_else(|error| {
            log::warn!("home: device registry unreadable: {error}");
            Vec::new()
        });

    let mut issue = None;
    let projects: Vec<UiPackageCard> = match store.list() {
        // Every listed package gets a card. Dropping the ones whose history
        // or provenance would not load is how a project vanished from the
        // gallery with only a `log::warn!` behind it; a card missing its
        // "edited 3 days ago" line is a far smaller lie than no card at all.
        Ok(summaries) => summaries
            .into_iter()
            .map(|summary| {
                package_card(&store, &registered, summary.clone()).unwrap_or_else(|error| {
                    log::warn!("home: {} listed without its history: {error}", summary.slug);
                    degraded_package_card(summary)
                })
            })
            .map(|mut card| {
                card.open_elsewhere = open_elsewhere.iter().any(|uid| *uid == card.uid);
                card
            })
            .collect(),
        Err(error) => {
            issue = Some(UiIssue::new(format!(
                "Your projects could not be listed: {error}"
            )));
            Vec::new()
        }
    };

    let devices = registered
        .iter()
        .map(|device| device_card(device, &projects))
        .collect();

    HomeInputs {
        projects,
        devices,
        registered,
        issue,
    }
}

/// Assemble the gallery view model from cached inputs. `inputs` is `None`
/// when no local store mounted (the gallery still shows examples and the
/// connect card). `pool` is the runtime pool's per-session evidence; the
/// D28 pairing happens here: a live device holding a known project keeps
/// its device card AND that project's card gets the [`UiCardConnection`]
/// chip, and the sim session's loaded project stamps its card's
/// "Running in simulator" indication (the sim arm).
pub fn build_home_view(
    inputs: Option<&HomeInputs>,
    opening: Option<String>,
    issue: Option<UiIssue>,
    pool: &HomePoolEvidence,
) -> UiHomeView {
    let examples = dedupe_by_key(
        embedded_examples()
            .iter()
            .map(|example| UiExampleCard {
                id: example.id.to_string(),
                name: example.name.to_string(),
                kind: example.kind.to_string(),
            })
            .collect(),
        |card| card.id.clone(),
        "example",
    );

    let Some(inputs) = inputs else {
        let (_, devices) = assemble_roster(&[], pool);
        return UiHomeView {
            devices: dedupe_by_key(devices, |card| card.render_key().to_string(), "device"),
            projects: Vec::new(),
            examples,
            library_available: false,
            opening,
            issue,
            backup: None,
            setup: None,
        };
    };

    let mut projects = inputs.projects.clone();
    let (live_connections, devices) = assemble_roster(&inputs.devices, pool);
    for (project_uid, connection) in live_connections {
        if let Some(card) = projects.iter_mut().find(|card| card.uid == project_uid) {
            card.connected_device = Some(connection);
        }
    }
    // The sim arm of the D28 pairing: the loaded project's card wears the
    // "Running in simulator" indication alongside (independent of) any
    // device connection.
    if let Some(chip) = pool.sim.as_ref().and_then(|sim| sim.project.as_ref())
        && let Some(card) = projects.iter_mut().find(|card| card.uid == chip.uid)
    {
        card.running_in_sim = true;
    }

    UiHomeView {
        devices: dedupe_by_key(devices, |card| card.render_key().to_string(), "device"),
        projects: dedupe_by_key(projects, |card| card.uid.clone(), "project"),
        examples,
        library_available: true,
        opening,
        issue: issue.or_else(|| inputs.issue.clone()),
        // The controller overlays a finished backup after the build (it is
        // controller state, not library/roster evidence).
        backup: None,
        setup: None,
    }
}

/// Drop cards whose render key repeats (keeping the first), warning loudly.
/// Keyed lists with duplicate keys PANIC the renderer and kill the whole
/// app (2026-07-15 home-gallery crash) — a corrupt registry or store must
/// degrade to a missing card, never to a dead UI.
fn dedupe_by_key<T>(cards: Vec<T>, key: impl Fn(&T) -> String, what: &'static str) -> Vec<T> {
    let mut seen = std::collections::HashSet::new();
    cards
        .into_iter()
        .filter(|card| {
            let card_key = key(card);
            let fresh = seen.insert(card_key.clone());
            if !fresh {
                log::warn!("home: dropping {what} card with duplicate key {card_key:?}");
            }
            fresh
        })
        .collect()
}

/// The D27 roster shape, fed from the pool (P4): the sim card pinned
/// first among live (the sim is always "now" — pinning keeps ties
/// stable), then live device cards in session order, then remembered
/// cards sorted by last seen (newest first). A live device stops being
/// "remembered offline" while it's here; each live project pairing (when
/// the contents are a known/adopted library project) returns alongside so
/// the project cards can carry the D28 chips.
///
/// Returns `(live project connections, device cards)`.
fn assemble_roster(
    registry_cards: &[UiDeviceCard],
    pool: &HomePoolEvidence,
) -> (Vec<(String, UiCardConnection)>, Vec<UiDeviceCard>) {
    let mut live_cards: Vec<UiDeviceCard> = Vec::new();
    let mut connections: Vec<(String, UiCardConnection)> = Vec::new();
    for live in &pool.devices {
        let mut live_card = live_device_card(live);
        // Connect-window attribution: the user clicked a SPECIFIC
        // remembered card; until the identity read lands, the live
        // evidence belongs to that card
        // (docs/defects/2026-07-23-reconnect-transient-twin-card).
        if let (Some(card), Some(pending)) = (&mut live_card, &live.pending_uid)
            && card.uid.is_none()
            && let Some(remembered) = registry_cards
                .iter()
                .find(|entry| entry.uid.as_deref() == Some(pending.as_str()))
        {
            card.uid = Some(pending.clone());
            card.name = remembered.name.clone();
            if card.transport.is_empty() {
                card.transport = remembered.transport.clone();
            }
        }
        let Some(card) = live_card else {
            continue;
        };
        if let Some(chip) = card.project.as_ref() {
            let relation = match live.sync.as_ref().map(|sync| &sync.content) {
                Some(DeviceContent::Known { relation, .. }) => Some(*relation),
                Some(DeviceContent::Adopted { .. }) => Some(lpc_history::SyncRelation::AtHead),
                _ => None,
            };
            if let Some(relation) = relation {
                connections.push((
                    chip.uid.clone(),
                    UiCardConnection {
                        device_key: card.identity_key().to_string(),
                        device_name: card.name.clone(),
                        relation,
                    },
                ));
            }
        }
        live_cards.push(card);
    }

    let mut devices: Vec<UiDeviceCard> = registry_cards
        .iter()
        .filter(|card| match &card.uid {
            // a live device is not "remembered offline" while it's here
            Some(uid) => !live_cards
                .iter()
                .any(|live| live.uid.as_deref() == Some(uid.as_str())),
            None => true,
        })
        .cloned()
        .collect();
    // Q4: an unplugged board's card keeps the last frame it published this
    // session. The live row yielded to the better-informed registry card
    // just above, so the frame has to cross over with it — otherwise the ▶
    // tab blanks at exactly the moment "what was it doing?" matters. Only
    // registry cards are in `devices` at this point, so nothing live is
    // overwritten.
    for live in &pool.devices {
        let (Some(frame), Some(key)) = (live.frame.as_ref(), live.frame_card_key.as_deref()) else {
            continue;
        };
        if let Some(card) = devices.iter_mut().find(|card| card.identity_key() == key) {
            card.frame_preview = Some(frame.clone());
            card.frame_age_secs = live.frame_age_secs;
        }
    }
    // last-seen sort (stable: hydration order breaks ties); live leads
    devices.sort_by(|a, b| {
        last_seen_sort_key(b)
            .partial_cmp(&last_seen_sort_key(a))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    for card in live_cards.into_iter().rev() {
        devices.insert(0, card);
    }
    if let Some(sim) = &pool.sim {
        devices.insert(0, sim_card(sim));
    }
    (connections, devices)
}

/// The chrome session strip (vision D15/D16): every LIVE runtime
/// session, in roster order (sim pinned first). Registry rows without a
/// live session never appear (D36: chip existence = session existence).
///
/// Reuses [`assemble_roster`]'s card derivation — the strip and the
/// gallery must never disagree on a session's status — then projects
/// each live card down to wayfinding facts (D43: name and status only,
/// no controls, no thumbnails).
pub fn chrome_sessions(
    registry_cards: &[UiDeviceCard],
    pool: &HomePoolEvidence,
    lens: Option<&crate::UiLensRuntime>,
) -> Vec<crate::UiChromeSession> {
    let (_, devices) = assemble_roster(registry_cards, pool);
    devices
        .into_iter()
        // Live = the sim (its card exists only while the session does)
        // or a device with pool-session evidence; registry-only rows
        // have no session_key.
        .filter(|card| card.sim || card.session_key.is_some())
        .map(|card| {
            let target = if card.sim {
                crate::UiChromeSessionTarget::Sim {
                    project_key: card.project.as_ref().map(|chip| chip.uid.clone()),
                }
            } else {
                crate::UiChromeSessionTarget::Device {
                    uid: card.uid.clone(),
                }
            };
            let lensed = match (lens, card.sim) {
                (Some(crate::UiLensRuntime::Sim { .. }), true) => true,
                (Some(crate::UiLensRuntime::Device { uid }), false) => {
                    // An unidentified lens device matches the (single)
                    // live card that has no uid yet — same no-honest-
                    // address window as the URL rule.
                    match uid {
                        Some(uid) => card.uid.as_deref() == Some(uid.as_str()),
                        None => card.uid.is_none(),
                    }
                }
                _ => false,
            };
            crate::UiChromeSession {
                key: card.identity_key().to_string(),
                name: card.name.clone(),
                sim: card.sim,
                transport: if card.sim {
                    String::new()
                } else {
                    card.transport.clone()
                },
                status: chip_status(&card.state),
                lensed,
                target,
            }
        })
        .collect()
}

/// Collapse the roster's honest vocabulary to the strip's three dots
/// (D16). Offline never reaches here (live-filtered above); everything
/// that is neither running-clean nor connected-empty reads as attention
/// — the chip only flags it, the card tells the story.
fn chip_status(state: &RosterCardState) -> crate::UiChromeSessionStatus {
    match state {
        RosterCardState::RunningUpToDate => crate::UiChromeSessionStatus::Run,
        RosterCardState::ConnectedEmpty | RosterCardState::ReadyToSetUp => {
            crate::UiChromeSessionStatus::Empty
        }
        _ => crate::UiChromeSessionStatus::Attention,
    }
}

/// Give the card an open setup flow is bound to the roster's LEADING
/// position and report its [`UiDeviceCard::identity_key`] — the key the
/// wizard's body takeover rides (G2 ruling, 2026-08-05: the wizard is a
/// state of the device card, not a card of its own).
///
/// `session_key` is the flow's binding: the bound session's `RuntimeId`
/// rendering on the hardware path, the sim's reserved card key on the sim
/// path. It is matched against `session_key` FIRST because that is the
/// thread that does not move — a board's `identity_key` becomes its uid
/// the instant identity lands mid-flow, and a takeover keyed by the uid
/// would lose its card at exactly that moment. The sim card carries no
/// session key, so its reserved `identity_key` is the second rung.
///
/// `None` (no flow, or a flow with nothing attached yet) leaves the roster
/// exactly as assembled: the wizard renders standalone in the entry-cards
/// slot, because there is no card to be the body of.
pub(crate) fn pin_setup_card(
    devices: &mut Vec<UiDeviceCard>,
    session_key: Option<&str>,
) -> Option<String> {
    let session_key = session_key?;
    let index = devices.iter().position(|card| {
        card.session_key.as_deref() == Some(session_key) || card.identity_key() == session_key
    })?;
    let card = devices.remove(index);
    let key = card.identity_key().to_string();
    // Ahead of everything, including the sim's own pin: a card mid-setup
    // is the one the user is looking at, and a leading slot is what keeps
    // it from hopping columns as facts (and other cards) land.
    devices.insert(0, card);
    Some(key)
}

/// The live sim card (D36): the shared card grammar in the sim
/// presentation. The session's existence is the status — Running when a
/// project is loaded, "Connected — nothing loaded" otherwise; no uid, no
/// transport, no firmware provenance (the sim is not a device, D22).
pub(crate) fn sim_card(sim: &HomeSimEvidence) -> UiDeviceCard {
    let state = if sim.project.is_some() {
        RosterCardState::RunningUpToDate
    } else {
        RosterCardState::ConnectedEmpty
    };
    UiDeviceCard {
        frame_preview: sim.frame.clone(),
        frame_age_secs: sim.frame_age_secs,
        frame_fps: sim.fps,
        port_label: None,
        session_key: None,
        uid: None,
        name: "Simulator".to_string(),
        transport: String::new(),
        state,
        project: sim.project.clone(),
        safe_clamp: None,
        fw: None,
        hardware: None,
        detected_chip: None,
        // D4: the sim's inherited board — the one card that carries this
        board_id: sim.board_id.clone(),
        sim: true,
        console_tail: sim.console_tail.clone(),
        ui: CardUiState::default(),
    }
}

/// The live session's card for the GALLERY roster. `None` when there is
/// no live evidence at all, and also when the evidence derives *Offline*
/// (a `Gone` link or stale sync) — there the registry card is the
/// better-informed offline view (it knows the last sighting), so the live
/// row yields to it rather than showing a duplicate.
pub(crate) fn live_device_card(live: &HomeDeviceEvidence) -> Option<UiDeviceCard> {
    if live.sync.is_none() && live.link.is_none() && live.connect == ConnectEvidence::Idle {
        return None;
    }
    let card = device_card_from_live_evidence(live);
    if matches!(card.state, RosterCardState::Offline { .. }) && !live.op_in_flight {
        // EXCEPT while a card-owned op flow is narrating (awaiting the
        // replug that finishes a recovery write): an anonymous session has
        // no registry card to yield to, so dropping the live row would take
        // the instruction with it.
        return None;
    }
    Some(card)
}

/// The same derivation WITHOUT the roster's Offline filter, and total
/// over every evidence combination (empty evidence derives Offline).
///
/// The editor's LENS card (D43) is the shell's only device surface, so it
/// must survive an unplug: a `Gone` link has to leave a faded
/// "Seen …/Reconnect" card standing, not a hole. Filtering Offline here
/// is what let the retired step-stack pane back on screen — defect
/// `docs/defects/2026-07-28-retired-device-pane-still-reachable.md`.
/// There is no registry entry to fall back on at this scale, so the
/// caller fills the sighting (see `StudioController::lens_device_card`).
pub(crate) fn device_card_from_live_evidence(live: &HomeDeviceEvidence) -> UiDeviceCard {
    let state = derive_roster_card_state(&RosterEvidence {
        link: live.link.as_ref(),
        content: live.sync.as_ref().map(|sync| &sync.content),
        observed_version: live.observed_version,
        head_version: live.head_version,
        local_saved_at: live.local_saved_at,
        pushed_at: live.pushed_at,
        // "Needs a name" is about the NAME, not the uid. A MAC-identified
        // board has a uid from its first hello (device identity design
        // §3) while still being a board nobody has named, so the gate
        // reads the name: identity absent, or present with nothing in it.
        unnamed: live.sync.as_ref().is_some_and(|sync| {
            sync.identity
                .as_ref()
                .is_none_or(|identity| identity.name.is_empty())
        }),
        registry: None,
        connect: live.connect.clone(),
    });
    let identity = live.sync.as_ref().and_then(|sync| sync.identity.as_ref());
    let project = live.sync.as_ref().and_then(|sync| match &sync.content {
        DeviceContent::Known {
            project_uid, slug, ..
        }
        | DeviceContent::Adopted {
            project_uid, slug, ..
        } => Some(UiDeviceProjectChip {
            uid: project_uid.clone(),
            name: slug.clone(),
        }),
        // No chip for the rest — including an OLD-FORMAT copy (P5), whose
        // library package is known but is NOT what the board is running:
        // the firmware refused to load it. The card's Health section says
        // what is there instead.
        _ => None,
    });
    // hello build + hardware facts: Technical evidence for the card's
    // rich-object detail (Ready links only — a pre-hello link has none).
    // Both halves ride the same hello, so they arrive and vanish together.
    let (fw, hardware) = match &live.link {
        Some(DeviceState::Ready { hello }) => {
            (Some(hello.build.clone()), Some(hello.hardware.clone()))
        }
        _ => (None, None),
    };
    UiDeviceCard {
        // The ▶ tab's picture: what the BOARD published, kept across an
        // unplug so an offline card shows its last frame (Q4).
        frame_preview: live.frame.clone(),
        frame_age_secs: live.frame_age_secs,
        // A dead link reports no rate; the age still ticks.
        frame_fps: match &live.link {
            Some(DeviceState::Ready { .. }) => live.fps,
            _ => None,
        },
        session_key: live.session_key.clone(),
        uid: identity.map(|identity| identity.uid.clone()),
        // Pre-provision title: the detected chip is the one honest,
        // discriminating name available before an identity is stamped
        // ("ESP32-C6" beats "Connected device" with two boards attached).
        // The title does NOT feed the render key — two same-chip boards
        // still key distinctly by `session_key` — see `identity_key`.
        name: identity
            .map(|identity| identity.name.clone())
            // An identity with an EMPTY name is a board that has a uid
            // and no name yet (device identity design §3) — it falls
            // through the same cascade an anonymous board takes.
            .filter(|name| !name.is_empty())
            .or_else(|| live.detected_chip.as_deref().and_then(display_chip_name))
            .unwrap_or_else(|| "Connected device".to_string()),
        transport: live.transport.clone().unwrap_or_default(),
        state,
        project,
        fw,
        hardware,
        detected_chip: live.detected_chip.clone(),
        // a device's board is a REGISTRY fact, read there (see the field doc)
        board_id: None,
        port_label: live.port_label.clone(),
        // Only a LIVE link's report counts: a stale clamp on a card whose
        // session is gone would tell the user a replug is still needed
        // after they already did it.
        safe_clamp: match &live.link {
            Some(DeviceState::Ready { .. }) => live
                .recovery
                .as_ref()
                .and_then(|recovery| recovery.output_clamp),
            _ => None,
        },
        sim: false,
        console_tail: live.console_tail.clone(),
        ui: CardUiState::default(),
    }
}

/// Human display form of a detected chip: resolve the reported spelling
/// (ROM banner `esp32c6`, esptool-js `ESP32-C6 (QFN32) (revision v0.2)`)
/// to its canonical id, then render it as Espressif writes it
/// (`ESP32-C6`). `None` when the report names silicon this build does not
/// know — the caller keeps its own fallback rather than showing a mangled
/// string.
fn display_chip_name(reported: &str) -> Option<String> {
    let id = lpa_link::chip_id_from_reported(reported)?;
    let rest = id.strip_prefix("esp32")?;
    if rest.is_empty() {
        return Some("ESP32".to_string());
    }
    Some(format!("ESP32-{}", rest.to_ascii_uppercase()))
}

/// Last-seen ordering key: live cards lead, sighted cards follow newest
/// first, never-sighted cards trail.
fn last_seen_sort_key(card: &UiDeviceCard) -> f64 {
    match &card.state {
        RosterCardState::Offline { last_seen_at } => last_seen_at.unwrap_or(f64::NEG_INFINITY),
        _ => f64::INFINITY,
    }
}

/// The display label a pattern project's kind reads as
/// ([`crate::app::library::package_manifest::kind_label`]).
const PATTERN_KIND_LABEL: &str = "Pattern";

/// Every pattern export the library offers, for the add-node picker's
/// import source (module authoring unit, P5).
///
/// Derived from the ALREADY-hydrated gallery cards rather than from a
/// second library read: the snapshot has been walked once at settle, and
/// the picker's needs are exactly two fields that walk already produced.
/// Blocked packages are skipped — a project this build cannot open is not
/// a project it can vendor bytes out of.
pub fn importable_patterns(inputs: &HomeInputs) -> Vec<crate::UiImportablePattern> {
    inputs
        .projects
        .iter()
        .filter(|card| card.project_kind == PATTERN_KIND_LABEL && card.health.is_openable())
        .flat_map(|card| {
            let family = card.exports.len() > 1;
            card.exports
                .iter()
                .map(move |export| crate::UiImportablePattern {
                    package_uid: card.uid.clone(),
                    package_label: card.slug.clone(),
                    export: export.clone(),
                    family,
                })
        })
        .collect()
}

fn package_card(
    store: &LibraryStore,
    registered: &[RegisteredDevice],
    summary: crate::app::library::PackageSummary,
) -> Result<UiPackageCard, crate::app::library::LibraryError> {
    let handle = store.open(summary.uid)?;
    let meta = crate::app::library::package_meta::read_meta(&*handle.package_fs.borrow())?;
    // Advisory board target (vision D3) + authored project kind (module
    // authoring unit, P1): straight passthrough from the container
    // manifest, same seam `provenance`/`on_device` use — no catalog lookup
    // here, that's the web renderer's job.
    let manifest_fields =
        crate::app::library::package_manifest::read_manifest(&*handle.package_fs.borrow())?;
    let target = manifest_fields.target;
    let project_kind =
        crate::app::library::package_manifest::kind_label(&manifest_fields.kind).to_string();
    let exports = manifest_fields.exports;

    let last_saved_at = handle
        .history
        .events()
        .iter()
        .rev()
        .find_map(|event| match event.kind {
            EventKind::Saved { .. } => Some(event.at),
            _ => None,
        })
        .or(meta.as_ref().map(|meta| meta.created_at));

    let uid = summary.uid.to_string();
    let on_device = handle.history.head().and_then(|head| {
        registered.iter().find_map(|device| {
            let association = device.association.as_ref()?;
            (association.project.to_string() == uid && association.version == head)
                .then(|| device.name.clone())
        })
    });

    Ok(UiPackageCard {
        uid,
        kind: summary.kind,
        project_kind,
        exports,
        slug: summary.slug,
        last_saved_at,
        provenance: meta.and_then(|meta| provenance_line(store, &meta)),
        on_device,
        open_elsewhere: false,  // stamped by the hydration pass
        connected_device: None, // stamped by the D28 pairing at view build
        running_in_sim: false,  // stamped by the D28 sim arm at view build
        target,
        health: summary.health,
    })
}

/// The card for a package whose history or provenance would not open. The
/// summary is all we have — and it is enough to name the package, show its
/// health, and offer export and delete.
fn degraded_package_card(summary: crate::app::library::PackageSummary) -> UiPackageCard {
    UiPackageCard {
        uid: summary.uid.to_string(),
        kind: summary.kind,
        // A degraded package's manifest may be unreadable; fall back to
        // the default kind rather than claim one we could not read.
        project_kind: crate::app::library::package_manifest::kind_label(
            &lpc_model::ProjectKind::General,
        )
        .to_string(),
        // Same reason as the kind above: nothing was readable, so nothing
        // is claimed.
        exports: Vec::new(),
        slug: summary.slug,
        last_saved_at: None,
        provenance: None,
        on_device: None,
        open_elsewhere: false,
        connected_device: None,
        running_in_sim: false,
        // A degraded package's manifest may be unreadable; no target claim.
        target: None,
        health: summary.health,
    }
}

/// The card's human provenance line; `None` for created-from-scratch.
fn provenance_line(store: &LibraryStore, meta: &PackageMeta) -> Option<String> {
    match &meta.provenance {
        PackageProvenance::Created => None,
        PackageProvenance::SeededFrom { source } => {
            let name = super::embedded_example::embedded_example(source)
                .map(|example| example.name.to_string())
                .unwrap_or_else(|| source.clone());
            Some(format!("Remixed from {name}"))
        }
        PackageProvenance::ImportedZip { .. } => Some("Imported from zip".to_string()),
        PackageProvenance::ImportedJson { .. } => Some("Pasted from JSON".to_string()),
        PackageProvenance::PulledFromDevice { device_name, .. } => {
            Some(format!("Pulled from {device_name}"))
        }
        // Project-name-centric on purpose: the service exposes no owner
        // profile, and guessing would be worse than saying what it is.
        PackageProvenance::OpenedFromLink => Some("Shared with you".to_string()),
        PackageProvenance::ForkedFrom { parent_project, .. } => {
            let parent = parent_project
                .parse()
                .ok()
                .and_then(|uid| {
                    store
                        .list()
                        .ok()?
                        .into_iter()
                        .find_map(|summary| (summary.uid == uid).then_some(summary.slug))
                })
                .unwrap_or_else(|| parent_project.clone());
            Some(format!("Forked from {parent}"))
        }
    }
}

/// A remembered device's card, derived through the same evidence function
/// as the live card (registry-only evidence = offline).
fn device_card(device: &RegisteredDevice, projects: &[UiPackageCard]) -> UiDeviceCard {
    // the last-known chip prefers the project's current name; a deleted
    // project's association falls back to its uid
    let project = device.association.as_ref().map(|association| {
        let uid = association.project.to_string();
        let name = projects
            .iter()
            .find_map(|card| (card.uid == uid).then(|| card.slug.clone()))
            .unwrap_or_else(|| uid.clone());
        UiDeviceProjectChip { uid, name }
    });
    let state = derive_roster_card_state(&RosterEvidence {
        link: None,
        content: None,
        observed_version: None,
        head_version: None,
        local_saved_at: None,
        pushed_at: None,
        unnamed: false,
        registry: Some(device),
        connect: ConnectEvidence::Idle,
    });
    UiDeviceCard {
        frame_preview: None,
        frame_age_secs: None,
        frame_fps: None,
        port_label: None,
        session_key: None,
        uid: Some(device.uid.clone()),
        name: device.name.clone(),
        safe_clamp: None,
        // recorded at last sight from the live session's connector class
        transport: device.transport.clone(),
        state,
        project,
        // remembered only: no live hello, no firmware provenance
        fw: None,
        hardware: None,
        detected_chip: None,
        // the registry's board fact rides the remembered card (G1b ruling
        // 9/10: Details names it; the gone device's ▶ box shows it)
        board_id: device.board_id.clone(),
        sim: false,
        // no session, no console (D42: the console is the session's)
        console_tail: Vec::new(),
        ui: CardUiState::default(),
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use lpc_history::{DeviceAssociation, PrefixedUid, SyncRelation, UidPrefix};
    use lpc_model::LpFeature;
    use lpc_wire::{BuildFacts, HardwareFacts, ServerHello, WIRE_PROTO_VERSION};
    use lpfs::LpFsMemory;

    use crate::app::places::{DeviceIdentity, DeviceSyncState};

    use super::*;

    fn store() -> LibraryStore {
        let counter = Rc::new(RefCell::new(0u8));
        LibraryStore::new(
            Rc::new(RefCell::new(LpFsMemory::new())),
            Rc::new(move || {
                *counter.borrow_mut() += 1;
                [*counter.borrow(); 16]
            }),
            Rc::new(|| "2026-07-09-1421".to_string()),
        )
    }

    fn view_of(store: &LibraryStore) -> UiHomeView {
        let inputs = hydrate_home_inputs(store.fs_handle(), &[]);
        build_home_view(Some(&inputs), None, None, &HomePoolEvidence::default())
    }

    #[test]
    fn an_unopenable_package_still_gets_a_card() {
        // Swallow point 2: the card builder's `filter_map` dropped anything
        // whose history or provenance would not load, on top of the store
        // already dropping anything whose manifest would not parse. A card
        // that says what is wrong is the only honest outcome.
        let store = store();
        store.create("Healthy", 1.0).unwrap();
        {
            let fs = store.fs_handle();
            let view = fs.borrow();
            view.write_file(
                lpc_model::LpPath::new("/packages/z-hand-copied/project.json"),
                br#"{"kind":"Project","format":2,"nodes":{}}"#,
            )
            .unwrap();
        }

        let view = view_of(&store);
        assert_eq!(view.projects.len(), 2, "neither package vanished");
        let stale = view
            .projects
            .iter()
            .find(|card| card.slug == "z-hand-copied")
            .expect("the unreadable package has a card");
        let (headline, remedy) = stale.health.blocked().expect("classified as blocked");
        assert_eq!(headline, "Format 2 — too old for this Studio");
        assert!(
            remedy.contains("too old to upgrade automatically"),
            "{remedy}"
        );
        // its identity is addressable, so the card's delete/export work
        assert!(stale.uid.starts_with("prj"));

        let healthy = view
            .projects
            .iter()
            .find(|card| card.slug == "2026-07-09-1421-healthy")
            .expect("the healthy package is unaffected");
        assert_eq!(healthy.health, crate::app::library::PackageHealth::Ready);
    }

    fn ready_link() -> DeviceState {
        DeviceState::Ready {
            hello: ServerHello {
                proto: WIRE_PROTO_VERSION,
                build: BuildFacts {
                    features: vec![
                        LpFeature::NodeButton,
                        LpFeature::NodeClock,
                        LpFeature::NodeFluid,
                        LpFeature::NodeFixture,
                        LpFeature::NodePlaylist,
                        LpFeature::NodeRadio,
                        LpFeature::NodeShader,
                        LpFeature::NodeTexture,
                        LpFeature::SvcButton,
                        LpFeature::SvcRadioEspnow,
                        LpFeature::GfxLpvm,
                    ],
                    package: "fw-esp32c6".to_string(),
                    commit: "abc123456789".to_string(),
                    dirty: false,
                    profile: "release-esp32".to_string(),
                },
                hardware: HardwareFacts {
                    radio: true,
                    button: true,
                    board_id: None,
                    ..Default::default()
                },
                device_uid: Some("devaaaaaaaaaaaaaaaa".to_string()),
            },
        }
    }

    #[test]
    fn the_pinned_anonymous_card_takes_its_own_sessions_op() {
        // The two halves of the recovery-write ending, together: the pin
        // keeps the anonymous card alive, AND the awaiting op rides it —
        // half a fix showed the user a bare "Not seen yet" with no
        // instruction (bench, 2026-07-31).
        let evidence = HomeDeviceEvidence {
            link: Some(DeviceState::Gone),
            op_in_flight: true,
            session_key: Some("runtime-1".to_string()),
            ..HomeDeviceEvidence::default()
        };
        let card = live_device_card(&evidence).expect("pinned");
        assert!(
            card.takes_card_op("runtime-1"),
            "the instruction must ride the pinned card"
        );
        assert!(
            !card.takes_card_op("runtime-2"),
            "another board's op is not this card's business"
        );

        // A REGISTERED offline card carries no session, so it cannot adopt
        // a stray op — the guard the old uid-less rule needed a special
        // case for is now structural.
        let registered = UiDeviceCard {
            session_key: None,
            uid: Some("devother".to_string()),
            ..card.clone()
        };
        assert!(!registered.takes_card_op("runtime-1"));
    }

    #[test]
    fn an_in_flight_op_pins_the_card_through_gone() {
        // The bench bug (2026-07-31): "Start in safe mode" resets the board,
        // the anonymous recovery session lands Gone, the state derives
        // Offline, and the card VANISHED — taking the "unplug the board and
        // plug it back in" instruction with it. An op in flight must pin the
        // card; without one, a Gone anonymous session stays invisible.
        let evidence = HomeDeviceEvidence {
            link: Some(DeviceState::Gone),
            op_in_flight: true,
            ..HomeDeviceEvidence::default()
        };
        let card = live_device_card(&evidence).expect("op in flight pins the card");
        assert!(matches!(card.state, RosterCardState::Offline { .. }));

        let without_op = HomeDeviceEvidence {
            link: Some(DeviceState::Gone),
            op_in_flight: false,
            ..HomeDeviceEvidence::default()
        };
        assert!(
            live_device_card(&without_op).is_none(),
            "a Gone anonymous session with no op stays invisible"
        );
    }

    /// Evidence for a live, Ready device carrying `sync`.
    fn live(sync: DeviceSyncState) -> HomeDeviceEvidence {
        HomeDeviceEvidence {
            sync: Some(sync),
            link: Some(ready_link()),
            transport: Some("USB".to_string()),
            ..HomeDeviceEvidence::default()
        }
    }

    #[test]
    fn chrome_sessions_project_live_sessions_only() {
        // One live identified device, one registry-only (offline) row, and
        // the sim: the strip shows sim (pinned first) + the live device;
        // the registry row never appears (D36: chip = session).
        let mut evidence = live(DeviceSyncState {
            identity: Some(DeviceIdentity {
                uid: "devaaaaaaaaaaaaaaaa".to_string(),
                name: "Desk C6".to_string(),
            }),
            content: DeviceContent::Empty,
        });
        evidence.session_key = Some("session-1".to_string());
        let mut offline_card = device_card_from_live_evidence(&HomeDeviceEvidence::default());
        offline_card.uid = Some("devbbbbbbbbbbbbbbbb".to_string());
        offline_card.name = "Shelf sign".to_string();
        let pool = HomePoolEvidence {
            devices: vec![evidence],
            sim: Some(HomeSimEvidence {
                project: Some(UiDeviceProjectChip {
                    uid: "prjcccccccccccccccc".to_string(),
                    name: "zook-dome".to_string(),
                }),
                ..HomeSimEvidence::default()
            }),
        };

        let sessions = chrome_sessions(
            std::slice::from_ref(&offline_card),
            &pool,
            Some(&crate::UiLensRuntime::Device {
                uid: Some("devaaaaaaaaaaaaaaaa".to_string()),
            }),
        );

        assert_eq!(
            sessions.len(),
            2,
            "sim + live device, never the registry row"
        );
        assert!(sessions[0].sim, "the sim is pinned first");
        assert_eq!(
            sessions[0].target,
            crate::UiChromeSessionTarget::Sim {
                project_key: Some("prjcccccccccccccccc".to_string())
            }
        );
        assert!(!sessions[0].lensed);
        // a project-less sim would read Empty; loaded = running clean
        assert_eq!(sessions[0].status, crate::UiChromeSessionStatus::Run);
        let device = &sessions[1];
        assert_eq!(device.name, "Desk C6");
        assert!(device.lensed, "the lens uid matches the live device");
        assert_eq!(device.transport, "USB");
        assert_eq!(
            device.status,
            crate::UiChromeSessionStatus::Empty,
            "connected with nothing running reads hollow"
        );
        assert_eq!(
            device.target,
            crate::UiChromeSessionTarget::Device {
                uid: Some("devaaaaaaaaaaaaaaaa".to_string())
            }
        );
    }

    #[test]
    fn a_live_heartbeat_clamp_reaches_the_card_and_a_dead_link_drops_it() {
        let clamped_recovery = lpc_wire::server::RecoveryStatus {
            level: lpc_wire::server::RecoveryLevelWire::Green,
            reset_reason: "power-on".to_string(),
            boot_count: 1,
            safe_mode: false,
            output_clamp: Some(26),
            last_crash: None,
            paths: Vec::new(),
        };
        let mut evidence = live(DeviceSyncState {
            identity: Some(DeviceIdentity {
                uid: "devaaaaaaaaaaaaaaaa".to_string(),
                name: "TestBoard1".to_string(),
            }),
            content: DeviceContent::Empty,
        });
        evidence.recovery = Some(clamped_recovery.clone());

        let card = device_card_from_live_evidence(&evidence);
        assert_eq!(
            card.safe_clamp,
            Some(26),
            "a Ready link's reported clamp is the card's safe-mode evidence"
        );

        // A Gone link keeps the last heartbeat in memory but the card must
        // not claim the (rebooted, unclamped) board is still in safe mode.
        evidence.link = Some(DeviceState::Gone);
        let card = device_card_from_live_evidence(&evidence);
        assert_eq!(card.safe_clamp, None);
    }

    /// A pool carrying one device session's evidence and no sim.
    fn device_pool(device: HomeDeviceEvidence) -> HomePoolEvidence {
        HomePoolEvidence {
            devices: vec![device],
            sim: None,
        }
    }

    /// A pool carrying only the live sim session's evidence.
    fn sim_pool(project: Option<UiDeviceProjectChip>) -> HomePoolEvidence {
        HomePoolEvidence {
            devices: Vec::new(),
            sim: Some(HomeSimEvidence {
                project,
                frame: None,
                frame_age_secs: None,
                fps: None,
                board_id: None,
                console_tail: Vec::new(),
            }),
        }
    }

    #[test]
    fn no_library_still_lists_examples() {
        let view = build_home_view(None, None, None, &HomePoolEvidence::default());
        assert!(!view.library_available);
        assert!(view.projects.is_empty());
        assert_eq!(view.examples.len(), embedded_examples().len());
        assert!(
            view.examples
                .iter()
                .any(|example| example.name == "Fyeah Sign")
        );
    }

    #[test]
    fn open_elsewhere_uids_stamp_their_cards() {
        let store = store();
        let held = store.create("Held", 1.0).unwrap();
        let free = store.create("Free", 2.0).unwrap();

        let inputs = hydrate_home_inputs(store.fs_handle(), &[held.uid.to_string()]);
        let by_uid = |uid: &str| inputs.projects.iter().find(|card| card.uid == uid).unwrap();
        assert!(by_uid(&held.uid.to_string()).open_elsewhere);
        assert!(!by_uid(&free.uid.to_string()).open_elsewhere);
    }

    #[test]
    fn package_cards_carry_meta_and_provenance() {
        let store = store();
        store.create("Scratch", 10.0).unwrap();
        store
            .install_package(
                "Basic",
                &[(
                    "project.json".to_string(),
                    br#"{"format":10,"name":"Basic"}"#.to_vec(),
                )],
                PackageProvenance::SeededFrom {
                    source: "examples/fyeah-sign".to_string(),
                },
                20.0,
            )
            .unwrap();

        let view = view_of(&store);
        assert!(view.library_available);
        assert_eq!(view.projects.len(), 2);

        let basic = view
            .projects
            .iter()
            .find(|card| card.slug == "2026-07-09-1421-basic")
            .unwrap();
        assert_eq!(basic.provenance.as_deref(), Some("Remixed from Fyeah Sign"));
        assert_eq!(basic.last_saved_at, Some(20.0));

        let scratch = view
            .projects
            .iter()
            .find(|card| card.slug == "2026-07-09-1421-scratch")
            .unwrap();
        assert_eq!(scratch.provenance, None);
        assert_eq!(scratch.kind, "Module");
    }

    /// P02: the advisory `target` passes through from the container
    /// manifest to the card, and stays `None` for an untargeted project
    /// (the common case — `store.create` writes no `target`).
    #[test]
    fn package_cards_carry_advisory_target() {
        let store = store();
        store.create("Untargeted", 1.0).unwrap();
        store
            .install_package(
                "Targeted",
                &[(
                    "project.json".to_string(),
                    br#"{"format":4,"name":"Targeted","target":"espressif/esp32-c6-devkitc-1"}"#
                        .to_vec(),
                )],
                PackageProvenance::Created,
                2.0,
            )
            .unwrap();

        let view = view_of(&store);
        let targeted = view
            .projects
            .iter()
            .find(|card| card.slug == "2026-07-09-1421-targeted")
            .unwrap();
        assert_eq!(
            targeted.target.as_deref(),
            Some("espressif/esp32-c6-devkitc-1")
        );

        let untargeted = view
            .projects
            .iter()
            .find(|card| card.slug == "2026-07-09-1421-untargeted")
            .unwrap();
        assert_eq!(untargeted.target, None);
    }

    #[test]
    fn fork_provenance_names_the_parent_slug() {
        let store = store();
        let original = store.create("Original", 1.0).unwrap();
        let copy_summary = store.duplicate(original.uid, 2.0).unwrap();
        // re-stamped label, uniqued against the (same-stamp) original
        assert_eq!(copy_summary.slug, "2026-07-09-1421-original-2");

        let view = view_of(&store);
        let copy = view
            .projects
            .iter()
            .find(|card| card.uid == copy_summary.uid.to_string())
            .unwrap();
        assert_eq!(
            copy.provenance.as_deref(),
            Some("Forked from 2026-07-09-1421-original")
        );
    }

    #[test]
    fn same_named_registered_devices_keep_unique_render_keys() {
        // Erasing and re-provisioning a board registers a NEW dev… uid
        // under the SAME name. Duplicate render keys panic the keyed diff
        // and kill the whole app (2026-07-15 crash) — uid-based keys must
        // stay unique, and both cards must survive.
        let store = store();
        let registry = DeviceRegistry::new(store.fs_handle());
        for seed in [7u8, 8u8] {
            registry
                .upsert(crate::app::places::RegisteredDevice {
                    uid: PrefixedUid::mint(UidPrefix::Device, &[seed; 16]).to_string(),
                    name: "lp-c6".to_string(),
                    transport: "USB".to_string(),
                    last_seen_at: 5.0,
                    association: None,
                    board_id: None,
                    hardware_id: None,
                    previous_uids: Vec::new(),
                })
                .unwrap();
        }

        let view = view_of(&store);
        assert_eq!(view.devices.len(), 2);
        let keys: std::collections::HashSet<_> = view
            .devices
            .iter()
            .map(|card| card.render_key().to_string())
            .collect();
        assert_eq!(keys.len(), 2, "render keys must be unique: {keys:?}");
    }

    #[test]
    fn duplicate_card_keys_degrade_to_a_dropped_card_not_a_dead_ui() {
        // A corrupt registry/store that repeats an identity must lose the
        // duplicate card (with a warning), never poison the keyed list.
        let offline = RosterCardState::Offline {
            last_seen_at: Some(5.0),
        };
        let cards = vec![
            UiDeviceCard {
                frame_preview: None,
                frame_age_secs: None,
                frame_fps: None,
                port_label: None,
                session_key: None,
                uid: Some("deva".to_string()),
                name: "one".to_string(),
                transport: "USB".to_string(),
                state: offline.clone(),
                project: None,
                fw: None,
                hardware: None,
                safe_clamp: None,
                sim: false,
                console_tail: Vec::new(),
                ui: CardUiState::default(),
                detected_chip: None,
                board_id: None,
            },
            UiDeviceCard {
                frame_preview: None,
                frame_age_secs: None,
                frame_fps: None,
                port_label: None,
                session_key: None,
                uid: Some("deva".to_string()),
                name: "two".to_string(),
                transport: "USB".to_string(),
                state: offline,
                project: None,
                fw: None,
                hardware: None,
                safe_clamp: None,
                sim: false,
                console_tail: Vec::new(),
                ui: CardUiState::default(),
                detected_chip: None,
                board_id: None,
            },
        ];
        let deduped = dedupe_by_key(cards, |card| card.render_key().to_string(), "device");
        assert_eq!(deduped.len(), 1);
        assert_eq!(deduped[0].name, "one");
    }

    #[test]
    fn registered_devices_become_offline_cards_with_project_chips() {
        let store = store();
        let summary = store.create("Porch", 1.0).unwrap();
        let head = store.open(summary.uid).unwrap().history.head().unwrap();

        let registry = DeviceRegistry::new(store.fs_handle());
        let device_uid = PrefixedUid::mint(UidPrefix::Device, &[9u8; 16]);
        registry
            .upsert(crate::app::places::RegisteredDevice {
                uid: device_uid.to_string(),
                name: "Luna's porch sign".to_string(),
                transport: "USB".to_string(),
                last_seen_at: 5.0,
                association: Some(DeviceAssociation {
                    device: device_uid,
                    project: summary.uid,
                    version: head,
                    at: 5.0,
                }),
                board_id: None,
                hardware_id: None,
                previous_uids: Vec::new(),
            })
            .unwrap();

        let view = view_of(&store);
        assert_eq!(view.devices.len(), 1);
        assert_eq!(view.devices[0].name, "Luna's porch sign");
        assert_eq!(
            view.devices[0].state,
            RosterCardState::Offline {
                last_seen_at: Some(5.0),
            }
        );
        let chip = view.devices[0].project.as_ref().expect("last-known chip");
        assert_eq!(chip.name, "2026-07-09-1421-porch");
        assert_eq!(chip.uid, summary.uid.to_string());

        let porch = view
            .projects
            .iter()
            .find(|card| card.slug == "2026-07-09-1421-porch")
            .unwrap();
        assert_eq!(porch.on_device.as_deref(), Some("Luna's porch sign"));
    }

    #[test]
    fn reconnect_window_renders_on_the_remembered_card_not_a_twin() {
        // Regression (2026-07-22 HW walk, second sighting): during the
        // connect window — link opening, no identity landed yet — the
        // live evidence must render ON the clicked remembered card, not
        // as a transient anonymous card that later collapses.
        let store = store();
        let registry = DeviceRegistry::new(store.fs_handle());
        registry
            .upsert(RegisteredDevice {
                uid: "devaaaaaaaaaaaaaaaa".to_string(),
                name: "TestBoard1".to_string(),
                transport: "USB".to_string(),
                last_seen_at: 5.0,
                association: None,
                board_id: None,
                hardware_id: None,
                previous_uids: Vec::new(),
            })
            .unwrap();
        let inputs = hydrate_home_inputs(store.fs_handle(), &[]);

        let evidence = HomeDeviceEvidence {
            connect: ConnectEvidence::Connecting {
                phase: crate::ConnectPhase::Connecting,
            },
            pending_uid: Some("devaaaaaaaaaaaaaaaa".to_string()),
            ..HomeDeviceEvidence::default()
        };
        let view = build_home_view(Some(&inputs), None, None, &device_pool(evidence));

        assert_eq!(view.devices.len(), 1, "the remembered card, no twin");
        let card = &view.devices[0];
        assert_eq!(card.name, "TestBoard1");
        assert_eq!(card.uid.as_deref(), Some("devaaaaaaaaaaaaaaaa"));
        assert!(matches!(
            card.state,
            RosterCardState::ConnectingRetrying { .. }
        ));
    }

    #[test]
    fn failed_read_live_card_keeps_its_identity_and_dedups_the_registry() {
        // Regression (2026-07-22 HW walk): a content-read failure after a
        // successful identity read must NOT spawn an anonymous second
        // card — partial knowledge survives, the live card wears the
        // remembered name and replaces the offline card.
        let store = store();
        let registry = DeviceRegistry::new(store.fs_handle());
        registry
            .upsert(RegisteredDevice {
                uid: "devaaaaaaaaaaaaaaaa".to_string(),
                name: "TestBoard1".to_string(),
                transport: "USB".to_string(),
                last_seen_at: 5.0,
                association: None,
                board_id: None,
                hardware_id: None,
                previous_uids: Vec::new(),
            })
            .unwrap();
        let inputs = hydrate_home_inputs(store.fs_handle(), &[]);

        let evidence = live(DeviceSyncState {
            identity: Some(DeviceIdentity {
                uid: "devaaaaaaaaaaaaaaaa".to_string(),
                name: "TestBoard1".to_string(),
            }),
            content: DeviceContent::Unreadable {
                detail: "could not read the device: hash package failed".to_string(),
            },
        });
        let view = build_home_view(Some(&inputs), None, None, &device_pool(evidence));

        assert_eq!(view.devices.len(), 1, "one card, not an anonymous twin");
        let card = &view.devices[0];
        assert_eq!(card.name, "TestBoard1");
        assert_eq!(card.uid.as_deref(), Some("devaaaaaaaaaaaaaaaa"));
        assert!(matches!(
            card.state,
            RosterCardState::HoldsUnreadableData { .. }
        ));
    }

    #[test]
    fn d28_connected_device_keeps_its_card_and_the_project_gets_the_chip() {
        let store = store();
        let summary = store.create("Porch", 1.0).unwrap();
        // the device is also remembered in the registry (it must not show
        // twice: not as remembered-offline while it is live)
        let registry = DeviceRegistry::new(store.fs_handle());
        registry
            .upsert(RegisteredDevice {
                uid: "devaaaaaaaaaaaaaaaa".to_string(),
                name: "Porch sign".to_string(),
                transport: "USB".to_string(),
                last_seen_at: 5.0,
                association: None,
                board_id: None,
                hardware_id: None,
                previous_uids: Vec::new(),
            })
            .unwrap();
        let inputs = hydrate_home_inputs(store.fs_handle(), &[]);

        let mut evidence = live(DeviceSyncState {
            identity: Some(DeviceIdentity {
                uid: "devaaaaaaaaaaaaaaaa".to_string(),
                name: "Porch sign".to_string(),
            }),
            content: DeviceContent::Known {
                project_uid: summary.uid.to_string(),
                slug: summary.slug.clone(),
                observed: lpc_history::ContentHash::of(b"v"),
                relation: SyncRelation::Behind,
            },
        });
        evidence.observed_version = Some(3);
        evidence.head_version = Some(5);
        let view = build_home_view(Some(&inputs), None, None, &device_pool(evidence));

        // one device card (live, not remembered-offline), with the state
        // derived through the roster evidence function
        assert_eq!(view.devices.len(), 1, "one card, not two (D28)");
        let card = &view.devices[0];
        assert_eq!(card.name, "Porch sign");
        assert_eq!(
            card.state,
            RosterCardState::RunningBehind {
                observed_version: Some(3),
                head_version: Some(5),
            }
        );
        let chip = card.project.as_ref().expect("live project chip");
        assert_eq!(chip.name, summary.slug);

        // AND the project card carries the live connection chip
        let project = view
            .projects
            .iter()
            .find(|card| card.uid == summary.uid.to_string())
            .unwrap();
        let connection = project.connected_device.as_ref().expect("indication set");
        assert_eq!(connection.device_name, "Porch sign");
        assert_eq!(connection.relation, SyncRelation::Behind);
    }

    #[test]
    fn roster_sorts_by_last_seen_with_the_live_card_first() {
        let store = store();
        let registry = DeviceRegistry::new(store.fs_handle());
        for (seed, name, seen) in [
            (1u8, "old", 10.0),
            (2u8, "newest", 300.0),
            (3u8, "middle", 200.0),
        ] {
            registry
                .upsert(RegisteredDevice {
                    uid: PrefixedUid::mint(UidPrefix::Device, &[seed; 16]).to_string(),
                    name: name.to_string(),
                    transport: "USB".to_string(),
                    last_seen_at: seen,
                    association: None,
                    board_id: None,
                    hardware_id: None,
                    previous_uids: Vec::new(),
                })
                .unwrap();
        }
        let inputs = hydrate_home_inputs(store.fs_handle(), &[]);

        let evidence = live(DeviceSyncState {
            identity: Some(DeviceIdentity {
                uid: "devbbbbbbbbbbbbbbbb".to_string(),
                name: "Live one".to_string(),
            }),
            content: DeviceContent::Empty,
        });
        let view = build_home_view(Some(&inputs), None, None, &device_pool(evidence));

        let names: Vec<&str> = view.devices.iter().map(|card| card.name.as_str()).collect();
        assert_eq!(names, vec!["Live one", "newest", "middle", "old"]);
    }

    #[test]
    fn blank_and_unknown_devices_keep_their_own_cards() {
        let store = store();
        let inputs = hydrate_home_inputs(store.fs_handle(), &[]);

        let blank = live(DeviceSyncState {
            identity: Some(DeviceIdentity {
                uid: "devbbbbbbbbbbbbbbbb".to_string(),
                name: "Fresh board".to_string(),
            }),
            content: DeviceContent::Empty,
        });
        let view = build_home_view(Some(&inputs), None, None, &device_pool(blank));
        assert_eq!(view.devices.len(), 1);
        assert_eq!(view.devices[0].state, RosterCardState::ConnectedEmpty);

        let anonymous = live(DeviceSyncState {
            identity: None,
            content: DeviceContent::PendingIdentity {
                observed: lpc_history::ContentHash::of(b"x"),
            },
        });
        let view = build_home_view(Some(&inputs), None, None, &device_pool(anonymous));
        assert_eq!(view.devices.len(), 1);
        assert_eq!(view.devices[0].state, RosterCardState::NeedsAName);
        assert_eq!(view.devices[0].name, "Connected device");
    }

    #[test]
    fn a_gone_link_yields_no_live_card_only_the_registry_view() {
        // cable yanked: the link reads Gone; the remembered card (which
        // knows the last sighting) is the honest view, not a live card
        // that would say "Not seen yet"
        let store = store();
        let registry = DeviceRegistry::new(store.fs_handle());
        registry
            .upsert(RegisteredDevice {
                uid: "devaaaaaaaaaaaaaaaa".to_string(),
                name: "Porch sign".to_string(),
                transport: "USB".to_string(),
                last_seen_at: 50.0,
                association: None,
                board_id: None,
                hardware_id: None,
                previous_uids: Vec::new(),
            })
            .unwrap();
        let inputs = hydrate_home_inputs(store.fs_handle(), &[]);

        let evidence = HomeDeviceEvidence {
            link: Some(DeviceState::Gone),
            ..HomeDeviceEvidence::default()
        };
        let view = build_home_view(Some(&inputs), None, None, &device_pool(evidence));
        assert_eq!(view.devices.len(), 1);
        assert_eq!(
            view.devices[0].state,
            RosterCardState::Offline {
                last_seen_at: Some(50.0),
            }
        );
    }

    #[test]
    fn opening_and_issue_pass_through() {
        let view = build_home_view(
            None,
            Some("prjx".to_string()),
            Some(UiIssue::new("boom")),
            &HomePoolEvidence::default(),
        );
        assert_eq!(view.opening.as_deref(), Some("prjx"));
        assert_eq!(view.issue.as_ref().unwrap().message, "boom");
        assert_eq!(
            view.render_text_lines(),
            vec![
                format!(
                    "Home: 0 devices, 0 projects, {} examples",
                    embedded_examples().len()
                ),
                "  opening prjx".to_string(),
                "  issue: boom".to_string(),
            ]
        );
    }

    #[test]
    fn sim_session_yields_the_live_sim_card_and_stamps_the_project() {
        // D36 + the D28 sim arm: a live sim session running a known
        // project = a Running sim card wearing the project chip, AND the
        // project card wearing "Running in simulator".
        let store = store();
        let summary = store.create("Porch", 1.0).unwrap();
        let inputs = hydrate_home_inputs(store.fs_handle(), &[]);

        let pool = sim_pool(Some(UiDeviceProjectChip {
            uid: summary.uid.to_string(),
            name: summary.slug.clone(),
        }));
        let view = build_home_view(Some(&inputs), None, None, &pool);

        assert_eq!(view.devices.len(), 1);
        let card = &view.devices[0];
        assert!(card.sim, "the card wears the sim presentation");
        assert_eq!(card.render_key(), "runtime-sim");
        assert_eq!(card.name, "Simulator");
        assert_eq!(card.state, RosterCardState::RunningUpToDate);
        let chip = card.project.as_ref().expect("loaded project chip");
        assert_eq!(chip.name, summary.slug);

        let project = view
            .projects
            .iter()
            .find(|card| card.uid == summary.uid.to_string())
            .unwrap();
        assert!(project.running_in_sim, "the sim arm stamps the project");
        assert!(project.connected_device.is_none());
    }

    #[test]
    fn sim_with_nothing_loaded_reads_connected_empty() {
        let store = store();
        store.create("Porch", 1.0).unwrap();
        let inputs = hydrate_home_inputs(store.fs_handle(), &[]);

        let view = build_home_view(Some(&inputs), None, None, &sim_pool(None));
        assert_eq!(view.devices.len(), 1);
        let card = &view.devices[0];
        assert!(card.sim);
        assert_eq!(card.state, RosterCardState::ConnectedEmpty);
        assert!(card.project.is_none());
        assert!(
            view.projects.iter().all(|card| !card.running_in_sim),
            "no loaded project, no sim stamp"
        );
    }

    #[test]
    fn sim_and_device_sessions_both_feed_cards_with_the_sim_first() {
        // Coexistence (P2) reaches the roster (P4): the sim card pins
        // first among live, the device card keeps its full derivation,
        // and BOTH D28 pairings stamp their project cards.
        let store = store();
        let porch = store.create("Porch", 1.0).unwrap();
        let sign = store.create("Sign", 2.0).unwrap();
        let inputs = hydrate_home_inputs(store.fs_handle(), &[]);

        let device = live(DeviceSyncState {
            identity: Some(DeviceIdentity {
                uid: "devaaaaaaaaaaaaaaaa".to_string(),
                name: "Porch sign".to_string(),
            }),
            content: DeviceContent::Known {
                project_uid: porch.uid.to_string(),
                slug: porch.slug.clone(),
                observed: lpc_history::ContentHash::of(b"v"),
                relation: SyncRelation::AtHead,
            },
        });
        let pool = HomePoolEvidence {
            devices: vec![device],
            sim: Some(HomeSimEvidence {
                project: Some(UiDeviceProjectChip {
                    uid: sign.uid.to_string(),
                    name: sign.slug.clone(),
                }),
                frame: None,
                frame_age_secs: None,
                fps: None,
                board_id: None,
                console_tail: Vec::new(),
            }),
        };
        let view = build_home_view(Some(&inputs), None, None, &pool);

        assert_eq!(view.devices.len(), 2);
        assert!(view.devices[0].sim, "the sim card leads");
        assert_eq!(view.devices[0].state, RosterCardState::RunningUpToDate);
        assert!(!view.devices[1].sim);
        assert_eq!(view.devices[1].name, "Porch sign");
        assert_eq!(view.devices[1].state, RosterCardState::RunningUpToDate);

        let by_uid = |uid: &str| {
            view.projects
                .iter()
                .find(|card| card.uid == uid.to_string())
                .unwrap()
        };
        let porch_card = by_uid(&porch.uid.to_string());
        assert!(porch_card.connected_device.is_some());
        assert!(!porch_card.running_in_sim);
        let sign_card = by_uid(&sign.uid.to_string());
        assert!(sign_card.running_in_sim);
        assert!(sign_card.connected_device.is_none());
    }

    /// Two anonymous live boards, distinct sessions. This was the
    /// multi-board defect (2026-08-02): both cards fell back to the name
    /// "Connected device" as their render key, `dedupe_by_key` dropped
    /// the second, and one of two attached boards silently vanished.
    /// (Reproduces the planning session's scratch test, made permanent.)
    #[test]
    fn two_anonymous_live_boards_both_survive_build_home_view() {
        let anonymous = |session_key: &str, chip: &str| HomeDeviceEvidence {
            session_key: Some(session_key.to_string()),
            link: Some(DeviceState::Booting),
            transport: Some("USB".to_string()),
            detected_chip: Some(chip.to_string()),
            ..HomeDeviceEvidence::default()
        };
        let pool = HomePoolEvidence {
            devices: vec![anonymous("rt_1", "esp32c6"), anonymous("rt_2", "esp32s3")],
            sim: None,
        };

        let view = build_home_view(None, None, None, &pool);

        assert_eq!(view.devices.len(), 2, "the second board must not vanish");
        assert_ne!(
            view.devices[0].render_key(),
            view.devices[1].render_key(),
            "distinct sessions render under distinct keys"
        );
        // Q2: the pre-provision title names the chip, not "Connected device".
        assert_eq!(view.devices[0].name, "ESP32-C6");
        assert_eq!(view.devices[1].name, "ESP32-S3");
    }

    /// The title must never be relied on for the key: two boards of the
    /// SAME chip still key distinctly by session. This is the test that
    /// catches someone "fixing" the key via the name.
    #[test]
    fn two_anonymous_boards_of_the_same_chip_keep_distinct_keys() {
        let anonymous = |session_key: &str| HomeDeviceEvidence {
            session_key: Some(session_key.to_string()),
            link: Some(DeviceState::Booting),
            transport: Some("USB".to_string()),
            detected_chip: Some("esp32c6".to_string()),
            ..HomeDeviceEvidence::default()
        };
        let pool = HomePoolEvidence {
            devices: vec![anonymous("rt_1"), anonymous("rt_2")],
            sim: None,
        };

        let view = build_home_view(None, None, None, &pool);

        assert_eq!(view.devices.len(), 2);
        assert_eq!(
            view.devices[0].name, view.devices[1].name,
            "same chip, same title"
        );
        assert_ne!(
            view.devices[0].render_key(),
            view.devices[1].render_key(),
            "the key comes from the session, never the title"
        );
    }

    #[test]
    fn a_stamped_card_keys_by_uid_even_with_a_session_key() {
        // uid stays FIRST in the fallback chain: `CardUiState` survives
        // session replaces only because a stamped board's key is stable
        // across them.
        let evidence = HomeDeviceEvidence {
            session_key: Some("rt_7".to_string()),
            ..live(DeviceSyncState {
                identity: Some(DeviceIdentity {
                    uid: "devaaaaaaaaaaaaaaaa".to_string(),
                    name: "TestBoard1".to_string(),
                }),
                content: DeviceContent::Empty,
            })
        };
        let card = live_device_card(&evidence).expect("live card");
        assert_eq!(card.identity_key(), "devaaaaaaaaaaaaaaaa");
    }

    #[test]
    fn chip_display_names_resolve_reporter_spellings() {
        assert_eq!(display_chip_name("esp32c6").as_deref(), Some("ESP32-C6"));
        assert_eq!(
            display_chip_name("ESP32-S3 (QFN56) (revision v0.2)").as_deref(),
            Some("ESP32-S3")
        );
        assert_eq!(display_chip_name("esp32").as_deref(), Some("ESP32"));
        assert_eq!(
            display_chip_name("rp2040"),
            None,
            "unknown silicon keeps the caller's fallback"
        );
    }
}
