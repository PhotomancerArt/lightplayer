//! Boot-state script for the fake ESP32 device.

use std::time::Duration;

use serde::Serialize;

/// Where the fake device keeps its (single) project storage, mirroring the
/// studio's demo storage id (`/projects/` root + `studio`).
pub const FAKE_DEVICE_PROJECT_DIR: &str = "/projects/studio";

/// The image identity the fake connector's scripted `FlashFirmware` writes
/// into the flashed device's provenance (`commit=` on the boot line).
pub const FAKE_IMAGE_IDENTITY: &str = "fake-esp32c6-image";

/// The base MAC the fake connector's scripted flash preflight "reads" from
/// efuse, in the UPPERCASE spelling a reporter is allowed to use — the
/// canonical stored form is lowercase, and the fake exists partly to prove
/// the normalization between them actually runs.
pub const FAKE_PROBED_MAC: &str = "60:55:F9:0A:0B:0C";

/// Stamped identity for a scripted LightPlayer state, written to
/// `/.lp/device.json` at the device's fs ROOT.
///
/// Serializes to the same JSON shape the studio writes when stamping
/// (`{"uid": "dev…", "name": "…"}`). The uid also rides the wire hello as
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
    /// speaking lp-link (a hello first on every link session).
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
    /// The factory efuse base MAC this board reports in its hello
    /// (`HardwareFacts::base_mac`) — the silicon half of device identity.
    /// `None` mimics pre-2026-08-03 firmware, which reported none.
    pub base_mac: Option<String>,
    /// Firmware identity for the boot line and the wire hello. Scripted
    /// flash (`fake_flash(image_identity)`) records the image identity here.
    /// Only the IDENTITY half: the fake's capabilities are the real host
    /// server's own, never scripted.
    pub provenance: lpc_wire::HelloIdentity,
    /// Never emit a hello on the wire (unsolicited or requested): mimics
    /// PRE-HELLO firmware whose server loop runs but never identifies
    /// itself. The device session's hello gate classifies this as
    /// `Incompatible`.
    pub suppress_hello: bool,
    /// Swallow every correlated response frame (`id != 0`) at the wire while
    /// unsolicited id-0 frames keep flowing: mimics firmware dropping
    /// responses under engine load (`[io_task] UART TX timed out` — the
    /// shared-UART starvation debt). Combined with
    /// [`heartbeat_interval`](Self::heartbeat_interval) this reproduces the
    /// wire that defeats any frame-gap timeout: alive, but never answering.
    ///
    /// The hello a board says first on every lp-link session is an answer
    /// too (to the host's handshake), so it is swallowed as well: a starved
    /// board identifies itself only once it heals and a hello request is
    /// answered.
    pub drop_responses: bool,
    /// Emit synthetic unsolicited id-0 heartbeat frames on this cadence,
    /// like real firmware's server loop (every 5 s on hardware). The fake's
    /// host `LpServer` never heartbeats on its own — heartbeat assembly
    /// lives in the firmware loop — so scripts that need a "live" wire
    /// opt in here.
    pub heartbeat_interval: Option<Duration>,
    /// Report this wire proto version in the hello instead of the build's
    /// [`lpc_wire::WIRE_PROTO_VERSION`]: mimics firmware built from an
    /// incompatible wire revision.
    pub proto_override: Option<u32>,
    /// Say THIS JSON, verbatim, wherever the board would say hello (the
    /// boot hello and every answer to a hello request): a board on another
    /// wire whose hello this build cannot decode, such as the wire-32 hello
    /// a fielded C6 sends (no `hardware.fs`). `None`: the server's own.
    pub hello_json_override: Option<String>,
    /// Auto-load the seeded project at boot, like real firmware's
    /// startup-project resume (fw-esp32c6 `boot::auto_load_project`): the
    /// server reports it via `project_list_loaded` from the first request.
    pub load_project_at_boot: bool,
    /// Absolute storage dir the seeded project files land in. Defaults to
    /// [`FAKE_DEVICE_PROJECT_DIR`]; override to mimic a device provisioned
    /// outside Studio (CLI uploads use other dirs under `/projects/`).
    pub project_dir: String,
    /// Answer `ClientRequest::SetEncoding` the way shipped firmware does
    /// (plan `lp-json-pack`): the hello names this build's dictionary, an
    /// opted-in link gets packed frames (`\n 0x00 'L' COBS 0x00`) until the
    /// port reopens or the device resets. On by default, as it is on every
    /// ESP firmware; `false` is a board that cannot pack (hello `0`).
    pub packs: bool,
    /// Files at absolute device paths beside the project (`/hardware.json`,
    /// `/.lp/access.json`, …), seeded at the fs root. A board rebuilt from a
    /// migrated flash image holds every file here.
    pub root_files: Vec<(String, Vec<u8>)>,
    /// Which partition layout the board's flash is on — what an
    /// `InspectLayout` reads, and where a raw read finds the files
    /// (`fake_flash_layout`). `Current` by default.
    pub layout: FakeFlashLayout,
    /// How the board's filesystem came up, reported in its hello
    /// (`HardwareFacts::fs`). `Mounted` by default, as a flashed board's is.
    pub fs_boot_state: lpc_wire::FsBootState,
    /// The board's whole flash, when a plan the fake executed wrote it —
    /// then THIS is what layout operations read, not an image synthesized
    /// from the files above (`fake_flash_layout`). `None` for a scripted
    /// board.
    pub flash: Option<std::sync::Arc<Vec<u8>>>,
    /// The board end's link configuration. `LinkConfig::usb()` by default: the
    /// C6 and S3 on USB-Serial-JTAG. A classic-shaped double (its UART0 link,
    /// plan `classic-uart-on-lp-link`) takes `LinkConfig::uart()` or a cut of it
    /// ([`with_link_config`](Self::with_link_config)).
    pub link_config: lpc_wire::lp_link::LinkConfig,
}

