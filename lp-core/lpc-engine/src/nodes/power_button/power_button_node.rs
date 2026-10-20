//! Runtime power-button node: turns a button or switch into a power-off request.
//!
//! Two modes (see [`PowerButtonMode`]):
//!
//! - **hold** — a momentary button to ground. A short press publishes `click`;
//!   holding it for `hold_ms` requests power-off and suppresses the click.
//!   Wakes on the pin going low.
//! - **switch** — a latching switch that drives the pin high when on. Off
//!   (including off at boot) requests power-off, unless a host is attached
//!   over the device's own link. Wakes on the pin going high.
//!
//! The node only *requests*: the [`PowerService`](crate::PowerService) queues
//! the request and the server carries it out after the frame.

use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use lp_collection::VecMap;

use lpc_hardware::{ButtonActive, ButtonConfig, ButtonEventKind, ButtonInput, ButtonPull};
use lpc_model::{
    ControlMessage, HwEndpointSpec, MapSlot, PowerButtonDefView, PowerButtonMode, PowerButtonState,
    Revision, SlotAccess, SlotPath, SlotShapeRegistry, SlotShapeRegistryError,
};

use crate::engine::{PowerOffRequest, PowerWakeLevel};
use crate::node::{
    DestroyCtx, MemPressureCtx, NodeError, NodeRuntime, PressureLevel, ProduceResult,
    RuntimeStateShape, TickContext,
};

/// Switch mode: how long after opening the pin a switch that has never read
/// "on" is taken to be off. At least this long, and never less than twice the
/// debounce, so a device waking because the switch went on is not put back to
/// sleep before its first debounced press has had a chance to arrive.
const SWITCH_BOOT_SETTLE_MS: u64 = 250;

/// A failed open is retried after this long, then after twice that, and so on
/// up to [`OPEN_RETRY_MAX_MS`]. Short enough that a driver that binds a moment
/// late is found within a heartbeat or two; the cap is long enough that a
/// project naming a button the board does not have costs one failed open every
/// few seconds instead of one per frame. The board's endpoint set is fixed at
/// boot, so a button that is missing now is almost always missing for good.
const OPEN_RETRY_INITIAL_MS: u64 = 500;

/// The longest the node waits between attempts to open its button.
const OPEN_RETRY_MAX_MS: u64 = 4_000;

/// Runtime node for `kind = "PowerButton"` artifacts.
pub struct PowerButtonNode {
    state: PowerButtonState,
    def_view: Option<PowerButtonDefView>,
    input: Option<Box<dyn ButtonInput>>,
    opened: Option<OpenedPowerButton>,
    /// The last open that failed: what was asked, what went wrong, and when to
    /// ask again. Cleared by a successful open and by a change of config.
    open_failure: Option<OpenFailure>,
    opened_at_ms: u64,
    /// Hold mode: the press in progress.
    press: Option<PressState>,
    /// Switch mode: the debounced switch position.
    switch_on: bool,
    /// Switch mode: whether the "stayed awake for the host" note was logged
    /// for the current off period.
    host_hold_logged: bool,
    /// A power-off has been requested (or refused) for the current
    /// press/off period; cleared when the button is released or the switch
    /// turns back on.
    power_requested: bool,
    last_tick_revision: Option<Revision>,
    fallback_now_ms: u64,
}

impl PowerButtonNode {
    pub fn new() -> Self {
        Self {
            state: PowerButtonState::default(),
            def_view: None,
            input: None,
            opened: None,
            open_failure: None,
            opened_at_ms: 0,
            press: None,
            switch_on: false,
            host_hold_logged: false,
            power_requested: false,
            last_tick_revision: None,
            fallback_now_ms: 0,
        }
    }

    fn tick_power_button(&mut self, ctx: &mut TickContext<'_>) -> Result<(), NodeError> {
        if self.last_tick_revision == Some(ctx.revision()) {
            return Ok(());
        }
        self.last_tick_revision = Some(ctx.revision());
        self.state.click = MapSlot::default();

        let config = self.read_config(ctx)?;
        let now_ms = self.next_now_ms(ctx);
        // No button service (a host with no hardware): nothing to read, so
        // nothing to do. The node is inert rather than failing every frame.
        if !self.ensure_input(&config, ctx, now_ms)? {
            return Ok(());
        }

        let event = self
            .input
            .as_mut()
            .ok_or_else(|| NodeError::msg("power button input missing after open"))?
            .poll(now_ms);

        match config.mode {
            PowerButtonMode::Hold => self.tick_hold(ctx, &config, event, now_ms),
            PowerButtonMode::Switch => self.tick_switch(ctx, &config, event, now_ms),
        }
    }

