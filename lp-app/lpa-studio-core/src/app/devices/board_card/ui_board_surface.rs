//! [`UiBoardSurface`]: the top of the board card, its main surface
//! (`docs/ux/anatomy/board-card.md`, "Surface"). Yona, 2026-10-17: "the
//! primary surface should always give the most important information" —
//! so it is never black — and "the surface explains; the primary gives the
//! thing to do".
//!
//! While the board is well the surface is its live picture and says
//! nothing (P4). Otherwise it carries one notice: the state's icon in the
//! state's tone, then one or two plain sentences, centred, the second the
//! primary's button said aloud ("Update the firmware to connect and edit."
//! beside **Update firmware**). A picture dims under the notice, fully
//! covered; work shows its progress under the sentences. There are no
//! buttons on the surface: the name bar's primary is the thing to do.
//!
//! Built by [`super::board_surface`], one table, first match wins.

use crate::UiStatusKind;

/// The card's surface.
#[derive(Clone, Debug, PartialEq)]
pub struct UiBoardSurface {
    /// Which state the surface says, for hooks, tests and stories
    /// (`data-surface`); the words are [`Self::notice`]'s.
    pub state: SurfaceState,
    /// How the card's picture is drawn under the surface.
    pub picture: SurfacePicture,
    /// What the surface says; `None` while the board is well (the live
    /// picture says it).
    pub notice: Option<UiSurfaceNotice>,
    /// How far along the work it reports is, drawn under the sentences.
    pub progress: Option<SurfaceProgress>,
}

/// How the picture is drawn under the surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfacePicture {
    /// As it is: the board is well, and nothing covers it.
    Live,
    /// Dimmed under the notice, fully covered. One dim: the cover's
    /// replaces a stale picture's own, never both.
    Dimmed,
    /// No picture to draw: the notice stands on the picture's dark ground.
    None,
}

/// The surface's notice: an icon in a tone, then one or two sentences.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiSurfaceNotice {
    /// The state's kind, as an icon token the rows use (`firmware`,
    /// `project`, `lock`, `usb`, `wifi`, `bluetooth`, `cloud`, `play`,
    /// `edit`): the same icon as the row whose state it is.
    pub icon: String,
    /// The icon's tone, in ui.md's status families: Attention (a device
    /// needs you), Error (broken), Working (under way), Neutral. The tone is
    /// the icon's alone; the sentences stay plain (the director's call,
    /// 2026-10-17).
    pub tone: UiStatusKind,
    /// What is going on with the board, one plain sentence.
    pub first: String,
    /// The primary's button said aloud — its verb and object, then what it
    /// gets you — or one more fact worth a sentence ("It was last seen 2
    /// hours ago."). Only when it adds something.
    pub second: Option<String>,
}

/// How far along the reported work is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceProgress {
    /// A percent, drawn as a bar and its number.
    Percent(u8),
    /// Under way with no number: the spinner, in the icon's place.
    Spinner,
}

