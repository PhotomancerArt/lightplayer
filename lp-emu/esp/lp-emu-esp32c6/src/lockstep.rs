//! Two (or more) machines advanced alternately over a fixed guest-cycle
//! quantum, in **one thread**, exchanging frames through an
//! [`Air`](lp_emu_esp_common::air::Air) at quantum boundaries (plan RD11).
//!
//! # Why this is a module of its own and not more of `machine.rs`
//!
//! `machine.rs` is the most contended file in this roadmap — four phases hold
//! it at once — and it is already 3,500 lines about *one* machine. A runner
//! that owns several of them is a layer above, not a member of, that file:
//! it uses only [`Esp32C6Machine::run_until`], the air seam
//! ([`Esp32C6Machine::arm_air`], [`take_air_frames`], [`offer_air_frame`])
//! and [`Outcome`], all of which are public. So the seam `machine.rs` gained
//! for this is small enough to read in one screen, and everything else about
//! running a pair lives here.
//!
//! [`take_air_frames`]: Esp32C6Machine::take_air_frames
//! [`offer_air_frame`]: Esp32C6Machine::offer_air_frame
//!
//! # One thread, two run loops
//!
//! Each machine owns its own scheduler, its own flash and its own cache
//! handle, and nothing in `machine.rs`, `host.rs` or `bus.rs` is a `static`
//! or a `thread_local` — so two machines in one process share nothing by
//! construction (M4 P0 §7). That is what makes lockstep possible, and
//! lockstep is what makes a two-board run **byte-identical on every replay**:
//!
//! ```text
//!   loop {
//!       for each live machine, in index order:
//!           run_until(stop_cycle = t + quantum)
//!       for each live machine, in index order:
//!           air.send(everything its radio armed this quantum)
//!       for each machine, in index order:
//!           offer it everything the air says is due at t + quantum
//!       t += quantum
//!   }
//! ```
//!
//! **Never a thread pair.** Two threads would reintroduce a race between the
//! machines' interleaving and the delivery order, and byte-identical replay
//! would be gone. If a future phase reaches for `std::thread` here, RD11 has
//! been abandoned and the transcripts stop meaning what they say.
//!
//! # The idle skip is what makes the horizon hold
//!
//! A machine whose guest sits in `wfi` does not step cycle by cycle: the run
//! loop moves guest time straight to the next scheduled event. **That skip
//! clamps to the slice's `stop_cycle`** (`machine.rs`, `SliceEnd::Wfi`:
//! `let wake = wake.max(self.cycles() + 1).min(stop_cycle)`), which is the
//! one fact this runner rests on. Without it an idle machine would jump past
//! the pair's horizon and the other machine's frames would arrive in its
//! past. `tests::an_idle_machine_stops_at_the_horizon_not_past_it` is the
//! test that holds it.
//!
//! # Choosing the two numbers
//!
//! See [`DEFAULT_LATENCY_CYCLES`] and [`DEFAULT_QUANTUM_CYCLES`]. The
//! relation between them is the design, and it is checked at construction:
//! **the quantum is never larger than the latency.**
//!
//! # What P1 does not do
//!
//! The receiving end **counts and logs**; it does not deliver. A frame the
//! air offers a machine is reported as "offered, not delivered". Writing one
//! into the receiver's RX ring, filling an `rx_ctrl` header in front of it
//! and raising the RX interrupt is M4 P2's, and it grows from
//! [`Esp32C6Machine::offer_air_frame`].

use lp_emu_core::sched::Cycles;
use lp_emu_esp_common::air::{Air, ParticipantId, PerfectAir};

use crate::machine::{Esp32C6Machine, MAX_SLICE_CYCLES, Outcome, StopCondition};
use crate::memmap;

