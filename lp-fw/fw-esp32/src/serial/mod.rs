#[cfg(feature = "esp32c6")]
pub mod usb_serial;

pub mod shared_serial;

#[cfg(feature = "esp32c6")]
pub use usb_serial::Esp32UsbSerialIo;

#[cfg(all(
    feature = "esp32c6",
    any(
        not(any(
            feature = "test_rmt",
            feature = "test_dither",
            feature = "test_gpio",
            feature = "test_usb",
            feature = "test_msafluid",
            feature = "test_fluid_demo",
        )),
        feature = "test_json",
    ),
))]
pub mod io_task;

#[cfg(all(
    feature = "esp32c6",
    any(
        not(any(
            feature = "test_rmt",
            feature = "test_dither",
            feature = "test_gpio",
            feature = "test_usb",
            feature = "test_msafluid",
            feature = "test_fluid_demo",
        )),
        feature = "test_json",
    ),
))]
pub use io_task::io_task;
