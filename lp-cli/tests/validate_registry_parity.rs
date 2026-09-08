//! The two halves of the validation system must agree, and only `lp-cli` can
//! see both.
//!
//! `lp-emu-validate` lives inside the `lp-emu/` MIT fence and must not import
//! `fw-checks`, which is AGPL and sits outside it (`just lint-emu-fence` says
//! so). So its payload registry **mirrors** `fw-checks`'s check registry: same
//! slugs, same firmware features, same done markers. Duplication that nothing
//! checks is duplication that drifts, and this is the check.
//!
//! `lp-cli` depends on both, so it is the natural home. If this test ever
//! fails, fix the registry that is wrong — do not relax the assertion.

use fw_checks::{FwCheckConfig, PayloadHeader, all_checks, find_check, write_header};
use lp_emu_validate::header::InbandHeader;
use lp_emu_validate::payload::{ALL_PAYLOADS, Sentinel};
use lp_emu_validate::{FieldClass, HEADER_PREFIX, RECORD_PREFIX, ValidateConfig};

fn fw_check_for(slug: &str) -> FwCheckConfig {
    find_check(slug).unwrap_or_else(|| {
        panic!(
            "lp-emu-validate knows payload `{slug}`, fw-checks does not. Known checks: {}",
            all_checks()
                .iter()
                .map(|c| c.slug())
                .collect::<Vec<_>>()
                .join(", ")
        )
    })
}

#[test]
fn every_payload_has_a_matching_fw_check() {
    for payload in ALL_PAYLOADS {
        let check = fw_check_for(payload.fw_check_slug);

        assert_eq!(
            check.firmware_features, payload.firmware_features,
            "payload `{}`: firmware features disagree",
            payload.name
        );
        assert_eq!(
            check.emits_header, payload.emits_header,
            "payload `{}`: fw-checks says emits_header={}, the registry says {}",
            payload.name, check.emits_header, payload.emits_header
        );
        assert_eq!(
            check.done_marker,
            payload.sentinel.fw_checks_done_marker(),
            "payload `{}`: sentinel disagrees with fw-checks' done_marker",
            payload.name
        );
        assert_eq!(
            check.emits_records,
            !payload.record_kinds.is_empty(),
            "payload `{}`: fw-checks says emits_records={}, the registry lists {:?}",
            payload.name,
            check.emits_records,
            payload.record_kinds
        );
    }
}

/// A `Ready` payload never finishes and a `State` payload never prints, so
/// neither may claim a done marker; a `Done` payload must.
#[test]
fn sentinels_and_done_markers_are_consistent() {
    for payload in ALL_PAYLOADS {
        let check = fw_check_for(payload.fw_check_slug);
        match payload.sentinel {
            Sentinel::Done(marker) => {
                assert_eq!(check.done_marker, Some(marker), "{}", payload.name);
            }
            Sentinel::Ready(_) => {
                assert!(
                    check.done_marker.is_none(),
                    "payload `{}` serves forever; it must not declare a done marker",
                    payload.name
                );
            }
            Sentinel::State(_) => {
                assert!(
                    check.done_marker.is_none(),
                    "payload `{}` prints nothing at all — its subject is machine state — so \
                     the firmware side has no marker to declare",
                    payload.name
                );
            }
        }
        // Whatever the shape, the two registries agree on what `--exit-on`
        // would be given: the done marker, or nothing.
        assert_eq!(
            payload.sentinel.exit_on(),
            check.done_marker,
            "payload `{}`: the marker a run stops on must be the one fw-checks declares",
            payload.name
        );
    }
}

