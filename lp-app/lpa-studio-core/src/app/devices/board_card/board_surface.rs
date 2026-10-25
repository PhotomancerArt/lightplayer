//! The card's surface: which state it says, in what words, over what
//! picture (`docs/ux/anatomy/board-card.md`, "Surface"; the types are
//! [`super::ui_board_surface`]).
//!
//! The surface explains; the primary gives the thing to do
//! ([`super::primary_action`] presses the offer each state names). The
//! second sentence is the primary's button said aloud — its verb and
//! object, then what it gets you — and it is said only while that button
//! is the primary: a board the editor holds has Done there instead, a
//! button its link refuses gives its reason in that place ("Firmware
//! updates need USB."), and a fix that isn't offered here says why and
//! what to do instead ([`missing_fix`]: "Connect over USB or Bluetooth to
//! install it.", "Studio doesn't have firmware for this board."). No
//! dead ends (P7). The icon is the row's whose state it is, in the
//! state's tone; the sentences stay plain. Read off the same facts the rows
//! read ([`BoardCardInput`]), never off their words.
//!
//! In order, first match wins:
//!
//! | State | Icon · tone | Surface | Primary |
//! |---|---|---|---|
//! | Connecting | link · working | "Connecting to this board over USB." + the spinner | the spinner alone |
//! | Reconnecting | link · working | "Reconnecting to this board over USB." + the spinner | Done |
//! | A take-over under way | link · working | "Moving this board to this tab." + the spinner | the spinner alone |
//! | The layout question open (its flash waits for the answer) | firmware · attention | "This update is waiting for your confirmation." | — (the Firmware row's details rise with it) |
//! | Updating (over the air, another device's, its missing part put back) | firmware · working | "This board is updating its firmware." + its progress | — |
//! | Installing (a flash) | firmware · working | "Installing the firmware on this board." + its progress | — |
//! | Erasing | firmware · working | "Erasing this board." + its progress | — |
//! | Sending | project · working | "Sending the project “studio” to this board." + its progress | — |
//! | Removing | project · working | "Removing the project from this board." + its progress | — |
//! | Identifying | chip · working | "Identifying this board." + the spinner | — |
//! | A take-over that failed | link · error | "Studio couldn't move this board to this tab." "Connect here to try again." | Connect here |
//! | Taken by another tab | link · attention | "Another tab took this board." "Connect here to move it back to this tab." | Connect here |
//! | Busy in another tab | link · attention | "This board is busy in another tab." "Connect here when it's done." | — |
//! | Open in another tab | link · attention | "This board is open in another tab." "Connect here to move it to this tab." | Connect here |
//! | Someone else holds its network slot | link · attention | "Someone else is connected to this board." "Retry when they're done." | Retry |
//! | A Wi‑Fi connect failed | wifi · error | "Studio couldn't reach this board over Wi‑Fi." "Make sure this computer is on the same Wi‑Fi, then retry." | Retry |
//! | …through lightplayer.app | cloud · error | "Studio couldn't reach this board through lightplayer.app." "Retry to connect." | Retry |
//! | The last Connect failed | link · error | "Studio couldn't connect to this board over USB." "Retry to connect." | Retry |
//! | Not answering (a board that said hello went quiet) | link · attention | "This board stopped responding over USB." "Retry to reconnect." | Retry |
//! | A stand-in, off | play · neutral | "This simulated board is off." | Power on |
//! | No link here, and the account's list says it is online at lightplayer.app | — | its picture from lightplayer.app, no words | Connect |
//! | Offline, Wi‑Fi or lightplayer.app reaches it | wifi / cloud · neutral | "This board isn't plugged in." "Connect over Wi‑Fi to use it." | Connect |
//! | Offline | link · neutral | "This board is offline." "It was last seen 2 hours ago." | Connect by its cable, when offered |
//! | Its files wait for newer firmware | firmware · attention | "This board's files need newer firmware." "Update the firmware to read them." | Update firmware |
//! | Its file store refused | firmware · attention | "This board's firmware can't read its files." "The files are kept on the board." | — |
//! | Its files were lost | project · attention | "This board's files were lost." "Restore them from a backup, or reset the board." | Restore (else Reset) |
//! | A move of its files stopped half-way | firmware · attention | "This board's firmware update was interrupted." "Resume the update to finish it." | Resume update |
//! | A send, update, install, erase or removal failed, while the row offers it again | the row's · error | "Studio couldn't send the project to this board." "Retry to send it again." | Retry (else the board's own state says what's next) |
//! | Keeps crashing | firmware · error | "This board's firmware keeps crashing." "Reinstall the firmware to fix it." | Reinstall firmware |
//! | Needs a version Studio can't get | firmware · attention | "This board needs a firmware version Studio can't get." "Install the firmware to use it." | Install firmware |
//! | Only its core runs | firmware · attention | "Part of this board's firmware is missing." "Install the firmware to use it." | Install firmware |
//! | An older LightPlayer | firmware · attention | "This board is running old firmware." "Update the firmware to connect and edit." | Update firmware |
//! | From before the hello | firmware · attention | "This board is running old firmware." "Install the firmware to connect and edit." | Install firmware |
//! | No LightPlayer (blank, another firmware) | firmware · neutral | "This board doesn't have LightPlayer firmware." "Install the firmware to use it." | Install firmware |
//! | In download mode | firmware · neutral | "This board is waiting in download mode." "Install the firmware to use it." | Install firmware |
//! | Says nothing (it never said hello) | firmware · attention | "This board doesn't respond." "Install the firmware to use it." | Install firmware |
//! | Locked | lock · neutral | "This board is locked." "Unlock it to connect." | Unlock |
//! | Its port closed | link · neutral | "Studio isn't connected to this board." "Connect over USB to use it." | Connect |
//! | A shader error | project · error | "This board's project has an error." "Edit the project to fix it." | Edit project |
//! | Runs no project | project · neutral | "This board has no project yet." "Add a project to play on it." | Add a project |
//! | The editor holds it (its last frame) | edit · neutral | "This board is open in the editor." | Done |
//! | The session open, or the editor showing it | — | the live picture, no words | Done |
//! | Frames too large to show over the link | project · neutral | "This board's picture is too large to show over Bluetooth." | Connect |
//! | All well | — | the live picture, no words | Connect |
//!
//! A picture dims under any notice, fully covered (the cover's one dim;
//! a stale picture's own dim is not added to it). With no picture the
//! notice stands on the picture's dark ground. **The rows go quiet:** the
//! row whose problem the surface says ([`carried_layer`]) loses its
//! attention or error tone, and keeps its state word and its own button.

use lpa_devices::device::DeviceStatus;
use lpa_devices::view::{FirmwareFace, LoadedProject, PendingLinkView};
use lpa_devices::{ActivityKind, HoldLevel, UpdateBoardState};

use super::bar_work::{failed_kind, retry};
use super::board_card_input::{BoardCardInput, link_icon};
use super::board_connection::BoardConnection;
use super::board_picture::board_picture;
use super::link_session::{connect_layer, held_layer};
use super::project_bar::data_lost;
use super::ui_bar_work::BarWorkState;
use super::ui_board_picture::{PictureSource, UiBoardPicture};
use super::ui_board_surface::{
    SurfacePicture, SurfaceProgress, SurfaceState, UiBoardSurface, UiSurfaceNotice,
};
use super::ui_card_action::UiCardAction;
use super::ui_name_bar::UiPrimary;
use super::ui_stack_bar::{BarLayer, UiStackBar};
use crate::app::devices::board_plays::BoardPlays;
use crate::app::devices::device_update_route::UpdateLink;
use crate::app::devices::device_update_standing::UpdateStanding;
use crate::app::devices::device_update_words::UpdateRowKind;
use crate::app::devices::ui_link_kind::UiLinkKind;
use crate::core::data_kinds::age_spoken;
use crate::{ConnectReach, RuntimeOp, UiOffer, UiStatusKind};

