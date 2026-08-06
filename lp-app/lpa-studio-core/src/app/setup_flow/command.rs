//! What the setup flow asks the world to do.
//!
//! Design: `docs/design/device-setup-flow.md` §8, which maps each variant
//! to the machinery that already implements it. Nothing here performs the
//! work; the reducer emits these values and
//! [`dispatch_for`](super::dispatch_for) turns them into the op values the
//! studio already runs.

/// One request from the reducer to the executor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetupCommand {
    /// Ask for a serial port grant (the browser's own chooser).
    RequestPort,
    /// Run one probe pass and report a `ProbeCompleted`.
    ProbeBoard,
    /// Drop the granted port and its session.
    ReleasePort,
    /// Flash the packaged firmware for `board_id`.
    ///
    /// `attempt` is 1-based. `replug_guidance` is set from the second
    /// attempt on: an ESP32 that failed a flash often needs a physical
    /// replug before the next one can work, and an incomplete flash is
    /// never trusted.
    Flash {
        board_id: String,
        attempt: u32,
        replug_guidance: bool,
    },
    /// Generate and install the first project for `board_id`.
    GenerateProject { board_id: String },
    /// Write the device's name and board under its resolved identity.
    /// Emitted whenever the target can be renamed — there is no stamp
    /// step; the name IS the registry row.
    ///
    /// `hardware_uid` is **advisory**: the uid the PROBE anchored, when it
    /// anchored one. A blank board probed in its boot loop anchors
    /// nothing (no hello, no efuse read yet), and the flash that follows
    /// is exactly what gives the board an identity — so the executor
    /// prefers the session's CURRENTLY resolved uid and falls back to
    /// this one (G2 blank-C6 walk, 2026-08-05: the reducer refusing to
    /// emit on a uid-less probe is why a typed name never landed, and the
    /// push then refused a board with no name). The reducer stays pure by
    /// not pretending to know an identity that only the wire has.
    WriteRegistry {
        hardware_uid: Option<String>,
        hardware_origin: Option<String>,
        name: String,
        board_id: String,
    },
    /// Adopt (ALREADY_LP → Done): record the sighting and write nothing
    /// else. The device keeps its name, its project, and its history.
    RecordSighting { hardware_uid: String },
    /// Push the generated project to the target.
    PushProject { project_uid: String },
    /// Land on the device home: the editor lensed to this target.
    OpenDeviceHome,
    /// Leave the card saying "incomplete flash — needs re-flash".
    MarkIncompleteFlash,
}

impl SetupCommand {
    /// Stable label for golden-trace assertions and event-log records
    /// (extend, do not rename — the M8 traces key off these).
    pub fn label(&self) -> &'static str {
        match self {
            Self::RequestPort => "request-port",
            Self::ProbeBoard => "probe-board",
            Self::ReleasePort => "release-port",
            Self::Flash { .. } => "flash",
            Self::GenerateProject { .. } => "generate-project",
            Self::WriteRegistry { .. } => "write-registry",
            Self::RecordSighting { .. } => "record-sighting",
            Self::PushProject { .. } => "push-project",
            Self::OpenDeviceHome => "open-device-home",
            Self::MarkIncompleteFlash => "mark-incomplete-flash",
        }
    }
}
