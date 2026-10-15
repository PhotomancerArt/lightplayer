//! The `emu_*` slice ABI: one guest slice at a time, driven from JavaScript.
//!
//! This module is the wasip1 module's second door. The
//! first is `_start`, which is the CLI binary and is untouched — a `lp-emu
//! esp32c6 run …` command line still works under a WASI runtime exactly as
//! it did. JavaScript never calls `_start`; it instantiates the module,
//! calls [`emu_create`], and then alternates [`emu_run`] with the byte and
//! control entry points, which is the only shape available to a host that
//! has one thread and must hand it back to the event loop.
//!
//! The consumer is `lp-app/lpa-studio-web/public/lpa-link/emulator_worker.js`
//! (AGPL, and on the other side of the MIT fence — it reaches this module by
//! URL and this crate knows nothing about it).
//!
//! # The contract
//!
//! - Every export takes and returns `i32` / `i64` only. Buffers are a
//!   pointer and a length into **this module's own memory**, which the host
//!   gets from [`emu_alloc`].
//! - A non-negative return is a count, a length, a flag or an outcome code.
//!   A negative return is an [`AbiError`]; [`emu_last_error`] carries the
//!   sentence, which is the difference between a failed build and a number.
//! - There is **one machine**. [`emu_create`] refuses a second; the host
//!   makes a new Worker, which is what "one board per Worker" means.
//! - **Wall time never enters here.** Nothing in this module reads a clock:
//!   [`emu_run`] takes a budget in *guest cycles* and the host converts from
//!   its own wall time on its own side of the wall. That is the same rule
//!   the machine has had since PD5, and it is why a hidden tab's guest falls
//!   behind rather than sprinting.
//!
//! # The static machine, and why it is sound
//!
//! The machine lives in one `static` slot reached through an `UnsafeCell`.
//! This is valid because the wasip1 build is single-threaded and every
//! export runs to completion before JS regains control: there is no
//! suspension point inside an export, no second thread that could hold a
//! reference, and no re-entrancy — JavaScript cannot call back into the
//! module while one of these is running, because the module has the only
//! thread. A `thread_local!` would express the same fact while also
//! depending on TLS initialisation the host never runs (A1).
//!
//! # ABI version
//!
//! [`emu_abi_version`] returns [`ABI_VERSION`]. The JS side asserts it at
//! startup and refuses a mismatch out loud rather than mis-reading a
//! renumbered outcome. Bump it whenever a signature, a code or a config key
//! changes meaning.
//!
//! # Why only half of this is `wasm32`-gated
//!
//! The [`exports`] module — the statics, the raw pointers and every
//! `extern "C"` function — is `#[cfg(target_arch = "wasm32")]`, because its
//! soundness argument is "there is one thread" and that is only true there.
//! Everything above it is ordinary Rust that compiles everywhere: the config
//! grammar, the outcome numbering and the error codes are the parts a
//! typo breaks, and they are the parts a `cargo test` on any host can hold
//! to account. The tests at the bottom of this file are that gate.

use lp_emu_esp_common::Strap;

use crate::flash::FlashBacking;
use crate::loader::{EfuseIdentity, ResetCause};
use crate::machine::{AppSource, BootMode, Esp32C6Builder, Outcome, TimeGrade, Uart0Sink, UsbHost};

/// The ABI revision. Mirrored by `emulator_wasi.js`'s `EMU_ABI`.
///
/// 2: the `seams=` / `seams_prefer=` config keys, [`emu_seams_info`] and the
/// `SEAM` outcome (docs/adr/2026-10-05-emulator-seams.md).
pub const ABI_VERSION: i32 = 2;

/// Every control reply this protocol produces fits here — `pins` with all
/// thirty-one pads is the longest, at a few hundred bytes. The host keeps
/// one scratch buffer of this size and never has to guess.
pub const REPLY_MAX: i32 = 4096;

/// What a negative return means.
///
/// The numbers are the ABI, so they are written out rather than derived, and
/// the JS side mirrors this list. Nothing here is recoverable *inside* the
/// module: an error leaves the machine exactly as it was.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum AbiError {
    /// No machine: [`emu_create`] has not been called, or [`emu_destroy`]
    /// has.
    NoMachine = -1,
    /// A second [`emu_create`] on a slot that already holds a machine.
    AlreadyCreated = -2,
    /// The config text is not valid: a key nobody knows, a value that does
    /// not parse, or bytes that are not UTF-8. [`emu_last_error`] says which.
    BadConfig = -3,
    /// `Esp32C6Builder::build` refused. [`emu_last_error`] carries its
    /// sentence, which is usually about the flash image's length.
    BuildFailed = -4,
    /// The output buffer is too small; nothing was written. Retry with a
    /// larger one ([`REPLY_MAX`] always suffices for a control reply).
    BufferTooSmall = -5,
    /// A pointer or length is not a range inside this module's memory, or a
    /// length is negative.
    BadBuffer = -6,
    /// A flash offset or length leaves the chip.
    OutOfRange = -7,
    /// The operation is not in this ABI version. The message names it.
    Unsupported = -8,
}