/// The link, the host plan and the emulator-only reason are **host-side**
/// properties and are deliberately not mirrored: the image is the same bytes
/// whichever host is on the other end of the cable, and what differs is what
/// that host does. This test says so out loud, so that "fw-checks does not
/// know about `host_plan`" reads as a decision rather than as an omission the
/// parity test forgot.
#[test]
fn the_host_side_properties_are_not_mirrored_and_that_is_the_point() {
    for payload in ALL_PAYLOADS {
        let check = fw_check_for(payload.fw_check_slug);
        // Same firmware, on every configuration that can run it.
        assert_eq!(
            check.firmware_features, payload.firmware_features,
            "payload `{}`",
            payload.name
        );
        // The three USB scenarios are one image asked three different
        // questions, and `fw-checks` cannot tell them apart — which is
        // exactly right, because nothing in the firmware differs.
        if payload.host_plan.is_some() {
            assert!(
                check.firmware_features.contains(&"server"),
                "payload `{}` asks about the host link, so it is the shipped image",
                payload.name
            );
        }
    }
    let scenarios: Vec<&str> = ALL_PAYLOADS
        .iter()
        .filter(|p| p.host_plan.is_some())
        .map(|p| p.name)
        .collect();
    assert_eq!(
        scenarios,
        vec![
            "boot-idle",
            "usb-negative-control",
            "usb-detach-reattach",
            "usb-host-absent",
            // M4's flash-backed twin of `boot-idle`, which is the shipped
            // image over the shipped link too — and P5's route to closing
            // DD30 on flash-backed bytes.
            "boot-idle-flash",
        ],
        "the emu-m6 set plus M4's flash-backed boot, and nothing else, drives the host"
    );
}

/// The GPIO calibration payload's readiness line is the one `fw-checks` emits.
#[test]
fn the_gpio_payloads_sentinel_is_the_line_the_firmware_prints() {
    use fw_checks::checks::gpio_calibrate::{CAL_READY_PREFIX, Response};

    let payload = ALL_PAYLOADS
        .iter()
        .find(|p| p.name == "gpio-calibrate")
        .expect("gpio-calibrate is registered");
    assert_eq!(payload.sentinel, Sentinel::Ready(CAL_READY_PREFIX));

    let ready = format!("{}", Response::Ready { target: "esp32c6" });
    assert!(
        ready.starts_with(payload.sentinel.marker()),
        "`{ready}` should start with the sentinel `{}`",
        payload.sentinel.marker()
    );
}

/// The `CAL PULSE` series the host parses matches the line the device prints.
#[test]
fn the_gpio_payloads_series_parses_the_line_the_firmware_prints() {
    use fw_checks::checks::gpio_calibrate::{DutyRamp, Response};

    let payload = ALL_PAYLOADS
        .iter()
        .find(|p| p.name == "gpio-calibrate")
        .unwrap();
    let spec = payload.series.first().expect("gpio-calibrate has a series");

    let mut duty = DutyRamp::new();
    duty.step();
    let line = format!(
        "{}",
        Response::Pulse {
            gpio: 18,
            duty: duty.percent(),
        }
    );
    let caps = spec
        .regex()
        .captures(&line)
        .unwrap_or_else(|| panic!("series `{}` should parse `{line}`", spec.name));
    assert_eq!(&caps["gpio"], "18");
    assert_eq!(&caps["duty"], "20");
}

/// The bridge's sentinel is the prefix `fw-checks` renders, and the series the
/// host parses is the line the firmware prints — including the hardware-overrun
/// value, which is the one number in it that must never be quietly reformatted.
#[test]
fn the_uart_bridges_ready_line_is_the_line_the_firmware_prints() {
    use fw_checks::checks::uart_bridge::{
        BRIDGE_READY_PREFIX, ROM_CONSOLE_BAUD, ReadyLine, UART0_RX_GPIO, UART0_TX_GPIO,
    };

    let payload = ALL_PAYLOADS
        .iter()
        .find(|p| p.name == "uart-bridge")
        .expect("uart-bridge is registered");
    assert_eq!(payload.sentinel, Sentinel::Ready(BRIDGE_READY_PREFIX));

    let spec = payload.series.first().expect("uart-bridge has a series");
    for (prev_to_uart, prev_to_usb) in [(0, 0), (7, u32::MAX)] {
        let line = format!(
            "{}",
            ReadyLine {
                baud: ROM_CONSOLE_BAUD,
                tx_gpio: UART0_TX_GPIO,
                rx_gpio: UART0_RX_GPIO,
                prev_drop_to_uart: prev_to_uart,
                prev_drop_to_usb: prev_to_usb,
            }
        );
        assert!(
            line.starts_with(payload.sentinel.marker()),
            "`{line}` should start with the sentinel"
        );
        let caps = spec
            .regex()
            .captures(&line)
            .unwrap_or_else(|| panic!("series `{}` should parse `{line}`", spec.name));
        assert_eq!(&caps["baud"], "115200");
        assert_eq!(&caps["tx"], "16");
        assert_eq!(&caps["rx"], "17");
        assert_eq!(caps["prev_drop_to_uart"], *prev_to_uart.to_string());
        assert_eq!(caps["prev_drop_to_usb"], *prev_to_usb.to_string());
    }
}

