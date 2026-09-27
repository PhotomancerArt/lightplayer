//! The delivery property, under any fault schedule:
//!
//! > Every message sent on a reliable channel is delivered exactly once, in
//! > order, or the link reports a reset; and once faults stop, everything the
//! > last session sent arrives.
//!
//! Order is per channel: the link sends the lowest-numbered channel first, so
//! a control message may overtake the rest of a proto message, never one of
//! its own channel's.
//!
//! Each case draws a transport, independent fault rates for each direction
//! (drops of packets, tails, writes and byte spans; bit flips; duplicates;
//! delay spikes), up to three board reboots, and a seed for the simulator.
//! `sim::checker` states the property precisely.
//!
//! Case count: 500 per variant by default (CI, ~6 s). For a soak, set
//! `PROPTEST_CASES`, e.g. `just link-soak` runs 5,000 per variant in release.

use lp_link::sim::{Faults, Scenario, Transport, Workload, run};
use lp_link::{Arq, GoBackN, SelectiveRepeat, StopAndWait};
use proptest::prelude::*;

#[derive(Clone, Debug)]
struct Case {
    transport: Transport,
    up: Faults,
    down: Faults,
    reboots: Vec<u64>,
    seed: u64,
}

const DURATION: u64 = 1_500_000;

fn rate() -> impl Strategy<Value = f64> {
    prop_oneof![3 => Just(0.0), 1 => 0.0..0.12f64, 1 => 0.0..0.01f64]
}

fn faults() -> impl Strategy<Value = Faults> {
    (
        rate(),
        rate(),
        rate(),
        rate(),
        rate(),
        rate(),
        rate(),
        0u64..400_000,
    )
        .prop_map(
            |(
                drop_packet,
                drop_tail,
                drop_write,
                drop_span,
                corrupt,
                duplicate,
                spike,
                spike_len,
            )| Faults {
                drop_packet,
                drop_tail,
                drop_write,
                drop_span,
                corrupt,
                duplicate,
                spike: spike / 4.0,
                spike_len,
            },
        )
}

fn case() -> impl Strategy<Value = Case> {
    let transport = prop_oneof![
        Just(Transport::Usb),
        Just(Transport::BleStream),
        Just(Transport::Ble),
        Just(Transport::Udp),
    ];
    let reboots = prop::collection::vec(0..DURATION, 0..=3);
    (transport, faults(), faults(), reboots, any::<u64>()).prop_map(
        |(transport, up, down, reboots, seed)| Case {
            transport,
            up,
            down,
            reboots,
            seed,
        },
    )
}

fn check<A: Arq>(c: &Case) -> Result<(), TestCaseError> {
    let workload = Workload::Random {
        mean_gap: 8_000,
        max_size: 3_000,
    };
    let mut sc = Scenario::new(c.transport, 0.0, workload, DURATION, c.seed);
    sc.faults_up = c.up.clone();
    sc.faults_down = c.down.clone();
    sc.board_reboots = c.reboots.clone();
    let r = run::<A>(&sc);
    prop_assert!(
        r.violations.is_empty(),
        "{} on {}: {:#?}",
        A::NAME,
        c.transport.name(),
        r.violations
    );
    prop_assert!(r.up.sent > 0 && r.down.sent > 0, "the workload ran");
    Ok(())
}

fn config() -> ProptestConfig {
    let cases = std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(500);
    ProptestConfig {
        cases,
        failure_persistence: None,
        ..ProptestConfig::default()
    }
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn selective_repeat_keeps_the_promise(c in case()) {
        check::<SelectiveRepeat>(&c)?;
    }

    #[test]
    fn go_back_n_keeps_the_promise(c in case()) {
        check::<GoBackN<127>>(&c)?;
    }

    #[test]
    fn stop_and_wait_keeps_the_promise(c in case()) {
        check::<StopAndWait>(&c)?;
    }
}
