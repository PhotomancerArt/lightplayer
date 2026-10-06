//! The firmware's own warm reset: quiet the radio, then reset the HP system.
//!
//! `esp_hal::system::software_reset()` resets the HP system only
//! (`rst:0x3 LP_SW_HPSYS`). The modem — the Bluetooth and Wi-Fi MACs and
//! basebands, with their DMA — is not part of it, and on silicon the
//! Bluetooth controller kept writing into RAM across that reset, into memory
//! the second-stage bootloader had just loaded its code into
//! (docs/defects/2026-10-05-a-requested-reboot-crashed-the-c6-bootloader.md).
//!
//! The fix for that crash is placement: the radio's buffers no longer live
//! where any bootloader loads (`c_heap`, `init::HEAP_RADIO`). This is the
//! second half, for the resets the firmware makes itself: hold every modem
//! block in reset, release it, and only then reset. A stray write the
//! placement fix would only move somewhere harmless — into the radio region,
//! which the next boot may lay out differently after an update or with
//! Bluetooth switched off — does not happen at all. A reset the host makes
//! (RTS on the USB serial line) never runs this code, which is why it is the
//! second half and not the fix.

/// Hold the modem's blocks in reset, then release them: the radio's DMA
/// stops, and the next boot's radio bring-up starts from reset as it would
/// after a power cycle. Each field of `MODEM_SYSCON.MODEM_RST_CONF` (as the
/// `esp32c6` PAC names them; the register resets to 0, every block running)
/// holds one modem block in reset while set. esp-hal writes this register
/// block at boot (`rtc_cntl::rtc::esp32c6`), so its clock is on by then.
fn quiesce_modem() {
    let regs = esp_hal::peripherals::MODEM_SYSCON::regs();
    regs.modem_rst_conf().modify(|_, w| {
        w.rst_wifibb().set_bit();
        w.rst_wifimac().set_bit();
        w.rst_fe().set_bit();
        w.rst_btmac_apb().set_bit();
        w.rst_btmac().set_bit();
        w.rst_btbb_apb().set_bit();
        w.rst_btbb().set_bit();
        w.rst_etm().set_bit();
        w.rst_zbmac().set_bit();
        w.rst_modem_ecb().set_bit();
        w.rst_modem_ccm().set_bit();
        w.rst_modem_bah().set_bit();
        w.rst_modem_sec().set_bit();
        w.rst_ble_timer().set_bit();
        w.rst_data_dump().set_bit()
    });
    regs.modem_rst_conf().reset();
}

/// Reset the chip's HP system with the radio quiet. Every firmware-initiated
/// warm reset on this board goes through here.
pub fn restart() -> ! {
    quiesce_modem();
    esp_hal::system::software_reset()
}