/// The state the card's surface says: the table in the module docs, first
/// match wins. [`super::primary_action`] reads the same state, so the
/// surface and the button beside the name can never disagree.
pub(crate) fn surface_state(input: &BoardCardInput<'_>) -> SurfaceState {
    use SurfaceState::*;
    let view = input.view;
    let connect = input.wifi_connect;
    // A board that stopped answering says so, with its Retry, rather than
    // a spinner over it (its link row does the same).
    let answering = view.status != DeviceStatus::NotResponding;
    if answering
        && (*input.connection == BoardConnection::Connecting
            || connect.is_some_and(|connect| connect.connecting))
    {
        return Connecting;
    }
    if *input.connection == BoardConnection::Reconnecting {
        return Reconnecting;
    }
    let take_over = input.take_over.filter(|_| input.no_open_link());
    if take_over.is_some_and(|over| !over.failed) {
        return MovingHere;
    }
    // The layout question rides a flash that waits for its answer: the
    // flash is not working until it is answered.
    if input.layout.is_some_and(|layout| layout.panel.is_some()) {
        return ConfirmationRequired;
    }
    if let Some(state) = working(input) {
        return state;
    }
    if take_over.is_some_and(|over| over.failed) {
        return MoveFailed;
    }
    if let Some(held) = input.held() {
        return match (held.taken_from_here, &held.level) {
            (true, _) => TakenByAnotherTab,
            (false, HoldLevel::Busy(_)) => AnotherTabBusy,
            (false, _) => AnotherTab,
        };
    }
    if let Some(connect) = connect {
        if connect.busy {
            return SomeoneElse;
        }
        if connect.error.is_some() {
            return NetworkFailed;
        }
    }
    if let BoardConnection::Failed { .. } = input.connection {
        return ConnectFailed;
    }
    // Only a board that said hello stops responding: one that never did
    // and says nothing is the model's NeedsAttention, and its face (Silent)
    // asks for the firmware below (`Device::status`, 2026-10-18).
    if view.status == DeviceStatus::NotResponding {
        return NotAnswering;
    }
    // Well, and reached through lightplayer.app: its picture there.
    if input.online_at_cloud() {
        return OnlineAtCloud;
    }
    if input.offline() {
        return match (input.stand_in(), offline_road(input).map(|(road, _)| road)) {
            (true, _) => PoweredOff,
            (false, Some(ConnectReach::Wifi | ConnectReach::Relay)) => OfflineReachable,
            (false, _) => Offline,
        };
    }
    // The board's files first: each of these names its own fix, which a
    // bare "didn't finish" would hide.
    if let Some(layout) = input.layout {
        if layout.store_needs_newer_firmware {
            return UpdateRequired;
        }
        if layout.store_refused {
            return DataUnreadable;
        }
    }
    if data_lost(input) {
        return DataLost;
    }
    if input.offer("finish-update").is_some() {
        return UpdateInterrupted;
    }
    // Work that failed, while its row offers the verb again; once it no
    // longer does, the board's own state says what to do next (P7).
    if let Some(kind) = failed_kind(input).filter(|kind| retry(input, *kind).is_some()) {
        return match kind {
            ActivityKind::Push => SendFailed,
            ActivityKind::Update => UpdateFailed,
            ActivityKind::Flash => InstallFailed,
            ActivityKind::Erase => EraseFailed,
            ActivityKind::RemoveProject => RemoveFailed,
            // A failed identify is the board's own state now (not
            // answering, needs firmware): `failed_kind` never names it.
            ActivityKind::Identify => AllWell,
        };
    }
    if let Some(update) = input
        .update
        .filter(|update| update.kind == UpdateRowKind::NeedsYou)
    {
        match update.standing {
            UpdateStanding::KeepsCrashing { .. } => return Crashing,
            UpdateStanding::CantGetVersion { .. } => return CantGetVersion,
            _ => {}
        }
    }
    if let Some(state) = face_state(&view.firmware_face) {
        return state;
    }
    if input.locked() {
        return Locked;
    }
    if view.status == DeviceStatus::Attached {
        return Disconnected;
    }
    if view.degraded.is_some() {
        return ProjectError;
    }
    if runs_no_project(input) {
        return NoProject;
    }
    if input.editor_holds_it {
        let picture = board_picture(input);
        return match picture.source == PictureSource::Lens && picture.dim {
            true => InTheEditor,
            false => Connected,
        };
    }
    if picture_too_large(input) {
        return PictureTooLarge;
    }
    AllWell
}

/// The card's surface for `state`: its notice, the picture under it, and
/// the progress of the work it reports. `primary` is the name bar's, which
/// the second sentence says aloud; `action` is the offer `state` names
/// ([`super::primary_action::state_action`]), whose refusal is said in
/// that sentence's place.
pub(crate) fn board_surface(
    input: &BoardCardInput<'_>,
    state: SurfaceState,
    bars: &[UiStackBar],
    picture: &UiBoardPicture,
    primary: Option<&UiPrimary>,
    action: Option<&UiCardAction>,
) -> UiBoardSurface {
    let missing = action
        .is_none()
        .then(|| missing_fix(input, state))
        .flatten();
    let notice = words(input, state).map(|words| words.notice(primary, action, missing));
    UiBoardSurface {
        state,
        picture: under(notice.is_some(), picture),
        progress: progress(state, bars),
        notice,
    }
}

/// A new board's surface: identifying while it says who it is, else what
/// its firmware face asks for, said as a roster board's would be.
pub(crate) fn pending_surface(
    pending: &PendingLinkView,
    primary: Option<&UiPrimary>,
    action: Option<&UiCardAction>,
) -> UiBoardSurface {
    let state = face_state(&pending.firmware_face)
        .filter(|_| pending.needs_firmware())
        .unwrap_or(SurfaceState::Identifying);
    let words = match state {
        SurfaceState::Identifying => identifying(),
        state => firmware_words(state),
    };
    UiBoardSurface {
        state,
        picture: SurfacePicture::None,
        progress: (state == SurfaceState::Identifying).then_some(SurfaceProgress::Spinner),
        notice: Some(words.notice(primary, action, None)),
    }
}

/// The row whose problem the surface says, which goes quiet: it keeps its
/// state word and its button, and loses its attention or error tone (the
/// surface is the one loud place). A link's problem is the row of the link
/// it is about: a Wi‑Fi connect that failed quiets the Wi‑Fi row, a board
/// held on its network slot the row of that way in.
pub(crate) fn carried_layer(input: &BoardCardInput<'_>, state: SurfaceState) -> Option<BarLayer> {
    use SurfaceState::*;
    match state {
        NotAnswering | ConnectFailed => Some(BarLayer::of_link(input.link_kind())),
        NetworkFailed | SomeoneElse => input.wifi_connect.map(connect_layer),
        AnotherTab | AnotherTabBusy | TakenByAnotherTab | MoveFailed => Some(held_layer(input)),
        OldFirmware | PreHelloFirmware | NoFirmware | DownloadMode | Silent | Crashing
        | Incomplete | CantGetVersion | ConfirmationRequired | UpdateRequired | DataUnreadable
        | UpdateInterrupted | UpdateFailed | InstallFailed | EraseFailed => {
            Some(BarLayer::Firmware)
        }
        DataLost | ProjectError | SendFailed | RemoveFailed => Some(BarLayer::Project),
        AllWell | Connected | Connecting | Reconnecting | MovingHere | Updating | Installing
        | Erasing | Sending | Removing | Identifying | Offline | OfflineReachable
        | OnlineAtCloud | Disconnected | PoweredOff | Locked | NoProject | InTheEditor
        | PictureTooLarge => None,
    }
}