    fn tick_hold(
        &mut self,
        ctx: &mut TickContext<'_>,
        config: &PowerButtonRuntimeConfig,
        event: Option<lpc_hardware::ButtonEvent>,
        now_ms: u64,
    ) -> Result<(), NodeError> {
        if let Some(event) = event {
            match event.kind() {
                ButtonEventKind::Pressed => {
                    self.press = Some(PressState { since_ms: now_ms });
                    self.power_requested = false;
                }
                ButtonEventKind::Released => {
                    if self.press.take().is_some() && !self.power_requested {
                        log::info!(
                            "PowerButton: click endpoint={} seq={}",
                            config.endpoint,
                            event.sequence()
                        );
                        self.state.click =
                            one_message_map(ctx.revision(), config.id, event.sequence());
                    }
                    self.power_requested = false;
                }
            }
        }

        if let Some(press) = &self.press
            && !self.power_requested
            && now_ms.saturating_sub(press.since_ms) >= config.hold_ms
        {
            log::info!(
                "PowerButton: held {} ms on {}; powering off",
                config.hold_ms,
                config.endpoint
            );
            self.power_requested = true;
            request_power_off(ctx, config)?;
        }
        Ok(())
    }

    fn tick_switch(
        &mut self,
        ctx: &mut TickContext<'_>,
        config: &PowerButtonRuntimeConfig,
        event: Option<lpc_hardware::ButtonEvent>,
        now_ms: u64,
    ) -> Result<(), NodeError> {
        if let Some(event) = event {
            self.switch_on = event.kind() == ButtonEventKind::Pressed;
            log::info!(
                "PowerButton: switch {} endpoint={}",
                if self.switch_on { "on" } else { "off" },
                config.endpoint
            );
            if self.switch_on {
                self.power_requested = false;
                self.host_hold_logged = false;
            }
        }

        let settle_ms = SWITCH_BOOT_SETTLE_MS.max(config.stable_ms.saturating_mul(2));
        let settled = now_ms.saturating_sub(self.opened_at_ms) >= settle_ms;
        if self.switch_on || self.power_requested || !settled {
            return Ok(());
        }

        // No power service (Studio's simulator, host tools): the switch is
        // read but can power nothing off, so a switch that reads "off" there
        // is not a fault.
        let Some(service) = ctx.power_service() else {
            return Ok(());
        };
        if service.host_attached() {
            if !self.host_hold_logged {
                log::info!(
                    "PowerButton: switch off on {}, staying awake while a host is attached",
                    config.endpoint
                );
                self.host_hold_logged = true;
            }
            return Ok(());
        }

        log::info!(
            "PowerButton: switch off on {}; powering off",
            config.endpoint
        );
        self.power_requested = true;
        request_power_off(ctx, config)
    }

    fn read_config(
        &mut self,
        ctx: &mut TickContext<'_>,
    ) -> Result<PowerButtonRuntimeConfig, NodeError> {
        let def = PowerButtonDefView::get_or_compile(&mut self.def_view, ctx.slot_shapes())
            .map_err(|e| NodeError::msg(format!("compile power button def view: {e}")))?;
        Ok(PowerButtonRuntimeConfig {
            endpoint: def.endpoint().get(ctx)?,
            mode: def.mode().get::<_, PowerButtonMode>(ctx)?,
            id: def.id().get::<_, u32>(ctx)?,
            stable_ms: u64::from(def.stable_ms().get::<_, u32>(ctx)?),
            hold_ms: u64::from(def.hold_ms().get::<_, u32>(ctx)?),
        })
    }

