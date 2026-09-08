//! M6's gates, as replays of committed transcripts.
//!
//! Like `m3_replays.rs` these need no firmware and no board: they read what
//! the runner recorded, beside what the desk recorded, and check the claims
//! the milestone is allowed to make.
//!
//! ```bash
//! # the DD26/DD30 arbitration, at sitting 1's own commit and its own bytes
//! scripts/emu/build-reference-image.sh esp32c6,server,radio,memory_fs 735af98ae none
//! cargo run -q -p lp-cli -- validate record boot-idle --config lp-emu:esp32c6:t1 \
//!   --date 2026-09-07 --commit 735af98ae9d9 --timeout-secs 8 \
//!   --image boot-idle=target/emu-ref/735af98ae-boot-idle-memfs-usb/fw-esp32c6
//!
//! # the three link-monitor scenarios, at main — see SCENARIO_COMMIT
//! scripts/emu/build-reference-image.sh esp32c6,server,radio,memory_fs 372392b9c none
//! cargo run -q -p lp-cli -- validate record usb-negative-control \
//!   --config lp-emu:esp32c6:t1 --date 2026-09-07 --commit 372392b9c \
//!   --image usb-negative-control=target/emu-ref/372392b9c-boot-idle-memfs-usb/fw-esp32c6
//! ```
//!
//! **Two commits, on purpose.** Sitting 1 flashed the board at `735af98ae`
//! and M6 P1b landed the connection monitor's stamps that afternoon, so the
//! image the board ran has no vehicle for the transitions three of these
//! four payloads exist to measure, and the image that has the vehicle is not
//! the one silicon captured. Each transcript's filename carries the commit
//! that produced it, which is what that field is for.
//!
//! **Never edit a transcript.** A failure here is a regression or a
//! re-capture with its own header, never a digit changed in a `.txt`.

use std::path::PathBuf;

use lp_emu_validate::grade::FieldClass;
use lp_emu_validate::replay::{ReplayOptions, ReplayReport, replay};
use lp_emu_validate::transcript::{Transcript, sidecar_path};
use lp_emu_validate::{TranscriptHeader, find_payload};

/// Sitting 1's commit: the DD30 arbitration's two transcripts.
const OURS_SILICON_COMMIT: &str = "lp-emu-esp32c6-t1-2026-09-07-735af98ae.txt";
const OURS_T2: &str = "lp-emu-esp32c6-t2-2026-09-07-735af98ae.txt";
const SILICON: &str = "silicon-esp32c6-2026-09-07-735af98ae.txt";
/// Main at M6 P3: the three link-monitor scenarios.
const OURS: &str = "lp-emu-esp32c6-t1-2026-09-07-372392b9c.txt";

fn transcripts() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../transcripts/esp32c6")
}

fn load(payload: &str, name: &str) -> Transcript {
    let path = transcripts().join(payload).join(name);
    Transcript::load(&path).unwrap_or_else(|e| panic!("loading {}: {e:#}", path.display()))
}

/// Reload a transcript with its body altered. The file on disk is never
/// touched — this is the negative control's vehicle, not an edit.
fn load_with_body(payload: &str, name: &str, edit: impl Fn(String) -> String) -> Transcript {
    let path = transcripts().join(payload).join(name);
    let body = std::fs::read_to_string(&path).unwrap();
    let header =
        TranscriptHeader::from_json(&std::fs::read_to_string(sidecar_path(&path)).unwrap())
            .unwrap();
    Transcript::from_parts(header, &edit(body)).unwrap()
}

fn series(
    t: &Transcript,
    payload: &str,
    name: &str,
) -> Vec<lp_emu_validate::transcript::SeriesSample> {
    let spec = find_payload(payload)
        .unwrap()
        .series
        .iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("payload `{payload}` has no series `{name}`"));
    t.series(spec)
}

