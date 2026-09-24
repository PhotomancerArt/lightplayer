//! The sample as a device would send it: every board→host line packed and
//! framed in place, console text between frames, and the whole stream scanned
//! back in uneven slices to the same JSON.

#[path = "support/fixture_dictionary.rs"]
mod fixture_dictionary;

use fixture_dictionary::{fixture_dictionary, fixture_lines};
use lp_json_pack::{
    DropReason, FRAME_KIND_PACK, PackEncoder, ScanEvent, VecFrameScanner, decode, frame_in_place,
    in_place_headroom,
};

/// Build the stream: `[log] line n\n` then the framed line, for every
/// board→host line. Returns the stream and the JSON expected back, in order.
fn device_stream() -> (Vec<u8>, Vec<&'static [u8]>) {
    let dict = fixture_dictionary();
    let mut stream = Vec::new();
    let mut expected = Vec::new();
    // The device's frame buffer: payload at the headroom, framed from 0.
    let mut frame_buf = vec![0u8; 20_000];
    let headroom = in_place_headroom(16_384);
    for (i, line) in fixture_lines().iter().filter(|l| l.dir == b'<').enumerate() {
        stream.extend_from_slice(format!("[log] line {i}\n").as_bytes());
        let mut enc = PackEncoder::new(&mut frame_buf[headroom..headroom + 16_384], dict);
        enc.json_value(line.json).unwrap();
        let n = enc.finish().unwrap();
        let framed = frame_in_place(&mut frame_buf, FRAME_KIND_PACK, headroom, n).unwrap();
        stream.extend_from_slice(&frame_buf[..framed]);
        expected.push(line.json);
    }
    stream.extend_from_slice(b"[log] done\n");
    (stream, expected)
}

#[test]
fn device_stream_scans_back_to_every_line_in_any_slicing() {
    let dict = fixture_dictionary();
    let (stream, expected) = device_stream();
    assert!(
        stream.contains(&b'\n') && expected.len() > 40,
        "a real sample"
    );
    for step in [1, 2, 7, 64, 1000, stream.len()] {
        let mut scanner = VecFrameScanner::with_max_body(32 * 1024);
        let mut got: Vec<Vec<u8>> = Vec::new();
        let mut text = Vec::new();
        let mut frames_with_newline = 0;
        for chunk in stream.chunks(step) {
            scanner.push(chunk, |e| match e {
                ScanEvent::Text(t) => text.extend_from_slice(t),
                ScanEvent::Frame { kind, payload } => {
                    assert_eq!(kind, FRAME_KIND_PACK);
                    if payload.contains(&b'\n') {
                        frames_with_newline += 1;
                    }
                    let mut json = Vec::new();
                    decode(dict, payload, &mut json).unwrap();
                    got.push(json);
                }
                ScanEvent::Dropped(why) => panic!("dropped {why:?}"),
            });
        }
        assert!(!scanner.in_frame());
        assert_eq!(got.len(), expected.len(), "step {step}");
        for (i, (g, e)) in got.iter().zip(&expected).enumerate() {
            assert_eq!(g.as_slice(), *e, "step {step}, frame {i}");
        }
        assert!(frames_with_newline > 0, "some payload carries 0x0A");
        let text = String::from_utf8(text).unwrap();
        assert_eq!(text.lines().count(), expected.len() + 1);
        assert!(text.ends_with("[log] done\n"));
    }
}

#[test]
fn a_reset_mid_frame_costs_only_that_frame() {
    let dict = fixture_dictionary();
    let (stream, expected) = device_stream();
    // Cut the stream partway into its third frame and splice in a reboot.
    let starts: Vec<usize> = stream
        .windows(2)
        .enumerate()
        .filter(|(_, w)| w[0] == 0 && w[1] == FRAME_KIND_PACK)
        .map(|(i, _)| i)
        .collect();
    // Halfway through it, whatever its length: a fixed offset can land past
    // the end of a short frame (the sample's third reply is whatever the
    // recording's third reply was).
    let resume = starts[3] - "[log] line 3\n".len();
    let cut = starts[2] + (resume - starts[2]) / 2;
    assert!(cut > starts[2] + 2, "the third frame has a body to tear");
    let mut torn = stream[..cut].to_vec();
    torn.extend_from_slice(b"ESP-ROM:esp32c6-20220919\nbuild:Mar 27 2021\n[INIT] boot\n");
    torn.extend_from_slice(&stream[resume..]);

    let mut scanner = VecFrameScanner::with_max_body(32 * 1024);
    let mut got = Vec::new();
    let mut dropped = Vec::new();
    for chunk in torn.chunks(13) {
        scanner.push(chunk, |e| match e {
            ScanEvent::Frame { payload, .. } => {
                let mut json = Vec::new();
                decode(dict, payload, &mut json).unwrap();
                got.push(json);
            }
            ScanEvent::Dropped(why) => dropped.push(why),
            ScanEvent::Text(_) => {}
        });
    }
    assert_eq!(dropped, [DropReason::BadCobs]);
    // Frames 0, 1, then 3.. — only the torn frame 2 is lost.
    assert_eq!(got.len(), expected.len() - 1);
    assert_eq!(got[1].as_slice(), expected[1]);
    assert_eq!(got[2].as_slice(), expected[3]);
}