    /// Make sure the button is open for this config. `Ok(false)` means there
    /// is nothing to poll this frame: no button service, or a failed open that
    /// is waiting out its backoff.
    ///
    /// A failed open is tried again after [`OPEN_RETRY_INITIAL_MS`], doubling
    /// to [`OPEN_RETRY_MAX_MS`], or at once when the config changes. Each
    /// attempt walks every button endpoint the board has and formats an error,
    /// which is too much to do sixty times a second. The failure is returned
    /// as an error once per distinct message (the same ruling as
    /// `docs/defects/2026-07-28-tick-error-restated-every-frame.md`): a
    /// retry that fails the same way is silent.
    fn ensure_input(
        &mut self,
        config: &PowerButtonRuntimeConfig,
        ctx: &TickContext<'_>,
        now_ms: u64,
    ) -> Result<bool, NodeError> {
        if self.input.is_some() && self.opened.as_ref().is_some_and(|o| o.matches(config)) {
            return Ok(true);
        }
        if let Some(failure) = &self.open_failure
            && failure.key.matches(config)
            && now_ms < failure.retry_at_ms
        {
            return Ok(false);
        }

        let Some(service) = ctx.button_service() else {
            return Ok(false);
        };
        let button_config = match config.mode {
            PowerButtonMode::Hold => ButtonConfig::new(config.stable_ms),
            PowerButtonMode::Switch => ButtonConfig::new(config.stable_ms)
                .with_pull(ButtonPull::Down)
                .with_active(ButtonActive::High),
        };
        let input = match service.open_button_by_spec(&config.endpoint, button_config) {
            Ok(input) => input,
            Err(error) => {
                return match self.open_failed(config, &error, now_ms) {
                    Some(report) => Err(report),
                    None => Ok(false),
                };
            }
        };
        log::info!(
            "PowerButton: opened endpoint={} mode={} stable_ms={} hold_ms={}",
            config.endpoint,
            config.mode.as_str(),
            config.stable_ms,
            config.hold_ms
        );
        self.input = Some(input);
        self.opened = Some(OpenedPowerButton::of(config));
        self.open_failure = None;
        self.opened_at_ms = now_ms;
        self.press = None;
        self.switch_on = false;
        self.host_hold_logged = false;
        self.power_requested = false;
        Ok(true)
    }

    /// Record a failed open and schedule the next attempt. Returns the error
    /// to raise, or `None` when this is the failure the node already reported.
    fn open_failed(
        &mut self,
        config: &PowerButtonRuntimeConfig,
        error: &lpc_hardware::HardwareEndpointError,
        now_ms: u64,
    ) -> Option<NodeError> {
        let message = format!("open power button {}: {error}", config.endpoint);
        let previous = self.open_failure.take().filter(|f| f.key.matches(config));
        let backoff_ms = previous.as_ref().map_or(OPEN_RETRY_INITIAL_MS, |f| {
            f.backoff_ms.saturating_mul(2).min(OPEN_RETRY_MAX_MS)
        });
        let repeat = previous.is_some_and(|f| f.message == message);
        self.open_failure = Some(OpenFailure {
            key: OpenedPowerButton::of(config),
            message: message.clone(),
            backoff_ms,
            retry_at_ms: now_ms.saturating_add(backoff_ms),
        });
        (!repeat).then(|| NodeError::msg(message))
    }

    fn next_now_ms(&mut self, ctx: &TickContext<'_>) -> u64 {
        if let Some(now_ms) = ctx.now_ms() {
            self.fallback_now_ms = now_ms;
            return now_ms;
        }
        self.fallback_now_ms = self.fallback_now_ms.saturating_add(1);
        self.fallback_now_ms
    }
}

impl Default for PowerButtonNode {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PowerButtonRuntimeConfig {
    endpoint: HwEndpointSpec,
    mode: PowerButtonMode,
    id: u32,
    stable_ms: u64,
    hold_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct OpenedPowerButton {
    endpoint: HwEndpointSpec,
    mode: PowerButtonMode,
    stable_ms: u64,
}

impl OpenedPowerButton {
    fn of(config: &PowerButtonRuntimeConfig) -> Self {
        Self {
            endpoint: config.endpoint.clone(),
            mode: config.mode,
            stable_ms: config.stable_ms,
        }
    }

