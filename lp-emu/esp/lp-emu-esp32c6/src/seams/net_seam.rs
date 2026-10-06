//! `net=lan`: the C6 answers the network seam (`lp_seam::net`) from the
//! board's endpoint on its virtual LAN.
//!
//! A **capability** seam in the switch shape: one atom arms all nine calls'
//! entries and `net_mac`'s engaged byte, so the firmware, reading that byte
//! at network bring-up, plugs its seam-backed station and frame device in
//! under the same IP stack instead of starting the radio. Each call's answer
//! is below; none charges a cycle (`seam_impl`, "What an answer costs"), and
//! each **writes only memory the call handed over** (`net_mac`'s `out`,
//! `net_take_frame`'s `buf`, `net_scan_take`'s `buf`) and **reads only the
//! call's own buffers** (`net_give_frame`'s frame, `net_connect`'s name and
//! password, which go nowhere but the LAN's join decision).
//!
//! # The board on its LAN
//!
//! The board is `<board>/net` ([`net_endpoint`]) on a [`SharedLan`]: the one
//! a host gave the builder ([`crate::machine::Esp32C6Builder::lan`]), or, with
//! none given, an **empty private LAN** made the first time the seam engages
//! — nothing in range, so a scan hears nothing and a join ends `not found`.
//! Its station MAC is the eFuse base MAC ([`crate::loader::EfuseIdentity`]),
//! the address ESP-IDF gives the Wi-Fi station. The board is attached at its
//! first engage (or at build, for a given LAN), and every chip start after
//! that keeps the attachment, the MAC and the lease but resets the station: a
//! restarted radio has no link and no untaken events.
//!
//! # Who carries the frames
//!
//! On a [`LanDriver::Runner`] LAN the lockstep runner does, at its quantum
//! boundaries. On a self-driven or wall-clock LAN this machine does: at the
//! top of each slice it pumps its endpoint into the LAN ([`SharedLan::pump`]),
//! and the LAN's next due cycle bounds its slices and its idle skip, so a
//! guest asleep in `wfi` does not sleep through a join landing or a DHCP
//! reply.
//!
//! # The wake
//!
//! The endpoint's wake bit is raised (through the pacer: one outstanding, the
//! spacing) when the endpoint holds a frame **or** the station holds an event
//! ([`super::seam_wake`]).

use lp_emu_core::sched::Cycles;
use lp_emu_esp_common::seam::net::{
    LanConfig, LanDriver, SharedLan, StationEvent, VirtualLan, net_endpoint,
};
use lp_emu_esp_common::seam::{EndpointEvent, EndpointId};
use lp_seam::net as abi;

use crate::machine::Esp32C6Machine;
use crate::memmap;

impl Esp32C6Machine {
    /// The LAN this board's network seam answers from, once it has one.
    pub fn lan(&self) -> Option<&SharedLan> {
        self.seams.lan.as_ref()
    }

    /// This board's network endpoint, `<board>/net`.
    pub fn net_endpoint_id(&self) -> EndpointId {
        net_endpoint(self.seams.board)
    }

    /// This board's address on its LAN, once its DHCP exchange finished.
    pub fn net_address(&self) -> Option<std::net::Ipv4Addr> {
        self.seams.lan.as_ref()?.address(self.net_endpoint_id())
    }

    /// `net=lan` engaged this chip start, on endpoint `index`: make sure the
    /// board has a LAN and is on it.
    pub(crate) fn net_engage(&mut self, index: usize) {
        let lan = self
            .seams
            .lan
            .get_or_insert_with(|| {
                SharedLan::new(
                    VirtualLan::new(LanConfig::new(memmap::CYCLES_PER_US)),
                    LanDriver::SelfDriven,
                )
            })
            .clone();
        let id = self.seams.endpoints[index].id;
        if let Err(why) = lan.attach(id, self.efuse().mac) {
            self.seams
                .announce(format!("SEAM net=lan: board {id} is not on its LAN: {why}"));
        }
        self.seams.net_endpoint = Some(index);
        self.seams.net_pumps = lan.driver() != LanDriver::Runner;
    }

    /// A chip start: the board's station forgets its link and its events.
    pub(crate) fn net_on_chip_start(&mut self) {
        if let Some(lan) = &self.seams.lan {
            lan.reset_board(net_endpoint(self.seams.board));
        }
    }

    /// The top of a slice on a self-driven LAN: hand over what the guest
    /// gave, drive the LAN if anything is due, take in what arrived.
    pub(crate) fn net_pump(&mut self, now: Cycles) {
        let (Some(lan), Some(i)) = (&self.seams.lan, self.seams.net_endpoint) else {
            return;
        };
        lan.pump(&mut self.seams.endpoints[i], now);
    }

    /// The guest cycle by which this machine must pump its LAN again, or
    /// `None` (a runner's LAN, or nothing due).
    pub(crate) fn net_deadline(&self) -> Option<Cycles> {
        if !self.seams.net_pumps {
            return None;
        }
        let (lan, i) = (self.seams.lan.as_ref()?, self.seams.net_endpoint?);
        let now = self.cycles();
        if self.seams.endpoints[i].has_outbound() {
            return Some(now);
        }
        lan.deadline(now)
    }

    /// Whether endpoint `index` is the network endpoint and its station has
    /// an event waiting (the wake's second kind of work).
    pub(crate) fn net_has_event(&self, index: usize) -> bool {
        self.seams.net_endpoint == Some(index)
            && self
                .seams
                .lan
                .as_ref()
                .is_some_and(|l| l.has_event(self.seams.endpoints[index].id))
    }

