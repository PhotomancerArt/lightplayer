//! The over-the-air update's verbs, as offers under `devices/<board>/…`,
//! one set per [`UpdateStanding`] row (the update-states spike's offers
//! table, as ruled: an over-the-air update to a newer version is Routine).
//!
//! | Row | Path verb | Button | Level |
//! |---|---|---|---|
//! | Update available | `update-firmware` | `Update` (another build: `Install Y`) | Routine |
//! | Up to date, Update available, Newer, Rolled back, Keeps crashing | `install-firmware` | `Other version…` (a `version` choice; `all_versions` widens it) | per choice: Routine only when known newer |
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
//! release index for its target, this Studio's own build, and the store's
//! `latest` when the index is missing): newest first, the board's own and a
//! build it refused drawn disabled, the newest [`RECENT_CHOICES`] shown and
//! the rest behind `all_versions`. It is offered when at least one choice
//! can be picked, and always shows its list — never a blind one-press
//! install. Each choice binds at its own level: **Routine** only when it is
//! known newer than the board's, speaks no older wire language than this
//! Studio and keeps Bluetooth updates; anything else is **Lasting**, with
//! copy that says what changes. `allow_downgrade` is set exactly when the
//! choice is older than the board's (what `decide()` needs).
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
use super::install_choice::{InstallChoice, InstallChoiceInputs, index_for, install_choices};
use crate::{
    ActionConfirmation, OfferArgError, OfferArgs, OfferBinder, OfferChoice, OfferParam, OfferPath,
    UiAction, UiOffer,
};

/// The install verb's version parameter.
pub const INSTALL_VERSION_PARAM: &str = "version";

/// The install verb's switch that widens its list to every version.
pub const INSTALL_ALL_VERSIONS_PARAM: &str = "all_versions";

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
            // Beside Update, the list must hold more than Update's version.
            let more = facts
                .choices
                .iter()
                .any(|c| c.is_pickable() && c.version.version != to.version);
            more.then(|| other_version(board))
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
        set.offers.push(offer);
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
    label: String,
    /// The version picked for the user; else the newest one that can be
    /// picked.
    preselect: Option<String>,
    /// With a single version to pick, the button installs it at one press
    /// ("Install Y"). "Other version…" always shows its list.
    one_press: bool,
}

impl InstallOffer {
    /// The offer: one `version` choice — every choice, the board's own and
    /// a refused build disabled — with `all_versions` when some are not
    /// recent, bound to an install of the chosen one at its own level.
    /// `None` when no version can be picked.
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
        let widened = self.choices.iter().any(|c| !c.recent);
        let Self {
            device,
            board,
            choices,
            listed,
            label,
            one_press,
            ..
        } = self;
        let choices = Rc::new(choices);
        let bind = move |version: &str| -> Option<UiAction> {
            let choice = choices.iter().find(|c| c.version.version == version)?;
            Some(bind_choice(device, &board, choice).with_label(label.clone()))
        };
        let template = bind(&preselect)?;
        if let Some(reason) = blocked {
            return Some(UiOffer::new(path, "download", template.disabled(reason)));
        }
        if one_press && single {
            return Some(UiOffer::new(path, "download", template));
        }
        let mut version =
            OfferParam::choice(INSTALL_VERSION_PARAM, "version", options, Some(preselect));
        if !listed {
            version = version.with_note(INSTALL_LIST_UNAVAILABLE);
        }
        let mut params = vec![version];
        if widened {
            params.push(OfferParam::toggle(
                INSTALL_ALL_VERSIONS_PARAM,
                "All versions",
                false,
            ));
        }
        Some(UiOffer::with_params(
            path,
            "download",
            params,
            OfferBinder::new(move |args: &OfferArgs| {
                let version = args.choice(INSTALL_VERSION_PARAM).unwrap_or_default();
                bind(version).ok_or_else(|| OfferArgError::Invalid {
                    name: INSTALL_VERSION_PARAM.to_string(),
                    reason: format!("{version} is not a version this Studio can get"),
                })
            }),
            template,
        ))
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
        option = option.only_with(INSTALL_ALL_VERSIONS_PARAM);
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
    let action = install_action(device, &choice.version.version, choice.is_older());
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
    if choice.is_known_newer() && !older_wire && !choice.needs_usb_after {
        return None;
    }
    let (title, mut message) = if choice.is_older() {
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
    fn the_list_is_newest_first_with_the_boards_own_disabled_and_the_rest_behind_a_switch() {
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

        let widened: Vec<bool> = options.iter().map(|o| o.only_with.is_some()).collect();
        assert_eq!(widened.iter().filter(|w| **w).count(), 4, "{widened:?}");
        assert!(widened[10..].iter().all(|w| *w), "the oldest four");
        assert_eq!(
            options[10].only_with.as_deref(),
            Some(INSTALL_ALL_VERSIONS_PARAM)
        );
        assert_eq!(other.params()[1].name, INSTALL_ALL_VERSIONS_PARAM);
        assert!(other.params()[0].note.is_none(), "the full list is known");
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
        assert_eq!(newer.meta().label, "Other version…");
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

        // Behind the switch: refused with it off, taken with it on.
        assert!(other.press(&args("2026.10.03-3")).is_err());
        let wide = args("2026.10.03-3").with(INSTALL_ALL_VERSIONS_PARAM, "true");
        assert!(other.press(&wide).unwrap().meta().consequence.arms());

        // The board's own is not a choice to press.
        assert!(other.press(&args("2026.10.05-2")).is_err());
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
                    ("install-firmware".to_string(), false)
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
            other.params()[0].note.as_deref(),
            Some(INSTALL_LIST_UNAVAILABLE)
        );
        assert_eq!(other.params().len(), 1, "no switch: nothing is behind it");
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
        let crate::OfferParamKind::Choice { options, preselect } = &offer.params()[0].kind else {
            panic!("{:?}", offer.params());
        };
        (options.clone(), preselect.clone())
    }

    fn args(version: &str) -> OfferArgs {
        OfferArgs::new().with(INSTALL_VERSION_PARAM, version)
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