    /// Whether a button opened this way serves `config`, without cloning the
    /// endpoint to ask.
    fn matches(&self, config: &PowerButtonRuntimeConfig) -> bool {
        self.endpoint == config.endpoint
            && self.mode == config.mode
            && self.stable_ms == config.stable_ms
    }
}

/// A failed attempt to open the button.
#[derive(Clone, Debug)]
struct OpenFailure {
    /// What was being opened.
    key: OpenedPowerButton,
    /// The error text last raised for it.
    message: String,
    /// How long this failure waits before the next attempt.
    backoff_ms: u64,
    /// The first `now_ms` at which the next attempt is made.
    retry_at_ms: u64,
}

#[derive(Clone, Debug)]
struct PressState {
    since_ms: u64,
}

impl NodeRuntime for PowerButtonNode {
    fn produce(
        &mut self,
        _slot: &SlotPath,
        ctx: &mut TickContext<'_>,
    ) -> Result<ProduceResult, NodeError> {
        self.tick_power_button(ctx)?;
        Ok(ProduceResult::Produced)
    }

    fn consume(&mut self, ctx: &mut TickContext<'_>) -> Result<(), NodeError> {
        self.tick_power_button(ctx)
    }

    fn destroy(&mut self, _ctx: &mut DestroyCtx) -> Result<(), NodeError> {
        self.input = None;
        self.opened = None;
        self.open_failure = None;
        self.press = None;
        self.last_tick_revision = None;
        Ok(())
    }

    fn handle_memory_pressure(
        &mut self,
        _level: PressureLevel,
        _ctx: &mut MemPressureCtx,
    ) -> Result<(), NodeError> {
        Ok(())
    }

    fn runtime_state_slots(&self) -> Option<&dyn SlotAccess> {
        Some(&self.state)
    }

    fn register_runtime_state_shapes(
        &self,
        registry: &mut SlotShapeRegistry,
    ) -> Result<(), SlotShapeRegistryError> {
        PowerButtonState::register_runtime_state_shape(registry).map(|_| ())
    }
}

fn request_power_off(
    ctx: &mut TickContext<'_>,
    config: &PowerButtonRuntimeConfig,
) -> Result<(), NodeError> {
    // Hold mode on a host with no power service: the click still works, the
    // hold simply does nothing.
    let Some(service) = ctx.power_service() else {
        log::info!(
            "PowerButton: no power service here; not powering off {}",
            config.endpoint
        );
        return Ok(());
    };
    let request = PowerOffRequest {
        endpoint: config.endpoint.clone(),
        wake_level: match config.mode {
            PowerButtonMode::Hold => PowerWakeLevel::Low,
            PowerButtonMode::Switch => PowerWakeLevel::High,
        },
    };
    service.request_power_off(request).map_err(|error| {
        log::warn!("PowerButton: power-off refused: {error}");
        NodeError::msg(format!("power off: {error}"))
    })
}

fn one_message_map(revision: Revision, id: u32, seq: u32) -> MapSlot<u32, ControlMessage> {
    let mut entries = VecMap::new();
    entries.insert(id, ControlMessage::new(id, seq));
    MapSlot::with_version(revision, entries)
}

pub fn power_button_click_path() -> SlotPath {
    SlotPath::parse("click").expect("power button click path")
}

#[cfg(test)]
mod tests {
    use alloc::boxed::Box;
    use alloc::format;
    use alloc::rc::Rc;
    use alloc::vec::Vec;
    use core::cell::{Cell, RefCell};

    use lpc_hardware::{
        HardwareEndpointError, HardwareSystem, HwAddress, HwRegistry, VirtualButtonDriver,
        default_esp32c6_hardware_manifest,
    };
    use lpc_model::{
        ArtifactLocation, LpValue, MutationOp, NodeId, NodeName, SlotData, SlotEdit, SlotMapKey,
        TreePath, current_revision,
    };
    use lpc_registry::ParseCtx;
    use lpc_shared::time::TimeProvider;
    use lpfs::lp_path::AsLpPath;
    use lpfs::{LpFs, LpFsMemory};

    use super::*;
    use crate::dataflow::resolver::{QueryKey, ResolveLogLevel};
    use crate::engine::{
        ButtonService, EngineServices, LoadedProjectRuntime, PowerError, PowerService,
        ProjectLoader,
    };

    /// D1 is GPIO1 on the XIAO C6 — an LP GPIO, so a legal wake pin.
    const D1: u32 = 1;

