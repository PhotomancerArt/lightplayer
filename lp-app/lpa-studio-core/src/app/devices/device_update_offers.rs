//! The over-the-air update's verbs, as offers under `devices/<board>/…`,
//! one set per [`UpdateStanding`] row (the update-states spike's offers
//! table, as ruled: an over-the-air update to a newer version is Routine).
//!
//! | Row | Path verb | Button | Level |
//! |---|---|---|---|
//! | Update available | `update-firmware` | `Update` (another build: `Install Y`) | Routine |
//! | Up to date, Update available, Newer, Rolled back, Keeps crashing | `install-firmware` | `Other version…` (a `find` box over a `version` choice; the press reads `Install`) | per choice: Routine only when known newer |
//! | Wherever `Other version…` opens its list | `install-firmware-file` | `From a file…` (the web's file picker; the build joins the list) | needs a real click; the install arms |
//! | Keeps crashing | `reinstall-firmware` | `Reinstall` | Routine |
//! | A version Studio can't get | `install-firmware` | `Install Y` (one press with one version, else the same choice) | per choice, as above |
//! | Backing up | `cancel` | the activity's own Cancel ([`super::device_offers`]) | Routine |
//! | Needs USB once, over USB | `update-firmware` | today's USB flash | Lasting |
//! | This Studio's build can't go over the air, over USB (standing `Nothing`, route `Flash`) | `update-firmware` | today's USB flash | Lasting |
//! | This Studio's build can't go over the air, over Bluetooth (`NoWirelessBuild`) | — | nothing; the line says why | — |
//! | everything else (play-only, the progress rows, needs USB once over the air) | — | nothing: Studio does it, or there is nothing to do | — |
//!
//! Heal and finish are never offers (DS4): the controller starts them. A
//! play-only user is offered nothing here; the device zone's "Enter a
//! password" is the way. Factory reset stays on every board except two —
//! keeps crashing and a version Studio can't get, where installing is the
//! repair — and the rows where firmware is being written.
//!
//! **"Other version…"** lists the board's [`InstallChoice`]s (the store's
//! release index for its target, releases looked up by exact version, this
//! Studio's own build, and the store's `latest` when the index is missing):
//! newest first, the board's own and a build it refused drawn disabled, the
//! newest [`RECENT_CHOICES`] shown. Above the list a box (`find`) filters
//! it: typing searches every choice ([`crate::UiOffer::shown_options`]),
//! and a whole version names one. A whole release version the list does not
//! hold binds the press to "Look up <version>" ([`FirmwareLookupOp`]); once
//! the store answers, the version is a choice like any other, and the same
//! press installs it. **"From a file…"** (`install-firmware-file`, beside it
//! wherever the list opens) picks a custom build's update files; once core
//! has checked them ([`super::firmware_file_build`]) the build leads the
//! list, and its install is always Lasting, with copy that says it is a
//! custom build from files. It is offered when at least one choice can be picked,
//! and always shows its list — never a blind one-press install. The chip
//! reads "Other version…", its press "Install". Each choice binds at its
//! own level: **Routine** only when it is known newer than the board's,
//! speaks no older wire language than this Studio and keeps Bluetooth
//! updates; anything else is **Lasting**, with copy that says what
//! changes. `allow_downgrade` is set exactly when the choice is older than
//! the board's (what `decide()` needs).
//!
//! Each verb binds `Action::Update { device, intent }`: `Update` and
//! `Install Y` install Y; `Other version…` installs the chosen version;
//! `Reinstall` writes the crashing board's own firmware again. An
//! over-the-air verb is drawn disabled with [`DeviceView::update_blocked`]'s
//! reason when the link cannot carry it; the flash and Factory reset keep
//! [`DeviceView::firmware_blocked`].
//!
//! [`RECENT_CHOICES`]: super::install_choice::RECENT_CHOICES

use std::rc::Rc;

use lpa_devices::view::{DeviceView, Escape};
use lpa_devices::{Action, DeviceId, FirmwareAge, UpdateIntentFacts};

use super::device_update_route::{UpdateLink, UpdateRoute, update_route};
use super::device_update_standing::{UpdateStanding, UpdateStandingInputs, update_standing};
use super::device_update_version::UpdateVersion;
use super::devices_op::DevicesOp;
use super::firmware_file_build::FirmwareFileOp;
use super::firmware_lookup_op::FirmwareLookupOp;
use super::install_choice::{InstallChoice, InstallChoiceInputs, index_for, install_choices};
use super::store_lookups::StoreLookup;
use crate::{
    ActionConfirmation, OfferArgError, OfferArgs, OfferBinder, OfferChoice, OfferParam, OfferPath,
    UiAction, UiOffer,
};

/// The install verb's version parameter.
pub const INSTALL_VERSION_PARAM: &str = "version";

/// The install verb's box: text that filters its version list, or names a
/// version exactly.
pub const INSTALL_FIND_PARAM: &str = "find";

/// What the install verb's press reads inside its panel (the chip reads
/// "Other version…").
pub const INSTALL_PRESS_LABEL: &str = "Install";

/// The line the version list carries when the store's full list could not
/// be read (offline, or a server without the index).
pub const INSTALL_LIST_UNAVAILABLE: &str = "The full list isn't available right now.";

/// What a board's standing offers, and which of today's firmware verbs it
/// keeps.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateOffers {
    /// The over-the-air verbs, in card order.
    pub offers: Vec<UiOffer>,
    /// Today's USB flash at `update-firmware` stays.
    pub keep_flash: bool,
    /// Factory reset stays.
    pub keep_erase: bool,
}

/// The facts the card's offers need about a board's update: its standing,
/// which way its update goes, and the versions it could take. The default
/// tells no story (today's card).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UpdateOfferFacts {
    pub standing: UpdateStanding,
    pub route: UpdateRoute,
    /// The versions "Other version…" lists, read against the board.
    pub choices: Vec<InstallChoice>,
    /// Whether the store's release index for the board's target is known:
    /// without it the choices are this Studio's build and the store's
    /// `latest`, and the list says so.
    pub listed: bool,
    /// The board's target, when its manifest names one: what a look-up
    /// asks the store for.
    pub target: Option<String>,
    /// The look-ups of versions for the board's target the store has not
    /// found (yet), by version.
    pub unfound: Vec<(String, LookupStand)>,
}

/// Where a look-up the store has not found stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LookupStand {
    Looking,
    Missing,
    Offline,
}

