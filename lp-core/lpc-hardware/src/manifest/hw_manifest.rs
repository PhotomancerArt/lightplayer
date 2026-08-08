use alloc::string::String;
use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

use crate::{HardwareTarget, HwAddress, HwCapability, HwResource, HwSoftLimits};

/// In-memory hardware profile for one board or virtual target.
///
/// The manifest is the source of truth for resources known to a
/// [`crate::HwRegistry`]. Drivers derive [`crate::HwEndpoint`]s from these
/// resources instead of hard-coding pins in the driver contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HwManifest {
    board_id: String,
    board_name: String,
    target: Option<HardwareTarget>,
    vendor: Option<String>,
    product: Option<String>,
    description: Option<String>,
    url: Option<String>,
    soft_limits: Option<HwSoftLimits>,
    resources: Vec<HwResource>,
    power_gates: Vec<HwPowerGate>,
}

impl HwManifest {
    pub fn new(
        board_id: impl Into<String>,
        board_name: impl Into<String>,
        resources: impl Into<Vec<HwResource>>,
    ) -> Self {
        Self {
            board_id: board_id.into(),
            board_name: board_name.into(),
            target: None,
            vendor: None,
            product: None,
            description: None,
            url: None,
            soft_limits: None,
            resources: resources.into(),
            power_gates: Vec::new(),
        }
    }

    pub fn virtual_single_rmt_gpio_board() -> Self {
        let mut resources = Vec::new();
        for pin in 0..=255 {
            let display_label = if pin == 18 {
                alloc::format!("D10")
            } else {
                alloc::format!("GPIO{pin}")
            };
            resources.push(HwResource::new(
                HwAddress::gpio(pin),
                [HwCapability::GpioOutput, HwCapability::GpioInput],
                display_label,
            ));
        }
        resources.push(HwResource::new(
            HwAddress::rmt_ws281x(0),
            [HwCapability::Rmt, HwCapability::Ws281xOutput],
            "RMT WS281x 0",
        ));
        resources.push(HwResource::new(
            HwAddress::radio(0),
            [HwCapability::Radio],
            "Virtual Radio 0",
        ));
        Self::new("virtual-single-rmt", "Virtual Single-RMT Board", resources)
            .with_target(HardwareTarget::Rv32imacEmu)
            .with_description("Virtual board profile for tests and emulation with GPIO resources, one shared WS281x/RMT resource, and one radio endpoint.")
    }

    /// Virtual board with four WS281x channels, as the XIAO ESP32-S3 Plus has.
    ///
    /// [`Self::virtual_single_rmt_gpio_board`] declares one timing resource, so
    /// a four-strip project running on it can only ever light one strip: the
    /// other three outputs fail to open and stay failed. That is a fine board
    /// for testing contention, and several tests rely on it for exactly that,
    /// but it makes the emulator disagree with the hardware it stands in for —
    /// on the desk S3 all four strips light.
    ///
    /// This board is the single-RMT one plus what the S3 has that it lacked:
    /// three more timing resources, and the `D9`/`D8`/`D7` labels the four-strip
    /// projects address. The GPIO numbers behind those three labels are the
    /// S3's own. `D10` keeps GPIO 18 rather than the S3's GPIO 9, because the
    /// virtual board has always put it there and nothing is gained by moving
    /// it; this is a stand-in, not a model of the real pinout.
    pub fn virtual_quad_rmt_gpio_board() -> Self {
        let mut resources = Vec::new();
        for pin in 0..=255 {
            let display_label = match pin {
                18 => alloc::format!("D10"),
                8 => alloc::format!("D9"),
                7 => alloc::format!("D8"),
                44 => alloc::format!("D7"),
                _ => alloc::format!("GPIO{pin}"),
            };
            resources.push(HwResource::new(
                HwAddress::gpio(pin),
                [HwCapability::GpioOutput, HwCapability::GpioInput],
                display_label,
            ));
        }
        for channel in 0..4 {
            resources.push(HwResource::new(
                HwAddress::rmt_ws281x(channel),
                [HwCapability::Rmt, HwCapability::Ws281xOutput],
                alloc::format!("RMT WS281x {channel}"),
            ));
        }
        resources.push(HwResource::new(
            HwAddress::radio(0),
            [HwCapability::Radio],
            "Virtual Radio 0",
        ));
        Self::new("virtual-quad-rmt", "Virtual Quad-RMT Board", resources)
            .with_target(HardwareTarget::Rv32imacEmu)
            .with_description(
                "Virtual board profile for tests and emulation with GPIO resources, four \
                 WS281x/RMT timing resources matching the XIAO ESP32-S3 Plus, and one radio \
                 endpoint.",
            )
    }

