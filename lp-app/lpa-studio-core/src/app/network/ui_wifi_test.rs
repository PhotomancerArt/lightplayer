//! [`UiWifiTest`]: the test that runs in a network's row right after it is
//! added (the spike's 2B) — saved first, then tried.
//!
//! Its steps: Looking for <ssid> → Checking the password → Getting an
//! address (· ip) → Reaching lightplayer.app (only while the cloud relay is
//! on AND the board reports reaching it: `relay` is `connecting`,
//! `connected` or `waitingForInternet`; Wi-Fi roadmap M7). What the station
//! says decides how far it got:
//!
//! | station / the network's `last` | the test |
//! |---|---|
//! | `unsupported` | Saved · this firmware can't connect… |
//! | `off` | Saved · Wi‑Fi is off… |
//! | `connecting { ssid, step }` | running, at the step (Looking for, Checking the password, Getting an address) |
//! | `connected { ssid, ip, rssi, host }` | Connected · <signal> signal · <ip> |
//! | `failed { ssid, wrongPassword }` / `last: wrongPassword` | Wrong password |
//! | `failed { ssid, notFound }` / `last: notFound` | Not in range |
//! | `failed { ssid, noAddress }` / `last: noAddress` | No address |
//!
//! Once joined, the relay decides the last step:
//!
//! | relay | the test |
//! |---|---|
//! | `connecting` | running, at Reaching lightplayer.app |
//! | `connected` | Connected |
//! | `waitingForInternet` | Connected, no internet |
//! | `noAccount`, `refused { unknownAccount \| updateFirmware }` | Connected; the step skipped, a note under the row says what to do |
//! | `off`, `refused { busy }` | Connected; no step |

use lpc_wire::server::{
    ConnectStep, LastAttempt, NetworkStatus, RelayState, StationFailure, StationState,
};

use super::wifi_words::relay as relay_words;
use super::wifi_words::test as words;

/// The in-row test of one just-added network.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiWifiTest {
    pub ssid: String,
    /// Whether the last step (Reaching lightplayer.app) is shown: Cloud
    /// relay is on and the board reports trying to reach it.
    pub relay_step: bool,
    /// A line under the row when the relay step is skipped for a reason
    /// the person can act on (no account key on the board, a reset key, an
    /// old firmware).
    pub relay_note: Option<&'static str>,
    pub progress: WifiTestProgress,
}

/// How far the test got.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WifiTestProgress {
    /// Saved on a firmware that cannot connect (every M5 image): no steps.
    Saved,
    /// Saved with the board's Wi‑Fi off: no steps.
    WifiOff,
    /// Under way, at this step.
    Running(WifiTestStep),
    /// Finished.
    Done(WifiTestOutcome),
}

/// The test's steps, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum WifiTestStep {
    Looking,
    CheckingPassword,
    GettingAddress,
    ReachingCloud,
}

/// How a finished test came out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WifiTestOutcome {
    Connected {
        rssi: i8,
        ip: String,
    },
    WrongPassword,
    NotInRange,
    NoAddress,
    /// Connected, but lightplayer.app did not answer (the relay step, M7).
    NoInternet {
        ip: String,
    },
}

/// One step's line in the test.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiWifiTestStepLine {
    pub label: String,
    pub state: WifiStepState,
}

/// Where one step stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WifiStepState {
    Done,
    Now,
    Bad,
    Todo,
}

/// The test's result line and what it offers next.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiWifiTestResult {
    pub headline: &'static str,
    pub body: String,
    /// The headline reads as good news (Connected).
    pub good: bool,
    pub next: WifiTestNext,
}

/// What a finished test offers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WifiTestNext {
    /// [Done]: dismiss the test.
    Done,
    /// [Remove] [Change password]: the password was refused.
    RemoveOrChangePassword,
}

