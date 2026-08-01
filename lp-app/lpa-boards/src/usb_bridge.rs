//! USB-UART bridge chips and their per-OS driver needs.
//!
//! Driver need is a property of the bridge chip, not the board — so boards
//! declare which bridge they carry (`BoardDisplayFile::usb_bridge`) and this
//! module owns the facts: display name, USB VID:PID, and what each OS needs
//! before the board shows up as a serial port. Decision D5 in the
//! hardware-board-selection plan (2026-07-31), driven by a real failure: a
//! CH340K board enumerates on macOS but exposes NO /dev port — silence a
//! non-expert cannot tell from a dead board.
//!
//! The VID:PID pairs are here on purpose: connect-time surfaces can map a
//! browser-serial vid/pid to guidance *before* the board's identity is known.
//!
//! Authoring policy applies: set `usb_bridge` on a sidecar only when the
//! chip is verified (silkscreen, vendor docs, or a real enumeration).

use serde::{Deserialize, Serialize};

/// The USB-UART bridge (or native USB) a board's USB connector goes through.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "kebab-case")]
pub enum UsbBridge {
    /// The SoC's own USB-Serial-JTAG peripheral (S3, C6). No bridge chip,
    /// no driver anywhere.
    NativeUsbJtag,
    /// WCH CH340G — covered by Apple's built-in driver on modern macOS.
    #[serde(rename = "ch340g")]
    Ch340G,
    /// WCH CH340C — covered by Apple's built-in driver on modern macOS.
    #[serde(rename = "ch340c")]
    Ch340C,
    /// WCH CH340K — NOT covered by Apple's built-in driver (it matches only
    /// 0x7523 and CH9102F); needs WCH's DriverKit extension on macOS.
    #[serde(rename = "ch340k")]
    Ch340K,
    /// WCH CH9102F — covered by Apple's built-in driver on modern macOS.
    #[serde(rename = "ch9102f")]
    Ch9102F,
    /// Silicon Labs CP2102 — driverless on modern macOS.
    Cp2102,
}

/// How loudly a driver situation needs to be surfaced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DriverNeedLevel {
    /// Works out of the box.
    Ok,
    /// May need a vendor driver depending on system particulars.
    Info,
    /// Will not appear as a serial port until the user acts. Surface
    /// pre-purchase and at connect time.
    Warning,
}

/// The OS the guidance is for. Callers detect and pass it; this crate stays
/// platform-blind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostOs {
    MacOs,
    Windows,
    Linux,
    Other,
}

impl HostOs {
    pub fn display_name(self) -> &'static str {
        match self {
            Self::MacOs => "macOS",
            Self::Windows => "Windows",
            Self::Linux => "Linux",
            Self::Other => "this OS",
        }
    }
}

/// What one OS needs for one bridge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DriverGuidance {
    pub level: DriverNeedLevel,
    /// One-line status ("needs the WCH driver — the board is invisible
    /// without it").
    pub summary: &'static str,
    /// Ordered instructions. Empty when level is `Ok`.
    pub steps: &'static [&'static str],
}

const OK: DriverGuidance = DriverGuidance {
    level: DriverNeedLevel::Ok,
    summary: "works out of the box — no driver needed",
    steps: &[],
};

/// The macOS CH340K procedure, verified end-to-end on real hardware
/// 2026-07-31 (classic-ESP32 bring-up session). Keep verbatim: the
/// open-the-app activation step is the part everyone misses.
const CH340K_MACOS: DriverGuidance = DriverGuidance {
    level: DriverNeedLevel::Warning,
    summary: "invisible on macOS without the WCH driver — the board enumerates but no serial port appears",
    steps: &[
        "Install the driver: `brew install --cask wch-ch34x-usb-serial-driver` (or WCH's CH34XSER_MAC package from wch.cn)",
        "Open /Applications/CH34xVCPDriver.app — launching the app is what requests activation; nothing appears in System Settings until you do",
        "System Settings → General → Login Items & Extensions → Driver Extensions → enable CH34xVCPDriver",
        "Replug the board — the port appears as /dev/cu.wchusbserial*",
        "If macOS warns that extensions signed by 'Nanjing Qinheng Microelectronics' \"need to be updated\", ignore it — that refers to the legacy kext in the package; the DriverKit extension is what loads",
    ],
};