    pub fn board_id(&self) -> &str {
        &self.board_id
    }

    pub fn board_name(&self) -> &str {
        &self.board_name
    }

    pub fn target(&self) -> Option<HardwareTarget> {
        self.target
    }

    pub fn vendor(&self) -> Option<&str> {
        self.vendor.as_deref()
    }

    pub fn product(&self) -> Option<&str> {
        self.product.as_deref()
    }

    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    pub fn url(&self) -> Option<&str> {
        self.url.as_deref()
    }

    /// Measured soft-limit records, if this board carries any. Evidence,
    /// not policy: exceeding one warns and proceeds — see [`HwSoftLimits`].
    pub fn soft_limits(&self) -> Option<&HwSoftLimits> {
        self.soft_limits.as_ref()
    }

    pub fn resources(&self) -> &[HwResource] {
        &self.resources
    }

    /// Board-level power-gate descriptors, if this board carries any (empty
    /// slice when absent). Metadata only: see [`HwPowerGate`] for what a
    /// driver is expected to do with one.
    pub fn power_gates(&self) -> &[HwPowerGate] {
        &self.power_gates
    }

    pub fn with_target(mut self, target: HardwareTarget) -> Self {
        self.target = Some(target);
        self
    }

    pub fn with_vendor(mut self, vendor: impl Into<String>) -> Self {
        self.vendor = Some(vendor.into());
        self
    }

    pub fn with_product(mut self, product: impl Into<String>) -> Self {
        self.product = Some(product.into());
        self
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn with_url(mut self, url: impl Into<String>) -> Self {
        self.url = Some(url.into());
        self
    }

    pub fn with_soft_limits(mut self, soft_limits: HwSoftLimits) -> Self {
        self.soft_limits = Some(soft_limits);
        self
    }

    pub fn with_power_gates(mut self, power_gates: impl Into<Vec<HwPowerGate>>) -> Self {
        self.power_gates = power_gates.into();
        self
    }

    pub fn resource(&self, address: &HwAddress) -> Option<&HwResource> {
        self.resources
            .iter()
            .find(|resource| resource.address() == address)
    }

    pub fn with_reserved(mut self, address: HwAddress, reason: impl Into<String>) -> Self {
        let reason = reason.into();
        if let Some(resource) = self
            .resources
            .iter_mut()
            .find(|resource| resource.address() == &address)
        {
            *resource = resource.clone().reserved(reason);
        }
        self
    }

    pub fn map_resources(mut self, map_fn: impl Fn(HwResource) -> HwResource) -> Self {
        self.resources = self.resources.into_iter().map(map_fn).collect();
        self
    }
}

/// Which logic level asserts a [`HwPowerGate`]. Polarity varies by install —
/// the Dig-Quad's Q1R drives a user-supplied external relay board, which may
/// invert relative to a solid-state gate — so it lives in metadata, never in
/// code. See docs/future/2026-08-06-quinled-board-metadata-prep.md.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "kebab-case")]
pub enum HwGateLevel {
    High,
    Low,
}

/// "Assert this pin or these outputs are dead" — board metadata, not a
/// claimable resource. See docs/future/2026-08-06-quinled-board-metadata-prep.md.
///
/// Deliberately not a [`HwCapability`] and not an endpoint: a capability says
/// "this resource can do X"; this says the outputs are dead until it is
/// asserted. The output provider owns the assert/settle/transmit and
/// debounce/deassert state machine — this type carries only the constants
/// that state machine needs, not the state itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HwPowerGate {
    gpio: HwAddress,
    active_level: HwGateLevel,
    open_drain: bool,
    settle_ms: u32,
    off_debounce_ms: u32,
    feeds: Vec<HwAddress>,
    note: Option<String>,
}