impl UiWifiTest {
    /// The test of `ssid` from what the board says now.
    pub fn of(ssid: &str, status: &NetworkStatus) -> Self {
        let last = status.network(ssid).and_then(|network| network.last);
        let relay_step = status.cloud_relay
            && matches!(
                status.relay,
                RelayState::Connecting | RelayState::Connected | RelayState::WaitingForInternet
            );
        let relay_note = if status.cloud_relay {
            relay_words::note(status.relay)
        } else {
            None
        };
        let progress = match &status.station {
            StationState::Unsupported => WifiTestProgress::Saved,
            StationState::Off => WifiTestProgress::WifiOff,
            StationState::Connected {
                ssid: on, ip, rssi, ..
            } if on == ssid => match status.relay {
                RelayState::Connecting if relay_step => {
                    WifiTestProgress::Running(WifiTestStep::ReachingCloud)
                }
                RelayState::WaitingForInternet if relay_step => {
                    WifiTestProgress::Done(WifiTestOutcome::NoInternet { ip: ip.clone() })
                }
                _ => WifiTestProgress::Done(WifiTestOutcome::Connected {
                    rssi: *rssi,
                    ip: ip.clone(),
                }),
            },
            StationState::Failed { ssid: on, reason } if on == ssid => {
                WifiTestProgress::Done(failure_outcome(*reason))
            }
            StationState::Connecting { ssid: on, step } if on == ssid => {
                WifiTestProgress::Running(match step {
                    ConnectStep::Looking => WifiTestStep::Looking,
                    ConnectStep::CheckingPassword => WifiTestStep::CheckingPassword,
                    ConnectStep::GettingAddress => WifiTestStep::GettingAddress,
                })
            }
            _ => match last {
                Some(LastAttempt::WrongPassword) => {
                    WifiTestProgress::Done(WifiTestOutcome::WrongPassword)
                }
                Some(LastAttempt::NotFound) => WifiTestProgress::Done(WifiTestOutcome::NotInRange),
                Some(LastAttempt::NoAddress) => WifiTestProgress::Done(WifiTestOutcome::NoAddress),
                Some(LastAttempt::Connected) | None => {
                    WifiTestProgress::Running(WifiTestStep::Looking)
                }
            },
        };
        Self {
            ssid: ssid.to_string(),
            relay_step,
            relay_note,
            progress,
        }
    }

    /// The step lines, in order: none for a network only saved.
    pub fn steps(&self) -> Vec<UiWifiTestStepLine> {
        let (reached, bad) = match &self.progress {
            WifiTestProgress::Saved | WifiTestProgress::WifiOff => return Vec::new(),
            WifiTestProgress::Running(step) => (*step, false),
            WifiTestProgress::Done(outcome) => match outcome {
                WifiTestOutcome::Connected { .. } => (WifiTestStep::ReachingCloud, false),
                WifiTestOutcome::WrongPassword => (WifiTestStep::CheckingPassword, true),
                WifiTestOutcome::NotInRange => (WifiTestStep::Looking, true),
                WifiTestOutcome::NoAddress => (WifiTestStep::GettingAddress, true),
                WifiTestOutcome::NoInternet { .. } => (WifiTestStep::ReachingCloud, true),
            },
        };
        let finished = matches!(
            self.progress,
            WifiTestProgress::Done(WifiTestOutcome::Connected { .. })
        );
        let ip = match &self.progress {
            WifiTestProgress::Done(
                WifiTestOutcome::Connected { ip, .. } | WifiTestOutcome::NoInternet { ip },
            ) => Some(ip.as_str()),
            _ => None,
        };
        let mut steps = vec![
            WifiTestStep::Looking,
            WifiTestStep::CheckingPassword,
            WifiTestStep::GettingAddress,
        ];
        if self.relay_step {
            steps.push(WifiTestStep::ReachingCloud);
        }
        steps
            .into_iter()
            .map(|step| {
                let state = if finished || step < reached {
                    WifiStepState::Done
                } else if step == reached {
                    if bad {
                        WifiStepState::Bad
                    } else {
                        WifiStepState::Now
                    }
                } else {
                    WifiStepState::Todo
                };
                let label = match step {
                    WifiTestStep::Looking => words::looking_for(&self.ssid),
                    WifiTestStep::CheckingPassword => words::CHECKING_PASSWORD.to_string(),
                    WifiTestStep::GettingAddress => match (state, ip) {
                        (WifiStepState::Done, Some(ip)) => {
                            format!("{} · {ip}", words::GETTING_ADDRESS)
                        }
                        _ => words::GETTING_ADDRESS.to_string(),
                    },
                    WifiTestStep::ReachingCloud => words::REACHING_CLOUD.to_string(),
                };
                UiWifiTestStepLine { label, state }
            })
            .collect()
    }