impl UpdateOfferFacts {
    /// A board's facts: its standing from `inputs`, its route — over the
    /// air when its manifest says it can update over its link and that link
    /// `carries_update_channel` — and its version choices. The one reading
    /// the controller and the stories share.
    ///
    /// The build half of the route is this Studio's own build: it has
    /// facts only when it is update-capable (`device_update_route`'s module
    /// docs). A board that could update over Bluetooth while this Studio's
    /// build cannot reads [`UpdateStanding::NoWirelessBuild`].
    pub fn read(inputs: &UpdateStandingInputs<'_>, carries_update_channel: bool) -> Self {
        let can_update_over_link = inputs
            .facts
            .and_then(|facts| lpa_update::BoardView::from_json(facts.manifest_json.as_bytes()))
            .is_some_and(|board| board.can_update_over_link());
        let route = update_route(
            can_update_over_link,
            inputs.own.is_some(),
            inputs.link,
            carries_update_channel,
        );
        let mut standing = update_standing(inputs);
        if route == UpdateRoute::NoWirelessBuild
            && standing == UpdateStanding::Nothing
            && let Some(version) = inputs.facts.and_then(|facts| facts.version.clone())
        {
            standing = UpdateStanding::NoWirelessBuild {
                board: UpdateVersion {
                    version,
                    build_id: inputs.facts.and_then(|facts| facts.build_id.clone()),
                },
            };
        }
        let target = inputs.facts.and_then(|facts| facts.target.as_deref());
        let unfound = match (inputs.store_lookups, target) {
            (Some(lookups), Some(target)) => lookups
                .unfound(target)
                .filter_map(|(version, lookup)| {
                    let stand = match lookup {
                        StoreLookup::Looking => LookupStand::Looking,
                        StoreLookup::Missing => LookupStand::Missing,
                        StoreLookup::Offline => LookupStand::Offline,
                        StoreLookup::Found(_) => return None,
                    };
                    Some((version.to_string(), stand))
                })
                .collect(),
            _ => Vec::new(),
        };
        let (choices, listed) = match (standing.board(), target) {
            (Some(board), Some(target)) => {
                let refused = match &standing {
                    UpdateStanding::RolledBack { refused, .. } => Some(refused),
                    _ => None,
                };
                let choices = install_choices(&InstallChoiceInputs {
                    board,
                    target,
                    refused,
                    own: inputs.own,
                    store_latest: inputs.store_latest,
                    store_releases: inputs.store_releases,
                    store_lookups: inputs.store_lookups,
                    file_build: inputs.file_build,
                    link: inputs.link,
                    studio_wire_proto: lpc_wire::WIRE_PROTO_VERSION,
                });
                (choices, index_for(inputs.store_releases, target).is_some())
            }
            _ => (Vec::new(), false),
        };
        Self {
            standing,
            route,
            choices,
            listed,
            target: target.map(str::to_string),
            unfound,
        }
    }
}

/// The update offers `view`'s card makes for `facts`, under `prefix`. See
/// the module docs.
pub fn update_offers(
    view: &DeviceView,
    facts: &UpdateOfferFacts,
    prefix: &OfferPath,
) -> UpdateOffers {
    let standing = &facts.standing;
    let mut set = UpdateOffers {
        offers: Vec::new(),
        keep_flash: true,
        keep_erase: !withdraws_factory_reset(standing),
    };
    match standing {
        UpdateStanding::Nothing => return set,
        // Over USB, today's flash is the one update this board takes; over
        // the air there is nothing to press, and the line says what to do.
        UpdateStanding::NeedsUsbOnce { link, .. } => {
            set.keep_flash = *link == UpdateLink::Usb;
            return set;
        }
        // This Studio's build cannot go over the air, and over Bluetooth
        // there is no flash either: nothing to install, the line says why.
        UpdateStanding::NoWirelessBuild { .. } => {
            set.keep_flash = false;
            return set;
        }
        _ => {}
    }
    match facts.route {
        UpdateRoute::Flash => return set,
        UpdateRoute::NoWirelessBuild => {
            set.keep_flash = false;
            return set;
        }
        UpdateRoute::OverTheAir => {}
    }
    set.keep_flash = false;
    let linked = view.escapes.contains(&Escape::Disconnect);
    if !linked || view.activity.is_some() {
        return set;
    }
    let device = view.id;
    let blocked = view.update_blocked.as_deref();
    let at = |verb: &str| prefix.clone().child(verb);
    let other_version = |board: &UpdateVersion| InstallOffer {
        device,
        board: board.clone(),
        choices: facts.choices.clone(),
        listed: facts.listed,
        target: facts.target.clone(),
        unfound: facts.unfound.clone(),
        label: "Other version…".to_string(),
        preselect: None,
        one_press: false,
    };
    let install = match standing {
        UpdateStanding::Available { board, to } => {
            let label = match board.age_against(to) {
                FirmwareAge::Older | FirmwareAge::Unknown => "Update".to_string(),
                _ => format!("Install {}", to.short()),
            };
            let action = install_action(device, &to.version, false).with_label(label);
            set.offers.push(UiOffer::new(
                at("update-firmware"),
                "download",
                gate(action, blocked),
            ));
            // Beside Update, the list must hold more than Update's version —
            // or, with the store's list known, its box can look up any
            // other release by version.
            let more = facts
                .choices
                .iter()
                .any(|c| c.is_pickable() && c.version.version != to.version);
            (more || facts.listed).then(|| other_version(board))
        }
        UpdateStanding::UpToDate { version: board }
        | UpdateStanding::Newer { board, .. }
        | UpdateStanding::RolledBack { board, .. } => Some(other_version(board)),
        UpdateStanding::KeepsCrashing { board } => {
            let reinstall = DevicesOp::action_for(Action::Update {
                device,
                intent: UpdateIntentFacts::Reinstall,
            });
            set.offers.push(UiOffer::new(
                at("reinstall-firmware"),
                "retry",
                gate(reinstall, blocked),
            ));
            Some(other_version(board))
        }
        UpdateStanding::CantGetVersion { board, own } => Some(InstallOffer {
            label: format!("Install {}", own.short()),
            preselect: Some(own.version.clone()),
            one_press: true,
            ..other_version(board)
        }),
        _ => None,
    };
    if let Some(offer) = install.and_then(|install| install.offer(at("install-firmware"), blocked))
    {
        // Where the list opens, a custom build can be picked from files
        // too (the web draws it in the list's panel).
        let lists = !offer.params().is_empty();
        set.offers.push(offer);
        if lists {
            set.offers.push(UiOffer::new(
                at("install-firmware-file"),
                "upload",
                gate(FirmwareFileOp::action_for(device), blocked),
            ));
        }
    }
    set
}

/// Whether the row takes Factory reset away: where installing is the
/// repair, and while firmware is being written.
fn withdraws_factory_reset(standing: &UpdateStanding) -> bool {
    standing.is_progress()
        || matches!(
            standing,
            UpdateStanding::KeepsCrashing { .. } | UpdateStanding::CantGetVersion { .. }
        )
}

/// `install-firmware` as one row offers it.
struct InstallOffer {
    device: DeviceId,
    board: UpdateVersion,
    choices: Vec<InstallChoice>,
    listed: bool,
    target: Option<String>,
    unfound: Vec<(String, LookupStand)>,
    /// What the chip reads; its press inside the panel reads
    /// [`INSTALL_PRESS_LABEL`].
    label: String,
    /// The version picked for the user; else the newest one that can be
    /// picked.
    preselect: Option<String>,
    /// With a single version to pick, the button installs it at one press
    /// ("Install Y"). "Other version…" always shows its list.
    one_press: bool,
}