/// `(high_water, stack_bytes)` from the FIRST `[stack]` line in a capture.
///
/// The series cannot answer this: it is keyed on the stack's size, which is a
/// link-time constant, so two reports of one boot collapse to the later one.
/// That key is right for what it was chosen for — two transcripts of the same
/// image are talking about the same stack or they are not comparable at all —
/// and it means a capture that saw the mark grow reports the grown one.
fn first_stack_report(t: &Transcript) -> (u64, u64) {
    let line = t
        .lines
        .iter()
        .find(|l| l.contains("[stack] heartbeat: high-water"))
        .expect("a stack report");
    let after = line.split("high-water ").nth(1).expect("the figures");
    let mut it = after.split_whitespace();
    let high: u64 = it.next().unwrap().parse().unwrap();
    it.next();
    it.next();
    let total: u64 = it.next().unwrap().parse().unwrap();
    (high, total)
}

fn diffs(report: &ReplayReport, class: FieldClass) -> Vec<String> {
    report
        .differences_in(class)
        .map(|d| format!("{}.{}: {} vs {}", d.scope, d.field, d.left, d.right))
        .collect()
}

/// **G4-3, the DD26/DD30 arbitration.** The shipped image over the link it
/// ships with, on our machine and on the desk board, from the same commit
/// built the same way — no `spike_uart0_link`, no cherry-pick, the same four
/// features, the same eFuse identity.
///
/// The result, and it is the milestone's headline number:
///
/// | figure | ours | silicon | |
/// |---|---:|---:|---|
/// | `[stack] high-water` | 11,784 B | 11,784 B | **equal** |
/// | of a stack of | 72,136 B | 72,136 B | equal |
/// | `totalBytes` | 325,536 | 325,536 | equal |
/// | `freeBytes` @ 5 s | 266,400 | 266,392 | **+8** |
/// | `usedBytes` @ 5 s | 59,136 | 59,144 | −8 |
///
/// DD26's 104 B is gone, and what it was is now visible: silicon's OWN
/// consecutive heartbeats read 266,392 / 266,496 / 266,388 at 5 / 10 / 15 s,
/// a swing of 108 B in one direction and back. That is a live allocation
/// whose presence at the sampled microsecond depends on interleaving, and
/// comparing a sample of it across two configurations was always comparing
/// the die roll. On the same bytes, at the same sample, the residual is
/// **eight bytes** — and the same eight at the 10 s sample too (266,504 vs
/// 266,496), which is what says it is a constant rather than more of the
/// same noise.
///
/// Eight bytes are not zero, so G4-3 as the brief wrote it is **not met**,
/// and the honest form is this test: the gap is pinned at exactly 8 B, in
/// that direction, and moves the day the machine or the image does. The
/// leading candidate is not the machine at all — the board is on its third
/// boot (`bootCount 3`, `resetReason user-reset`) against our first, so its
/// recovery state is not ours. P5 closes it, on flash-backed bytes, ideally
/// against a first-boot capture. **Never tuned toward 266,392.**
#[test]
fn boot_idle_usb_vs_silicon_attached() {
    let ours = load("boot-idle", OURS_SILICON_COMMIT);
    let silicon = load("boot-idle", SILICON);
    // Same image, stated by both sidecars rather than assumed.
    assert_eq!(ours.header.firmware_commit, "735af98ae9d9");
    assert_eq!(silicon.header.firmware_commit, "735af98ae9d9");
    assert_eq!(
        ours.header.firmware_features,
        silicon.header.firmware_features
    );
    assert!(
        !ours
            .header
            .firmware_features
            .iter()
            .any(|f| f.contains("spike")),
        "the arbitration image carries no UART0 workaround: {:?}",
        ours.header.firmware_features
    );

    let report = replay(&ours, &silicon, ReplayOptions::default()).expect("the replay runs");
    println!("{}", report.render());

    // The stack probe's FIRST report, which is the one both runs made. It is
    // read off the lines rather than the series because the series is keyed
    // on the stack's size (a link-time constant) and last-writes then picks
    // silicon's later, deeper report — see below.
    assert_eq!(
        first_stack_report(&ours),
        first_stack_report(&silicon),
        "the first high-water report is the same mark on both, off the same image"
    );
    assert_eq!(first_stack_report(&ours), (11_784, 72_136));

    // The heap at the five-second sample, which is what this payload is
    // about, now that the series is keyed on the tick rather than collapsing
    // to whichever heartbeat a capture happened to end on.
    let beat = |t: &Transcript, field: &str| -> i64 {
        series(t, "boot-idle", "heartbeat")
            .iter()
            .find(|s| s.key == "5")
            .expect("the five-second heartbeat")
            .values[field]
            .parse()
            .unwrap()
    };
    assert_eq!(beat(&ours, "total_bytes"), beat(&silicon, "total_bytes"));
    assert_eq!(
        beat(&ours, "free_bytes") - beat(&silicon, "free_bytes"),
        8,
        "the DD30 residual is eight bytes, and this test is where it is noticed if it moves"
    );
    assert_eq!(
        beat(&ours, "used_bytes") - beat(&silicon, "used_bytes"),
        -8,
        "and it is complementary: one live block, not a leak on either side"
    );

    // Four memory differences in the whole replay, and every one of them is
    // accounted for. Two are the eight bytes above. Two are the stack probe,
    // and they are not an artefact of capture length: `stack_probe` reports
    // only when the high-water mark has GROWN, silicon's grew to 11,912 B by
    // fifteen seconds, and a sixteen-second run of ours never produced a
    // second report at all. Our interleaving does not reach silicon's
    // deepest interrupted call chain. Reported, carried to P5, not tuned.
    let memory = diffs(&report, FieldClass::Memory);
    assert_eq!(memory.len(), 4, "{memory:?}");
    for expected in [
        "heartbeat[5].free_bytes: 266400 vs 266392",
        "heartbeat[5].used_bytes: 59136 vs 59144",
        "stack-heartbeat[72136].high_water: 11784 vs 11912",
        "stack-heartbeat[72136].headroom: 60352 vs 60224",
    ] {
        assert!(
            memory.iter().any(|d| d == expected),
            "{expected}: {memory:?}"
        );
    }

    // And the shape of the two captures: ours stops at the payload's
    // sentinel, silicon's desk script read on to fifteen seconds. The replay
    // says so rather than comparing across it.
    let structural_problems = report.structural_problems.join("\n");
    for missing in ["sample `10`", "sample `15`"] {
        assert!(
            structural_problems.contains(missing),
            "a heartbeat silicon has and we do not must be named: {structural_problems}"
        );
    }

    // Everything structural agrees except what the board's own history says.
    // A board on its third boot after a user reset is not a machine on its
    // first, and neither is wrong.
    let structural = diffs(&report, FieldClass::Structural);
    assert_eq!(
        structural.len(),
        2,
        "only the boot history may differ: {structural:?}"
    );
    for field in ["reset_reason", "boot_count"] {
        assert!(
            structural.iter().any(|d| d.contains(field)),
            "{field} is missing from {structural:?}"
        );
    }
    // The wire hello — protocol version, features, board profile — is
    // identical, which is the same-image claim read back off the wire.
    assert_eq!(report.differences_in(FieldClass::Wire).count(), 0);
    assert!(report.compared(FieldClass::Wire) > 0);
    assert_eq!(
        report.differences_in(FieldClass::UsbSerialJtag).count(),
        0,
        "nothing in this payload's series is a link claim, and none differ"
    );
}