/// Whether the surface, in `state`, says the work of the row at `layer`
/// with its own bar or spinner: that row is carried and goes still
/// ([`super::ui_bar_work::UiBarWork::carried_by_surface`]), so one work
/// moves in one place (P2). A connect is the row of the link it takes.
pub(crate) fn carries_work(state: SurfaceState, layer: BarLayer) -> bool {
    use SurfaceState::*;
    match state {
        Updating | Installing | Erasing => layer == BarLayer::Firmware,
        Sending | Removing => layer == BarLayer::Project,
        Identifying => layer == BarLayer::Hardware,
        Connecting | Reconnecting | MovingHere => layer.is_link(),
        _ => false,
    }
}

/// How an offline board is reached, and the offer that reaches it: its
/// `connect` (Wi‑Fi first, then lightplayer.app, then its cable, as the
/// controller chose the road), else — a board with no registry row — the
/// road's own offer. `None` when nothing reaches it.
pub(crate) fn offline_road<'a>(input: &BoardCardInput<'a>) -> Option<(ConnectReach, &'a UiOffer)> {
    if let Some(connect) = input.offer("connect") {
        let reach = match connect.action.op_as::<RuntimeOp>() {
            Some(RuntimeOp::ConnectDevice {
                reach: Some(reach), ..
            }) => *reach,
            _ => ConnectReach::Usb,
        };
        return Some((reach, connect));
    }
    [
        ("connect-wifi", ConnectReach::Wifi),
        ("connect-relay", ConnectReach::Relay),
        ("reconnect", ConnectReach::Usb),
    ]
    .into_iter()
    .find_map(|(verb, reach)| input.offer(verb).map(|offer| (reach, offer)))
}

/// The link a road takes: Wi‑Fi, lightplayer.app, or the board's own.
pub(crate) fn road_link(input: &BoardCardInput<'_>, road: ConnectReach) -> UiLinkKind {
    match road {
        ConnectReach::Wifi => UiLinkKind::Wifi,
        ConnectReach::Relay => UiLinkKind::Relay,
        ConnectReach::Usb => input.link_kind(),
    }
}

/// The work the board is doing, as its state: its activity, an update
/// story in progress with no activity here (another device's, one about
/// to start, its missing part put back), or a core finishing an update.
fn working(input: &BoardCardInput<'_>) -> Option<SurfaceState> {
    if let Some(activity) = &input.view.activity {
        return Some(match activity.kind {
            ActivityKind::Update => SurfaceState::Updating,
            ActivityKind::Flash => SurfaceState::Installing,
            ActivityKind::Erase => SurfaceState::Erasing,
            ActivityKind::Push => SurfaceState::Sending,
            ActivityKind::RemoveProject => SurfaceState::Removing,
            ActivityKind::Identify => SurfaceState::Identifying,
        });
    }
    let story = input
        .update
        .is_some_and(|update| update.kind == UpdateRowKind::Progress && update.progress.is_some());
    let finishing = matches!(
        input.view.firmware_face,
        FirmwareFace::CoreOnly {
            state: UpdateBoardState::Updating | UpdateBoardState::OnTrial,
            ..
        }
    );
    (story || finishing).then_some(SurfaceState::Updating)
}

/// The state a firmware face asks for, when it asks for one: an older
/// LightPlayer, no LightPlayer at all, a core with its engine missing or
/// crashing. `None` for a LightPlayer (and while nothing is known).
fn face_state(face: &FirmwareFace) -> Option<SurfaceState> {
    Some(match face {
        FirmwareFace::CoreOnly {
            state: UpdateBoardState::EngineCrashing,
            ..
        } => SurfaceState::Crashing,
        FirmwareFace::CoreOnly {
            state: UpdateBoardState::Updating | UpdateBoardState::OnTrial,
            ..
        } => SurfaceState::Updating,
        FirmwareFace::CoreOnly { .. } => SurfaceState::Incomplete,
        FirmwareFace::OlderLightPlayer { .. } => SurfaceState::OldFirmware,
        FirmwareFace::NoHello => SurfaceState::PreHelloFirmware,
        FirmwareFace::Blank | FirmwareFace::Foreign { .. } => SurfaceState::NoFirmware,
        FirmwareFace::Bootloader => SurfaceState::DownloadMode,
        FirmwareFace::Silent => SurfaceState::Silent,
        FirmwareFace::Unknown | FirmwareFace::LightPlayer { .. } => return None,
    })
}

/// A ready, idle board that said it runs nothing: the NoProject surface,
/// whose primary is Add a project ([`super::primary_action::add_a_project`]
/// reads the same predicate, so the surface never says it with no button).
pub(crate) fn runs_no_project(input: &BoardCardInput<'_>) -> bool {
    let ready = matches!(
        input.view.status,
        DeviceStatus::Ready | DeviceStatus::Degraded
    );
    ready
        && input.idle()
        && (input.view.loaded_project == LoadedProject::Empty
            || *input.plays == BoardPlays::Nothing)
}

/// Frames flow, but without the geometry to draw them: the board declined
/// the display layout (over the link's read budget at this scale).
fn picture_too_large(input: &BoardCardInput<'_>) -> bool {
    input
        .feed
        .filter(|_| !input.locked())
        .and_then(|feed| feed.frame.as_ref())
        .is_some_and(|frame| frame.display_layout.is_none())
}

/// How the picture is drawn under a notice: dimmed when there is one to
/// draw (a frame, or the board's own update light), else nothing.
fn under(notice: bool, picture: &UiBoardPicture) -> SurfacePicture {
    match (notice, picture.frame.is_some() || picture.light.is_some()) {
        (false, _) => SurfacePicture::Live,
        (true, true) => SurfacePicture::Dimmed,
        (true, false) => SurfacePicture::None,
    }
}

/// The progress of the work `state` reports: the percent of the running
/// work on its row — the one number that row's words say and its foot
/// draws ([`super::bar_work::running_work`]) — else the spinner.
fn progress(state: SurfaceState, bars: &[UiStackBar]) -> Option<SurfaceProgress> {
    use SurfaceState::*;
    let layer = match state {
        Connecting | Reconnecting | MovingHere | Identifying => {
            return Some(SurfaceProgress::Spinner);
        }
        Updating | Installing | Erasing => BarLayer::Firmware,
        Sending | Removing => BarLayer::Project,
        _ => return None,
    };
    let percent = bars
        .iter()
        .find(|bar| bar.layer == layer)
        .and_then(|bar| bar.work.as_ref())
        .filter(|work| work.state == BarWorkState::Running)
        .and_then(|work| work.percent);
    Some(match percent {
        Some(percent) => SurfaceProgress::Percent(percent.min(100)),
        None => SurfaceProgress::Spinner,
    })
}

/// A notice before the primary decides its second sentence.
struct Words {
    icon: String,
    tone: UiStatusKind,
    first: String,
    second: Option<Second>,
}

/// The second sentence: the primary's button said aloud, said only while
/// that button is the primary; or a fact, said always.
enum Second {
    Button(String),
    Fact(String),
}

impl Words {
    fn new(icon: impl Into<String>, tone: UiStatusKind, first: impl Into<String>) -> Self {
        Self {
            icon: icon.into(),
            tone,
            first: first.into(),
            second: None,
        }
    }

    /// The button said aloud.
    fn then_press(mut self, sentence: impl Into<String>) -> Self {
        self.second = Some(Second::Button(sentence.into()));
        self
    }