impl InstallOffer {
    /// The offer: a `find` box over one `version` choice — every choice,
    /// the board's own and a refused build disabled, the older ones shown
    /// only while the box finds them — bound to an install of the chosen
    /// one at its own level, or to a look-up of a version the box names
    /// that the list does not hold. `None` when no version can be picked.
    fn offer(self, path: OfferPath, blocked: Option<&str>) -> Option<UiOffer> {
        let pickable = || self.choices.iter().filter(|c| c.is_pickable());
        let preselect = self
            .preselect
            .clone()
            .filter(|version| pickable().any(|c| c.version.version == *version))
            .or_else(|| {
                pickable()
                    .find(|c| c.recent)
                    .map(|c| c.version.version.clone())
            })?;
        let single = pickable().count() == 1;
        let options: Vec<OfferChoice> = self.choices.iter().map(choice_option).collect();
        let Self {
            device,
            board,
            choices,
            listed,
            target,
            unfound,
            label: chip,
            one_press,
            ..
        } = self;
        let choices = Rc::new(choices);
        let pick = {
            let choices = Rc::clone(&choices);
            move |version: &str| -> Option<UiAction> {
                let choice = choices.iter().find(|c| c.version.version == version)?;
                Some(bind_choice(device, &board, choice))
            }
        };
        let template = pick(&preselect)?;
        if let Some(reason) = blocked {
            return Some(UiOffer::new(
                path,
                "download",
                template.with_label(chip).disabled(reason),
            ));
        }
        if one_press && single {
            return Some(UiOffer::new(path, "download", template.with_label(chip)));
        }
        let mut version =
            OfferParam::choice(INSTALL_VERSION_PARAM, "version", options, Some(preselect))
                .filtered_by(INSTALL_FIND_PARAM);
        if !listed {
            version = version.with_note(INSTALL_LIST_UNAVAILABLE);
        }
        let find = OfferParam::text(INSTALL_FIND_PARAM, "find a version", "Type a version")
            .optional()
            .max_len(40);
        let typed = TypedVersion {
            choices,
            target,
            unfound,
        };
        let mut offer = UiOffer::with_params(
            path,
            "download",
            vec![find, version],
            OfferBinder::new(move |args: &OfferArgs| {
                match args.choice(INSTALL_VERSION_PARAM) {
                    Some(version) => pick(version)
                        .map(|install| install.with_label(INSTALL_PRESS_LABEL))
                        .ok_or_else(|| OfferArgError::Invalid {
                            name: INSTALL_VERSION_PARAM.to_string(),
                            reason: format!("{version} is not a version this Studio can get"),
                        }),
                    // The box names no version the list can install: a
                    // look-up, reading as one ("Look up 2026.09.30-2").
                    None => typed.press(args.text(INSTALL_FIND_PARAM).unwrap_or_default()),
                }
            }),
            template.with_label(INSTALL_PRESS_LABEL),
        );
        // The chip names the verb; the press inside its panel installs.
        offer.action = offer.action.with_label(chip);
        Some(offer)
    }
}

/// What the box's text binds when it names no choice that can be picked: a
/// look-up of a whole release version the list does not hold, or why not.
struct TypedVersion {
    choices: Rc<Vec<InstallChoice>>,
    target: Option<String>,
    unfound: Vec<(String, LookupStand)>,
}

impl TypedVersion {
    fn press(&self, typed: &str) -> Result<UiAction, OfferArgError> {
        let invalid = |reason: String| OfferArgError::Invalid {
            name: INSTALL_FIND_PARAM.to_string(),
            reason,
        };
        if typed.is_empty() {
            return Err(OfferArgError::Missing {
                name: INSTALL_VERSION_PARAM.to_string(),
                label: "version".to_string(),
            });
        }
        if lpc_firmware_release::ReleaseVersion::parse(typed).is_none() {
            let found = self
                .choices
                .iter()
                .any(|c| c.is_pickable() && choice_option(c).matches(typed));
            if found {
                return Err(OfferArgError::Missing {
                    name: INSTALL_VERSION_PARAM.to_string(),
                    label: "version".to_string(),
                });
            }
            return Err(invalid(format!(
                "no version here matches “{typed}”; type a whole version (2026.10.03-1) to look \
                 it up"
            )));
        }
        if let Some(choice) = self.choices.iter().find(|c| c.version.version == typed) {
            let reason = choice_option(choice)
                .disabled
                .unwrap_or_else(|| "it cannot be picked".to_string());
            return Err(OfferArgError::OptionDisabled {
                name: INSTALL_VERSION_PARAM.to_string(),
                value: typed.to_string(),
                reason,
            });
        }
        let Some(target) = self.target.as_deref() else {
            return Err(OfferArgError::Unavailable {
                reason: "this board has not said what it was built for".to_string(),
            });
        };
        let lookup = FirmwareLookupOp::action_for(target, typed);
        match self
            .unfound
            .iter()
            .find(|(version, _)| version == typed)
            .map(|(_, stand)| *stand)
        {
            None => Ok(lookup),
            Some(LookupStand::Looking) => Ok(lookup.disabled(format!("Looking up {typed}…"))),
            Some(LookupStand::Offline) => Ok(lookup.with_summary(format!(
                "Studio couldn't reach the firmware store to look up {typed}; try again."
            ))),
            Some(LookupStand::Missing) => Err(invalid(format!(
                "the firmware store has no {typed} for this board"
            ))),
        }
    }
}

/// One choice as the list draws it: the version, a date line and at most
/// one warning, or why it cannot be picked.
fn choice_option(choice: &InstallChoice) -> OfferChoice {
    let mut option = OfferChoice::new(&choice.version.version, choice.version.short());
    let mut detail: Vec<String> = Vec::new();
    if choice.own {
        detail.push("this Studio's build".to_string());
    }
    if choice.from_file {
        detail.push("from your files".to_string());
    }
    if let Some(date) = date_line(choice) {
        detail.push(date);
    }
    if !detail.is_empty() {
        option = option.with_detail(detail.join(" · "));
    }
    if let Some(warning) = warning(choice) {
        option = option.with_warning(warning);
    }
    if choice.on_board {
        option = option.disabled("On this board now");
    } else if choice.refused {
        option = option.disabled("This board refused it after it failed to start");
    }
    if !choice.recent {
        option = option.only_with(INSTALL_FIND_PARAM);
    }
    option
}

/// When the release was published (`Oct 7, 05:41 UTC`), else the day its
/// version names (`Oct 7`); `None` for a dev build.
fn date_line(choice: &InstallChoice) -> Option<String> {
    if let Some(at) = choice.published_at.as_deref()
        && let Some((date, time)) = at.split_once('T')
        && let Some(day) = month_day(date)
    {
        let hhmm: String = time.chars().take(5).collect();
        if hhmm.len() == 5 {
            return Some(format!("{day}, {hhmm} UTC"));
        }
        return Some(day);
    }
    let (date, _) = choice.version.version.split_once('-')?;
    month_day(&date.replace('.', "-"))
}

