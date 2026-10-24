//! The pad probe (feature `ws281x_pad_probe`, never shipped): reads each open
//! lane's pad registers at bind and on every frame, and says when one moved.
//!
//! Written for the D6 defect
//! (`docs/defects/2026-10-24-the-s3s-d6-pad-garbles-its-lcd-ws281x-lane.md`):
//! GPIO43 is UART0's TX pad, so the question is whether anything puts U0TXD
//! back on it, or writes UART0, while the lane runs. Per lane it reads
//! `IO_MUX_GPIOn_REG` (MCU_SEL, FUN_IE, drive, pulls), `GPIO_FUNCn_OUT_SEL_CFG_REG`
//! (the matrix's choice of signal), the pad's bit of `GPIO_ENABLE*`, its
//! `GPIO_PINn_REG` (open drain), and its input level just before a frame
//! starts, when the bus has been idle (low) for the latch. UART0's TX_DONE
//! raw interrupt is cleared at bind and read every frame: set means something
//! sent a byte on UART0 since.

use lp_ws281x::parallel::LANES;

const IO_MUX: usize = 0x6000_9004;
const GPIO: usize = 0x6000_4000;
const GPIO_ENABLE: usize = GPIO + 0x20;
const GPIO_ENABLE1: usize = GPIO + 0x2c;
const GPIO_IN: usize = GPIO + 0x3c;
const GPIO_IN1: usize = GPIO + 0x40;
const GPIO_PIN: usize = GPIO + 0x74;
const GPIO_FUNC_OUT_SEL: usize = GPIO + 0x554;
const UART0_INT_RAW: usize = 0x6000_0004;
const UART0_INT_CLR: usize = 0x6000_0010;
const UART0_STATUS: usize = 0x6000_001c;
const UART0_TX_DONE: u32 = 1 << 14;
/// Frames between summary lines.
const LOG_EVERY: u32 = 500;

#[derive(Clone, Copy, PartialEq, Eq)]
struct PadRegs {
    io_mux: u32,
    out_sel: u32,
    enabled: bool,
    pin: u32,
}

impl PadRegs {
    fn read(gpio: u8) -> Self {
        let n = gpio as usize;
        let (enable_reg, bit) = if n < 32 {
            (GPIO_ENABLE, n)
        } else {
            (GPIO_ENABLE1, n - 32)
        };
        Self {
            io_mux: reg(IO_MUX + 4 * n),
            out_sel: reg(GPIO_FUNC_OUT_SEL + 4 * n),
            enabled: reg(enable_reg) & (1 << bit) != 0,
            pin: reg(GPIO_PIN + 4 * n),
        }
    }
}

impl core::fmt::Display for PadRegs {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "io_mux={:#06x} (mcu_sel={} fun_ie={} drv={} wpu={} wpd={}) out_sel_cfg={:#05x} \
             (sig={} oen_sel={}) enabled={} pin={:#x}",
            self.io_mux,
            (self.io_mux >> 12) & 0x7,
            (self.io_mux >> 9) & 1,
            (self.io_mux >> 10) & 0x3,
            (self.io_mux >> 8) & 1,
            (self.io_mux >> 7) & 1,
            self.out_sel,
            self.out_sel & 0x1ff,
            (self.out_sel >> 10) & 1,
            self.enabled,
            self.pin,
        )
    }
}

#[derive(Clone, Copy)]
struct ProbedPad {
    gpio: u8,
    last: PadRegs,
    changes: u32,
    /// Frames whose start found the pad high (the bus idles low).
    high_at_start: u32,
}

pub struct PadProbe {
    pads: [Option<ProbedPad>; LANES],
    frames: u32,
    /// Frames whose read found UART0's TX_DONE set since the last clear.
    uart_tx_frames: u32,
}

impl PadProbe {
    pub fn new() -> Self {
        log::info!("[pad-probe] armed");
        Self {
            pads: [None; LANES],
            frames: 0,
            uart_tx_frames: 0,
        }
    }

    /// The lane's pad was just bound: record what bind left, enable its input
    /// (the level check reads it) and clear UART0's TX_DONE.
    pub fn bind(&mut self, lane: u8, gpio: u8) {
        let before = PadRegs::read(gpio);
        let io_mux = IO_MUX + 4 * gpio as usize;
        // SAFETY: a read-modify-write of FUN_IE on a pad this lane's lease holds.
        unsafe { (io_mux as *mut u32).write_volatile(reg(io_mux) | 1 << 9) };
        let after = PadRegs::read(gpio);
        let uart = reg(UART0_INT_RAW);
        clear_uart_tx_done();
        log::info!(
            "[pad-probe] bind lane={lane} gpio={gpio}: {before}; with fun_ie: {after}; \
             uart0 int_raw={uart:#x} status={:#x}",
            reg(UART0_STATUS),
        );
        self.pads[lane as usize] = Some(ProbedPad {
            gpio,
            last: after,
            changes: 0,
            high_at_start: 0,
        });
    }

    pub fn unbind(&mut self, lane: u8) {
        self.pads[lane as usize] = None;
    }

    /// Just before a frame starts (bus idle since the last latch).
    pub fn before_frame(&mut self) {
        self.frames = self.frames.wrapping_add(1);
        let uart = reg(UART0_INT_RAW);
        if uart & UART0_TX_DONE != 0 {
            self.uart_tx_frames += 1;
            if self.uart_tx_frames <= 8 {
                log::warn!(
                    "[pad-probe] frame {}: UART0 sent something (int_raw={uart:#x} status={:#x})",
                    self.frames,
                    reg(UART0_STATUS),
                );
            }
            clear_uart_tx_done();
        }
        let frames = self.frames;
        for (lane, pad) in self.pads.iter_mut().enumerate() {
            let Some(pad) = pad else { continue };
            let now = PadRegs::read(pad.gpio);
            if now != pad.last {
                pad.changes += 1;
                log::warn!(
                    "[pad-probe] frame {frames}: lane={lane} gpio={} MOVED from {} to {now}",
                    pad.gpio,
                    pad.last,
                );
                pad.last = now;
            }
            if level(pad.gpio) {
                pad.high_at_start += 1;
            }
        }
        if frames % LOG_EVERY == 0 {
            for (lane, pad) in self.pads.iter().enumerate() {
                let Some(pad) = pad else { continue };
                log::info!(
                    "[pad-probe] frames={frames} lane={lane} gpio={} changes={} high_at_start={} \
                     uart_tx_frames={} now: {}",
                    pad.gpio,
                    pad.changes,
                    pad.high_at_start,
                    self.uart_tx_frames,
                    pad.last,
                );
            }
        }
    }
}

fn level(gpio: u8) -> bool {
    let n = gpio as usize;
    if n < 32 {
        reg(GPIO_IN) & (1 << n) != 0
    } else {
        reg(GPIO_IN1) & (1 << (n - 32)) != 0
    }
}

fn clear_uart_tx_done() {
    // SAFETY: a write-1-to-clear of one raw interrupt bit; nothing in this
    // image uses UART0's interrupts.
    unsafe { (UART0_INT_CLR as *mut u32).write_volatile(UART0_TX_DONE) };
}

fn reg(address: usize) -> u32 {
    // SAFETY: a read of a memory-mapped peripheral register on this chip.
    unsafe { (address as *const u32).read_volatile() }
}