    /// A fact worth a second sentence.
    fn then_fact(mut self, sentence: impl Into<String>) -> Self {
        self.second = Some(Second::Fact(sentence.into()));
        self
    }

    /// The notice, its second sentence decided (P7, no dead ends): a fact
    /// always; the button's words only while `action` is the primary; the
    /// refusal when the link refuses `action`; and `missing` — why the fix
    /// isn't offered here, and what to do instead — when there is no
    /// `action` to press.
    fn notice(
        self,
        primary: Option<&UiPrimary>,
        action: Option<&UiCardAction>,
        missing: Option<String>,
    ) -> UiSurfaceNotice {
        let pressed = |action: &UiCardAction| matches!(primary, Some(UiPrimary::Offer(shown)) if shown.offer == action.offer);
        let second = match (self.second, action) {
            (Some(Second::Fact(fact)), _) => Some(fact),
            (_, Some(action)) if action.refused.is_some() => {
                action.refused.as_deref().map(sentence)
            }
            (Some(Second::Button(said)), Some(action)) if pressed(action) => Some(said),
            (Some(Second::Button(_)), None) => missing,
            _ => None,
        };
        UiSurfaceNotice {
            icon: self.icon,
            tone: self.tone,
            first: self.first,
            second,
        }
    }
}

/// `reason` as a sentence: it ends with a full stop.
fn sentence(reason: &str) -> String {
    match reason.ends_with(['.', '!', '?']) {
        true => reason.to_string(),
        false => format!("{reason}."),
    }
}

/// How a sentence names a link: "over USB", "through lightplayer.app".
fn way(link: UiLinkKind) -> String {
    match link {
        UiLinkKind::Relay => format!("through {}", link.label()),
        link => format!("over {}", link.label()),
    }
}

/// What `state` says; `None` while the board is well.
fn words(input: &BoardCardInput<'_>, state: SurfaceState) -> Option<Words> {
    use SurfaceState::*;
    use UiStatusKind::{Attention, Error, Neutral, Working};
    let link = input.link_kind();
    let icon = link_icon(link);
    let connect_link = input
        .wifi_connect
        .map(|connect| match connect.through_relay {
            true => UiLinkKind::Relay,
            false => UiLinkKind::Wifi,
        });
    let held_icon = link_icon(input.held_link());
    Some(match state {
        AllWell | Connected | OnlineAtCloud => return None,
        Connecting => {
            let link = connect_link
                .filter(|_| input.wifi_connect.is_some_and(|c| c.connecting))
                .unwrap_or(link);
            Words::new(
                link_icon(link),
                Working,
                format!("Connecting to this board {}.", way(link)),
            )
        }
        Reconnecting => Words::new(
            icon,
            Working,
            format!("Reconnecting to this board {}.", way(link)),
        ),
        MovingHere => Words::new(held_icon, Working, "Moving this board to this tab."),
        Updating => Words::new("firmware", Working, updating_sentence(input)),
        Installing => Words::new(
            "firmware",
            Working,
            "Installing the firmware on this board.",
        ),
        Erasing => Words::new("firmware", Working, "Erasing this board."),
        Sending => Words::new(
            "project",
            Working,
            match input.sending {
                Some(name) => format!("Sending the project \u{201c}{name}\u{201d} to this board."),
                None => "Sending a project to this board.".to_string(),
            },
        ),
        Removing => Words::new("project", Working, "Removing the project from this board."),
        Identifying => identifying(),
        MoveFailed => Words::new(
            held_icon,
            Error,
            "Studio couldn't move this board to this tab.",
        )
        .then_press("Connect here to try again."),
        TakenByAnotherTab => Words::new(held_icon, Attention, "Another tab took this board.")
            .then_press("Connect here to move it back to this tab."),
        // Nothing to press until that tab's work ends (its take-over waits
        // for it): the next step, said as a fact.
        AnotherTabBusy => Words::new(held_icon, Attention, "This board is busy in another tab.")
            .then_fact("Connect here when it's done."),
        AnotherTab => Words::new(held_icon, Attention, "This board is open in another tab.")
            .then_press("Connect here to move it to this tab."),
        SomeoneElse => Words::new(
            link_icon(connect_link.unwrap_or(link)),
            Attention,
            "Someone else is connected to this board.",
        )
        .then_press("Retry when they're done."),
        NetworkFailed => {
            let link = connect_link.unwrap_or(UiLinkKind::Wifi);
            let words = Words::new(
                link_icon(link),
                Error,
                format!("Studio couldn't reach this board {}.", way(link)),
            );
            match link {
                UiLinkKind::Relay => words.then_press("Retry to connect."),
                _ => words.then_press(format!(
                    "Make sure this computer is on the same {}, then retry.",
                    UiLinkKind::Wifi.label()
                )),
            }
        }
        ConnectFailed => Words::new(
            icon,
            Error,
            format!("Studio couldn't connect to this board {}.", way(link)),
        )
        .then_press("Retry to connect."),
        NotAnswering => Words::new(
            icon,
            Attention,
            format!("This board stopped responding {}.", way(link)),
        )
        .then_press("Retry to reconnect."),
        PoweredOff => Words::new(
            "play",
            Neutral,
            format!(
                "This {} board is off.",
                input
                    .runtime
                    .map_or("simulated", |runtime| runtime.kind_word())
                    .to_lowercase()
            ),
        ),
        OfflineReachable => {
            let road = offline_road(input).map_or(link, |(road, _)| road_link(input, road));
            Words::new(link_icon(road), Neutral, "This board isn't plugged in.")
                .then_press(format!("Connect {} to use it.", way(road)))
        }
        Offline => {
            let words = Words::new(icon, Neutral, "This board is offline.");
            match input.last_seen_at {
                Some(seen) => words.then_fact(format!(
                    "It was last seen {}.",
                    age_spoken(input.now - seen)
                )),
                None => words,
            }
        }
        Disconnected => Words::new(icon, Neutral, "Studio isn't connected to this board.")
            .then_press(format!("Connect {} to use it.", way(link))),
        SendFailed => Words::new(
            "project",
            Error,
            "Studio couldn't send the project to this board.",
        )
        .then_press("Retry to send it again."),
        UpdateFailed => Words::new(
            "firmware",
            Error,
            "This board's firmware update didn't finish.",
        )
        .then_press("Retry to update it."),
        InstallFailed => Words::new(
            "firmware",
            Error,
            "Installing the firmware on this board didn't finish.",
        )
        .then_press("Retry to install it."),
        EraseFailed => Words::new("firmware", Error, "Erasing this board didn't finish.")
            .then_press("Retry to erase it."),
        RemoveFailed => Words::new(
            "project",
            Error,
            "Removing the project from this board didn't finish.",
        )
        .then_press("Retry to remove it."),
        DataLost => Words::new("project", Attention, "This board's files were lost.")
            .then_press("Restore them from a backup, or reset the board."),
        ProjectError => Words::new("project", Error, "This board's project has an error.")
            .then_press("Edit the project to fix it."),
        NoProject => Words::new("project", Neutral, "This board has no project yet.")
            .then_press("Add a project to play on it."),
        Locked => {
            Words::new("lock", Neutral, "This board is locked.").then_press("Unlock it to connect.")
        }
        InTheEditor => Words::new("edit", Neutral, "This board is open in the editor."),
        PictureTooLarge => Words::new(
            "project",
            Neutral,
            format!("This board's picture is too large to show {}.", way(link)),
        ),
        state => firmware_words(state),
    })
}