/// The header line every C6 harness now prints through
/// `fw_checks::write_header(&mut esp_println::Printer, ..)` (G3 sitting-1
/// blocker: `test_gpio_calibrate` installed no logger, so the log-based
/// `emit_header` never reached a silicon capture there). Pinned per payload
/// so a change to `PayloadHeader`'s `Display` or to `write_header`'s framing
/// (the trailing `\n`) shows up here first, rather than at a desk.
#[test]
fn every_payloads_header_line_is_pinned() {
    let cases: &[(&str, &str, &str)] = &[
        (
            "shader-compile-stress",
            "esp32c6,test_shader_compile_incremental",
            "[fw-checks-header] {\"schema\":1,\"payload\":\"shader-compile-stress\",\"chip\":\"esp32c6\",\"firmware_commit\":\"d6cfaa2051ae\",\"firmware_features\":\"esp32c6,test_shader_compile_incremental\",\"firmware_dirty\":false}\n",
        ),
        (
            "gpio-calibrate",
            "esp32c6,test_gpio_calibrate",
            "[fw-checks-header] {\"schema\":1,\"payload\":\"gpio-calibrate\",\"chip\":\"esp32c6\",\"firmware_commit\":\"d6cfaa2051ae\",\"firmware_features\":\"esp32c6,test_gpio_calibrate\",\"firmware_dirty\":false}\n",
        ),
        (
            "uart-bridge",
            "esp32c6,test_uart_bridge",
            "[fw-checks-header] {\"schema\":1,\"payload\":\"uart-bridge\",\"chip\":\"esp32c6\",\"firmware_commit\":\"d6cfaa2051ae\",\"firmware_features\":\"esp32c6,test_uart_bridge\",\"firmware_dirty\":false}\n",
        ),
        (
            "jit-math-perf",
            "esp32c6,test_jit_math_perf",
            "[fw-checks-header] {\"schema\":1,\"payload\":\"jit-math-perf\",\"chip\":\"esp32c6\",\"firmware_commit\":\"d6cfaa2051ae\",\"firmware_features\":\"esp32c6,test_jit_math_perf\",\"firmware_dirty\":false}\n",
        ),
    ];
    for (payload, firmware_features, expected) in cases {
        assert!(
            find_check(payload).expect("registered").emits_header,
            "`{payload}` has a pinned header line but does not claim to print one"
        );
        let header = PayloadHeader {
            payload,
            chip: "esp32c6",
            firmware_commit: "d6cfaa2051ae",
            firmware_features,
            firmware_dirty: false,
        };
        let mut out = String::new();
        write_header(&mut out, &header).expect("writing to a String never fails");
        assert_eq!(&out, expected, "payload `{payload}`");
    }

    // A payload with no row above is a payload this test forgot — unless it
    // says it prints no header at all. `boot-idle` is the one of those: it has
    // no `fw-checks` module, because the payload IS the shipped image, so its
    // provenance is the transcript's sidecar and nothing else.
    for payload in ALL_PAYLOADS {
        assert_eq!(
            cases.iter().any(|(name, ..)| *name == payload.name),
            payload.emits_header,
            "payload `{}` (emits_header={}) has no pinned header line in this test",
            payload.name,
            payload.emits_header,
        );
    }
}

/// A payload that prints no in-band header has no `fw-checks` module either,
/// and the other way round. The two are the same fact — a module is what
/// prints the line — and stating it here keeps a future payload from claiming
/// half of it.
#[test]
fn a_payload_without_a_module_prints_no_header() {
    for payload in ALL_PAYLOADS {
        assert_eq!(
            payload.fw_checks_feature.is_some(),
            payload.emits_header,
            "payload `{}`: fw_checks_feature={:?} but emits_header={}",
            payload.name,
            payload.fw_checks_feature,
            payload.emits_header
        );
    }
}