/// `2026-10-07` as `Oct 7`.
fn month_day(date: &str) -> Option<String> {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let mut parts = date.split('-');
    let (_year, month, day) = (parts.next()?, parts.next()?, parts.next()?);
    let month: usize = month.parse().ok()?;
    let day: u32 = day.parse().ok()?;
    Some(format!("{} {day}", MONTHS.get(month.checked_sub(1)?)?))
}

/// The one warning a choice's line carries, most lasting first.
fn warning(choice: &InstallChoice) -> Option<&'static str> {
    if choice.needs_usb_after {
        Some("needs a USB cable after, over Bluetooth")
    } else if choice.speaks_older_wire() {
        Some("older language than Studio")
    } else if choice.speaks_newer_wire() {
        Some("newer than Studio — reload to edit")
    } else {
        None
    }
}

/// An install of `choice` on a board running `board`, at the choice's own
/// level (see the module docs).
fn bind_choice(device: DeviceId, board: &UpdateVersion, choice: &InstallChoice) -> UiAction {
    // A build from files has no order the host can trust: `decide()` may
    // take it whichever way it compares.
    let allow_downgrade = choice.is_older() || choice.from_file;
    let action = install_action(device, &choice.version.version, allow_downgrade);
    match install_confirmation(board, choice) {
        Some(copy) => action.lasting(copy),
        None => action,
    }
}

/// What installing `choice` changes, when it is more than an update: the
/// two-click arm's copy. `None` for a choice known newer than the board's
/// that speaks this Studio's language (or newer) and keeps Bluetooth
/// updates.
fn install_confirmation(
    board: &UpdateVersion,
    choice: &InstallChoice,
) -> Option<ActionConfirmation> {
    let version = choice.version.long();
    let older_wire = choice.speaks_older_wire();
    if choice.is_known_newer() && !older_wire && !choice.needs_usb_after && !choice.from_file {
        return None;
    }
    let (title, mut message) = if choice.from_file {
        (
            "Install a custom build?".to_string(),
            format!(
                "{version} is a custom build from files on this computer, not a release from the \
                 store: Studio checked the files against their manifest, but not where they came \
                 from, and it may not read the board's project."
            ),
        )
    } else if choice.is_older() {
        (
            "Install an older version?".to_string(),
            format!(
                "{version} is older than what this board runs, and an older version may not \
                 read the board's project."
            ),
        )
    } else if choice.is_known_newer() {
        (
            format!("Install {}?", choice.version.short()),
            String::new(),
        )
    } else {
        (
            "Install another build?".to_string(),
            format!(
                "{version} can't be compared with what this board runs ({}), so it may not \
                 read the board's project.",
                board.long()
            ),
        )
    };
    if older_wire {
        let lead = if message.is_empty() { "It" } else { " It also" };
        message.push_str(&format!(
            "{lead} speaks an older language than this Studio: Studio can still update it, \
             but may not edit its project until you do."
        ));
    }
    if choice.needs_usb_after {
        if !message.is_empty() {
            message.push(' ');
        }
        message.push_str(
            "Over Bluetooth, this version can't be updated again until it has been connected \
             by USB once.",
        );
    }
    Some(ActionConfirmation::new(title, message, "install"))
}

/// An install of `version`; the op's own meta makes a downgrade Lasting.
fn install_action(device: DeviceId, version: &str, allow_downgrade: bool) -> UiAction {
    DevicesOp::action_for(Action::Update {
        device,
        intent: UpdateIntentFacts::Install {
            version: version.to_string(),
            allow_downgrade,
        },
    })
}

/// `action`, disabled with `blocked` when the link cannot carry an update.
fn gate(action: UiAction, blocked: Option<&str>) -> UiAction {
    match blocked {
        Some(reason) => action.disabled(reason),
        None => action,
    }
}

#[cfg(test)]
mod tests {
    use lpa_devices::UpdateStageFacts;
    use lpa_devices::view::FIRMWARE_NEEDS_USB;

    use super::super::device_update_fixtures::{UpdateFixture, UpdateFixtureRow};
    use super::super::device_update_standing::tests::{ready_view, updating_view};
    use super::*;
    use crate::{ActionConsequence, ActionEnablement};

    fn x() -> UpdateVersion {
        UpdateVersion::with_build_id("2026.10.03-1", "2026.10.03-1+a41c9e2d11f0")
    }
    fn y() -> UpdateVersion {
        UpdateVersion::with_build_id("2026.10.05-2", "2026.10.05-2+626a1b851aaa")
    }

    fn prefix() -> OfferPath {
        OfferPath::board(&crate::BoardRef::Mac(
            lpa_devices::BoardKey::parse("60:55:f9:0a:0b:0c").unwrap(),
        ))
    }

    fn over_the_air(standing: UpdateStanding) -> UpdateOfferFacts {
        UpdateOfferFacts {
            standing,
            route: UpdateRoute::OverTheAir,
            ..UpdateOfferFacts::default()
        }
    }

    /// Each offer's path verb and whether it arms.
    fn set_of(view: &DeviceView, facts: &UpdateOfferFacts) -> (Vec<(String, bool)>, bool, bool) {
        let set = update_offers(view, facts, &prefix());
        let offers = set
            .offers
            .iter()
            .map(|offer| {
                let path = offer.path.to_string();
                assert!(path.starts_with("devices/mac-6055f90a0b0c/"), "{path}");
                (
                    offer.path.last().unwrap().to_string(),
                    offer.consequence().arms(),
                )
            })
            .collect();
        (offers, set.keep_flash, set.keep_erase)
    }

    fn bound_intent(offer: &UiOffer, args: &OfferArgs) -> UpdateIntentFacts {
        let action = offer.press(args).expect("press");
        match action.op_as::<DevicesOp>().unwrap().action() {
            Action::Update { intent, .. } => intent.clone(),
            other => panic!("{other:?}"),
        }
    }