    #[test]
    fn hold_short_press_clicks_and_does_not_power_off() {
        let mut h =
            Harness::load(r#""endpoint": "button:local:D1", "stable_ms": 1, "hold_ms": 100"#);

        h.set_pin(true);
        h.run(5, 10);
        h.set_pin(false);
        h.run(2, 10);

        assert!(h.power.requests.borrow().is_empty());
        assert!(h.saw_click, "a short press publishes click");
    }

    #[test]
    fn hold_long_press_requests_power_off_waking_low_and_suppresses_click() {
        let mut h =
            Harness::load(r#""endpoint": "button:local:D1", "stable_ms": 1, "hold_ms": 100"#);

        h.set_pin(true);
        h.run(20, 10);
        h.set_pin(false);
        h.run(3, 10);

        let requests = h.power.requests.borrow();
        assert_eq!(requests.len(), 1, "one request per hold");
        assert_eq!(requests[0].wake_level, PowerWakeLevel::Low);
        assert_eq!(requests[0].endpoint.as_str(), "button:local:D1");
        assert!(!h.saw_click, "a hold that powered off is not also a click");
        assert!(h.tick_errors.is_empty(), "{:?}", h.tick_errors);
    }

    #[test]
    fn switch_off_at_boot_requests_power_off_waking_high_after_the_settle() {
        let mut h = Harness::load(r#""endpoint": "button:local:D1", "mode": "switch""#);

        h.run(20, 10);
        assert!(
            h.power.requests.borrow().is_empty(),
            "no decision inside the {SWITCH_BOOT_SETTLE_MS} ms settle"
        );

        h.run(10, 10);
        let requests = h.power.requests.borrow();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].wake_level, PowerWakeLevel::High);
    }

    #[test]
    fn switch_on_at_boot_stays_up_until_switched_off() {
        let mut h = Harness::load(r#""endpoint": "button:local:D1", "mode": "switch""#);

        h.set_pin(true);
        h.run(50, 10);
        assert!(h.power.requests.borrow().is_empty());

        h.set_pin(false);
        h.run(10, 10);
        assert_eq!(h.power.requests.borrow().len(), 1);
        assert!(!h.saw_click, "switch mode never clicks");
    }

    #[test]
    fn switch_off_waits_while_a_host_is_attached() {
        let mut h = Harness::load(r#""endpoint": "button:local:D1", "mode": "switch""#);
        h.power.host_attached.set(true);

        h.run(60, 10);
        assert!(
            h.power.requests.borrow().is_empty(),
            "Studio keeps it awake"
        );

        h.power.host_attached.set(false);
        h.run(2, 10);
        assert_eq!(h.power.requests.borrow().len(), 1, "detach lets it sleep");
    }

    #[test]
    fn switch_back_on_rearms_a_refused_request() {
        let mut h = Harness::load(r#""endpoint": "button:local:D1", "mode": "switch""#);
        h.power.refuse.set(true);

        h.run(40, 10);
        assert_eq!(h.power.requests.borrow().len(), 1, "asked once, refused");
        h.run(20, 10);
        assert_eq!(
            h.power.requests.borrow().len(),
            1,
            "a refusal is not retried"
        );
        assert_eq!(
            h.tick_errors.len(),
            1,
            "the refusal surfaces once: {:?}",
            h.tick_errors
        );

        h.power.refuse.set(false);
        h.set_pin(true);
        h.run(5, 10);
        h.set_pin(false);
        h.run(5, 10);
        assert_eq!(h.power.requests.borrow().len(), 2, "on then off asks again");
    }

    /// Studio's simulator and the host tools have no power service, and some
    /// hosts no button service either: a switch that reads "off" there must
    /// not fault the project every frame.
    #[test]
    fn without_a_power_or_button_service_the_node_is_inert() {
        let mut h = Harness::load_with(
            r#""endpoint": "button:local:D1", "mode": "switch""#,
            Services::ButtonOnly,
        );
        h.run(40, 10);
        assert!(h.tick_errors.is_empty(), "{:?}", h.tick_errors);

        let mut h = Harness::load_with(
            r#""endpoint": "button:local:D1", "mode": "switch""#,
            Services::None,
        );
        h.run(40, 10);
        assert!(h.tick_errors.is_empty(), "{:?}", h.tick_errors);
    }

    /// A button the board does not have is asked for once, again after the
    /// backoff (doubling), and at once when the config changes. It is
    /// reported once, not every frame.
    #[test]
    fn a_failing_open_is_retried_on_a_backoff_and_at_once_on_a_config_change() {
        let mut h = Harness::load(r#""endpoint": "button:local:D99", "stable_ms": 1"#);

        h.run(1, 100); // t = 100
        assert_eq!(h.opens.opens.get(), 1, "tried at once");
        assert_eq!(h.tick_errors.len(), 1, "and reported: {:?}", h.tick_errors);
        assert!(h.tick_errors[0].contains("D99"), "{:?}", h.tick_errors);

        h.run(4, 100); // t = 200..500, inside the first 500 ms
        assert_eq!(h.opens.opens.get(), 1, "not again inside the backoff");

        h.run(1, 100); // t = 600
        assert_eq!(h.opens.opens.get(), 2, "again after it");
        assert_eq!(h.tick_errors.len(), 1, "the same failure is not restated");

        h.run(9, 100); // t = 700..1500, inside the doubled backoff
        assert_eq!(h.opens.opens.get(), 2, "the backoff doubled");
        h.run(1, 100); // t = 1600
        assert_eq!(h.opens.opens.get(), 3);
        assert_eq!(h.tick_errors.len(), 1);

        // A new endpoint is tried at once, mid-backoff, and a new failure is
        // a new thing to say.
        h.run(1, 100);
        h.set_endpoint("button:local:D98");
        h.run(1, 100);
        assert_eq!(h.opens.opens.get(), 4, "a config change retries at once");
        assert_eq!(h.tick_errors.len(), 2, "{:?}", h.tick_errors);
        assert!(h.tick_errors[1].contains("D98"), "{:?}", h.tick_errors);

        // A real one opens, works, and is never asked for again.
        h.set_endpoint("button:local:D1");
        h.run(1, 100);
        assert_eq!(h.opens.opens.get(), 5);
        h.set_pin(true);
        h.run(5, 10);
        h.set_pin(false);
        h.run(5, 10);
        assert!(h.saw_click, "the node works once the open succeeds");
        h.run(100, 100);
        assert_eq!(h.opens.opens.get(), 5, "an open button is not reopened");
        assert_eq!(h.tick_errors.len(), 2, "{:?}", h.tick_errors);
    }

    #[test]
    fn the_backoff_stops_doubling_at_its_cap() {
        let mut h = Harness::load(r#""endpoint": "button:local:D99", "stable_ms": 1"#);

        // 100 s of 100 ms frames. Attempts at 0.1, 0.6, 1.6, 3.6, 7.6 s, then
        // one every OPEN_RETRY_MAX_MS: (100 - 7.6) / 4 = 23 more.
        h.run(1000, 100);
        let attempts = h.opens.opens.get();
        assert!((27..=29).contains(&attempts), "{attempts} attempts");
        assert_eq!(h.tick_errors.len(), 1, "{:?}", h.tick_errors);
    }

    #[derive(Clone, Copy, PartialEq)]
    enum Services {
        All,
        ButtonOnly,
        None,
    }

    struct Harness {
        fs: LpFsMemory,
        rt: LoadedProjectRuntime,
        node: NodeId,
        opens: Rc<CountingButtons>,
        button: VirtualButtonDriver,
        power: Rc<FakePower>,
        time: Rc<TestTime>,
        saw_click: bool,
        tick_errors: Vec<alloc::string::String>,
    }

    impl Harness {
        fn load(fields: &str) -> Self {
            Self::load_with(fields, Services::All)
        }

        fn load_with(fields: &str, with: Services) -> Self {
            let fs = LpFsMemory::new();
            fs.write_file(
                "/project.json".as_path(),
                format!(
                    "{{\n  \"format\": {}\n}}\n",
                    lpc_model::PROJECT_FORMAT_VERSION
                )
                .as_bytes(),
            )
            .expect("container manifest");
            fs.write_file(
                "/module.json".as_path(),
                br#"{ "kind": "Module", "nodes": { "power": { "ref": "./power.json" } } }"#,
            )
            .expect("module");
            fs.write_file(
                "/power.json".as_path(),
                format!(r#"{{ "kind": "PowerButton", {fields} }}"#).as_bytes(),
            )
            .expect("power button");

            let registry = Rc::new(HwRegistry::new(default_esp32c6_hardware_manifest()));
            let button = VirtualButtonDriver::new(Rc::clone(&registry));
            let mut hardware = HardwareSystem::new(registry);
            hardware.add_button_driver(Box::new(button.clone()));
            let hardware = Rc::new(hardware);
            let opens = Rc::new(CountingButtons {
                inner: hardware,
                opens: Cell::new(0),
            });
            let button_service: Rc<dyn ButtonService> = opens.clone();
            let power = Rc::new(FakePower::default());
            let power_service: Rc<dyn PowerService> = power.clone();
            let time = Rc::new(TestTime::default());
            let time_provider: Rc<dyn TimeProvider> = time.clone();

            let mut services = EngineServices::new(TreePath::parse("/power.show").unwrap());
            if with != Services::None {
                services.set_button_service(Some(button_service));
            }
            if with == Services::All {
                services.set_power_service(Some(power_service));
            }
            services.set_time_provider(Some(time_provider));
            let rt = ProjectLoader::load_from_root(&fs, services).expect("load");
            let node = rt
                .tree()
                .lookup_sibling(rt.tree().root(), NodeName::parse("power").unwrap())
                .expect("power node");
            Self {
                fs,
                rt,
                node,
                opens,
                button,
                power,
                time,
                saw_click: false,
                tick_errors: Vec::new(),
            }
        }

        /// Edit the node's `endpoint` the way an authoring edit does.
        fn set_endpoint(&mut self, endpoint: &str) {
            let shapes = self.rt.engine().slot_shapes().clone();
            let (engine, registry) = self.rt.split_mut();
            let result = registry
                .mutate(
                    &self.fs,
                    MutationOp::PutSlotEdit {
                        artifact: ArtifactLocation::file("/power.json"),
                        edit: SlotEdit::assign_value(
                            SlotPath::parse("endpoint").unwrap(),
                            LpValue::String(endpoint.into()),
                        ),
                    },
                    current_revision(),
                    &ParseCtx { shapes: &shapes },
                )
                .expect("edit the endpoint");
            engine
                .apply_project_changes(&self.fs, registry, &result.changes)
                .expect("apply the edit");
        }

        fn set_pin(&self, active: bool) {
            self.button.set_pressed(HwAddress::gpio(D1), active);
        }

        /// Tick `frames` frames of `delta_ms` each, noting any click.
        fn run(&mut self, frames: u32, delta_ms: u32) {
            for _ in 0..frames {
                self.time
                    .now_ms
                    .set(self.time.now_ms.get() + u64::from(delta_ms));
                if let Err(error) = self.rt.tick(delta_ms) {
                    self.tick_errors.push(format!("{error:?}"));
                }
                if self.click_present() {
                    self.saw_click = true;
                }
            }
        }

        fn click_present(&mut self) -> bool {
            let (production, _) = self
                .rt
                .resolve_with_engine_host(
                    QueryKey::ProducedSlot {
                        node: self.node,
                        slot: power_button_click_path(),
                    },
                    ResolveLogLevel::Off,
                )
                .expect("click production");
            let SlotData::Map(map) = production.data().clone() else {
                panic!("click should be a map");
            };
            map.entries.contains_key(&SlotMapKey::U32(1))
        }
    }

    /// The hardware's button service, counting how often a button is opened.
    struct CountingButtons {
        inner: Rc<HardwareSystem>,
        opens: Cell<u32>,
    }

    impl ButtonService for CountingButtons {
        fn open_button_by_spec(
            &self,
            spec: &HwEndpointSpec,
            config: ButtonConfig,
        ) -> Result<Box<dyn ButtonInput>, HardwareEndpointError> {
            self.opens.set(self.opens.get() + 1);
            self.inner.open_button_by_spec(spec, config)
        }
    }

    #[derive(Default)]
    struct FakePower {
        requests: RefCell<Vec<PowerOffRequest>>,
        host_attached: Cell<bool>,
        refuse: Cell<bool>,
    }

    impl PowerService for FakePower {
        fn request_power_off(&self, request: PowerOffRequest) -> Result<(), PowerError> {
            self.requests.borrow_mut().push(request);
            if self.refuse.get() {
                return Err(PowerError::msg("refused by test"));
            }
            Ok(())
        }

        fn host_attached(&self) -> bool {
            self.host_attached.get()
        }
    }

    #[derive(Default)]
    struct TestTime {
        now_ms: Cell<u64>,
    }

    impl TimeProvider for TestTime {
        fn now_ms(&self) -> u64 {
            self.now_ms.get()
        }
    }
}