impl AbiError {
    /// The number a failing export returns.
    pub fn code(self) -> i32 {
        self as i32
    }
}

/// The outcome codes [`emu_run`] answers with. `Deadline` is zero because it
/// is the ordinary answer: the slice ran out, nothing went wrong.
pub mod outcome_code {
    pub const DEADLINE: i32 = 0;
    pub const EXIT_MATCHED: i32 = 1;
    pub const FAULT: i32 = 2;
    pub const STRICT_BUS: i32 = 3;
    pub const RESET: i32 = 4;
    pub const BREAKPOINT: i32 = 5;
    /// Unreachable through this ABI — no slice ever carries a wall timeout —
    /// and here so the mapping is total rather than a `_ =>` that would
    /// quietly rename a new outcome.
    pub const WALL_TIMEOUT: i32 = 6;
    /// The guest wrote itself to sleep (`Outcome::DeepSleep`); the wake
    /// itself is not modelled.
    pub const DEEP_SLEEP: i32 = 7;
    /// A strict seam request cannot engage (`Outcome::Seam`). A tab asks
    /// softly (`seams_prefer=`), so a Studio board never sees it.
    pub const SEAM: i32 = 8;
    /// A `--flash-cut` plan fired (`Outcome::PowerCut`). Unreachable through
    /// this ABI — a tab's configuration cannot arm one — and here so the
    /// mapping stays total.
    pub const POWER_CUT: i32 = 9;
}

pub fn code_for(outcome: &Outcome) -> i32 {
    match outcome {
        Outcome::Deadline { .. } => outcome_code::DEADLINE,
        Outcome::ExitMatched { .. } => outcome_code::EXIT_MATCHED,
        Outcome::Fault { .. } => outcome_code::FAULT,
        Outcome::StrictBus { .. } => outcome_code::STRICT_BUS,
        Outcome::Reset { .. } => outcome_code::RESET,
        Outcome::Breakpoint { .. } => outcome_code::BREAKPOINT,
        Outcome::WallTimeout { .. } => outcome_code::WALL_TIMEOUT,
        Outcome::DeepSleep { .. } => outcome_code::DEEP_SLEEP,
        Outcome::Seam { .. } => outcome_code::SEAM,
        Outcome::PowerCut { .. } => outcome_code::POWER_CUT,
    }
}

// ---- the board's configuration ------------------------------------------

/// The machine's configuration, parsed from the host's text.
///
/// Text rather than a packed struct because the two sides are a Rust crate
/// and a hand-written JavaScript file: a struct layout is a thing to keep in
/// step silently, and `key=value` lines are a thing that fails by name.
///
/// Public, and portable to every target, because the grammar is the part a
/// typo breaks and it should be readable and testable without a wasm
/// toolchain. The fields stay private: the way to get a machine out of one
/// is [`builder`](Self::builder).
pub struct Config {
    mac: Option<[u8; 6]>,
    boot: BootMode,
    grade: TimeGrade,
    flash_len: u32,
    strict: bool,
    reboot_on_reset: bool,
    usb_host: UsbHost,
    strap: Strap,
    reset_cause: ResetCause,
    /// Emulator seams (`seams=` strict, `seams_prefer=` soft). The capability
    /// defaults unless asked: `net=lan`, softly. A tab board is given no LAN,
    /// so an image that carries the network seam engages it on an **empty**
    /// LAN of its own (plan A8): a scan hears nothing and any network a user
    /// adds is honestly "not in range". There is no socket in the page.
    seams: lp_emu_esp_common::seam::SeamRequest,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            mac: None,
            boot: BootMode::RomUp,
            grade: TimeGrade::T1,
            flash_len: crate::flash::DEFAULT_FLASH_LEN,
            strict: false,
            // A board, not a run: a reset dance must reboot the chip rather
            // than end the world (the `emu serve` door's PD11 rule).
            reboot_on_reset: true,
            // No cable. The consumer plugs one in with a control line —
            // and a board that wants its boot log on the wire asks for
            // `usb_host=attached` instead, as `emu run --monitor` does.
            usb_host: UsbHost::Absent,
            strap: Strap::App,
            reset_cause: ResetCause::PowerOn,
            seams: lp_emu_esp_common::seam::SeamRequest::default(),
        }
    }
}

/// Parse `mac=aa:bb:cc:dd:ee:ff`, the spelling `--efuse-mac` takes.
pub fn parse_mac(text: &str) -> Option<[u8; 6]> {
    EfuseIdentity::parse_mac(text).ok()
}