    /// The director's note on P8, read end to end: a board that can update
    /// over the air, and this Studio with or without an update-capable build
    /// of its own, over USB and over Bluetooth.
    #[test]
    fn the_route_reads_the_board_and_this_studios_build() {
        use super::super::device_update_standing::tests::{board_x, facts_of, inputs, studio_y};
        let view = ready_view();
        let facts = facts_of(&board_x());
        let y = studio_y();

        // Both can, over USB: Update, over the air.
        let read = UpdateOfferFacts::read(&inputs(&view, Some(&facts), Some(&y)), true);
        assert_eq!(read.route, UpdateRoute::OverTheAir);
        assert!(matches!(read.standing, UpdateStanding::Available { .. }));
        assert_eq!(
            set_of(&view, &read),
            (vec![("update-firmware".to_string(), false)], false, true)
        );

        // A single-image Studio over USB: today's card, today's flash.
        let read = UpdateOfferFacts::read(&inputs(&view, Some(&facts), None), true);
        assert_eq!(read.route, UpdateRoute::Flash);
        assert_eq!(read.standing, UpdateStanding::Nothing);
        assert_eq!(set_of(&view, &read), (vec![], true, true));

        // The same over Bluetooth: nothing to install, and the line says why.
        let mut ble = inputs(&view, Some(&facts), None);
        ble.link = UpdateLink::Bluetooth;
        let read = UpdateOfferFacts::read(&ble, true);
        assert_eq!(read.route, UpdateRoute::NoWirelessBuild);
        assert!(
            matches!(&read.standing, UpdateStanding::NoWirelessBuild { board } if board.version == "2026.10.03-1"),
            "{:?}",
            read.standing
        );
        assert_eq!(set_of(&view, &read), (vec![], false, true));
        let words = crate::update_words(&read.standing).expect("words");
        assert_eq!(words.line, "Can't update over Bluetooth from this Studio");

        // Both can over Bluetooth: over the air there too.
        let mut ble = inputs(&view, Some(&facts), Some(&y));
        ble.link = UpdateLink::Bluetooth;
        assert_eq!(
            UpdateOfferFacts::read(&ble, true).route,
            UpdateRoute::OverTheAir
        );
    }

    #[test]
    fn up_to_date_offers_no_update_and_keeps_factory_reset() {
        let facts = over_the_air(UpdateStanding::UpToDate { version: y() });
        assert_eq!(set_of(&ready_view(), &facts), (vec![], false, true));
    }

    #[test]
    fn update_available_is_one_routine_update() {
        let facts = over_the_air(UpdateStanding::Available {
            board: x(),
            to: y(),
        });
        assert_eq!(
            set_of(&ready_view(), &facts),
            (vec![("update-firmware".to_string(), false)], false, true)
        );
        let set = update_offers(&ready_view(), &facts, &prefix());
        let update = &set.offers[0];
        assert_eq!(update.label(), "Update");
        assert!(update.consequence().is_routine(), "the agent presses it");
        assert_eq!(
            bound_intent(update, &OfferArgs::new()),
            UpdateIntentFacts::Install {
                version: "2026.10.05-2".to_string(),
                allow_downgrade: false
            }
        );
    }

    #[test]
    fn a_dev_board_is_offered_install_y() {
        let facts = over_the_air(UpdateStanding::Available {
            board: UpdateVersion::new("5eb70a7c2"),
            to: y(),
        });
        let set = update_offers(&ready_view(), &facts, &prefix());
        assert_eq!(set.offers[0].label(), "Install 2026.10.05-2");
        assert!(set.offers[0].params().is_empty(), "Y alone: one press");
        let facts = over_the_air(UpdateStanding::Available {
            board: x(),
            to: UpdateVersion::new("626a1b851"),
        });
        let set = update_offers(&ready_view(), &facts, &prefix());
        assert_eq!(set.offers[0].label(), "Install dev 626a1b8");
    }

    #[test]
    fn the_update_waits_for_a_link_that_carries_it() {
        let mut view = ready_view();
        view.update_blocked = Some(FIRMWARE_NEEDS_USB.to_string());
        let facts = over_the_air(UpdateStanding::Available {
            board: x(),
            to: y(),
        });
        let set = update_offers(&view, &facts, &prefix());
        assert_eq!(
            set.offers[0].action.meta().enablement,
            ActionEnablement::Disabled {
                reason: FIRMWARE_NEEDS_USB.to_string()
            }
        );
    }

    #[test]
    fn backing_up_offers_nothing_here_the_activitys_cancel_is_the_way_out() {
        let view = updating_view(Some(UpdateStageFacts::BackingUp), Some(18));
        assert!(view.escapes.contains(&Escape::Cancel), "the card's cancel");
        let facts = over_the_air(UpdateStanding::BackingUp {
            board: x(),
            to: y(),
            percent: Some(18),
        });
        assert_eq!(set_of(&view, &facts), (vec![], false, false));
    }

    #[test]
    fn while_firmware_is_written_nothing_is_offered() {
        let view = updating_view(Some(UpdateStageFacts::Updating), Some(40));
        assert!(
            !view.escapes.contains(&Escape::Cancel),
            "no Cancel once writing"
        );
        for standing in [
            UpdateStanding::Updating {
                board: x(),
                to: y(),
                link: UpdateLink::Bluetooth,
                percent: Some(40),
            },
            UpdateStanding::Finishing {
                board: x(),
                to: y(),
                percent: Some(70),
                running: true,
                resumed: false,
            },
            UpdateStanding::Restoring {
                board: x(),
                percent: Some(35),
                running: true,
            },
        ] {
            assert_eq!(
                set_of(&view, &over_the_air(standing)),
                (vec![], false, false)
            );
        }
        // Heal and finish about to start themselves, and another device's
        // update: still nothing (DS4), and no Factory reset under them.
        for standing in [
            UpdateStanding::Restoring {
                board: x(),
                percent: None,
                running: false,
            },
            UpdateStanding::Finishing {
                board: x(),
                to: y(),
                percent: Some(70),
                running: false,
                resumed: true,
            },
            UpdateStanding::AnotherDevice {
                board: x(),
                to: Some(y()),
                percent: Some(40),
            },
        ] {
            assert_eq!(
                set_of(&ready_view(), &over_the_air(standing)),
                (vec![], false, false)
            );
        }
    }

    #[test]
    fn needs_usb_once_keeps_the_flash_over_usb_and_offers_nothing_over_bluetooth() {
        for route in [UpdateRoute::OverTheAir, UpdateRoute::Flash] {
            let usb = UpdateOfferFacts {
                standing: UpdateStanding::NeedsUsbOnce {
                    board: x(),
                    to: y(),
                    link: UpdateLink::Usb,
                },
                route,
                ..UpdateOfferFacts::default()
            };
            assert_eq!(set_of(&ready_view(), &usb), (vec![], true, true));
            let ble = UpdateOfferFacts {
                standing: UpdateStanding::NeedsUsbOnce {
                    board: x(),
                    to: y(),
                    link: UpdateLink::Bluetooth,
                },
                route,
                ..UpdateOfferFacts::default()
            };
            assert_eq!(set_of(&ready_view(), &ble), (vec![], false, true));
        }
    }