/// The shipped-image payload, whose whole point is that it is not a `test_*`
/// module: several features, no done marker of its own making, no records and
/// no header. It is the one entry where the two registries could drift into
/// something meaningless without anybody noticing, because there is no
/// firmware code on the other side to fail to compile.
#[test]
fn the_boot_idle_payload_is_the_shipped_image_on_both_sides() {
    let payload = ALL_PAYLOADS
        .iter()
        .find(|p| p.name == "boot-idle")
        .expect("boot-idle is registered");
    let check = fw_check_for(payload.fw_check_slug);

    assert_eq!(payload.firmware_features, ["server", "radio", "memory_fs"]);
    assert_eq!(check.firmware_features, payload.firmware_features);
    assert_eq!(payload.fw_checks_feature, None);
    assert!(!payload.emits_header && !check.emits_header);
    assert!(!check.emits_records && payload.record_kinds.is_empty());
    assert_eq!(
        payload.sentinel,
        Sentinel::Done("[stack] heartbeat: high-water")
    );
    assert_eq!(check.done_marker, Some("[stack] heartbeat: high-water"));
    // The three series it parses out of a boot.
    let names: Vec<&str> = payload.series.iter().map(|s| s.name).collect();
    assert_eq!(names, ["hello", "heartbeat", "stack-heartbeat"]);
}

/// The two prefixes must not collide, and the header schema must match.
#[test]
fn the_wire_prefixes_and_schema_agree_across_the_fence() {
    assert_eq!(HEADER_PREFIX, fw_checks::FW_CHECKS_HEADER_PREFIX);
    assert_eq!(RECORD_PREFIX, fw_checks::FW_CHECK_JSON_PREFIX);
    assert_ne!(HEADER_PREFIX, RECORD_PREFIX);
    assert_eq!(
        lp_emu_validate::HEADER_SCHEMA,
        fw_checks::HEADER_SCHEMA,
        "the device and the host must speak the same header schema"
    );
}

/// The line `fw-checks` prints is the line `lp-emu-validate` parses. This is
/// the only place the two implementations of the header meet.
#[test]
fn the_emitted_header_parses_back_into_the_hosts_type() {
    let emitted = format!(
        "{HEADER_PREFIX}{}",
        PayloadHeader {
            payload: "shader-compile-stress",
            chip: "esp32c6",
            firmware_commit: "d6cfaa2051ae",
            firmware_features: "esp32c6,spike_uart0_link,test_shader_compile_incremental",
            firmware_dirty: false,
        }
    );
    let parsed = InbandHeader::from_line(&emitted)
        .expect("the emitted header is valid JSON")
        .expect("the prefix is present");

    assert_eq!(parsed.schema, lp_emu_validate::HEADER_SCHEMA);
    assert_eq!(parsed.payload, "shader-compile-stress");
    assert_eq!(parsed.chip, "esp32c6");
    assert_eq!(parsed.firmware_commit, "d6cfaa2051ae");
    assert_eq!(
        parsed.firmware_features,
        "esp32c6,spike_uart0_link,test_shader_compile_incremental"
    );
    assert_eq!(parsed.firmware_dirty, Some(false));
}

/// A header whose payload name is not in the registry is a header nobody can
/// replay. Catch it here rather than at a desk.
#[test]
fn every_emitted_payload_name_is_a_registered_payload() {
    for payload in ALL_PAYLOADS {
        let emitted = format!(
            "{HEADER_PREFIX}{}",
            PayloadHeader {
                payload: payload.name,
                chip: "esp32c6",
                firmware_commit: "0123456789ab",
                firmware_features: "esp32c6",
                firmware_dirty: false,
            }
        );
        let parsed = InbandHeader::from_line(&emitted).unwrap().unwrap();
        lp_emu_validate::find_payload(&parsed.payload)
            .unwrap_or_else(|e| panic!("{}: {e}", payload.name));
    }
}

/// The runner's own table has to hold together: every set names known
/// payloads, and every configuration parses.
#[test]
fn the_embedded_validate_table_is_coherent() {
    let cfg = ValidateConfig::embedded();
    for set in &cfg.sets {
        let payloads = cfg.payloads_in(&set.name).unwrap();
        assert!(!payloads.is_empty(), "set `{}` is empty", set.name);
        for payload in payloads {
            fw_check_for(payload.fw_check_slug);
        }
    }
    for entry in &cfg.configurations {
        entry.parsed().unwrap();
    }
}

/// Every trust entry states a reason. A grade without a `because` is a guess
/// with a table around it.
#[test]
fn every_trust_entry_says_why() {
    let cfg = ValidateConfig::embedded();
    for entry in &cfg.configurations {
        for class in FieldClass::ALL {
            if let Some(why) = entry.trust.because(*class) {
                assert!(
                    why.len() > 20,
                    "{} / {class}: `{why}` is not a reason",
                    entry.name
                );
            }
        }
    }
}