/// Why the firmware fix `state` names isn't offered on this card, and what
/// to do instead, said in the button's place (P7: no dead ends): over a
/// link this board can't update over yet, the link that can; a version
/// Studio can't get, the internet; else this Studio has no firmware that
/// fits. `None` for a state that is not about what runs the board.
fn missing_fix(input: &BoardCardInput<'_>, state: SurfaceState) -> Option<String> {
    use SurfaceState::*;
    let verb = match state {
        Crashing => "reinstall",
        OldFirmware | UpdateRequired => "update",
        CantGetVersion | Incomplete | PreHelloFirmware | NoFirmware | DownloadMode | Silent => {
            "install"
        }
        _ => return None,
    };
    Some(match input.update.map(|update| &update.standing) {
        Some(UpdateStanding::NotOverWifiYet {
            link: UpdateLink::Relay,
            ..
        }) => format!("Connect nearby to {verb} it."),
        Some(UpdateStanding::NotOverWifiYet { .. }) => {
            format!("Connect over USB or Bluetooth to {verb} it.")
        }
        Some(UpdateStanding::NoWirelessBuild { .. }) => format!("Connect over USB to {verb} it."),
        Some(UpdateStanding::CantGetVersion { .. }) => {
            "Connect this computer to the internet to get it.".to_string()
        }
        _ if state == UpdateRequired => {
            "Studio doesn't have firmware new enough to read them.".to_string()
        }
        _ => "Studio doesn't have firmware for this board.".to_string(),
    })
}

/// The words of a state about what runs the board.
fn firmware_words(state: SurfaceState) -> Words {
    use SurfaceState::*;
    use UiStatusKind::{Attention, Error, Neutral};
    let install = "Install the firmware to use it.";
    match state {
        ConfirmationRequired => Words::new(
            "firmware",
            Attention,
            "This update is waiting for your confirmation.",
        ),
        UpdateRequired => Words::new(
            "firmware",
            Attention,
            "This board's files need newer firmware.",
        )
        .then_press("Update the firmware to read them."),
        DataUnreadable => Words::new(
            "firmware",
            Attention,
            "This board's firmware can't read its files.",
        )
        .then_fact("The files are kept on the board."),
        UpdateInterrupted => Words::new(
            "firmware",
            Attention,
            "This board's firmware update was interrupted.",
        )
        .then_press("Resume the update to finish it."),
        Crashing => Words::new("firmware", Error, "This board's firmware keeps crashing.")
            .then_press("Reinstall the firmware to fix it."),
        CantGetVersion => Words::new(
            "firmware",
            Attention,
            "This board needs a firmware version Studio can't get.",
        )
        .then_press(install),
        Incomplete => Words::new(
            "firmware",
            Attention,
            "Part of this board's firmware is missing.",
        )
        .then_press(install),
        OldFirmware => Words::new("firmware", Attention, "This board is running old firmware.")
            .then_press("Update the firmware to connect and edit."),
        PreHelloFirmware => {
            Words::new("firmware", Attention, "This board is running old firmware.")
                .then_press("Install the firmware to connect and edit.")
        }
        NoFirmware => Words::new(
            "firmware",
            Neutral,
            "This board doesn't have LightPlayer firmware.",
        )
        .then_press(install),
        DownloadMode => Words::new(
            "firmware",
            Neutral,
            "This board is waiting in download mode.",
        )
        .then_press(install),
        Silent => {
            Words::new("firmware", Attention, "This board doesn't respond.").then_press(install)
        }
        _ => identifying(),
    }
}

/// Studio finding out what the board is.
fn identifying() -> Words {
    Words::new("chip", UiStatusKind::Working, "Identifying this board.")
}