    /// "Other version…" is offered on every idle row a person could want
    /// another version from, and nowhere an update runs, a play-only user
    /// looks, or the board needs USB first.
    #[test]
    fn other_version_is_offered_in_exactly_the_idle_rows() {
        use crate::app::devices::device_update_fixtures::{UpdateFixture, UpdateFixtureRow as Row};
        for row in Row::ALL {
            let fixture = UpdateFixture::new(row, ready_view());
            let set = update_offers(&fixture.view, &fixture.offer_facts(), &prefix());
            let install = set
                .offers
                .iter()
                .find(|o| o.path.last() == Some("install-firmware"));
            let expected = match row {
                Row::UpToDate
                | Row::Available
                | Row::AvailableDevBoard
                | Row::KeepsCrashing
                | Row::RolledBack
                | Row::Newer => Some("Other version…"),
                Row::CantGetVersion => Some("Install 2026.10.05-2"),
                Row::BackingUp
                | Row::Updating
                | Row::Finishing
                | Row::FinishingResumed
                | Row::Restoring
                | Row::AnotherDevice
                | Row::NeedsUsbOnce
                | Row::PlayOnly => None,
            };
            assert_eq!(install.map(UiOffer::label), expected, "{row:?}");
            if let Some(install) = install {
                assert!(!install.params().is_empty(), "{row:?}: a list to pick from");
            }
        }

        // A link that cannot carry the update draws it disabled, with why.
        let mut fixture = UpdateFixture::new(Row::UpToDate, ready_view());
        fixture.view.update_blocked = Some(FIRMWARE_NEEDS_USB.to_string());
        let set = update_offers(&fixture.view, &fixture.offer_facts(), &prefix());
        assert_eq!(
            set.offers[0].action.meta().enablement,
            ActionEnablement::Disabled {
                reason: FIRMWARE_NEEDS_USB.to_string()
            }
        );
    }

    #[test]
    fn the_list_is_newest_first_with_the_boards_own_disabled_and_the_rest_behind_the_box() {
        let other = install_of(&fixture(UpdateFixtureRow::UpToDate));
        let (options, preselect) = choice_param(&other);
        let values: Vec<&str> = options.iter().map(|o| o.value.as_str()).collect();
        assert_eq!(values.len(), 14);
        assert_eq!(
            &values[..3],
            ["2026.10.07-4", "2026.10.07-3", "2026.10.07-2"]
        );
        assert_eq!(values[13], "2026.10.02-1");
        assert_eq!(preselect.as_deref(), Some("2026.10.07-4"), "the newest");

        let y = &options[4];
        assert_eq!(y.value, "2026.10.05-2");
        assert_eq!(y.disabled.as_deref(), Some("On this board now"));
        assert_eq!(
            y.detail.as_deref(),
            Some("this Studio's build · Oct 5, 21:40 UTC")
        );
        assert_eq!(options[0].detail.as_deref(), Some("Oct 7, 16:28 UTC"));
        assert_eq!(options[9].detail.as_deref(), Some("Oct 3, 23:05 UTC"));
        assert_eq!(
            options[9].warning.as_deref(),
            Some("older language than Studio")
        );
        assert_eq!(options[0].warning, None);

        // The newest five and the board's own show; the box finds the rest.
        let widened: Vec<bool> = options.iter().map(|o| o.only_with.is_some()).collect();
        assert_eq!(widened.iter().filter(|w| **w).count(), 9, "{widened:?}");
        assert!(!widened[4], "the board's own, among the five");
        assert!(widened[5..].iter().all(|w| *w), "the rest");
        assert_eq!(options[5].only_with.as_deref(), Some(INSTALL_FIND_PARAM));
        let names: Vec<&str> = other.params().iter().map(|p| p.name.as_str()).collect();
        assert_eq!(
            names,
            [INSTALL_FIND_PARAM, INSTALL_VERSION_PARAM],
            "the box first"
        );
        let version = &other.params()[1];
        assert_eq!(version.filter.as_deref(), Some(INSTALL_FIND_PARAM));
        assert!(version.note.is_none(), "the full list is known");
        let shown = |find: &str| -> Vec<String> {
            other
                .shown_options(version, &OfferArgs::new().with(INSTALL_FIND_PARAM, find))
                .iter()
                .map(|o| o.value.clone())
                .collect()
        };
        assert_eq!(shown("").len(), 5, "the newest five");
        assert_eq!(
            shown("10.03"),
            [
                "2026.10.03-4",
                "2026.10.03-3",
                "2026.10.03-2",
                "2026.10.03-1"
            ],
            "typing searches the whole list, newest first"
        );
        assert_eq!(
            shown("10.05"),
            ["2026.10.05-2", "2026.10.05-1"],
            "the board's own too"
        );
        assert_eq!(other.label(), "Other version…", "the chip");
        assert_eq!(other.action.meta().label, "Other version…");
    }

    /// The box: a whole version in the list picks it; one the list does not
    /// hold binds a look-up, then where the look-up stands, then — found —
    /// the version itself.
    #[test]
    fn a_typed_version_picks_from_the_list_or_is_looked_up() {
        let other = install_of(&fixture(UpdateFixtureRow::UpToDate));
        let typed = |find: &str| OfferArgs::new().with(INSTALL_FIND_PARAM, find);

        // In the list, behind the box: picked, and it arms (older).
        let action = other.press(&typed(" 2026.10.03-2 ")).expect("found");
        assert_eq!(action.meta().label, INSTALL_PRESS_LABEL);
        assert!(action.meta().consequence.arms());
        assert_eq!(
            bound_intent(&other, &typed("2026.10.03-2")),
            UpdateIntentFacts::Install {
                version: "2026.10.03-2".to_string(),
                allow_downgrade: true
            }
        );
        // The board's own: refused with its reason.
        assert!(matches!(
            other.press(&typed("2026.10.05-2")),
            Err(OfferArgError::OptionDisabled { reason, .. }) if reason == "On this board now"
        ));
        // Part of a version: pick from what it finds; nothing: say so.
        let refusal = other.press(&typed("10.03")).unwrap_err().to_string();
        assert!(refusal.contains("choose a version"), "{refusal}");
        let refusal = other.press(&typed("nope")).unwrap_err().to_string();
        assert!(refusal.contains("no version here matches"), "{refusal}");

        // Older than the list: a Routine look-up of it.
        let lookup = other.press(&typed("2026.09.30-2")).expect("a look-up");
        assert_eq!(lookup.meta().label, "Look up 2026.09.30-2");
        assert!(
            lookup.meta().consequence.is_routine(),
            "the agent presses it"
        );
        assert_eq!(
            lookup.op_as::<FirmwareLookupOp>(),
            Some(&FirmwareLookupOp {
                target: "esp32c6-4mb".to_string(),
                version: "2026.09.30-2".to_string()
            })
        );

        // Asked: the press waits; not there: says so; offline: ask again.
        let looking =
            fixture(UpdateFixtureRow::UpToDate).looked_up("2026.09.30-2", StoreLookup::Looking);
        let refusal = install_of(&looking)
            .press(&typed("2026.09.30-2"))
            .unwrap_err()
            .to_string();
        assert!(refusal.contains("Looking up 2026.09.30-2"), "{refusal}");
        let missing =
            fixture(UpdateFixtureRow::UpToDate).looked_up("2026.09.30-2", StoreLookup::Missing);
        let refusal = install_of(&missing)
            .press(&typed("2026.09.30-2"))
            .unwrap_err()
            .to_string();
        assert!(refusal.contains("has no 2026.09.30-2"), "{refusal}");
        let offline =
            fixture(UpdateFixtureRow::UpToDate).looked_up("2026.09.30-2", StoreLookup::Offline);
        assert!(
            install_of(&offline)
                .press(&typed("2026.09.30-2"))
                .unwrap()
                .op_as::<FirmwareLookupOp>()
                .is_some()
        );

        // Found: a choice the box finds, at its own level, with the copy.
        let found = fixture(UpdateFixtureRow::UpToDate).looked_up(
            "2026.09.30-2",
            super::super::device_update_fixtures::looked_up_release("2026.09.30-2"),
        );
        let other = install_of(&found);
        let version = &other.params()[1];
        assert!(
            other
                .shown_options(version, &OfferArgs::new())
                .iter()
                .all(|o| o.value != "2026.09.30-2"),
            "not among the recent"
        );
        let action = other.press(&typed("2026.09.30-2")).expect("installs");
        let copy = lasting(&action);
        assert_eq!(copy.title, "Install an older version?");
        assert!(copy.message.contains("older language"), "{}", copy.message);
        assert_eq!(
            bound_intent(&other, &typed("2026.09.30-2")),
            UpdateIntentFacts::Install {
                version: "2026.09.30-2".to_string(),
                allow_downgrade: true
            }
        );
    }