/// The air's latency: **672 µs**, or 107,520 guest cycles at 160 MHz.
///
/// Where the number comes from, so that it can be argued with rather than
/// inherited: it is the air time of the 60-byte frame M4 P0 observed, at
/// 802.11b's 1 Mbit/s long-preamble basic rate — 192 µs of preamble and PLCP
/// header, then 60 bytes at 8 µs a byte, which is 192 + 480 = 672 µs.
///
/// It is a **stated constant, not a function of the frame**. A longer frame
/// does not take longer here, because deriving the latency per frame would
/// be a PHY model and no PHY is modelled (RD10). The arithmetic above is
/// where the number was *chosen* from; it is not a claim that this emulator
/// reproduces 802.11b timing. Its grade is `modeled`.
///
/// It is never zero: a receiver that could see a frame in the cycle it was
/// armed would be able to answer inside one quantum, which no radio does.
///
/// Settable — [`Lockstep::with_latency`] — so M4 P3 can state the value a
/// transcript was recorded at in its sidecar rather than assuming this one.
pub const DEFAULT_LATENCY_US: u64 = 672;

/// [`DEFAULT_LATENCY_US`] in guest cycles.
pub const DEFAULT_LATENCY_CYCLES: Cycles = DEFAULT_LATENCY_US * memmap::CYCLES_PER_US;

/// The quantum: **8,192 guest cycles**, 51.2 µs — the machine's own
/// [`MAX_SLICE_CYCLES`].
///
/// Two things decide it.
///
/// 1. **It bounds how stale a delivery can be.** A frame armed at cycle `s`
///    is due at `s + latency` and is offered at the first quantum boundary at
///    or after that, so it is at most one quantum late. At 8,192 cycles that
///    is 51.2 µs of slop on a 672 µs latency — 7.6 %.
/// 2. **A quantum larger than the latency would make the latency a lie.** A
///    frame could then be armed and offered inside the same boundary pair,
///    delivering it earlier than the stated latency allows.
///    [`Lockstep::with_quantum`] refuses that outright.
///
/// Matching `MAX_SLICE_CYCLES` also means the pair pays no slice boundaries a
/// single run was not already paying: the machine already ends a slice every
/// 8,192 cycles, so a quantum boundary lands where a slice boundary would
/// have. A smaller quantum would be more accurate about staleness and cost
/// run time for it; that trade is [`Lockstep::with_quantum`]'s to make.
pub const DEFAULT_QUANTUM_CYCLES: Cycles = MAX_SLICE_CYCLES;

/// Why a pair could not be built.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LockstepError {
    /// A quantum larger than the latency: a frame could be delivered sooner
    /// than the air says it travels. See [`DEFAULT_QUANTUM_CYCLES`].
    QuantumExceedsLatency { quantum: Cycles, latency: Cycles },
    /// A zero quantum never advances, and a zero latency is refused by the
    /// air itself.
    ZeroQuantum,
    /// A pair of one. An air with a single participant delivers nothing, and
    /// a runner for it is a plain `run_until` with extra steps.
    NotAPair { machines: usize },
}

impl std::fmt::Display for LockstepError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LockstepError::QuantumExceedsLatency { quantum, latency } => write!(
                f,
                "lockstep: the quantum ({quantum} cycles) is larger than the air's latency \
                 ({latency} cycles), so a frame could be delivered sooner than the air says \
                 it travels"
            ),
            LockstepError::ZeroQuantum => {
                f.write_str("lockstep: the quantum must be at least one cycle")
            }
            LockstepError::NotAPair { machines } => write!(
                f,
                "lockstep: {machines} machine(s); an air needs at least two participants"
            ),
        }
    }
}

impl std::error::Error for LockstepError {}

/// How one machine's participation ended, and what the air did for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MachineReport {
    /// Its seat on the air, which is also its index in the pair.
    pub id: ParticipantId,
    /// Guest cycles it reached.
    pub cycles: Cycles,
    /// Why it stopped. `Deadline` means it ran to the pair's horizon;
    /// anything else ended its participation early, and the other machines
    /// carried on without it.
    pub outcome: Outcome,
    /// Frames its radio handed the air.
    pub frames_sent: u64,
    /// Frames the air offered it. **Not delivered** — see the module docs.
    pub frames_offered: u64,
}