/// Which state the surface says. One per line of the anatomy's "Surface"
/// table; [`Self::as_str`] is its hook word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceState {
    /// The board is well: the live picture, no words.
    AllWell,
    /// The session is open on the card (its panel in the rows' place), or
    /// the editor shows the board: the live picture, no words.
    Connected,
    /// A Connect is under way.
    Connecting,
    /// The connected session's link dropped; the lens holds on.
    Reconnecting,
    /// A take-over from another tab is under way.
    MovingHere,
    /// The firmware is being updated (over the air, by another device, or
    /// its missing part put back).
    Updating,
    /// Firmware is being written by a flash.
    Installing,
    /// The board is being erased.
    Erasing,
    /// A project is being sent.
    Sending,
    /// The project is being removed.
    Removing,
    /// Studio is finding out what the board is.
    Identifying,
    /// Another tab of this browser holds the board.
    AnotherTab,
    /// …and is busy with it.
    AnotherTabBusy,
    /// Another tab took the board from this one.
    TakenByAnotherTab,
    /// A take-over that did not bring the board here.
    MoveFailed,
    /// Someone Studio cannot name holds the board's network slot.
    SomeoneElse,
    /// A Wi‑Fi or lightplayer.app connect that failed.
    NetworkFailed,
    /// The last Connect did not open.
    ConnectFailed,
    /// The board stopped answering on its link.
    NotAnswering,
    /// Offline, and nothing but its cable reaches it.
    Offline,
    /// Offline from here, and Wi‑Fi or lightplayer.app reaches it.
    OfflineReachable,
    /// The port is there and closed.
    Disconnected,
    /// A simulated or emulated board, powered off.
    PoweredOff,
    /// A push failed.
    SendFailed,
    /// An update failed.
    UpdateFailed,
    /// A flash failed.
    InstallFailed,
    /// An erase failed.
    EraseFailed,
    /// A removal failed.
    RemoveFailed,
    /// The layout question waits for an answer.
    ConfirmationRequired,
    /// The board's files wait for a firmware that reads their store.
    UpdateRequired,
    /// The board's firmware refused its file store.
    DataUnreadable,
    /// The board came back without its files.
    DataLost,
    /// A move of the board's files stopped half-way.
    UpdateInterrupted,
    /// The board's firmware keeps crashing.
    Crashing,
    /// The board needs a firmware version Studio can't get.
    CantGetVersion,
    /// Only the board's core runs: part of its firmware is missing.
    Incomplete,
    /// An older LightPlayer, which this Studio's link cannot speak to.
    OldFirmware,
    /// A LightPlayer from before the hello.
    PreHelloFirmware,
    /// No LightPlayer: blank, or somebody else's firmware.
    NoFirmware,
    /// Parked in the ROM downloader.
    DownloadMode,
    /// The port is open and the board says nothing at all.
    Silent,
    /// Nothing this link holds unlocks it.
    Locked,
    /// The board's project faulted (a shader error).
    ProjectError,
    /// The board runs no project.
    NoProject,
    /// The editor holds the board; the picture is the last it left.
    InTheEditor,
    /// Frames flow, but the project's lamp layout is too large to show
    /// over this link.
    PictureTooLarge,
}

impl SurfaceState {
    /// The hook word (`data-surface="old-firmware"`).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AllWell => "all-well",
            Self::Connected => "connected",
            Self::Connecting => "connecting",
            Self::Reconnecting => "reconnecting",
            Self::MovingHere => "moving-here",
            Self::Updating => "updating",
            Self::Installing => "installing",
            Self::Erasing => "erasing",
            Self::Sending => "sending",
            Self::Removing => "removing",
            Self::Identifying => "identifying",
            Self::AnotherTab => "another-tab",
            Self::AnotherTabBusy => "another-tab-busy",
            Self::TakenByAnotherTab => "taken-by-another-tab",
            Self::MoveFailed => "move-failed",
            Self::SomeoneElse => "someone-else",
            Self::NetworkFailed => "network-failed",
            Self::ConnectFailed => "connect-failed",
            Self::NotAnswering => "not-answering",
            Self::Offline => "offline",
            Self::OfflineReachable => "offline-reachable",
            Self::Disconnected => "disconnected",
            Self::PoweredOff => "powered-off",
            Self::SendFailed => "send-failed",
            Self::UpdateFailed => "update-failed",
            Self::InstallFailed => "install-failed",
            Self::EraseFailed => "erase-failed",
            Self::RemoveFailed => "remove-failed",
            Self::ConfirmationRequired => "confirmation-required",
            Self::UpdateRequired => "update-required",
            Self::DataUnreadable => "data-unreadable",
            Self::DataLost => "data-lost",
            Self::UpdateInterrupted => "update-interrupted",
            Self::Crashing => "crashing",
            Self::CantGetVersion => "cant-get-version",
            Self::Incomplete => "incomplete",
            Self::OldFirmware => "old-firmware",
            Self::PreHelloFirmware => "pre-hello-firmware",
            Self::NoFirmware => "no-firmware",
            Self::DownloadMode => "download-mode",
            Self::Silent => "silent",
            Self::Locked => "locked",
            Self::ProjectError => "project-error",
            Self::NoProject => "no-project",
            Self::InTheEditor => "in-the-editor",
            Self::PictureTooLarge => "picture-too-large",
        }
    }
}