    /// Routine only when known newer; an older version arms with the copy
    /// that says what changes, and asks `decide()` to allow it.
    #[test]
    fn each_choice_binds_at_its_own_level() {
        let other = install_of(&fixture(UpdateFixtureRow::UpToDate));

        let newer = press(&other, "2026.10.07-4");
        assert!(
            newer.meta().consequence.is_routine(),
            "the agent presses it"
        );
        assert_eq!(newer.meta().label, INSTALL_PRESS_LABEL);
        assert_eq!(
            bound_intent(&other, &args("2026.10.07-4")),
            UpdateIntentFacts::Install {
                version: "2026.10.07-4".to_string(),
                allow_downgrade: false
            }
        );

        let older = press(&other, "2026.10.05-1");
        let copy = lasting(&older);
        assert_eq!(copy.title, "Install an older version?");
        assert_eq!(
            copy.message,
            "2026.10.05-1 is older than what this board runs, and an older version may not \
             read the board's project."
        );
        assert_eq!(copy.confirm_label, "install");
        assert_eq!(
            bound_intent(&other, &args("2026.10.05-1")),
            UpdateIntentFacts::Install {
                version: "2026.10.05-1".to_string(),
                allow_downgrade: true
            }
        );

        // An older version in an older language: both sentences.
        let copy = lasting(&press(&other, "2026.10.03-4"));
        assert!(
            copy.message.starts_with("2026.10.03-4 is older"),
            "{}",
            copy.message
        );
        assert!(
            copy.message.ends_with(
                "It also speaks an older language than this Studio: Studio can still update \
                 it, but may not edit its project until you do."
            ),
            "{}",
            copy.message
        );

        // Behind the box: refused until the box finds it, taken then.
        let picked = OfferArgs::new().with(INSTALL_VERSION_PARAM, "2026.10.03-3");
        assert!(other.press(&picked).is_err());
        let found = picked.with(INSTALL_FIND_PARAM, "10.03");
        assert!(other.press(&found).unwrap().meta().consequence.arms());

        // The board's own is not a choice to press.
        assert!(other.press(&args("2026.10.05-2")).is_err());
    }

    /// "From a file…" sits beside the list, for the user's own click; the
    /// build it reads leads the list, picked, and always arms with copy
    /// that says it is a custom build — even one named like a newer release.
    #[test]
    fn a_build_from_files_leads_the_list_and_always_arms() {
        let plain = fixture(UpdateFixtureRow::UpToDate);
        let set = update_offers(&plain.view, &plain.offer_facts(), &prefix());
        let file = set
            .offers
            .iter()
            .find(|o| o.path.last() == Some("install-firmware-file"))
            .expect("From a file… beside the list");
        assert_eq!(file.label(), "From a file…");
        assert!(file.action.meta().needs_user_activation, "the user's click");
        assert!(
            file.consequence().is_routine(),
            "picking files changes nothing"
        );

        let picked = fixture(UpdateFixtureRow::UpToDate).with_file_build("9c1e4b7a2");
        let other = install_of(&picked);
        let (options, preselect) = choice_param(&other);
        assert_eq!(options[0].value, "9c1e4b7a2");
        assert_eq!(options[0].detail.as_deref(), Some("from your files"));
        assert_eq!(preselect.as_deref(), Some("9c1e4b7a2"), "just picked");
        let copy = lasting(&press(&other, "9c1e4b7a2"));
        assert_eq!(copy.title, "Install a custom build?");
        assert!(
            copy.message
                .starts_with("dev build 9c1e4b7 is a custom build from files on this computer"),
            "{}",
            copy.message
        );
        assert_eq!(
            bound_intent(&other, &args("9c1e4b7a2")),
            UpdateIntentFacts::Install {
                version: "9c1e4b7a2".to_string(),
                allow_downgrade: true
            }
        );

        // Named like a newer release, and standing in for the store's: still
        // a custom build.
        let named = fixture(UpdateFixtureRow::UpToDate).with_file_build("2026.10.07-4");
        let other = install_of(&named);
        let (options, _) = choice_param(&other);
        assert_eq!(
            options.iter().filter(|o| o.value == "2026.10.07-4").count(),
            1
        );
        assert_eq!(
            lasting(&press(&other, "2026.10.07-4")).title,
            "Install a custom build?"
        );
    }

    /// A release on a dev board (or a dev build on a released one) does not
    /// order: it arms, and is not a downgrade.
    #[test]
    fn a_choice_that_does_not_compare_with_the_board_arms() {
        let other = install_of(&fixture(UpdateFixtureRow::AvailableDevBoard));
        let copy = lasting(&press(&other, "2026.10.07-4"));
        assert_eq!(copy.title, "Install another build?");
        assert_eq!(
            copy.message,
            "2026.10.07-4 can't be compared with what this board runs (dev build 5eb70a7), so \
             it may not read the board's project."
        );
        assert_eq!(
            bound_intent(&other, &args("2026.10.07-4")),
            UpdateIntentFacts::Install {
                version: "2026.10.07-4".to_string(),
                allow_downgrade: false
            }
        );
    }

    /// Over Bluetooth, a version from before Bluetooth updates says the
    /// board will need a cable afterwards; over USB nothing does.
    #[test]
    fn over_bluetooth_an_older_choice_says_a_cable_is_needed_after() {
        let mut ble = fixture(UpdateFixtureRow::UpToDate);
        ble.link = UpdateLink::Bluetooth;
        let other = install_of(&ble);
        let copy = lasting(&press(&other, "2026.10.05-1"));
        assert!(
            copy.message.ends_with(
                "Over Bluetooth, this version can't be updated again until it has been \
                 connected by USB once."
            ),
            "{}",
            copy.message
        );
        let (options, _) = choice_param(&other);
        let older = options.iter().find(|o| o.value == "2026.10.05-1").unwrap();
        assert_eq!(
            older.warning.as_deref(),
            Some("needs a USB cable after, over Bluetooth")
        );

        let usb = install_of(&fixture(UpdateFixtureRow::UpToDate));
        let copy = lasting(&press(&usb, "2026.10.05-1"));
        assert!(!copy.message.contains("Bluetooth"), "{}", copy.message);
    }