/// What a pair's run came to. Both machines are named, whichever ended how.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LockstepReport {
    /// The horizon the pair reached: the last quantum boundary crossed.
    pub cycles: Cycles,
    /// One per machine, in index order.
    pub machines: Vec<MachineReport>,
    /// The air's stated latency, in guest cycles — what a transcript sidecar
    /// records.
    pub latency: Cycles,
    /// The quantum the pair advanced in.
    pub quantum: Cycles,
}

impl LockstepReport {
    /// `true` when every machine ran to the horizon with no fault, refusal,
    /// breakpoint or reset.
    pub fn all_reached_the_horizon(&self) -> bool {
        self.machines
            .iter()
            .all(|m| matches!(m.outcome, Outcome::Deadline { .. }))
    }

    /// The exit code a CLI would return: the first machine that ended
    /// unhappily decides, in index order, exactly as a single run would.
    pub fn exit_code(&self) -> i32 {
        self.machines
            .iter()
            .map(|m| m.outcome.exit_code())
            .find(|&c| c != 0)
            .unwrap_or(0)
    }
}

/// Several machines on one air, advanced alternately in one thread.
pub struct Lockstep {
    machines: Vec<Esp32C6Machine>,
    air: PerfectAir,
    quantum: Cycles,
    /// The pair's clock: every machine has been run to here.
    now: Cycles,
    /// `Some(outcome)` once a machine has stopped participating.
    ended: Vec<Option<Outcome>>,
    sent: Vec<u64>,
}

impl Lockstep {
    /// A pair (or more) on an air with [`DEFAULT_LATENCY_CYCLES`] and
    /// [`DEFAULT_QUANTUM_CYCLES`].
    ///
    /// Every machine is armed for the air as it joins, and a machine that
    /// never joins one is untouched — that is the whole off switch.
    pub fn new(machines: Vec<Esp32C6Machine>) -> Result<Self, LockstepError> {
        Self::with_numbers(machines, DEFAULT_LATENCY_CYCLES, DEFAULT_QUANTUM_CYCLES)
    }

    /// As [`new`](Self::new), with a stated latency in guest cycles.
    pub fn with_latency(
        machines: Vec<Esp32C6Machine>,
        latency: Cycles,
    ) -> Result<Self, LockstepError> {
        let quantum = DEFAULT_QUANTUM_CYCLES.min(latency);
        Self::with_numbers(machines, latency, quantum)
    }

    /// As [`new`](Self::new), with both numbers stated. The quantum must not
    /// exceed the latency — see [`DEFAULT_QUANTUM_CYCLES`].
    pub fn with_quantum(
        machines: Vec<Esp32C6Machine>,
        latency: Cycles,
        quantum: Cycles,
    ) -> Result<Self, LockstepError> {
        Self::with_numbers(machines, latency, quantum)
    }

    fn with_numbers(
        mut machines: Vec<Esp32C6Machine>,
        latency: Cycles,
        quantum: Cycles,
    ) -> Result<Self, LockstepError> {
        if machines.len() < 2 {
            return Err(LockstepError::NotAPair {
                machines: machines.len(),
            });
        }
        if quantum == 0 {
            return Err(LockstepError::ZeroQuantum);
        }
        if quantum > latency {
            return Err(LockstepError::QuantumExceedsLatency { quantum, latency });
        }
        let air = PerfectAir::try_new(machines.len(), latency)
            .ok_or(LockstepError::QuantumExceedsLatency { quantum, latency })?;
        for (i, m) in machines.iter_mut().enumerate() {
            m.arm_air(ParticipantId(i));
        }
        let n = machines.len();
        Ok(Self {
            machines,
            air,
            quantum,
            now: 0,
            ended: vec![None; n],
            sent: vec![0; n],
        })
    }

    /// The air's stated latency, in guest cycles.
    pub fn latency(&self) -> Cycles {
        self.air.latency()
    }

    /// The quantum, in guest cycles.
    pub fn quantum(&self) -> Cycles {
        self.quantum
    }

