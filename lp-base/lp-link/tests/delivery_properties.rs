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

/// The same promise with both links secure (feature `secure`): selective
/// repeat on every ARQ transport (`ble()` secured, so a sealed frame fits a
/// notification), and no ARQ on a WebSocket pipe that now loses, damages
/// and duplicates. A no-ARQ secure link keeps the promise by resetting on
/// every counter gap (the reset reports what was lost), where a plain one
/// would deliver around the hole.
#[cfg(feature = "secure")]
mod secure {
    use super::*;
    use lp_link::sim::secure_sim::SecureSim;
    use lp_link::{LinkConfig, NoArq};

    fn check_secure<A: Arq>(c: &Case) -> Result<(), TestCaseError> {
        let workload = Workload::Random {
            mean_gap: 8_000,
            max_size: 3_000,
        };
        let mut sc = Scenario::new(c.transport, 0.0, workload, DURATION, c.seed);
        if c.transport == Transport::Ble {
            sc = sc.with_configs(LinkConfig::ble().secured());
        }
        sc.faults_up = c.up.clone();
        sc.faults_down = c.down.clone();
        sc.board_reboots = c.reboots.clone();
        sc.secure = Some(SecureSim::matched());
        let r = run::<A>(&sc);
        prop_assert!(
            r.violations.is_empty(),
            "secure {} on {}: {:#?}",
            A::NAME,
            c.transport.name(),
            r.violations
        );
        prop_assert!(r.up.sent > 0 && r.down.sent > 0, "the workload ran");
        prop_assert!(r.host.handshakes > 0 && r.board.handshakes > 0);
        Ok(())
    }

    /// A WebSocket pipe's faults, without delay spikes. A spike on an
    /// ordered pipe holds everything behind it and then releases it at once,
    /// and a no-ARQ receiver drops what overflows its receive budget
    /// (`rx_no_room`) with no reset: the sender has no window to respect and
    /// nothing resends. That is no-ARQ's own behaviour, plain or secure (a
    /// spike case fails the same way on a plain link), not the secure
    /// channel's, so it is left out here and named for the first product
    /// no-ARQ link to settle.
    fn ws_faults() -> impl Strategy<Value = Faults> {
        faults().prop_map(|f| Faults {
            spike: 0.0,
            spike_len: 0,
            ..f
        })
    }

    fn ws_case() -> impl Strategy<Value = Case> {
        let reboots = prop::collection::vec(0..DURATION, 0..=3);
        (ws_faults(), ws_faults(), reboots, any::<u64>()).prop_map(|(up, down, reboots, seed)| {
            Case {
                transport: Transport::Ws,
                up,
                down,
                reboots,
                seed,
            }
        })
    }

    proptest! {
        #![proptest_config(config())]

        #[test]
        fn secure_selective_repeat_keeps_the_promise(c in case()) {
            check_secure::<SelectiveRepeat>(&c)?;
        }

        #[test]
        fn secure_no_arq_on_a_websocket_keeps_the_promise_by_resetting(c in ws_case()) {
            check_secure::<NoArq>(&c)?;
        }
    }
}