    /// The result line, once there is one.
    pub fn result(&self) -> Option<UiWifiTestResult> {
        let done = |headline, body: String| UiWifiTestResult {
            headline,
            body,
            good: false,
            next: WifiTestNext::Done,
        };
        Some(match &self.progress {
            WifiTestProgress::Running(_) => return None,
            WifiTestProgress::Saved => done(words::SAVED, words::SAVED_BODY.to_string()),
            WifiTestProgress::WifiOff => done(words::SAVED, words::SAVED_WIFI_OFF_BODY.to_string()),
            WifiTestProgress::Done(WifiTestOutcome::Connected { rssi, ip }) => UiWifiTestResult {
                good: true,
                ..done(words::CONNECTED, words::connected_body(*rssi, ip))
            },
            WifiTestProgress::Done(WifiTestOutcome::WrongPassword) => UiWifiTestResult {
                next: WifiTestNext::RemoveOrChangePassword,
                ..done(
                    words::WRONG_PASSWORD,
                    words::WRONG_PASSWORD_BODY.to_string(),
                )
            },
            WifiTestProgress::Done(WifiTestOutcome::NotInRange) => {
                done(words::NOT_IN_RANGE, words::NOT_IN_RANGE_BODY.to_string())
            }
            WifiTestProgress::Done(WifiTestOutcome::NoAddress) => {
                done(words::NO_ADDRESS, words::NO_ADDRESS_BODY.to_string())
            }
            WifiTestProgress::Done(WifiTestOutcome::NoInternet { ip }) => {
                done(words::NO_INTERNET, words::no_internet_body(ip))
            }
        })
    }
}