    /// The pair's clock: the last quantum boundary every machine reached.
    pub fn cycles(&self) -> Cycles {
        self.now
    }

    /// One machine, for a caller that wants at its console or its RAM.
    pub fn machine(&self, id: ParticipantId) -> Option<&Esp32C6Machine> {
        self.machines.get(id.index())
    }

    pub fn machine_mut(&mut self, id: ParticipantId) -> Option<&mut Esp32C6Machine> {
        self.machines.get_mut(id.index())
    }

    /// Run the pair to `horizon` guest cycles, or until every machine has
    /// stopped participating.
    ///
    /// Resumable: calling it again with a later horizon carries on from where
    /// this one left off, machines, air and all.
    ///
    /// `base` supplies each machine's `exit_on` and `probes`; its
    /// `stop_cycle` is ignored in favour of `horizon`, and its `wall_timeout`
    /// is **not** honoured — a wall-clock net restarted every quantum would
    /// measure nothing, and a pair run is deterministic by construction
    /// (PD5). Pass [`StopCondition::default`] when neither is wanted.
    pub fn run_until(&mut self, horizon: Cycles, base: &StopCondition) -> LockstepReport {
        while self.now < horizon && self.ended.iter().any(Option::is_none) {
            let next = horizon.min(self.now.saturating_add(self.quantum));

            // 1. Advance every live machine to the same horizon, in index
            //    order. The idle skip clamps to `stop_cycle`, so a machine
            //    that sleeps the whole quantum still comes back at `next`.
            for i in 0..self.machines.len() {
                if self.ended[i].is_some() {
                    continue;
                }
                let stop = StopCondition {
                    stop_cycle: Some(next),
                    exit_on: base.exit_on.clone(),
                    wall_timeout: None,
                    probes: base.probes.clone(),
                };
                match self.machines[i].run_until(&stop) {
                    Outcome::Deadline { .. } => {}
                    // A fault, a strict refusal, a breakpoint, a reset or an
                    // `--exit-on` match ends this machine's participation.
                    // The others carry on, and the report names both.
                    other => self.ended[i] = Some(other),
                }
            }

            // 2. Everything their radios armed goes onto the air, in index
            //    order and then in arming order.
            for i in 0..self.machines.len() {
                for frame in self.machines[i].take_air_frames() {
                    self.sent[i] += 1;
                    self.air.send(ParticipantId(i), frame.at, &frame.bytes);
                }
            }

            // 3. Everything the air says is due by this boundary is offered,
            //    again in index order. Because the quantum is never larger
            //    than the latency, nothing sent in step 2 is due here.
            for i in 0..self.machines.len() {
                for frame in self.air.take_due(ParticipantId(i), next) {
                    self.machines[i].offer_air_frame(&frame);
                }
            }

            self.now = next;
        }
        self.report()
    }

    /// The pair's state as a report, without running anything.
    pub fn report(&self) -> LockstepReport {
        LockstepReport {
            cycles: self.now,
            latency: self.air.latency(),
            quantum: self.quantum,
            machines: self
                .machines
                .iter()
                .enumerate()
                .map(|(i, m)| MachineReport {
                    id: ParticipantId(i),
                    cycles: m.cycles(),
                    outcome: self.ended[i]
                        .clone()
                        .unwrap_or(Outcome::Deadline { cycle: m.cycles() }),
                    frames_sent: self.sent[i],
                    frames_offered: m.air_frames_offered(),
                })
                .collect(),
        }
    }