impl Config {
    /// One `key=value` per line; `#` comments and blank lines are skipped.
    /// An unknown key is an error, not a shrug: a host that misspells a key
    /// would otherwise get a default board and no idea why.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut cfg = Config::default();
        for (n, raw) in text.lines().enumerate() {
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                return Err(format!("line {}: `{line}` is not key=value", n + 1));
            };
            let (key, value) = (key.trim(), value.trim());
            let bad = |what: &str| format!("line {}: `{value}` is not {what}", n + 1);
            match key {
                "mac" => cfg.mac = Some(parse_mac(value).ok_or_else(|| bad("a MAC address"))?),
                "boot" => {
                    cfg.boot = match value {
                        "rom-up" => BootMode::RomUp,
                        "direct" => BootMode::Direct,
                        _ => return Err(bad("a boot mode (rom-up, direct)")),
                    }
                }
                "grade" => {
                    cfg.grade =
                        TimeGrade::parse(value).map_err(|e| format!("line {}: {e}", n + 1))?
                }
                "flash_len" => {
                    cfg.flash_len = value.parse().map_err(|_| bad("a byte count"))?;
                }
                "strict" => cfg.strict = parse_flag(value).ok_or_else(|| bad("0 or 1"))?,
                "reboot_on_reset" => {
                    cfg.reboot_on_reset = parse_flag(value).ok_or_else(|| bad("0 or 1"))?
                }
                // `HostState::parse`'s own three words, not a second
                // spelling of them: `attached` is the cable in with the port
                // OPEN from power-on (the CLI's default, and the door's),
                // and `attached-idle` is the cable in with the port closed.
                // Inventing a fourth word here once inverted the pair and
                // cost an afternoon.
                "usb_host" => {
                    cfg.usb_host = UsbHost::parse(value)
                        .ok_or_else(|| bad("a host state (absent, attached, attached-idle)"))?;
                }
                "strap" => {
                    cfg.strap = match value {
                        "app" => Strap::App,
                        "download" => Strap::Download,
                        _ => return Err(bad("a strap (app, download)")),
                    }
                }
                "reset_cause" => {
                    cfg.reset_cause = ResetCause::parse(value)
                        .ok_or_else(|| bad("a reset cause (poweron, usb-uart-hpsys)"))?;
                }
                // `seams=led=fast` splits on the FIRST `=`, so the atom keeps
                // its own. Strict refuses an image it cannot engage on;
                // `seams_prefer=` engages what the image allows and says why
                // not otherwise — what a Studio board asks.
                "seams" | "seams_prefer" => {
                    let strength = if key == "seams" {
                        lp_emu_esp_common::seam::Strength::Strict
                    } else {
                        lp_emu_esp_common::seam::Strength::Soft
                    };
                    cfg.seams = std::mem::take(&mut cfg.seams)
                        .with(value, strength)
                        .map_err(|e| format!("line {}: {key}: {e}", n + 1))?;
                }
                other => {
                    return Err(format!(
                        "line {}: no config key `{other}` (mac, boot, grade, flash_len, strict, \
                         reboot_on_reset, usb_host, strap, reset_cause, seams, seams_prefer)",
                        n + 1
                    ));
                }
            }
        }
        Ok(cfg)
    }

    pub fn builder(&self, flash: FlashBacking, app: AppSource) -> Esp32C6Builder {
        let mut builder = Esp32C6Builder::new()
            .boot_mode(self.boot)
            .app(app)
            .flash(flash)
            .flash_len(self.flash_len)
            .time_grade(self.grade)
            .strict(self.strict)
            .reboot_on_reset(self.reboot_on_reset)
            .strap(self.strap)
            .reset_cause(self.reset_cause)
            .usb_host(self.usb_host)
            .seams(self.seams.clone())
            // Both logs are collected in memory and drained by the host; the
            // module has no stdout worth writing to.
            .uart0(Uart0Sink::Memory)
            .usb_sj_queue_source();
        if let Some(mac) = self.mac {
            builder = builder.efuse(EfuseIdentity {
                mac,
                ..EfuseIdentity::default()
            });
        }
        builder
    }
}

/// What [`emu_seams_info`] answers: this chip start's emulator seams, as one
/// line of JSON —
/// `{"label":"lp-emu:esp32c6:t2+led=fast","engaged":["led=fast"],"lines":[…],"none_why":null}`.
/// `none_why` is the reason a soft request engaged nothing, or `null`.
/// Valid after `emu_create` and after every restart; a ROM-up board's seams
/// resolve once its app runs, so until then `engaged` is empty and so is
/// `lines`.
pub fn seams_info_json(machine: &crate::machine::Esp32C6Machine) -> String {
    let s = machine.seams();
    let list = |items: &mut dyn Iterator<Item = String>| {
        items.map(|i| json_string(&i)).collect::<Vec<_>>().join(",")
    };
    format!(
        "{{\"label\":{},\"engaged\":[{}],\"lines\":[{}],\"none_why\":{}}}",
        json_string(&machine.configuration_label()),
        list(&mut s.engaged_impls().iter().map(|i| i.atom())),
        list(&mut s.lines.iter().cloned()),
        s.none_why
            .as_deref()
            .map_or_else(|| "null".to_string(), json_string)
    )
}

/// A JSON string literal: the quote, the backslash and control bytes escaped.
fn json_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

pub fn parse_flag(value: &str) -> Option<bool> {
    match value {
        "0" | "false" => Some(false),
        "1" | "true" => Some(true),
        _ => None,
    }
}

/// The `extern "C"` door itself: the statics, the raw pointers, and every
/// `emu_*` export. `wasm32` only — its soundness argument is "there is one
/// thread", which is a fact about that target and about nothing else.
#[cfg(target_arch = "wasm32")]
pub mod exports;

#[cfg(test)]
mod tests;