impl HwPowerGate {
    /// Default trailing all-black debounce before deassert. Tuned as a
    /// power-saving heuristic driven by content, not a UI-responsiveness one
    /// — it wants seconds, not the ~600 ms a manual toggle would use.
    pub const DEFAULT_OFF_DEBOUNCE_MS: u32 = 5_000;

    pub fn new(gpio: HwAddress, active_level: HwGateLevel, settle_ms: u32) -> Self {
        Self {
            gpio,
            active_level,
            open_drain: false,
            settle_ms,
            off_debounce_ms: Self::DEFAULT_OFF_DEBOUNCE_MS,
            feeds: Vec::new(),
            note: None,
        }
    }

    pub fn with_open_drain(mut self, open_drain: bool) -> Self {
        self.open_drain = open_drain;
        self
    }

    pub fn with_off_debounce_ms(mut self, off_debounce_ms: u32) -> Self {
        self.off_debounce_ms = off_debounce_ms;
        self
    }

    pub fn with_feeds(mut self, feeds: impl Into<Vec<HwAddress>>) -> Self {
        self.feeds = feeds.into();
        self
    }

    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }

    /// `/gpio/N` of the gate pin. Profiles should also reserve this GPIO on
    /// the board's own resource entry so no driver claims it as a wire.
    pub fn gpio(&self) -> &HwAddress {
        &self.gpio
    }

    pub fn active_level(&self) -> HwGateLevel {
        self.active_level
    }

    pub fn open_drain(&self) -> bool {
        self.open_drain
    }

    /// Rail-up settling time before the first frame may transmit.
    pub fn settle_ms(&self) -> u32 {
        self.settle_ms
    }

    pub fn off_debounce_ms(&self) -> u32 {
        self.off_debounce_ms
    }

    /// Addresses of the outputs this gate feeds. Empty means all outputs.
    ///
    /// Entries name **endpoint** addresses (the `/gpio/N` a wire's endpoint
    /// resolves to), not `/rmt/ws281xK` timing slots: on the classic a slot
    /// is acquired per transmission, so it is not a stable identity to scope
    /// a rail by. A single-rail board should simply leave this empty.
    pub fn feeds(&self) -> &[HwAddress] {
        &self.feeds
    }

    /// Provenance for the timing constants (who measured, when).
    pub fn note(&self) -> Option<&str> {
        self.note.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_resource_by_internal_address_not_label() {
        let manifest = HwManifest::new(
            "board",
            "Board",
            [HwResource::new(
                HwAddress::gpio(18),
                [HwCapability::GpioOutput],
                "D6",
            )],
        );

        let resource = manifest.resource(&HwAddress::gpio(18)).unwrap();
        assert_eq!(resource.display_label(), "D6");
    }

    #[test]
    fn stores_optional_board_metadata() {
        let manifest = HwManifest::new("board", "Board", [])
            .with_target(HardwareTarget::Esp32c6)
            .with_vendor("vendor")
            .with_product("product")
            .with_description("A board profile")
            .with_url("https://example.com/board");

        assert_eq!(manifest.target(), Some(HardwareTarget::Esp32c6));
        assert_eq!(manifest.vendor(), Some("vendor"));
        assert_eq!(manifest.product(), Some("product"));
        assert_eq!(manifest.description(), Some("A board profile"));
        assert_eq!(manifest.url(), Some("https://example.com/board"));
    }
}