fn failure_outcome(reason: StationFailure) -> WifiTestOutcome {
    match reason {
        StationFailure::WrongPassword => WifiTestOutcome::WrongPassword,
        StationFailure::NotFound => WifiTestOutcome::NotInRange,
        StationFailure::NoAddress => WifiTestOutcome::NoAddress,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_wire::server::SavedNetworkInfo;

    const SSID: &str = "lp-walk-net";

    fn status(station: StationState, last: Option<LastAttempt>) -> NetworkStatus {
        NetworkStatus {
            wifi: true,
            cloud_relay: true,
            networks: vec![SavedNetworkInfo {
                ssid: SSID.to_string(),
                has_password: true,
                hidden: false,
                last,
            }],
            station,
            relay: RelayState::Off,
        }
    }

    fn states(test: &UiWifiTest) -> Vec<WifiStepState> {
        test.steps().into_iter().map(|step| step.state).collect()
    }

    /// A fake station walks the test through every outcome; each reads in
    /// its own words.
    #[test]
    fn every_station_answer_reads_as_its_outcome() {
        use WifiStepState::{Bad, Done, Now, Todo};

        let saved = UiWifiTest::of(SSID, &status(StationState::Unsupported, None));
        assert!(saved.steps().is_empty());
        let result = saved.result().unwrap();
        assert_eq!(
            (result.headline, result.body.as_str()),
            (
                "Saved",
                "this firmware can't connect to Wi‑Fi yet. It will after an update."
            )
        );

        let looking = UiWifiTest::of(
            SSID,
            &status(
                StationState::Connecting {
                    ssid: SSID.to_string(),
                    step: ConnectStep::Looking,
                },
                None,
            ),
        );
        assert_eq!(states(&looking), [Now, Todo, Todo]);
        assert_eq!(looking.steps()[0].label, "Looking for lp-walk-net");
        assert_eq!(looking.result(), None);

        // The board's own step moves the test along live.
        for (step, expected) in [
            (ConnectStep::CheckingPassword, [Done, Now, Todo]),
            (ConnectStep::GettingAddress, [Done, Done, Now]),
        ] {
            let running = UiWifiTest::of(
                SSID,
                &status(
                    StationState::Connecting {
                        ssid: SSID.to_string(),
                        step,
                    },
                    None,
                ),
            );
            assert_eq!(states(&running), expected);
            assert_eq!(running.result(), None);
        }

        let connected = UiWifiTest::of(
            SSID,
            &status(
                StationState::Connected {
                    ssid: SSID.to_string(),
                    ip: "192.168.1.42".to_string(),
                    rssi: -48,
                    host: "lp-8e30.local".to_string(),
                },
                Some(LastAttempt::Connected),
            ),
        );
        assert_eq!(states(&connected), [Done, Done, Done]);
        assert_eq!(
            connected.steps()[2].label,
            "Getting an address · 192.168.1.42"
        );
        let result = connected.result().unwrap();
        assert!(result.good);
        assert_eq!(result.body, "strong signal · 192.168.1.42");
        assert_eq!(result.next, WifiTestNext::Done);

        let wrong = UiWifiTest::of(
            SSID,
            &status(
                StationState::Failed {
                    ssid: SSID.to_string(),
                    reason: StationFailure::WrongPassword,
                },
                None,
            ),
        );
        assert_eq!(states(&wrong), [Done, Bad, Todo]);
        assert_eq!(
            wrong.result().unwrap().next,
            WifiTestNext::RemoveOrChangePassword
        );

        // The station moved on to another network; the row still knows.
        let away = UiWifiTest::of(
            SSID,
            &status(StationState::NotConnected, Some(LastAttempt::NotFound)),
        );
        assert_eq!(states(&away), [Bad, Todo, Todo]);
        assert_eq!(away.result().unwrap().headline, "Not in range");
        assert!(away.result().unwrap().body.contains("2.4 GHz"));
    }

    /// A joined board's relay decides the last step, in the board's words.
    #[test]
    fn the_relay_decides_the_last_step_once_joined() {
        use WifiStepState::{Bad, Done, Now};
        use lpc_wire::server::RelayRefusal;

        let joined = |relay| NetworkStatus {
            relay,
            ..status(
                StationState::Connected {
                    ssid: SSID.to_string(),
                    ip: "192.168.1.42".to_string(),
                    rssi: -48,
                    host: "lp-8e30.local".to_string(),
                },
                Some(LastAttempt::Connected),
            )
        };

        let reaching = UiWifiTest::of(SSID, &joined(RelayState::Connecting));
        assert_eq!(states(&reaching), [Done, Done, Done, Now]);
        assert_eq!(reaching.steps()[3].label, "Reaching lightplayer.app");
        assert_eq!(reaching.result(), None);

        let reached = UiWifiTest::of(SSID, &joined(RelayState::Connected));
        assert_eq!(states(&reached), [Done, Done, Done, Done]);
        assert_eq!(reached.result().unwrap().headline, "Connected");
        assert_eq!(reached.relay_note, None);

        let no_internet = UiWifiTest::of(SSID, &joined(RelayState::WaitingForInternet));
        assert_eq!(states(&no_internet), [Done, Done, Done, Bad]);
        assert_eq!(
            no_internet.result().unwrap().headline,
            "Connected, no internet"
        );

        for (relay, note) in [
            (
                RelayState::NoAccount,
                "Sign in to Studio and plug this board in once to use lightplayer.app",
            ),
            (
                RelayState::Refused {
                    reason: RelayRefusal::UnknownAccount,
                },
                "Plug this board into Studio once to refresh its account",
            ),
            (
                RelayState::Refused {
                    reason: RelayRefusal::UpdateFirmware,
                },
                "Update this board's firmware to use lightplayer.app",
            ),
        ] {
            let skipped = UiWifiTest::of(SSID, &joined(relay));
            assert_eq!(states(&skipped), [Done, Done, Done], "{relay:?}");
            assert_eq!(skipped.relay_note, Some(note));
            assert_eq!(skipped.result().unwrap().headline, "Connected");
        }

        for relay in [
            RelayState::Off,
            RelayState::Refused {
                reason: RelayRefusal::Busy,
            },
        ] {
            let plain = UiWifiTest::of(SSID, &joined(relay));
            assert_eq!(states(&plain), [Done, Done, Done], "{relay:?}");
            assert_eq!(plain.relay_note, None);
        }

        // Cloud relay off: whatever the board says, no step and no note.
        let mut off = joined(RelayState::NoAccount);
        off.cloud_relay = false;
        let off = UiWifiTest::of(SSID, &off);
        assert!(!off.relay_step);
        assert_eq!(off.relay_note, None);

        // Still joining: the relay step waits its turn.
        let mut joining = joined(RelayState::WaitingForInternet);
        joining.station = StationState::Connecting {
            ssid: SSID.to_string(),
            step: ConnectStep::GettingAddress,
        };
        assert_eq!(
            states(&UiWifiTest::of(SSID, &joining)),
            [Done, Done, Now, WifiStepState::Todo]
        );
    }

    #[test]
    fn the_relay_step_shows_only_when_it_is_known() {
        let no_internet = UiWifiTest {
            ssid: SSID.to_string(),
            relay_step: true,
            relay_note: None,
            progress: WifiTestProgress::Done(WifiTestOutcome::NoInternet {
                ip: "10.20.4.118".to_string(),
            }),
        };
        use WifiStepState::{Bad, Done};
        assert_eq!(states(&no_internet), [Done, Done, Done, Bad]);
        assert_eq!(no_internet.steps()[3].label, "Reaching lightplayer.app");
        let result = no_internet.result().unwrap();
        assert_eq!(result.headline, "Connected, no internet");
        assert!(result.body.contains("10.20.4.118"));
    }
}
