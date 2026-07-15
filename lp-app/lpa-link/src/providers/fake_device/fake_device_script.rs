//! Boot-state script for the fake ESP32 device.

use std::time::Duration;

use serde::Serialize;

/// Where the fake device keeps its (single) project storage, mirroring the
/// studio's demo storage id (`/projects/` root + `studio`).
pub const FAKE_DEVICE_PROJECT_DIR: &str = "/projects/studio";

/// The image identity the fake connector's scripted `FlashFirmware` writes
/// into the flashed device's provenance (`commit=` on the boot line).
pub const FAKE_IMAGE_IDENTITY: &str = "fake-esp32c6-image";

/// Stamped identity for a scripted LightPlayer state, written to
/// `/.lp/device.json` at the device's fs ROOT.
///
/// Serializes to the same JSON shape the studio writes when stamping
/// (`{"uid": "dev_…", "name": "…"}`). The uid also rides the wire hello as
/// `device_uid`.
#[derive(Clone, Debug, Serialize)]
pub struct FakeDeviceIdentity {
    pub uid: String,
    pub name: String,
}

impl FakeDeviceIdentity {
    pub fn new(uid: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            uid: uid.into(),
            name: name.into(),
        }
    }
}

/// One boot state of the scripted device. Reset-signal sequences re-run the
/// current state's boot; `fake_flash`/`fake_erase` transition between states.
#[derive(Clone)]
pub enum FakeBootState {
    /// Blank or erased flash: the boot ROM repeatedly prints
    /// `invalid header: 0xffffffff` (the studio readiness classifier keys on
    /// this line).
    BlankFlash,
    /// The ROM serial downloader: prints `waiting for download` once.
    RomDownloadMode,
    /// Known replaceable non-LightPlayer firmware (a factory demo).
    ForeignFirmware,
    /// LightPlayer firmware: scripted boot output, the real M2-shaped
    /// server-start line, then a REAL host `LpServer` over `LpFsMemory`
    /// speaking `M!` frames (including the unsolicited wire hello).
    LightPlayer(FakeLightPlayerState),
}

/// The `LightPlayer` boot state's script.
#[derive(Clone)]
pub struct FakeLightPlayerState {
    /// Wall-clock delay between (re)boot and the first boot output. Client
    /// bytes written during this window are DISCARDED, like real hardware
    /// whose server loop is not reading yet.
    pub boot_delay: Duration,
    /// Project files seeded into the device's storage dir
    /// ([`FAKE_DEVICE_PROJECT_DIR`]), as storage-relative paths
    /// (e.g. `project.json`).
    pub project_files: Vec<(String, Vec<u8>)>,
    /// Stamped identity: written to `/.lp/device.json` at the device's fs
    /// root and reported as the hello's `device_uid`.
    pub identity: Option<FakeDeviceIdentity>,
    /// Firmware provenance for the boot line and the wire hello. Scripted
    /// flash (`fake_flash(image_identity)`) records the image identity here.
    pub provenance: lpc_wire::FwProvenance,
    /// Never emit a hello on the wire (unsolicited or requested): mimics
    /// PRE-HELLO firmware whose server loop runs but never identifies
    /// itself. The device session's hello gate classifies this as
    /// `Incompatible`.
    pub suppress_hello: bool,
    /// Report this wire proto version in the hello instead of the build's
    /// [`lpc_wire::WIRE_PROTO_VERSION`]: mimics firmware built from an
    /// incompatible wire revision.
    pub proto_override: Option<u32>,
}

impl FakeLightPlayerState {
    pub fn new() -> Self {
        Self {
            boot_delay: Duration::ZERO,
            project_files: Vec::new(),
            identity: None,
            provenance: fake_provenance("fake-firmware"),
            suppress_hello: false,
            proto_override: None,
        }
    }

    pub fn with_boot_delay(mut self, boot_delay: Duration) -> Self {
        self.boot_delay = boot_delay;
        self
    }

    pub fn with_project_files(mut self, files: Vec<(String, Vec<u8>)>) -> Self {
        self.project_files = files;
        self
    }

    pub fn with_identity(mut self, identity: FakeDeviceIdentity) -> Self {
        self.identity = Some(identity);
        self
    }

    pub fn with_suppressed_hello(mut self) -> Self {
        self.suppress_hello = true;
        self
    }

    pub fn with_proto_override(mut self, proto: u32) -> Self {
        self.proto_override = Some(proto);
        self
    }
}

impl Default for FakeLightPlayerState {
    fn default() -> Self {
        Self::new()
    }
}

/// The whole device script: the current boot state plus scripted management
/// behavior (flash/erase latency and optional failure).
#[derive(Clone)]
pub struct FakeDeviceScript {
    pub boot: FakeBootState,
    /// Scripted latency for `manage()` operations (flash/erase/reset).
    pub manage_latency: Duration,
    /// When set, the NEXT `manage()` operation fails with this message
    /// (consumed once).
    pub manage_failure: Option<String>,
}

impl FakeDeviceScript {
    pub fn new(boot: FakeBootState) -> Self {
        Self {
            boot,
            manage_latency: Duration::ZERO,
            manage_failure: None,
        }
    }

    pub fn with_manage_latency(mut self, latency: Duration) -> Self {
        self.manage_latency = latency;
        self
    }

    pub fn with_manage_failure(mut self, message: impl Into<String>) -> Self {
        self.manage_failure = Some(message.into());
        self
    }
}

/// A plausible fake firmware provenance whose `commit` is the given image
/// identity.
pub fn fake_provenance(image_identity: &str) -> lpc_wire::FwProvenance {
    lpc_wire::FwProvenance {
        package: "fw-esp32".to_string(),
        commit: image_identity.to_string(),
        dirty: false,
        profile: "release-esp32".to_string(),
    }
}