impl FakeLightPlayerState {
    pub fn new() -> Self {
        Self {
            boot_delay: Duration::ZERO,
            project_files: Vec::new(),
            identity: None,
            base_mac: None,
            provenance: fake_provenance("fake-firmware"),
            suppress_hello: false,
            drop_responses: false,
            heartbeat_interval: None,
            proto_override: None,
            hello_json_override: None,
            load_project_at_boot: false,
            project_dir: FAKE_DEVICE_PROJECT_DIR.to_string(),
            packs: true,
            root_files: Vec::new(),
            layout: FakeFlashLayout::Current,
            fs_boot_state: lpc_wire::FsBootState::Mounted,
            flash: None,
            link_config: lpc_wire::lp_link::LinkConfig::usb(),
        }
    }

    /// Files at absolute device paths, seeded at the fs root.
    pub fn with_root_files(mut self, files: Vec<(String, Vec<u8>)>) -> Self {
        self.root_files = files;
        self
    }

    /// A board still on the pre-repartition C6 layout (its files at
    /// `0x310000`): what a fielded board looks like to an `InspectLayout`.
    pub fn with_legacy_layout(mut self) -> Self {
        self.layout = FakeFlashLayout::Legacy;
        self
    }

    /// Report this filesystem boot state in the hello.
    pub fn with_fs_boot_state(mut self, fs: lpc_wire::FsBootState) -> Self {
        self.fs_boot_state = fs;
        self
    }

    /// The board end of the link on `config` rather than the USB preset: a
    /// classic-shaped double is `LinkConfig::uart()`, or the classic board's
    /// own timings on top of it (`min_rto` 200 ms, `syn_backoff` 4;
    /// `fw_esp32_common::uart_link::uart_board_link_config`).
    pub fn with_link_config(mut self, config: lpc_wire::lp_link::LinkConfig) -> Self {
        self.link_config = config;
        self
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

    /// Report a factory base MAC in the hello (the A1 identity source).
    pub fn with_base_mac(mut self, base_mac: &str) -> Self {
        self.base_mac = Some(base_mac.to_string());
        self
    }

    pub fn with_suppressed_hello(mut self) -> Self {
        self.suppress_hello = true;
        self
    }

    /// Drop every correlated response at the wire (id-0 frames still flow):
    /// the response-starved device of the 2026-08-24 request-idle defect.
    pub fn with_dropped_responses(mut self) -> Self {
        self.drop_responses = true;
        self
    }

    /// A board that cannot pack: its hello names no dictionary, and an
    /// opt-in is answered `json`.
    pub fn without_packing(mut self) -> Self {
        self.packs = false;
        self
    }

    /// Heartbeat on `interval` like real firmware's server loop.
    pub fn with_heartbeat_interval(mut self, interval: Duration) -> Self {
        self.heartbeat_interval = Some(interval);
        self
    }

    pub fn with_proto_override(mut self, proto: u32) -> Self {
        self.proto_override = Some(proto);
        self
    }

    /// Say `json` verbatim as every hello: a board on another wire whose
    /// hello this build may not be able to decode
    /// ([`Self::hello_json_override`]).
    pub fn with_hello_json(mut self, json: impl Into<String>) -> Self {
        self.hello_json_override = Some(json.into());
        self
    }

    /// Boot with the seeded project LOADED (the real-hardware shape since
    /// the standalone startup-resume): connect-time probes see a running
    /// project.
    pub fn with_loaded_project(mut self) -> Self {
        self.load_project_at_boot = true;
        self
    }

    /// Seed the project into `/projects/<dir>` instead of the default
    /// slot: mimics a device provisioned outside Studio (CLI upload).
    pub fn with_project_dir(mut self, dir: &str) -> Self {
        self.project_dir = format!("/projects/{dir}");
        self
    }
}

impl Default for FakeLightPlayerState {
    fn default() -> Self {
        Self::new()
    }
}

/// Which partition layout a scripted board's flash is on.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FakeFlashLayout {
    /// The layout the fake firmware package carries
    /// (`fake_flash_layout::fake_target_table`): nothing to migrate.
    Current,
    /// The frozen pre-repartition C6 layout: an Update must migrate.
    Legacy,
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
    /// The heartbeat cadence of the LightPlayer a scripted flash
    /// (`fake_flash`) installs. Real firmware heartbeats, and the
    /// loaded-project fact rides the heartbeat; `None` keeps the silent
    /// default every other script relies on.
    pub flashed_heartbeat_interval: Option<Duration>,
    /// What the boot ROM prints on every reset, before anything else: the
    /// chip's own banner. A C6's ROM by default; a classic ESP32's is its
    /// fixed build date ([`CLASSIC_ESP32_ROM_BANNER`]), which is how Studio
    /// tells the chips apart.
    pub rom_banner: Vec<String>,
    /// What the [`FakeBootState::ForeignFirmware`] state prints after the
    /// ROM banner: the Seeed XIAO C6's factory demo by default; a WLED
    /// controller's is its own boot line (`---WLED … INIT---`).
    pub foreign_banner: Vec<String>,
    /// The board's runtime pin map (`boards/<vendor>/<product>.json`), when
    /// the fake stands for a particular board. Like the efuse MAC it is the
    /// BOARD's, not a boot state's: every LightPlayer this device runs —
    /// scripted, or installed by a flash — opens its outputs against it, so
    /// a project on a pin the board does not have fails here the way it
    /// fails on silicon. `None` keeps the permissive outputs every other
    /// script relies on.
    pub board_manifest: Option<String>,
}