/// A time grade must not move a heap byte, and on the shipped image over its
/// own link it does not. The stack high-water does not move here either —
/// M3 saw it move (11,432 under t1, 11,752 under t2 at `d6cfaa205`), so this
/// is recorded as what these two runs did, not promoted into a rule.
#[test]
fn t2_matches_t1_on_the_shipped_link() {
    let t1 = load("boot-idle", OURS_SILICON_COMMIT);
    let t2 = load("boot-idle", OURS_T2);
    let report = replay(&t2, &t1, ReplayOptions::default()).expect("the replay runs");
    println!("{}", report.render());
    assert_eq!(
        report.differences_in(FieldClass::Memory).count(),
        0,
        "{:?}",
        diffs(&report, FieldClass::Memory)
    );
    assert_eq!(report.differences_in(FieldClass::Structural).count(), 0);
    assert_eq!(report.differences_in(FieldClass::Wire).count(), 0);
    assert!(report.is_ok(), "{:?}", report.failures());
    assert_eq!(first_stack_report(&t2), first_stack_report(&t1));
}

/// **The negative control's own transitions**, on the firmware's clock.
///
/// The port is closed from boot and opened at eight seconds. The monitor
/// latches once at 893 ms — two 250 ms write timeouts after the hello — and
/// recovers at 8,145 ms, which is the first 2 s probe to find the endpoint
/// free after the open. Both stamps present, in that order, count 1: per
/// DD38 the PAIR is the transition claim, never the count.
///
/// The silicon half (G1b-3) has not landed — the desk was held by another
/// lane when P1b tried — so this gates our own capture against itself and
/// the silicon transcript is **owed**. When it lands, this is where the two
/// are compared: presence and order hard, the two millisecond figures
/// reported with their ratio (PD9/D13, DD33).
#[test]
fn the_negative_control_stamps_the_pair() {
    let t = load("usb-negative-control", OURS);
    assert!(t.sentinel_line().is_some(), "the recovery stamp arrived");

    let link = series(&t, "usb-negative-control", "link-monitor");
    assert_eq!(link.len(), 1, "one link object, from the last heartbeat");
    let v = &link[0].values;
    assert_eq!(v["count"], "1", "one silence this boot");
    let latched: u64 = v["not_draining_ms"].parse().unwrap();
    let again: u64 = v["draining_again_ms"].parse().unwrap();
    assert_eq!(latched, 893, "the same 893 ms G1b-4 measured with no host");
    assert!(
        (8_000..=10_000).contains(&again),
        "the recovery is the first probe of io_task's 2 s grid after the open at 8 s, \
         so it is bounded by [open, open + PROBE_INTERVAL], not a millisecond: {again}"
    );
    assert!(latched < again, "the pair is in order");

    // The hello is NOT here, and its absence is the payload's subject: it
    // went out at server start, into a port nobody had open.
    assert!(
        !t.lines.iter().any(|l| l.contains(r#""hello":{"proto""#)),
        "a hello in this capture would mean the port was open when it should not have been"
    );
    // The firmware's own account of the recovery reached the host; its twin
    // did not, because the latch it reports is what dropped it (discovery §4).
    assert!(
        t.lines.iter().any(|l| l.contains("host draining again")),
        "the recovery line is the first thing a new reader sees"
    );
    assert!(
        !t.lines.iter().any(|l| l.contains("host not draining")),
        "the not-draining line is self-erasing on this link and must never appear"
    );
    // And the device kept working throughout the silence.
    let beat = series(&t, "usb-negative-control", "heartbeat");
    assert_eq!(beat.len(), 1, "one heartbeat crossed, at ten seconds");
    assert_eq!(beat[0].key, "10");
    assert!(
        beat[0].values["frame_count"].parse::<u64>().unwrap() > 9_000,
        "frames kept being rendered while nobody was listening: {:?}",
        beat[0].values["frame_count"]
    );
}

/// **The cable out at six seconds and back in at nine.** The device side of
/// `scripts/device-scenarios/s7-unplug-mid-op.json`, which has wanted a
/// trace since M2.
///
/// Two things this pins that nothing else does. The session recovers with
/// **no re-hello**: exactly one hello frame in the whole transcript, so a
/// host that reconnects gets no fresh identity and everything downstream has
/// to survive that. And the firmware **noticed nothing** — both stamps
/// absent, count 0 — because the monitor resets its latch optimistically
/// when SOF returns and in the 500 ms the port was closed it had nothing to
/// say (M6 P3's finding, DD41; the discovery's "re-attach" row is corrected
/// to match).
#[test]
fn detach_reattach_recovers() {
    let t = load("usb-detach-reattach", OURS);
    assert!(
        t.sentinel_line().is_some(),
        "a whole heartbeat crossed the recovered session"
    );

    let hellos = t
        .lines
        .iter()
        .filter(|l| l.contains(r#""hello":{"proto""#))
        .count();
    assert_eq!(
        hellos, 1,
        "the firmware does not re-hello on a re-attach; a second one would be a \
         different device model than the one Studio is written against"
    );

    // Both heartbeats: the one before the unplug and the one after the
    // recovery. The 10 s one is the sentinel, so it is the last.
    let beat = series(&t, "usb-detach-reattach", "heartbeat");
    let ticks: Vec<&str> = beat.iter().map(|b| b.key.as_str()).collect();
    assert_eq!(
        ticks,
        vec!["5", "10"],
        "one heartbeat each side of the unplug, and nothing in between \
         (the samples are indexed, so they read in key order)"
    );

    // The link stayed silent about it, and that is the finding.
    assert!(
        !t.lines.iter().any(|l| l.contains("hostNotDrainingMs")),
        "no latch: the monitor resets optimistically on SOF and had nothing to write"
    );
    assert!(
        t.lines
            .iter()
            .any(|l| l.contains(r#""notDrainingCount":0"#)),
        "the count says zero, on the firmware's own clock"
    );
}

/// **G6-3 as a transcript.** No cable at all: the device says nothing, so
/// the transcript is what the machine could see of it.
///
/// `TIMED_OUT` 1 — esp-println spun 50,000 times on a FIFO nobody drains and
/// gave up, once, for the rest of the boot. `HOST_NOT_DRAINING_MS` 893 —
/// two 250 ms write timeouts land before the three-poll enumeration verdict,
/// so the monitor latches *before* it concludes there is no cable (DD38's
/// correction to the discovery's "absent" row). `HOST_DRAINING_AGAIN_MS`
/// `0xffffffff` — the "never this boot" sentinel, and never is the claim:
/// this is the half of the pair that tells absent apart from
/// attached-and-unread, which latches the same way and then recovers.
#[test]
fn host_absent_pins_the_state() {
    let t = load("usb-host-absent", OURS);
    assert!(t.sentinel_line().is_some(), "the last probe answered");

    let probes = series(&t, "usb-host-absent", "probe");
    assert_eq!(probes.len(), 4, "four questions, four answers");
    let value = |symbol: &str| -> String {
        probes
            .iter()
            .find(|p| p.key.ends_with(symbol))
            .unwrap_or_else(|| panic!("no probe for {symbol}"))
            .values["value"]
            .clone()
    };
    assert_eq!(value("TIMED_OUT"), "00000001");
    assert_eq!(value("HOST_NOT_DRAINING_MS"), "0000037d", "893 ms");
    assert_eq!(
        value("HOST_DRAINING_AGAIN_MS"),
        "ffffffff",
        "never this boot — an absent host never comes back, and that is the discriminator"
    );
    assert_eq!(value("NOT_DRAINING_COUNT"), "00000001");

    // Nothing the device said is in here, because nothing could be.
    assert!(
        !t.lines.iter().any(|l| l.contains("[INIT]")),
        "the boot lines went into an endpoint no host will ever drain"
    );
    assert!(
        find_payload("usb-host-absent")
            .unwrap()
            .emulator_only
            .is_some()
    );
}

/// The negative control for the negative controls. A digit changed in the
/// state these transcripts exist to report has to fail, or none of the above
/// means anything.
///
/// Note **which** digit. `hostNotDrainingMs` and `hostDrainingAgainMs` are
/// `Timing`, reported with their ratio and never gated (PD9/D13, DD33), so
/// swapping them is not what a replay is for. What is hard is the count and
/// the "never" sentinel — the two things that say what the link *did*.
#[test]
fn a_link_that_pretends_to_have_recovered_fails_the_replay() {
    // An absent host that claims a recovery stamp: `usb-serial-jtag`, hard.
    let good = load("usb-host-absent", OURS);
    let liar = load_with_body("usb-host-absent", OURS, |body| {
        body.replace("= 0xffffffff", "= 0x00002000")
    });
    let report = replay(&liar, &good, ReplayOptions::default()).unwrap();
    assert!(!report.is_ok(), "a fabricated recovery must fail");
    assert!(
        report
            .failures()
            .iter()
            .any(|f| f.contains("HOST_DRAINING_AGAIN_MS")),
        "{:?}",
        report.failures()
    );

    // A second silence where there was one: `notDrainingCount`, hard.
    let good = load("usb-negative-control", OURS);
    let liar = load_with_body("usb-negative-control", OURS, |body| {
        body.replace(r#""notDrainingCount":1"#, r#""notDrainingCount":2"#)
    });
    let report = replay(&liar, &good, ReplayOptions::default()).unwrap();
    assert!(!report.is_ok(), "a second silence must fail");
    assert!(
        report.failures().iter().any(|f| f.contains("count")),
        "{:?}",
        report.failures()
    );

    // And the heap, on the arbitration payload, against silicon's own file.
    let liar = load_with_body("boot-idle", OURS_SILICON_COMMIT, |body| {
        body.replace(r#""freeBytes":266400"#, r#""freeBytes":266392"#)
    });
    let report = replay(
        &liar,
        &load("boot-idle", OURS_SILICON_COMMIT),
        ReplayOptions::default(),
    )
    .unwrap();
    assert!(
        report.failures().iter().any(|f| f.contains("free_bytes")),
        "even a change that would make the DD30 gap vanish is a difference: {:?}",
        report.failures()
    );
}

/// Every transcript M6 recorded is filed where its own header says, carries
/// its sidecar, and says which image it came from — including that the image
/// carried no UART0 workaround, which is the whole of the link change.
#[test]
fn the_m6_transcripts_are_filed_where_their_headers_say() {
    let root = transcripts();
    for (payload, name, commit) in [
        ("boot-idle", OURS_SILICON_COMMIT, "735af98ae9d9"),
        ("boot-idle", OURS_T2, "735af98ae9d9"),
        ("usb-negative-control", OURS, "372392b9c"),
        ("usb-detach-reattach", OURS, "372392b9c"),
        ("usb-host-absent", OURS, "372392b9c"),
    ] {
        let path = root.join(payload).join(name);
        let t = Transcript::load(&path).unwrap();
        assert_eq!(t.header.payload, payload);
        assert_eq!(t.header.chip, "esp32c6");
        assert_eq!(t.header.firmware_commit, commit);
        assert_eq!(
            t.header.firmware_features,
            vec!["esp32c6", "server", "radio", "memory_fs"],
            "{payload}: every M6 scenario is the shipped image minus flash, over its own link"
        );
        assert_eq!(
            root.join(t.header.relative_path().unwrap().replace("esp32c6/", "")),
            path
        );
        assert!(t.header.tools.contains_key("lp-emu-esp32c6"));
        let source = t.header.source.as_deref().unwrap();
        assert!(source.contains("--strict-bus"), "{source}");
        assert!(
            !source.contains("spike_uart0_link"),
            "the M6 scenarios run the shipped link: {source}"
        );
        // Every class this configuration reports is still `modeled`, with the
        // evidence in the reason (DD33 Q4). When a class is earned, this is
        // what says so.
        assert_eq!(
            t.header.trust.grade(FieldClass::UsbSerialJtag),
            lp_emu_validate::grade::Grade::Modeled
        );
    }
}