    #[test]
    fn keeps_crashing_offers_reinstall_and_other_version_and_withdraws_factory_reset() {
        let fixture = fixture(UpdateFixtureRow::KeepsCrashing);
        let facts = fixture.offer_facts();
        assert_eq!(
            set_of(&fixture.view, &facts),
            (
                vec![
                    ("reinstall-firmware".to_string(), false),
                    ("install-firmware".to_string(), false),
                    ("install-firmware-file".to_string(), false)
                ],
                false,
                false
            )
        );
        let set = update_offers(&fixture.view, &facts, &prefix());
        assert_eq!(set.offers[0].label(), "Reinstall");
        assert_eq!(
            bound_intent(&set.offers[0], &OfferArgs::new()),
            UpdateIntentFacts::Reinstall
        );
        let (options, _) = choice_param(&set.offers[1]);
        let x = options.iter().find(|o| o.value == "2026.10.03-1").unwrap();
        assert_eq!(
            x.disabled.as_deref(),
            Some("On this board now"),
            "Reinstall's"
        );
    }

    /// FD9: after a rollback the card offers another version, with the
    /// refused build drawn but not pickable.
    #[test]
    fn rolled_back_offers_other_version_with_the_refused_build_disabled() {
        let other = install_of(&fixture(UpdateFixtureRow::RolledBack));
        let (options, preselect) = choice_param(&other);
        let y = options.iter().find(|o| o.value == "2026.10.05-2").unwrap();
        assert_eq!(
            y.disabled.as_deref(),
            Some("This board refused it after it failed to start")
        );
        assert_eq!(preselect.as_deref(), Some("2026.10.07-4"));
        assert!(other.press(&args("2026.10.05-2")).is_err());
        assert!(
            press(&other, "2026.10.05-1")
                .meta()
                .consequence
                .is_routine()
        );
    }

    /// With no index, the list is this Studio's build (and the store's
    /// latest, when known), and it says the full list is not available.
    #[test]
    fn with_no_index_the_list_is_short_and_says_why() {
        let newer = fixture(UpdateFixtureRow::Newer).offline();
        let other = install_of(&newer);
        assert_eq!(other.label(), "Other version…");
        let (options, preselect) = choice_param(&other);
        let values: Vec<&str> = options.iter().map(|o| o.value.as_str()).collect();
        assert_eq!(values, ["2026.10.05-2"]);
        assert_eq!(preselect.as_deref(), Some("2026.10.05-2"));
        assert_eq!(
            other.params()[1].note.as_deref(),
            Some(INSTALL_LIST_UNAVAILABLE)
        );
        assert!(other.consequence().arms(), "Y is older than this board's");

        // Beside Update, a list of Update's version alone is not offered.
        let available = fixture(UpdateFixtureRow::Available).offline();
        assert_eq!(
            set_of(&available.view, &available.offer_facts()).0,
            vec![("update-firmware".to_string(), false)]
        );
    }

    #[test]
    fn a_version_studio_cant_get_offers_install_y_and_withdraws_factory_reset() {
        // With only this Studio's build to get: one press.
        let offline = fixture(UpdateFixtureRow::CantGetVersion).offline();
        let facts = offline.offer_facts();
        assert_eq!(
            set_of(&offline.view, &facts),
            (vec![("install-firmware".to_string(), false)], false, false)
        );
        let set = update_offers(&offline.view, &facts, &prefix());
        assert_eq!(set.offers[0].label(), "Install 2026.10.05-2");
        assert!(set.offers[0].params().is_empty(), "Y alone: one press");
        assert_eq!(
            bound_intent(&set.offers[0], &OfferArgs::new()),
            UpdateIntentFacts::Install {
                version: "2026.10.05-2".to_string(),
                allow_downgrade: false
            }
        );

        // With the index: the same button opens the list, Y preselected.
        let install = install_of(&fixture(UpdateFixtureRow::CantGetVersion));
        assert_eq!(install.label(), "Install 2026.10.05-2");
        assert_eq!(choice_param(&install).1.as_deref(), Some("2026.10.05-2"));
    }

    #[test]
    fn play_only_offers_nothing_even_with_versions_to_pick() {
        let fixture = fixture(UpdateFixtureRow::PlayOnly);
        let facts = fixture.offer_facts();
        assert!(!facts.choices.is_empty());
        assert_eq!(set_of(&fixture.view, &facts), (vec![], false, true));
    }

    /// The flash route (a link without the update channel): today's verbs,
    /// words aside.
    #[test]
    fn on_the_flash_route_the_card_keeps_todays_verbs() {
        let facts = UpdateOfferFacts {
            standing: UpdateStanding::Available {
                board: x(),
                to: y(),
            },
            route: UpdateRoute::Flash,
            ..UpdateOfferFacts::default()
        };
        assert_eq!(set_of(&ready_view(), &facts), (vec![], true, true));
        assert_eq!(
            set_of(&ready_view(), &UpdateOfferFacts::default()),
            (vec![], true, true)
        );
    }

    // ---- Helpers ---------------------------------------------------------

    fn fixture(row: UpdateFixtureRow) -> UpdateFixture {
        UpdateFixture::new(row, ready_view())
    }

    /// The fixture's `install-firmware` offer.
    fn install_of(fixture: &UpdateFixture) -> UiOffer {
        update_offers(&fixture.view, &fixture.offer_facts(), &prefix())
            .offers
            .into_iter()
            .find(|o| o.path.last() == Some("install-firmware"))
            .expect("install-firmware offered")
    }

    fn choice_param(offer: &UiOffer) -> (Vec<OfferChoice>, Option<String>) {
        let version = offer
            .params()
            .iter()
            .find(|p| p.name == INSTALL_VERSION_PARAM)
            .expect("a version choice");
        let crate::OfferParamKind::Choice { options, preselect } = &version.kind else {
            panic!("{:?}", offer.params());
        };
        (options.clone(), preselect.clone())
    }

    /// `version` picked, with the box finding it (as a person typing it
    /// and clicking it would press).
    fn args(version: &str) -> OfferArgs {
        OfferArgs::new()
            .with(INSTALL_VERSION_PARAM, version)
            .with(INSTALL_FIND_PARAM, version)
    }

    fn press(offer: &UiOffer, version: &str) -> UiAction {
        offer.press(&args(version)).expect("pressable")
    }

    fn lasting(action: &UiAction) -> ActionConfirmation {
        match &action.meta().consequence {
            ActionConsequence::Lasting(copy) => copy.clone(),
            other => panic!("not Lasting: {other:?}"),
        }
    }
}