/// Whose update it is, and what it does.
fn updating_sentence(input: &BoardCardInput<'_>) -> &'static str {
    match input.update.map(|update| &update.standing) {
        Some(UpdateStanding::AnotherDevice { .. }) => {
            "Another device is updating this board's firmware."
        }
        Some(UpdateStanding::Restoring { .. }) => "This board is restoring its firmware.",
        _ => "This board is updating its firmware.",
    }
}

#[cfg(test)]
mod tests {
    use lpa_devices::view::{Escape, OutcomeView};

    use super::super::board_card_builder::board_card;
    use super::super::card_fixtures::{CardFixture, activity, feed};
    use super::*;
    use crate::app::devices::activity_ends::ActivityEnd;
    use crate::app::devices::device_card_feed_view::FeedLiveness;
    use crate::app::devices::device_update_fixtures::{UpdateFixture, UpdateFixtureRow};
    use crate::app::devices::take_over_state::UiTakeOver;
    use crate::app::devices::wifi_connects::UiWifiConnect;

    /// The desk's fixture C6 on lightplayer.app, 2026-10-17: an older
    /// LightPlayer on USB, its board known. The surface says what it is and
    /// what the button does; the primary is that button; nothing on the
    /// card is a dead end.
    #[test]
    fn an_older_lightplayer_says_old_firmware_and_offers_the_update() {
        let mut fixture = older_lightplayer();
        let card = board_card(&fixture.input());
        assert_eq!(card.surface.state, SurfaceState::OldFirmware);
        let notice = card.surface.notice.clone().expect("a notice");
        assert_eq!(notice.icon, "firmware");
        assert_eq!(notice.tone, UiStatusKind::Attention);
        assert_eq!(notice.first, "This board is running old firmware.");
        assert_eq!(
            notice.second.as_deref(),
            Some("Update the firmware to connect and edit.")
        );
        assert_eq!(card.surface.picture, SurfacePicture::None);
        let Some(UiPrimary::Offer(primary)) = card.name_bar.primary.clone() else {
            panic!("a primary: {:?}", card.name_bar.primary);
        };
        assert_eq!(primary.word, "Update firmware");
        assert_eq!(
            primary.verb_alone.as_deref(),
            Some("Update"),
            "a narrow name bar says the verb alone"
        );
        assert!(primary.offer.to_string().ends_with("/update-firmware"));
        assert_eq!(primary.refused, None);
    }

    /// One update has one number (round 2: the new firmware is the first
    /// half, its last part the second): the surface's percent is the one
    /// the Firmware row's work carries and its words say, for every update
    /// story — never the step's own.
    #[test]
    fn the_surface_and_the_firmware_row_say_one_number() {
        let mut said = Vec::new();
        for row in UpdateFixtureRow::ALL {
            let mut fixture = update_story(row, false);
            let card = board_card(&fixture.input());
            let Some(SurfaceProgress::Percent(percent)) = card.surface.progress else {
                continue;
            };
            let work = card
                .bars
                .iter()
                .find(|bar| bar.layer == BarLayer::Firmware)
                .and_then(|bar| bar.work.clone())
                .expect("the Firmware row works");
            // The row is carried: its verb alone, still; the number is
            // the surface's to say (P2).
            assert_eq!(work.percent, Some(percent), "{row:?}");
            assert!(work.carried, "{row:?}");
            assert!(!work.words.contains('%'), "{row:?}: {}", work.words);
            said.push((row, percent));
        }
        // The new firmware at 40 % is the whole update at 20 %; its last
        // part at 70 % is 85 %.
        assert!(said.contains(&(UpdateFixtureRow::Updating, 20)), "{said:?}");
        assert!(
            said.contains(&(UpdateFixtureRow::Finishing, 85)),
            "{said:?}"
        );
    }

    /// A version Studio can't get (P7): Install firmware is the primary
    /// whether Studio's own build is one press, or the row's details list
    /// the versions (with the store's index) — then its press installs
    /// the one the list picks, this Studio's own.
    #[test]
    fn a_version_studio_cant_get_is_installed_from_the_primary() {
        for offline in [true, false] {
            let mut fixture = update_story(UpdateFixtureRow::CantGetVersion, offline);
            let card = board_card(&fixture.input());
            assert_eq!(card.surface.state, SurfaceState::CantGetVersion);
            assert_eq!(
                card.surface.notice.and_then(|notice| notice.second),
                Some("Install the firmware to use it.".to_string())
            );
            let Some(UiPrimary::Offer(primary)) = card.name_bar.primary else {
                panic!("a primary: {:?}", card.name_bar.primary);
            };
            assert_eq!(primary.word, "Install firmware");
            assert_eq!(primary.verb_alone.as_deref(), Some("Install"));
            assert!(primary.offer.to_string().ends_with("/install-firmware"));
            let offers = fixture.offers();
            let install = offers
                .iter()
                .find(|offer| offer.path == primary.offer)
                .expect("offered");
            assert_eq!(
                install.params().is_empty(),
                offline,
                "one press without the index, the list with it"
            );
            assert!(
                install.press(&primary.args).is_ok(),
                "the press binds the version the list picks"
            );
        }
    }

    /// An update that found no copy of the engine failed, and its Retry is
    /// gone (the board needs a version Studio can't get): the surface says
    /// the board's own state and its next step, never a failure with
    /// nothing to press (P7).
    #[test]
    fn failed_work_with_no_retry_hands_over_to_the_boards_own_state() {
        let mut fixture = update_story(UpdateFixtureRow::CantGetVersion, true);
        fixture.view.last_outcome = Some(OutcomeView {
            summary: "This Studio couldn't get v2026.09.28-4".to_string(),
            ok: false,
        });
        fixture.ended = Some(ActivityEnd {
            kind: ActivityKind::Update,
            ok: false,
            at: fixture.now - 30.0,
        });
        let card = board_card(&fixture.input());
        assert_eq!(card.surface.state, SurfaceState::CantGetVersion);
        assert!(matches!(
            card.name_bar.primary,
            Some(UiPrimary::Offer(ref primary)) if primary.word == "Install firmware"
        ));
    }

    /// When a state's fix isn't offered here, its second sentence says why
    /// and what to do instead (P7): a core-only board over Wi‑Fi that
    /// updates over USB or Bluetooth first.
    #[test]
    fn a_fix_that_isnt_offered_says_why() {
        let mut fixture =
            update_story(UpdateFixtureRow::NotOverWifiYet, false).over(UiLinkKind::Wifi);
        fixture.view.firmware_face = FirmwareFace::CoreOnly {
            version: Some("2026.10.08-9".to_string()),
            state: UpdateBoardState::NeedsEngine,
        };
        let card = board_card(&fixture.input());
        assert_eq!(card.surface.state, SurfaceState::Incomplete);
        assert_eq!(card.name_bar.primary, None, "nothing to press here");
        assert_eq!(
            card.surface.notice.and_then(|notice| notice.second),
            Some("Connect over USB or Bluetooth to install it.".to_string())
        );
    }

    #[test]
    fn a_board_that_is_well_shows_its_live_picture_and_says_nothing() {
        let mut fixture = CardFixture::ready();
        fixture.feed = Some(feed(FeedLiveness::Live, true));
        let surface = board_card(&fixture.input()).surface;
        assert_eq!(surface.state, SurfaceState::AllWell);
        assert_eq!(surface.notice, None);
        assert_eq!(surface.picture, SurfacePicture::Live);
        assert_eq!(surface.progress, None);
    }

    /// B covers a picture: it dims under the notice (another tab, offline,
    /// updating, sending), and a dark board's notice stands alone.
    #[test]
    fn a_picture_dims_under_a_notice_and_none_stays_none() {
        let mut sending = CardFixture::ready().with_activity(activity(
            ActivityKind::Push,
            "Sending the project",
            Some(40),
        ));
        sending.feed = Some(feed(FeedLiveness::Live, true));
        sending.sending = Some("studio".to_string());
        let surface = board_card(&sending.input()).surface;
        assert_eq!(surface.state, SurfaceState::Sending);
        assert_eq!(surface.picture, SurfacePicture::Dimmed);
        assert_eq!(surface.progress, Some(SurfaceProgress::Percent(40)));
        let notice = surface.notice.expect("a notice");
        assert_eq!(
            notice.first,
            "Sending the project \u{201c}studio\u{201d} to this board."
        );
        assert_eq!(notice.tone, UiStatusKind::Working);

        let mut offline = CardFixture::offline();
        offline.feed = Some(feed(FeedLiveness::Offline, true));
        offline.view.escapes = vec![Escape::Forget];
        offline.last_seen_at = Some(offline.now - 7_200.0);
        let surface = board_card(&offline.input()).surface;
        assert_eq!(surface.state, SurfaceState::Offline);
        assert_eq!(surface.picture, SurfacePicture::Dimmed);
        let notice = surface.notice.expect("a notice");
        assert_eq!(notice.first, "This board is offline.");
        assert_eq!(
            notice.second.as_deref(),
            Some("It was last seen 2 hours ago.")
        );
    }

    /// Every state in the table, its words and its primary, over the
    /// fixtures that reach it.
    #[test]
    fn every_state_says_its_words_and_names_its_primary() {
        for (mut fixture, state, first, second, primary) in table() {
            let card = board_card(&fixture.input());
            assert_eq!(card.surface.state, state, "{first}");
            let notice = card.surface.notice.clone();
            assert_eq!(
                notice.as_ref().map(|notice| notice.first.as_str()),
                Some(first),
                "{state:?}"
            );
            assert_eq!(
                notice.as_ref().and_then(|notice| notice.second.as_deref()),
                second,
                "{state:?}"
            );
            assert_eq!(
                card.name_bar
                    .primary
                    .as_ref()
                    .and_then(|primary| primary.word()),
                primary,
                "{state:?}"
            );
        }
    }

    /// The second sentence says the button aloud only while it is the
    /// primary: a board the editor holds has Done there instead.
    #[test]
    fn the_button_is_said_aloud_only_while_it_is_the_primary() {
        let mut fixture = CardFixture::not_answering();
        let notice = board_card(&fixture.input()).surface.notice.unwrap();
        assert_eq!(notice.second.as_deref(), Some("Retry to reconnect."));
        fixture.editor_holds_it = true;
        let card = board_card(&fixture.input());
        assert_eq!(
            card.name_bar.primary.as_ref().and_then(UiPrimary::word),
            Some("Done")
        );
        assert_eq!(card.surface.notice.unwrap().second, None);
    }

    /// A button the link refuses is no primary (never a greyed verb), and
    /// the surface says why in its place.
    #[test]
    fn a_refused_button_is_said_as_its_reason() {
        let mut fixture = CardFixture::ready().over(UiLinkKind::Bluetooth);
        fixture.view.firmware_face = FirmwareFace::Blank;
        fixture.view.status = DeviceStatus::NeedsAttention;
        fixture.view.loaded_project = LoadedProject::Unknown;
        fixture.view.can_receive_project = false;
        fixture.view.can_remove_project = false;
        let card = board_card(&fixture.input());
        assert_eq!(card.surface.state, SurfaceState::NoFirmware);
        assert_eq!(card.name_bar.primary, None);
        assert_eq!(
            card.surface.notice.unwrap().second.as_deref(),
            Some("Firmware updates need USB.")
        );
    }

    /// The rows go quiet: the row whose problem the surface says keeps its
    /// word and its button, and loses its tone.
    #[test]
    fn the_row_the_surface_speaks_for_goes_quiet() {
        let mut fixture = CardFixture::not_answering();
        let card = board_card(&fixture.input());
        let link = card.bar(BarLayer::Usb);
        assert_eq!(link.summary, "not responding");
        assert_eq!(link.tone, UiStatusKind::Neutral, "the surface is loud");
        assert_eq!(link.action.as_ref().map(|a| a.word.as_str()), Some("Retry"));

        // A failed send: its row keeps its words and Retry, quietly.
        let mut failed = CardFixture::ready();
        failed.view.last_outcome = Some(OutcomeView {
            summary: "The board did not answer".to_string(),
            ok: false,
        });
        failed.ended = Some(ActivityEnd {
            kind: ActivityKind::Push,
            ok: false,
            at: failed.now - 5.0,
        });
        let card = board_card(&failed.input());
        assert_eq!(card.surface.state, SurfaceState::SendFailed);
        let project = card.bar(BarLayer::Project);
        assert!(matches!(
            project.work.as_ref().map(|work| &work.state),
            Some(BarWorkState::Failed { .. })
        ));
        assert_eq!(project.tone, UiStatusKind::Neutral);
        // A row the surface does not speak for keeps its tone.
        let mut done = CardFixture::ready();
        done.view.last_outcome = Some(OutcomeView {
            summary: "ok".to_string(),
            ok: true,
        });
        done.ended = Some(ActivityEnd {
            kind: ActivityKind::Push,
            ok: true,
            at: done.now,
        });
        assert_eq!(
            board_card(&done.input()).bar(BarLayer::Project).tone,
            UiStatusKind::Good
        );
    }

    #[test]
    fn a_new_board_identifies_then_asks_for_firmware() {
        let mut pending = super::super::primary_action::tests::pending_view();
        let surface = pending_surface(&pending, None, None);
        assert_eq!(surface.state, SurfaceState::Identifying);
        assert_eq!(surface.progress, Some(SurfaceProgress::Spinner));
        pending.firmware_face = FirmwareFace::Blank;
        let surface = pending_surface(&pending, None, None);
        assert_eq!(surface.state, SurfaceState::NoFirmware);
        assert_eq!(
            surface.notice.unwrap().first,
            "This board doesn't have LightPlayer firmware."
        );
    }

    /// Every state has its hook word, and none is said twice.
    #[test]
    fn every_state_has_its_own_hook_word() {
        let words: Vec<&str> = table()
            .into_iter()
            .map(|(_, state, ..)| state.as_str())
            .collect();
        let mut unique = words.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), words.len(), "{words:?}");
        assert_eq!(SurfaceState::OldFirmware.as_str(), "old-firmware");
    }

    /// One state of the table: its fixture, the state, its first sentence,
    /// its second, and the primary's word.
    type Row = (
        CardFixture,
        SurfaceState,
        &'static str,
        Option<&'static str>,
        Option<&'static str>,
    );

    /// Fixtures for every notice state.
    fn table() -> Vec<Row> {
        use SurfaceState::*;
        let mut rows = Vec::new();

        let mut connecting = CardFixture::ready();
        connecting.connection = BoardConnection::Connecting;
        rows.push((
            connecting,
            Connecting,
            "Connecting to this board over USB.",
            None,
            None,
        ));

        let mut reconnecting = CardFixture::offline();
        reconnecting.editor_holds_it = true;
        reconnecting.connection = BoardConnection::Reconnecting;
        rows.push((
            reconnecting,
            Reconnecting,
            "Reconnecting to this board over USB.",
            None,
            Some("Done"),
        ));

        let mut moving = CardFixture::held(HoldLevel::Watching);
        moving.take_over = Some(UiTakeOver {
            words: "Asking the other tab\u{2026}".to_string(),
            failed: false,
        });
        rows.push((
            moving,
            MovingHere,
            "Moving this board to this tab.",
            None,
            None,
        ));

        rows.push((
            CardFixture::ready().with_activity(activity(
                ActivityKind::Update,
                "Updating",
                Some(42),
            )),
            Updating,
            "This board is updating its firmware.",
            None,
            None,
        ));
        rows.push((
            CardFixture::ready().with_activity(activity(ActivityKind::Flash, "Flashing", Some(30))),
            Installing,
            "Installing the firmware on this board.",
            None,
            None,
        ));
        rows.push((
            CardFixture::ready().with_activity(activity(ActivityKind::Erase, "Erasing", None)),
            Erasing,
            "Erasing this board.",
            None,
            None,
        ));
        rows.push((
            CardFixture::ready().with_activity(activity(ActivityKind::Push, "Sending", Some(40))),
            Sending,
            "Sending a project to this board.",
            None,
            None,
        ));
        rows.push((
            CardFixture::ready().with_activity(activity(
                ActivityKind::RemoveProject,
                "Removing",
                None,
            )),
            Removing,
            "Removing the project from this board.",
            None,
            None,
        ));
        rows.push((
            CardFixture::ready().with_activity(activity(
                ActivityKind::Identify,
                "Identifying",
                None,
            )),
            Identifying,
            "Identifying this board.",
            None,
            None,
        ));

        let mut move_failed = CardFixture::held(HoldLevel::Watching);
        move_failed.take_over = Some(UiTakeOver {
            words: "That tab didn't answer".to_string(),
            failed: true,
        });
        rows.push((
            move_failed,
            MoveFailed,
            "Studio couldn't move this board to this tab.",
            Some("Connect here to try again."),
            Some("Connect here"),
        ));
        let mut taken = CardFixture::held(HoldLevel::Watching);
        taken.view.held_elsewhere.as_mut().unwrap().taken_from_here = true;
        rows.push((
            taken,
            TakenByAnotherTab,
            "Another tab took this board.",
            Some("Connect here to move it back to this tab."),
            Some("Connect here"),
        ));
        rows.push((
            CardFixture::held(HoldLevel::Busy("Updating \u{b7} 42%".to_string())),
            AnotherTabBusy,
            "This board is busy in another tab.",
            Some("Connect here when it's done."),
            None,
        ));
        rows.push((
            CardFixture::held(HoldLevel::Open),
            AnotherTab,
            "This board is open in another tab.",
            Some("Connect here to move it to this tab."),
            Some("Connect here"),
        ));

        let mut someone = CardFixture::offline();
        someone.wifi_address = true;
        someone.wifi_connect = Some(network_connect(true, false));
        rows.push((
            someone,
            SomeoneElse,
            "Someone else is connected to this board.",
            Some("Retry when they're done."),
            Some("Retry"),
        ));
        let mut wifi_failed = CardFixture::offline();
        wifi_failed.wifi_address = true;
        wifi_failed.wifi_connect = Some(network_connect(false, false));
        rows.push((
            wifi_failed,
            NetworkFailed,
            "Studio couldn't reach this board over Wi\u{2011}Fi.",
            Some("Make sure this computer is on the same Wi\u{2011}Fi, then retry."),
            Some("Retry"),
        ));
        let mut connect_failed = CardFixture::ready();
        connect_failed.connection = BoardConnection::Failed {
            reason: "the board did not answer".to_string(),
        };
        rows.push((
            connect_failed,
            ConnectFailed,
            "Studio couldn't connect to this board over USB.",
            Some("Retry to connect."),
            Some("Retry"),
        ));
        rows.push((
            CardFixture::not_answering(),
            NotAnswering,
            "This board stopped responding over USB.",
            Some("Retry to reconnect."),
            Some("Retry"),
        ));

        let mut sim_off = CardFixture::offline();
        sim_off.runtime = Some(crate::UiRuntimeBand::sim("seeed/xiao-esp32-c6", None));
        rows.push((
            sim_off,
            PoweredOff,
            "This simulated board is off.",
            None,
            Some("Power on"),
        ));
        let mut reachable = CardFixture::offline();
        reachable.wifi_address = true;
        rows.push((
            reachable,
            OfflineReachable,
            "This board isn't plugged in.",
            Some("Connect over Wi\u{2011}Fi to use it."),
            Some("Connect"),
        ));
        let mut offline = CardFixture::offline();
        offline.view.escapes = vec![Escape::Forget];
        offline.last_seen_at = Some(offline.now - 7_200.0);
        rows.push((
            offline,
            Offline,
            "This board is offline.",
            Some("It was last seen 2 hours ago."),
            None,
        ));

        let mut disconnected = CardFixture::ready();
        disconnected.view.status = DeviceStatus::Attached;
        disconnected.view.can_receive_project = false;
        disconnected.view.can_remove_project = false;
        rows.push((
            disconnected,
            Disconnected,
            "Studio isn't connected to this board.",
            Some("Connect over USB to use it."),
            Some("Connect"),
        ));

        rows.push((
            failed(ActivityKind::Push),
            SendFailed,
            "Studio couldn't send the project to this board.",
            Some("Retry to send it again."),
            Some("Retry"),
        ));
        rows.push((
            failed(ActivityKind::Erase),
            EraseFailed,
            "Erasing this board didn't finish.",
            Some("Retry to erase it."),
            Some("Retry"),
        ));
        rows.push((
            failed(ActivityKind::RemoveProject),
            RemoveFailed,
            "Removing the project from this board didn't finish.",
            Some("Retry to remove it."),
            Some("Retry"),
        ));

        rows.push((
            older_lightplayer(),
            OldFirmware,
            "This board is running old firmware.",
            Some("Update the firmware to connect and edit."),
            Some("Update firmware"),
        ));
        rows.push((
            needs_firmware(FirmwareFace::NoHello),
            PreHelloFirmware,
            "This board is running old firmware.",
            Some("Install the firmware to connect and edit."),
            Some("Install firmware"),
        ));
        rows.push((
            needs_firmware(FirmwareFace::Blank),
            NoFirmware,
            "This board doesn't have LightPlayer firmware.",
            Some("Install the firmware to use it."),
            Some("Install firmware"),
        ));
        rows.push((
            needs_firmware(FirmwareFace::Bootloader),
            DownloadMode,
            "This board is waiting in download mode.",
            Some("Install the firmware to use it."),
            Some("Install firmware"),
        ));
        rows.push((
            needs_firmware(FirmwareFace::Silent),
            Silent,
            "This board doesn't respond.",
            Some("Install the firmware to use it."),
            Some("Install firmware"),
        ));
        rows.push((
            core_only(UpdateBoardState::NeedsEngine),
            Incomplete,
            "Part of this board's firmware is missing.",
            Some("Studio doesn't have firmware for this board."),
            None,
        ));
        rows.push((
            core_only(UpdateBoardState::EngineCrashing),
            Crashing,
            "This board's firmware keeps crashing.",
            Some("Studio doesn't have firmware for this board."),
            None,
        ));

        rows.push((
            CardFixture::ready().over(UiLinkKind::Bluetooth).locked(),
            Locked,
            "This board is locked.",
            Some("Unlock it to connect."),
            Some("Unlock"),
        ));
        let mut faulted = CardFixture::ready();
        faulted.view.status = DeviceStatus::Degraded;
        faulted.view.degraded = Some("orbit.shader:12 'tme' is not declared".to_string());
        rows.push((
            faulted,
            ProjectError,
            "This board's project has an error.",
            Some("Edit the project to fix it."),
            Some("Edit project"),
        ));
        let mut empty = CardFixture::ready();
        empty.view.loaded_project = LoadedProject::Empty;
        empty.view.can_remove_project = false;
        empty.plays = BoardPlays::Nothing;
        rows.push((
            empty,
            NoProject,
            "This board has no project yet.",
            Some("Add a project to play on it."),
            Some("Add a project"),
        ));

        let mut editor = CardFixture::ready();
        editor.editor_holds_it = true;
        editor.feed = Some(feed(FeedLiveness::Lens, true));
        rows.push((
            editor,
            InTheEditor,
            "This board is open in the editor.",
            None,
            Some("Done"),
        ));

        let mut too_large = CardFixture::ready().over(UiLinkKind::Bluetooth);
        too_large.feed = Some(feed(FeedLiveness::Live, false));
        rows.push((
            too_large,
            PictureTooLarge,
            "This board's picture is too large to show over Bluetooth.",
            None,
            Some("Connect"),
        ));
        rows
    }

    /// The relay's failed connect says the relay's words.
    #[test]
    fn a_relay_connect_that_failed_says_lightplayer_app() {
        let mut relay = CardFixture::offline();
        relay.relay = true;
        relay.wifi_connect = Some(network_connect(false, true));
        let card = board_card(&relay.input());
        assert_eq!(card.surface.state, SurfaceState::NetworkFailed);
        let notice = card.surface.notice.unwrap();
        assert_eq!(notice.icon, "cloud");
        assert_eq!(
            notice.first,
            "Studio couldn't reach this board through lightplayer.app."
        );
        assert_eq!(notice.second.as_deref(), Some("Retry to connect."));
    }

    /// An older LightPlayer on the desk C6: wire 33, its board known.
    fn older_lightplayer() -> CardFixture {
        let mut fixture = needs_firmware(FirmwareFace::OlderLightPlayer { proto: Some(33) });
        fixture.view.state_label = "Older LightPlayer firmware".to_string();
        fixture
    }

    /// The ready board with `face` on its flash: it needs attention, and
    /// says nothing of a project.
    fn needs_firmware(face: FirmwareFace) -> CardFixture {
        let mut fixture = CardFixture::ready();
        fixture.view.firmware_face = face;
        fixture.view.status = DeviceStatus::NeedsAttention;
        fixture.view.loaded_project = LoadedProject::Unknown;
        fixture.view.engine_fps = None;
        fixture.view.can_receive_project = false;
        fixture.view.can_remove_project = false;
        fixture.plays = BoardPlays::Unknown;
        fixture
    }

    /// `row`'s update story on the ready board: its words and the offers
    /// its standing publishes. `offline`: this Studio could not read the
    /// store's release index.
    fn update_story(row: UpdateFixtureRow, offline: bool) -> CardFixture {
        let mut fixture = CardFixture::ready();
        let mut update = UpdateFixture::new(row, fixture.view.clone());
        if offline {
            update = update.offline();
        }
        fixture.view = update.view.clone();
        if fixture.view.activity.is_some() {
            fixture.view.status = DeviceStatus::Busy;
            fixture.view.can_receive_project = false;
        }
        fixture.update = update.words();
        fixture.update_facts = update.offer_facts();
        fixture
    }

    /// A core with no engine, in `state`.
    fn core_only(state: UpdateBoardState) -> CardFixture {
        needs_firmware(FirmwareFace::CoreOnly {
            version: Some("2026.10.08-9".to_string()),
            state,
        })
    }

    /// The ready board whose last `kind` failed a moment ago.
    fn failed(kind: ActivityKind) -> CardFixture {
        let mut fixture = CardFixture::ready();
        fixture.view.last_outcome = Some(OutcomeView {
            summary: "The board did not answer".to_string(),
            ok: false,
        });
        fixture.ended = Some(ActivityEnd {
            kind,
            ok: false,
            at: fixture.now - 30.0,
        });
        fixture
    }

    /// A Wi‑Fi (or relay) connect that ended: turned away busy, or failed.
    fn network_connect(busy: bool, through_relay: bool) -> UiWifiConnect {
        UiWifiConnect {
            host: "10.0.0.5".to_string(),
            through_relay,
            connecting: false,
            error: (!busy).then(|| "The board didn't answer at 10.0.0.5.".to_string()),
            busy,
        }
    }
}