impl UsbBridge {
    pub fn display_name(self) -> &'static str {
        match self {
            Self::NativeUsbJtag => "native USB-Serial-JTAG",
            Self::Ch340G => "CH340G",
            Self::Ch340C => "CH340C",
            Self::Ch340K => "CH340K",
            Self::Ch9102F => "CH9102F",
            Self::Cp2102 => "CP2102",
        }
    }

    /// USB (vendor id, product id) as the chip enumerates.
    pub fn vid_pid(self) -> (u16, u16) {
        match self {
            // Espressif USB-Serial-JTAG.
            Self::NativeUsbJtag => (0x303A, 0x1001),
            // WCH: CH340G and CH340C share a product id; CH340K differs —
            // which is exactly why Apple's driver misses it.
            Self::Ch340G | Self::Ch340C => (0x1A86, 0x7523),
            Self::Ch340K => (0x1A86, 0x7522),
            Self::Ch9102F => (0x1A86, 0x55D4),
            // Silicon Labs.
            Self::Cp2102 => (0x10C4, 0xEA60),
        }
    }

    /// The bridge a (vid, pid) pair enumerated as, for connect-time guidance
    /// before the board is identified. Shared product ids resolve to the
    /// family representative (CH340G for 0x7523) — the guidance is identical.
    pub fn from_vid_pid(vid: u16, pid: u16) -> Option<Self> {
        match (vid, pid) {
            (0x303A, 0x1001) => Some(Self::NativeUsbJtag),
            (0x1A86, 0x7523) => Some(Self::Ch340G),
            (0x1A86, 0x7522) => Some(Self::Ch340K),
            (0x1A86, 0x55D4) => Some(Self::Ch9102F),
            (0x10C4, 0xEA60) => Some(Self::Cp2102),
            _ => None,
        }
    }

    /// What `os` needs before this bridge shows up as a serial port.
    ///
    /// Facts encoded only where verified: the CH340K macOS procedure ran
    /// end-to-end on hardware; Apple's built-in driver covers 0x7523 and
    /// CH9102F; CP2102 is driverless on modern macOS. Windows/Linux entries
    /// stay at `Info` with no steps until someone verifies a procedure.
    pub fn guidance(self, os: HostOs) -> DriverGuidance {
        match (self, os) {
            (Self::NativeUsbJtag, _) => OK,
            (Self::Ch340K, HostOs::MacOs) => CH340K_MACOS,
            (Self::Ch340G | Self::Ch340C | Self::Ch9102F | Self::Cp2102, HostOs::MacOs) => OK,
            // Linux ships ch341/cp210x kernel modules.
            (_, HostOs::Linux) => OK,
            (Self::Ch340G | Self::Ch340C | Self::Ch340K, HostOs::Windows) => DriverGuidance {
                level: DriverNeedLevel::Info,
                summary: "may need the WCH CH340/CH341 driver from wch.cn on Windows",
                steps: &[],
            },
            (Self::Ch9102F, HostOs::Windows) => DriverGuidance {
                level: DriverNeedLevel::Info,
                summary: "may need the WCH CH9102 driver from wch.cn on Windows",
                steps: &[],
            },
            (Self::Cp2102, HostOs::Windows) => DriverGuidance {
                level: DriverNeedLevel::Info,
                summary: "may need the Silicon Labs CP210x VCP driver on Windows",
                steps: &[],
            },
            (_, HostOs::Other) => DriverGuidance {
                level: DriverNeedLevel::Info,
                summary: "driver needs unknown on this platform",
                steps: &[],
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ch340k_is_a_macos_warning_with_steps() {
        let guidance = UsbBridge::Ch340K.guidance(HostOs::MacOs);
        assert_eq!(guidance.level, DriverNeedLevel::Warning);
        assert!(!guidance.steps.is_empty());
    }

    #[test]
    fn apple_covered_bridges_are_ok_on_macos() {
        for bridge in [
            UsbBridge::NativeUsbJtag,
            UsbBridge::Ch340G,
            UsbBridge::Ch340C,
            UsbBridge::Ch9102F,
            UsbBridge::Cp2102,
        ] {
            assert_eq!(bridge.guidance(HostOs::MacOs).level, DriverNeedLevel::Ok);
        }
    }

    #[test]
    fn vid_pid_round_trips_to_guidance_equivalent_bridges() {
        for bridge in [
            UsbBridge::NativeUsbJtag,
            UsbBridge::Ch340G,
            UsbBridge::Ch340K,
            UsbBridge::Ch9102F,
            UsbBridge::Cp2102,
        ] {
            let (vid, pid) = bridge.vid_pid();
            let resolved = UsbBridge::from_vid_pid(vid, pid).expect("known pair resolves");
            // Same guidance on every OS — shared-pid families may resolve to
            // a representative variant.
            for os in [HostOs::MacOs, HostOs::Windows, HostOs::Linux, HostOs::Other] {
                assert_eq!(resolved.guidance(os), bridge.guidance(os));
            }
        }
        assert_eq!(UsbBridge::from_vid_pid(0xFFFF, 0xFFFF), None);
    }

    #[test]
    fn serializes_kebab_case() {
        assert_eq!(
            serde_json::to_string(&UsbBridge::Ch340K).unwrap(),
            "\"ch340k\""
        );
        assert_eq!(
            serde_json::from_str::<UsbBridge>("\"native-usb-jtag\"").unwrap(),
            UsbBridge::NativeUsbJtag
        );
    }
}
