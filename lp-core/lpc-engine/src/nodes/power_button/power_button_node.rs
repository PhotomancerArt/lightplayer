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

/// Runtime node for `kind = "PowerButton"` artifacts.
pub struct PowerButtonNode {
    state: PowerButtonState,
    def_view: Option<PowerButtonDefView>,
    input: Option<Box<dyn ButtonInput>>,
    opened: Option<OpenedPowerButton>,
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
        self.ensure_input(&config, ctx, now_ms)?;

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

        let service = ctx
            .power_service()
            .ok_or_else(|| NodeError::msg("power button node has no power service"))?;
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

    fn ensure_input(
        &mut self,
        config: &PowerButtonRuntimeConfig,
        ctx: &TickContext<'_>,
        now_ms: u64,
    ) -> Result<(), NodeError> {
        let opened = OpenedPowerButton {
            endpoint: config.endpoint.clone(),
            mode: config.mode,
            stable_ms: config.stable_ms,
        };
        if self.opened.as_ref() == Some(&opened) && self.input.is_some() {
            return Ok(());
        }

        let service = ctx
            .button_service()
            .ok_or_else(|| NodeError::msg("power button node has no button service"))?;
        let button_config = match config.mode {
            PowerButtonMode::Hold => ButtonConfig::new(config.stable_ms),
            PowerButtonMode::Switch => ButtonConfig::new(config.stable_ms)
                .with_pull(ButtonPull::Down)
                .with_active(ButtonActive::High),
        };
        let input = service
            .open_button_by_spec(&config.endpoint, button_config)
            .map_err(|error| {
                NodeError::msg(format!("open power button {}: {error}", config.endpoint))
            })?;
        log::info!(
            "PowerButton: opened endpoint={} mode={} stable_ms={} hold_ms={}",
            config.endpoint,
            config.mode.as_str(),
            config.stable_ms,
            config.hold_ms
        );
        self.input = Some(input);
        self.opened = Some(opened);
        self.opened_at_ms = now_ms;
        self.press = None;
        self.switch_on = false;
        self.host_hold_logged = false;
        self.power_requested = false;
        Ok(())
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
    let service = ctx
        .power_service()
        .ok_or_else(|| NodeError::msg("power button node has no power service"))?;
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
        HardwareSystem, HwAddress, HwRegistry, VirtualButtonDriver,
        default_esp32c6_hardware_manifest,
    };
    use lpc_model::{NodeId, NodeName, SlotData, SlotMapKey, TreePath};
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

    struct Harness {
        rt: LoadedProjectRuntime,
        node: NodeId,
        button: VirtualButtonDriver,
        power: Rc<FakePower>,
        time: Rc<TestTime>,
        saw_click: bool,
        tick_errors: Vec<alloc::string::String>,
    }

    impl Harness {
        fn load(fields: &str) -> Self {
            let fs = LpFsMemory::new();
            fs.write_file("/project.json".as_path(), b"{\n  \"format\": 10\n}\n")
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
            let button_service: Rc<dyn ButtonService> = hardware;
            let power = Rc::new(FakePower::default());
            let power_service: Rc<dyn PowerService> = power.clone();
            let time = Rc::new(TestTime::default());
            let time_provider: Rc<dyn TimeProvider> = time.clone();

            let mut services = EngineServices::new(TreePath::parse("/power.show").unwrap());
            services.set_button_service(Some(button_service));
            services.set_power_service(Some(power_service));
            services.set_time_provider(Some(time_provider));
            let rt = ProjectLoader::load_from_root(&fs, services).expect("load");
            let node = rt
                .tree()
                .lookup_sibling(rt.tree().root(), NodeName::parse("power").unwrap())
                .expect("power node");
            Self {
                rt,
                node,
                button,
                power,
                time,
                saw_click: false,
                tick_errors: Vec::new(),
            }
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
