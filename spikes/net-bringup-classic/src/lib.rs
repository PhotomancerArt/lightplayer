//! Shared bit-banged SMI (clause-22 MDIO) helpers for the bring-up bins.

#![no_std]

use esp_hal::{delay::Delay, gpio::Flex};
use esp_println::println;

pub fn identify(id1: u16, id2: u16) -> &'static str {
    match (id1, id2 & 0xFFF0) {
        (0x0007, 0xC0F0) => "LAN8720A",
        (0x0007, 0xC130) => "LAN8742A",
        (0x001C, 0xC810) => "RTL8201",
        (0x0243, 0x0C50) => "IP101",
        (0x0022, 0x1560) => "KSZ8081",
        _ => "unknown",
    }
}

fn clock_bit(mdc: &mut Flex<'_>, mdio: &mut Flex<'_>, bit: bool, delay: &Delay) {
    mdc.set_low();
    if bit {
        mdio.set_high();
    } else {
        mdio.set_low();
    }
    delay.delay_micros(2);
    mdc.set_high();
    delay.delay_micros(2);
}

fn read_bit(mdc: &mut Flex<'_>, mdio: &Flex<'_>, delay: &Delay) -> bool {
    mdc.set_low();
    delay.delay_micros(2);
    mdc.set_high();
    delay.delay_micros(2);
    mdio.is_high()
}

/// One IEEE 802.3 clause-22 read frame, bit-banged.
pub fn smi_read(mdc: &mut Flex<'_>, mdio: &mut Flex<'_>, phy: u8, reg: u8, delay: &Delay) -> u16 {
    mdio.set_output_enable(true);
    for _ in 0..32 {
        clock_bit(mdc, mdio, true, delay);
    }
    for &b in &[false, true, true, false] {
        clock_bit(mdc, mdio, b, delay);
    }
    for i in (0..5).rev() {
        clock_bit(mdc, mdio, (phy >> i) & 1 == 1, delay);
    }
    for i in (0..5).rev() {
        clock_bit(mdc, mdio, (reg >> i) & 1 == 1, delay);
    }
    mdio.set_output_enable(false);
    let _ = read_bit(mdc, mdio, delay);
    let _ = read_bit(mdc, mdio, delay);
    let mut value: u16 = 0;
    for _ in 0..16 {
        value = (value << 1) | read_bit(mdc, mdio, delay) as u16;
    }
    mdc.set_low();
    value
}

/// Sweep every ordered (MDC, MDIO) pair, all 32 PHY addresses. Returns hits.
pub fn sweep(pins: &mut [(u8, Flex<'static>)], skip: Option<u8>, delay: &Delay) -> u32 {
    let mut hits = 0;
    let n = pins.len();
    for i in 0..n {
        for j in 0..n {
            if i == j {
                continue;
            }
            let (ni, nj) = (pins[i].0, pins[j].0);
            if Some(ni) == skip || Some(nj) == skip {
                continue;
            }
            let (mdc, mdio) = if i < j {
                let (l, r) = pins.split_at_mut(j);
                (&mut l[i].1, &mut r[0].1)
            } else {
                let (l, r) = pins.split_at_mut(i);
                (&mut r[0].1, &mut l[j].1)
            };
            for addr in 0u8..32 {
                let id1 = smi_read(mdc, mdio, addr, 2, delay);
                if id1 == 0xFFFF || id1 == 0x0000 || id1 == 0x7FFF {
                    continue;
                }
                let id1b = smi_read(mdc, mdio, addr, 2, delay);
                if id1b != id1 {
                    continue;
                }
                let id2 = smi_read(mdc, mdio, addr, 3, delay);
                println!(
                    "  HIT mdc=GPIO{} mdio=GPIO{} addr={} ID1={:#06x} ID2={:#06x} => {}",
                    ni,
                    nj,
                    addr,
                    id1,
                    id2,
                    identify(id1, id2)
                );
                hits += 1;
            }
            let (a, b) = if i < j {
                let (l, r) = pins.split_at_mut(j);
                (&mut l[i].1, &mut r[0].1)
            } else {
                let (l, r) = pins.split_at_mut(i);
                (&mut r[0].1, &mut l[j].1)
            };
            a.set_output_enable(false);
            b.set_output_enable(false);
        }
    }
    hits
}
