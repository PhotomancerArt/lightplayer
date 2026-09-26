//! Power-off end to end through the server: a `PowerButton` in switch mode
//! asks, the server finishes the frame, unloads every project, then calls the
//! platform — exactly once, and never while a host is attached.

extern crate alloc;

use alloc::rc::Rc;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::cell::{Cell, RefCell};

use lp_gfx_lpvm::TargetLpvmGraphics;
use lpa_server::{
    ButtonService, LpGraphics, LpServer, PowerError, PowerOffRequest, PowerPlatform, PowerWakeLevel,
};
use lpc_hardware::{
    HardwareSystem, HwAddress, HwRegistry, VirtualButtonDriver, default_esp32c6_hardware_manifest,
};
use lpc_model::AsLpPath;
use lpc_shared::output::MemoryOutputProvider;
use lpc_shared::time::TimeProvider;
use lpfs::{LpFs, LpFsMemory};

#[test]
fn switch_off_unloads_projects_then_enters_power_off_once() {
    let mut rig = Rig::new(false);

    rig.run(40);

    let entered = rig.platform.entered.borrow();
    assert_eq!(entered.len(), 1, "one power-off");
    assert_eq!(entered[0].wake_level, PowerWakeLevel::High);
    assert_eq!(entered[0].endpoint.as_str(), "button:local:D0");
    drop(entered);
    assert!(
        rig.server
            .project_manager()
            .list_loaded_projects()
            .is_empty(),
        "every project is unloaded on the way to power-off"
    );
}

#[test]
fn switch_off_with_a_host_attached_stays_up() {
    let mut rig = Rig::new(true);

    rig.run(40);

    assert!(rig.platform.entered.borrow().is_empty());
    assert_eq!(rig.server.project_manager().list_loaded_projects().len(), 1);
}

#[test]
fn a_pin_the_platform_cannot_wake_from_is_refused_before_anything_unloads() {
    let mut rig = Rig::new(false);
    rig.platform.refuse.set(true);

    rig.run(40);

    assert!(rig.platform.entered.borrow().is_empty());
    assert_eq!(rig.server.project_manager().list_loaded_projects().len(), 1);
}

struct Rig {
    server: LpServer,
    platform: Rc<FakePlatform>,
    time: Rc<TestTime>,
    _button: VirtualButtonDriver,
}

impl Rig {
    fn new(host_attached: bool) -> Self {
        let fs = LpFsMemory::new();
        fs.write_file(
            "/projects/p/project.json".as_path(),
            format!(
                "{{\n  \"format\": {}\n}}\n",
                lpc_model::PROJECT_FORMAT_VERSION
            )
            .as_bytes(),
        )
        .unwrap();
        fs.write_file(
            "/projects/p/module.json".as_path(),
            br#"{ "kind": "Module", "nodes": { "power": { "ref": "./power.json" } } }"#,
        )
        .unwrap();
        fs.write_file(
            "/projects/p/power.json".as_path(),
            br#"{ "kind": "PowerButton", "endpoint": "button:local:D0", "mode": "switch" }"#,
        )
        .unwrap();

        let registry = Rc::new(HwRegistry::new(default_esp32c6_hardware_manifest()));
        let button = VirtualButtonDriver::new(Rc::clone(&registry));
        let mut hardware = HardwareSystem::new(registry);
        hardware.add_button_driver(Box::new(button.clone()));
        let button_service: Rc<dyn ButtonService> = Rc::new(hardware);
        // The switch stays off (the pin is never driven high).
        button.set_pressed(HwAddress::gpio(0), false);

        let output_provider: Rc<RefCell<dyn lpc_shared::output::OutputProvider>> =
            Rc::new(RefCell::new(MemoryOutputProvider::new()));
        let graphics: Arc<dyn LpGraphics> =
            Arc::new(TargetLpvmGraphics::new(lpa_server::DEVICE_SHADER_FRONTEND));
        let time = Rc::new(TestTime::default());
        let time_provider: Rc<dyn TimeProvider> = time.clone();
        let mut server = LpServer::new_with_hardware_services(
            output_provider,
            Box::new(fs),
            "projects/".as_path(),
            None,
            Some(time_provider),
            Some(button_service),
            None,
            graphics,
        );

        let platform = Rc::new(FakePlatform {
            host_attached: Cell::new(host_attached),
            ..FakePlatform::default()
        });
        server.set_power_platform(Some(platform.clone()));
        server.load_project("projects/p".as_path()).expect("load");

        Self {
            server,
            platform,
            time,
            _button: button,
        }
    }

    fn run(&mut self, frames: u32) {
        for _ in 0..frames {
            self.time.now_ms.set(self.time.now_ms.get() + 10);
            self.server.advance_frame(10).expect("frame");
        }
    }
}

#[derive(Default)]
struct FakePlatform {
    entered: RefCell<Vec<PowerOffRequest>>,
    host_attached: Cell<bool>,
    refuse: Cell<bool>,
}

impl PowerPlatform for FakePlatform {
    fn check_power_off(&self, _request: &PowerOffRequest) -> Result<(), PowerError> {
        if self.refuse.get() {
            return Err(PowerError::msg("not a wake-capable pin"));
        }
        Ok(())
    }

    fn enter_power_off(&self, request: &PowerOffRequest) -> Result<(), PowerError> {
        self.entered.borrow_mut().push(request.clone());
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