impl FakeDeviceScript {
    pub fn new(boot: FakeBootState) -> Self {
        Self {
            boot,
            manage_latency: Duration::ZERO,
            manage_failure: None,
            flashed_heartbeat_interval: None,
            rom_banner: vec![C6_ROM_BANNER.to_string()],
            foreign_banner: vec![XIAO_FACTORY_DEMO_LINE.to_string()],
            board_manifest: None,
        }
    }

    /// The ROM banner a classic ESP32 prints (its fixed build date), in
    /// place of the C6's — a classic-shaped board.
    pub fn with_classic_esp32_rom(mut self) -> Self {
        self.rom_banner = vec![
            CLASSIC_ESP32_ROM_BANNER.to_string(),
            "rst:0x1 (POWERON_RESET),boot:0x13 (SPI_FAST_FLASH_BOOT)".to_string(),
        ];
        self
    }

    /// What foreign firmware says at boot, after the ROM banner.
    pub fn with_foreign_banner(mut self, lines: &[&str]) -> Self {
        self.foreign_banner = lines.iter().map(|line| line.to_string()).collect();
        self
    }

    /// The board's runtime pin map, as its checked-in JSON.
    pub fn with_board_manifest(mut self, json: impl Into<String>) -> Self {
        self.board_manifest = Some(json.into());
        self
    }

    pub fn with_manage_latency(mut self, latency: Duration) -> Self {
        self.manage_latency = latency;
        self
    }

    pub fn with_manage_failure(mut self, message: impl Into<String>) -> Self {
        self.manage_failure = Some(message.into());
        self
    }

    pub fn with_flashed_heartbeat_interval(mut self, interval: Duration) -> Self {
        self.flashed_heartbeat_interval = Some(interval);
        self
    }
}

/// The ESP32-C6 mask ROM's banner, the fake's default chip.
pub const C6_ROM_BANNER: &str = "ESP-ROM:esp32c6-20220919";

/// The classic ESP32 mask ROM's banner: a fixed build date, the line Studio
/// reads the chip off.
pub const CLASSIC_ESP32_ROM_BANNER: &str = "ets Jun  8 2016 00:22:57";

/// The line the Seeed XIAO C6's factory demo prints at boot.
pub const XIAO_FACTORY_DEMO_LINE: &str = "Hello from Seeed Studio XIAO ESP32-C6";

/// A plausible fake firmware identity whose `commit` is the given image
/// identity. Its version is `unknown`, so Studio makes no older/newer claim
/// about a fake board unless a test sets one (`HelloIdentity::version`).
pub fn fake_provenance(image_identity: &str) -> lpc_wire::HelloIdentity {
    lpc_wire::HelloIdentity::new(
        "fw-esp32c6",
        "unknown",
        image_identity,
        false,
        "release-esp32",
    )
}
