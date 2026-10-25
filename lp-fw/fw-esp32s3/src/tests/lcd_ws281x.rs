//! The LED-channels spike's harness (feature `test_lcd_ws281x`, never
//! shipped): every case in [`crate::output::lcd_spike`] — five ragged lanes,
//! eight lanes at 124 and 500 LEDs, the CPU masked, the CPU stalled in a
//! flash erase and write, a 1,000-frame soak — repeated forever so a host
//! that attaches late still reads a whole run.

use esp_hal::peripherals::Peripherals;

use crate::output::lcd_spike::{Pads, Rig};

pub fn run(p: Peripherals) -> ! {
    let pads = Pads {
        lanes: [
            p.GPIO1.into(),
            p.GPIO2.into(),
            p.GPIO3.into(),
            p.GPIO4.into(),
            p.GPIO5.into(),
            p.GPIO6.into(),
            p.GPIO7.into(),
            p.GPIO10.into(),
        ],
        clk: p.GPIO11.into(),
        vsync: p.GPIO12.into(),
    };
    let flash = esp_storage::FlashStorage::new(p.FLASH);
    let mut rig = Rig::new(p.LCD_CAM, p.DMA_CH0, p.DMA_CH1, pads, Some(flash), 500 * 3);
    for run in 0.. {
        esp_println::println!("LCD: RUN {run}");
        rig.failures = 0;
        rig.run_all();
    }
    unreachable!()
}
