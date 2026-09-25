//! The wire tear: a test-only fault the door can put on a board's byte
//! stream, so a host reader's handling of a packed frame lost in flight can
//! be exercised against the shipped image.
//!
//! The emulated C6 link never loses bytes (its single writer makes the
//! IN-endpoint gate a no-op), but a real one did: frames arriving a few
//! bytes short with no sign on the board
//! (`docs/defects/2026-09-24-the-real-c6-link-loses-bytes-inside-a-packed-frame.md`).
//! With learned packed frames (plan
//! `lp2025/2026-09-25-0006-learned-wire-dictionary`) such a loss leaves the
//! host's table behind the board's, and the host must notice, drop what it
//! cannot read, and ask for a reset. This is how to make that happen on
//! purpose.
//!
//! Set `LP_EMU_WIRE_TEAR=<k>[,<k>…][:<n>]` before starting `emu serve`: on
//! every board→host byte connection, the k-th packed frame (1-based, counted
//! from the connection's first byte) loses `n` bytes (default 5) from the
//! middle of its body. Unset, the tear is off and costs one branch per
//! chunk. It is a fault injector, never a model: the link model's own loss
//! is the fidelity defect's follow-up.

/// The environment variable that turns the tear on.
pub const WIRE_TEAR_ENV: &str = "LP_EMU_WIRE_TEAR";

/// Bytes a torn frame loses unless the variable says otherwise: what the
/// real C6 link lost (~5 B per frame).
const DEFAULT_TEAR_BYTES: usize = 5;

/// One byte connection's tear, or nothing when the tear is off.
pub struct WireTear(Option<Tearing>);

/// A tear that is armed.
struct Tearing {
    /// Which packed frames to tear, 1-based.
    frames: Vec<usize>,
    /// Bytes each loses.
    bytes: usize,
    /// Packed frames seen so far.
    seen: usize,
    /// The frame being read (from its opening `0x00`), while inside one.
    frame: Option<Vec<u8>>,
}

impl WireTear {
    /// The tear `LP_EMU_WIRE_TEAR` asks for, or none. A value that does not
    /// parse is refused loudly rather than ignored.
    pub fn from_env() -> anyhow::Result<Self> {
        match std::env::var(WIRE_TEAR_ENV) {
            Ok(spec) => {
                Self::parse(&spec).map_err(|e| anyhow::anyhow!("{WIRE_TEAR_ENV}={spec:?}: {e}"))
            }
            Err(_) => Ok(Self(None)),
        }
    }

    /// `<k>[,<k>…][:<n>]`.
    pub fn parse(spec: &str) -> Result<Self, String> {
        let (frames, bytes) = match spec.split_once(':') {
            Some((frames, bytes)) => (
                frames,
                bytes
                    .parse::<usize>()
                    .map_err(|_| format!("{bytes:?} is not a byte count"))?,
            ),
            None => (spec, DEFAULT_TEAR_BYTES),
        };
        let frames = frames
            .split(',')
            .map(|k| match k.trim().parse::<usize>() {
                Ok(k) if k > 0 => Ok(k),
                _ => Err(format!("{k:?} is not a frame number (1-based)")),
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self(Some(Tearing {
            frames,
            bytes,
            seen: 0,
            frame: None,
        })))
    }

    /// The bytes to pass on for `bytes` read from the board. Text passes
    /// through at once; a packed frame is held until it closes, then passed
    /// on whole, or torn.
    pub fn filter(&mut self, bytes: &[u8]) -> Vec<u8> {
        let Some(t) = self.0.as_mut() else {
            return bytes.to_vec();
        };
        let mut out = Vec::with_capacity(bytes.len());
        for &b in bytes {
            match t.frame.as_mut() {
                None if b == 0 => t.frame = Some(vec![0]),
                None => out.push(b),
                Some(frame) => {
                    frame.push(b);
                    if b == 0 {
                        let mut frame = t.frame.take().unwrap_or_default();
                        t.seen += 1;
                        if t.frames.contains(&t.seen) {
                            tear(&mut frame, t.bytes);
                            eprintln!(
                                "emu serve: {WIRE_TEAR_ENV}: tore packed frame {} ({} B lost)",
                                t.seen, t.bytes
                            );
                        }
                        out.extend_from_slice(&frame);
                    }
                }
            }
        }
        out
    }
}

/// Remove `n` bytes from the middle of a framed `0x00 kind COBS 0x00`,
/// keeping both delimiters and the kind byte, so the damage is inside the
/// body where the real link's was.
fn tear(frame: &mut Vec<u8>, n: usize) {
    let body = frame.len().saturating_sub(3);
    let n = n.min(body.saturating_sub(1));
    let mid = 2 + (body - n) / 2;
    frame.drain(mid..mid + n);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn off_passes_everything() {
        let mut tear = WireTear(None);
        assert_eq!(tear.filter(b"a\0Lxyz\0b"), b"a\0Lxyz\0b");
    }

    #[test]
    fn the_kth_frame_loses_bytes_from_its_middle_across_chunks() {
        let mut tear = WireTear::parse("2:2").unwrap();
        let stream = b"t1\n\0L12345678\0t2\n\0Labcdefgh\0\0L0\0";
        let mut out = Vec::new();
        for chunk in stream.chunks(3) {
            out.extend(tear.filter(chunk));
        }
        assert_eq!(out, b"t1\n\0L12345678\0t2\n\0Labcfgh\0\0L0\0");
    }

    #[test]
    fn a_bad_spec_is_refused() {
        assert!(WireTear::parse("0").is_err());
        assert!(WireTear::parse("x").is_err());
        assert!(WireTear::parse("3:y").is_err());
        assert!(WireTear::parse("3,4:7").is_ok());
    }
}