    /// One network seam call, `a0..a3` as the call passed them; the value is
    /// the call's `u32` result. See [the module docs](self).
    pub(crate) fn serve_net(&mut self, decl_id: u16) -> u32 {
        let r = self.harts[0].regs();
        let (a0, a1, a2, a3) = (r[10] as u32, r[11] as u32, r[12] as u32, r[13] as u32);
        let (Some(lan), Some(i)) = (self.seams.lan.clone(), self.seams.net_endpoint) else {
            return 0;
        };
        let id = self.seams.endpoints[i].id;
        let now = self.cycles();
        let answer = match decl_id {
            lp_seam::net_mac::ID => {
                let mac = self.efuse().mac;
                u32::from(self.poke_bytes(a0, &mac))
            }
            lp_seam::net_take_frame::ID => {
                match self.seams.endpoints[i].take_one(a1 as usize) {
                    Some(frame) if self.poke_bytes(a0, &frame) => {
                        if let Some(s) = self.seams.wake_stats.get_mut(i) {
                            s.bytes_taken += frame.len() as u64;
                        }
                        frame.len() as u32
                    }
                    // A buffer the bus refused: the frame is gone, as a
                    // driver's would be on a bad DMA address.
                    Some(_) | None => 0,
                }
            }
            lp_seam::net_give_frame::ID => {
                let len = a1 as usize;
                if len == 0 || len > abi::MAX_FRAME_LEN || !lan.link_up(id) {
                    0
                } else if let Some(bytes) = self.peek_call_buffer(a0, len) {
                    self.seams.endpoints[i].push_outbound(EndpointEvent { at: now, bytes });
                    1
                } else {
                    0
                }
            }
            lp_seam::net_link::ID => u32::from(lan.link_up(id)),
            lp_seam::net_scan_start::ID => u32::from(lan.scan_start(id, now)),
            lp_seam::net_scan_take::ID => {
                let mut out = Vec::new();
                let mut count = 0u32;
                for rec in lan.scan_results(id) {
                    let name = rec.name.as_bytes();
                    if name.len() > abi::MAX_SSID_LEN {
                        continue;
                    }
                    if out.len() + abi::scan_record_len(name.len()) > a1 as usize {
                        break;
                    }
                    out.push(name.len() as u8);
                    out.extend_from_slice(name);
                    out.push(rec.signal_dbm as u8);
                    out.push(u8::from(rec.secure));
                    count += 1;
                }
                if out.is_empty() || self.poke_bytes(a0, &out) {
                    count
                } else {
                    0
                }
            }
            lp_seam::net_connect::ID => {
                let (ssid_len, pw_len) = (a1 as usize, a3 as usize);
                if ssid_len > abi::MAX_SSID_LEN || pw_len > abi::MAX_PASSWORD_LEN {
                    0
                } else {
                    match (
                        self.peek_call_buffer(a0, ssid_len),
                        self.peek_call_buffer(a2, pw_len),
                    ) {
                        (Some(ssid), Some(password)) => {
                            u32::from(lan.connect(id, now, &ssid, &password))
                        }
                        _ => 0,
                    }
                }
            }
            lp_seam::net_disconnect::ID => {
                lan.disconnect(id);
                1
            }
            lp_seam::net_event_take::ID => match lan.take_event(id) {
                None => abi::EVENT_NONE,
                Some(StationEvent::Associated) => abi::EVENT_ASSOCIATED,
                Some(StationEvent::AuthFailed) => abi::EVENT_AUTH_FAILED,
                Some(StationEvent::NotFound) => abi::EVENT_NOT_FOUND,
                Some(StationEvent::LinkLost) => abi::EVENT_LINK_LOST,
                Some(StationEvent::ScanDone) => abi::EVENT_SCAN_DONE,
            },
            _ => 0,
        };
        // Under `--trace`, what an event take handed over, beside the call's
        // own `SEAM net=lan event-take` line.
        if self.bus.trace.is_enabled() && decl_id == lp_seam::net_event_take::ID && answer != 0 {
            let line = format!("cyc={now} SEAM net=lan event {}", event_word(answer));
            self.bus.trace.note(&line);
        }
        answer
    }

    /// `len` bytes of guest memory at `address`, read through the bus's own
    /// decode; `None` if any word refused.
    fn peek_call_buffer(&mut self, address: u32, len: usize) -> Option<Vec<u8>> {
        let mut out = Vec::with_capacity(len);
        let mut at = address;
        while out.len() < len {
            let word_at = at & !3;
            let lane = (at - word_at) as usize;
            let word = self.peek_word(word_at)?.to_le_bytes();
            let take = (4 - lane).min(len - out.len());
            out.extend_from_slice(&word[lane..lane + take]);
            at = at.checked_add(take as u32)?;
        }
        Some(out)
    }
}

/// The trace word for an event code: the station's own words.
fn event_word(code: u32) -> &'static str {
    match code {
        abi::EVENT_ASSOCIATED => StationEvent::Associated.word(),
        abi::EVENT_AUTH_FAILED => StationEvent::AuthFailed.word(),
        abi::EVENT_NOT_FOUND => StationEvent::NotFound.word(),
        abi::EVENT_LINK_LOST => StationEvent::LinkLost.word(),
        abi::EVENT_SCAN_DONE => StationEvent::ScanDone.word(),
        _ => "none",
    }
}