    /// Give the machines back, run or not.
    pub fn into_machines(self) -> Vec<Esp32C6Machine> {
        self.machines
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::machine::Esp32C6Builder;
    use crate::periph::wifi_stub;
    use lp_emu_core::Bus as _;

    /// `wfi; j -4` at the top of HP-SRAM: a guest that does nothing but
    /// sleep, wake and sleep again. The jump matters — a lone `wfi` runs off
    /// into unwritten RAM on the first wake and faults, which is not what
    /// this file is testing.
    const IDLE_GUEST: [u32; 2] = [0x1050_0073, 0xffdf_f06f];

    fn idle_machine() -> Esp32C6Machine {
        let mut m = Esp32C6Builder::new().build().unwrap();
        let mut image = Vec::new();
        for word in IDLE_GUEST {
            image.extend_from_slice(&word.to_le_bytes());
        }
        m.bus.load_image(memmap::HP_SRAM_BASE, &image).unwrap();
        m.harts[0].set_pc(memmap::HP_SRAM_BASE);
        m
    }

    #[test]
    fn the_quantum_is_never_larger_than_the_latency() {
        let err = Lockstep::with_quantum(vec![idle_machine(), idle_machine()], 1_000, 1_001)
            .err()
            .expect("a quantum past the latency is refused");
        assert_eq!(
            err,
            LockstepError::QuantumExceedsLatency {
                quantum: 1_001,
                latency: 1_000
            }
        );
        assert!(err.to_string().contains("sooner than the air says"));
        assert!(matches!(
            Lockstep::with_quantum(vec![idle_machine(), idle_machine()], 1_000, 0),
            Err(LockstepError::ZeroQuantum)
        ));
        assert!(matches!(
            Lockstep::new(vec![idle_machine()]),
            Err(LockstepError::NotAPair { machines: 1 })
        ));
        // The shipped pair of numbers satisfies its own rule.
        assert!(DEFAULT_QUANTUM_CYCLES <= DEFAULT_LATENCY_CYCLES);
        assert_eq!(DEFAULT_LATENCY_CYCLES, 107_520);
        assert_eq!(DEFAULT_QUANTUM_CYCLES, 8_192);
    }

    /// G1-2's second half. A machine whose guest does nothing but `wfi` must
    /// come back **at** the pair's horizon, not past it — the idle skip's
    /// clamp to `stop_cycle` is the only thing that makes that true, and if
    /// it ever stops being true a pair's deliveries land in a machine's past.
    #[test]
    fn an_idle_machine_stops_at_the_horizon_not_past_it() {
        let mut pair = Lockstep::new(vec![idle_machine(), idle_machine()]).unwrap();
        let quantum = pair.quantum();
        // One quantum at a time, for ten of them: every machine lands exactly
        // on the boundary each time, never a cycle beyond.
        for step in 1..=10u64 {
            let horizon = step * quantum;
            let report = pair.run_until(horizon, &StopCondition::default());
            assert_eq!(report.cycles, horizon);
            for m in &report.machines {
                assert_eq!(
                    m.cycles, horizon,
                    "machine {} overshot the horizon at step {step}",
                    m.id
                );
            }
        }
        // And a horizon that is not a multiple of the quantum is still hit
        // exactly: the last quantum is short, not long.
        let odd = 10 * quantum + quantum / 3;
        let report = pair.run_until(odd, &StopCondition::default());
        assert_eq!(report.cycles, odd);
        for m in &report.machines {
            assert_eq!(m.cycles, odd);
        }
        assert!(report.all_reached_the_horizon());
        assert_eq!(report.exit_code(), 0);
    }

    /// G1-2's first half, on the runner itself: the same pair run twice is
    /// the same run twice, cycle totals and reports alike.
    #[test]
    fn two_runs_of_the_same_pair_are_identical() {
        let run = || {
            let mut pair = Lockstep::new(vec![idle_machine(), idle_machine()]).unwrap();
            pair.run_until(64 * pair.quantum(), &StopCondition::default())
        };
        let a = run();
        let b = run();
        assert_eq!(a, b);
        assert_eq!(a.latency, DEFAULT_LATENCY_CYCLES);
        assert_eq!(a.quantum, DEFAULT_QUANTUM_CYCLES);
    }

    /// The whole seam, end to end, with no firmware: arm a frame in one
    /// machine the way the blob does — a descriptor and a buffer in guest
    /// RAM, then PLCP0 with the go strobe — and watch it come out of the
    /// other machine's `frames offered` one latency later.
    ///
    /// The register values are M4 P0's shape (`m4/discovery-air.md` §1–§2):
    /// `{dw0, buf, next}` twelve bytes at `0x4080_0000 | (plcp0 & 0xf_ffff)`,
    /// the buffer's first word the length including the FCS it does not
    /// carry, and the frame eight bytes in.
    #[test]
    fn a_frame_one_machine_arms_reaches_the_other_after_the_latency() {
        const DESC: u32 = memmap::HP_SRAM_BASE | 0x0002_0000;
        const BUF: u32 = memmap::HP_SRAM_BASE | 0x0002_0100;
        const FRAME: [u8; 8] = [0xd0, 0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66];

        let mut sender = idle_machine();
        // The descriptor: owner and eof set, `size` 96, a null link.
        sender.poke_word(DESC, 0xc000_0060);
        sender.poke_word(DESC + 4, BUF);
        sender.poke_word(DESC + 8, 0);
        // The buffer: `len` = the frame plus a four-byte FCS, then eight
        // bytes of header, then the frame.
        sender.poke_word(BUF, FRAME.len() as u32 + 4);
        sender.poke_word(BUF + 4, 0);
        sender.poke_word(BUF + 8, u32::from_le_bytes(FRAME[..4].try_into().unwrap()));
        sender.poke_word(BUF + 12, u32::from_le_bytes(FRAME[4..].try_into().unwrap()));

        let mut pair = Lockstep::new(vec![sender, idle_machine()]).unwrap();
        let (a, b) = (ParticipantId(0), ParticipantId(1));

        // The blob's own two writes: the pointer, then the same word with
        // the go strobe. Only the second is a handoff.
        let plcp0 = memmap::periph::MODEM_WINDOW + wifi_stub::TX_PLCP0_OFFSET;
        let pointer = DESC & wifi_stub::TX_DESC_PTR_MASK;
        let m = pair.machine_mut(a).unwrap();
        m.bus.write_word(plcp0, pointer as i32).unwrap();
        m.bus
            .write_word(plcp0, (pointer | wifi_stub::TX_GO_MASK) as i32)
            .unwrap();

        // One quantum: the sender's radio has armed it, the air holds it,
        // and it is nowhere near due.
        pair.run_until(pair.quantum(), &StopCondition::default());
        let report = pair.report();
        assert_eq!(report.machines[0].frames_sent, 1, "{report:?}");
        assert_eq!(report.machines[1].frames_offered, 0, "not due yet");
        assert_eq!(report.machines[0].frames_offered, 0, "never to its sender");

        // Run past the latency. It is offered exactly once, to the other
        // machine only, and never to the one that sent it.
        let past = DEFAULT_LATENCY_CYCLES + 4 * pair.quantum();
        let report = pair.run_until(past, &StopCondition::default());
        assert_eq!(report.machines[1].frames_offered, 1, "{report:?}");
        assert_eq!(report.machines[0].frames_offered, 0);
        assert_eq!(
            pair.machine(b).unwrap().air_frames_offered(),
            1,
            "and the machine agrees with the report"
        );

        // Still exactly one after another stretch: delivered once, not
        // once per quantum.
        let report = pair.run_until(past * 2, &StopCondition::default());
        assert_eq!(report.machines[1].frames_offered, 1);
        assert!(report.all_reached_the_horizon());
    }

    /// The off switch, at the seam rather than at the CLI: a machine that was
    /// never armed records nothing, offers nothing and hands the air nothing.
    #[test]
    fn a_machine_that_is_not_in_an_air_records_nothing() {
        let mut m = idle_machine();
        assert!(m.air_participant().is_none());
        let plcp0 = memmap::periph::MODEM_WINDOW + wifi_stub::TX_PLCP0_OFFSET;
        m.bus.write_word(plcp0, 0xc002_0000u32 as i32).unwrap();
        m.run_until(&StopCondition::after_micros(100));
        assert!(m.take_air_frames().is_empty());
        assert_eq!(m.air_frames_offered(), 0);
    }
}
