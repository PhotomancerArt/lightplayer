//! Firmware logging onto the link's log channel.
//!
//! Log calls format into a fixed ring (no allocation, fits in a `static`);
//! the task driving the link moves records into the log channel with
//! [`Link::pump_log`](crate::Link::pump_log) while the link is up. While it is
//! down the ring keeps the newest records and drops the oldest, counting them,
//! so logging stays cheap and bounded, and the host learns how much it missed:
//! the next record out is a notice, `"<n> log records dropped"`.
//!
//! One record on the wire (a best-effort message): `level ‖ UTF-8 text`, with
//! level 1 = error … 5 = trace, and 0 for the link's own notices.
//!
//! Early boot text and panics do not come here: they are written raw to the
//! transport, outside frames, and arrive as [`LinkEvent::Text`](crate::LinkEvent::Text).

use core::fmt::{self, Write};

/// Level byte of the link's own notices.
pub const LEVEL_NOTICE: u8 = 0;
pub const LEVEL_ERROR: u8 = 1;
pub const LEVEL_WARN: u8 = 2;
pub const LEVEL_INFO: u8 = 3;
pub const LEVEL_DEBUG: u8 = 4;
pub const LEVEL_TRACE: u8 = 5;

/// Longest text kept per record; longer text is cut.
pub const MAX_RECORD_TEXT: usize = 200;

/// A ring of `N` bytes of log records. Each record is stored as
/// `len:u8 ‖ level:u8 ‖ text[len]`.
pub struct LogRing<const N: usize> {
    buf: [u8; N],
    head: usize,
    len: usize,
    dropped: u32,
    dropped_total: u32,
}

impl<const N: usize> LogRing<N> {
    pub const fn new() -> Self {
        LogRing {
            buf: [0; N],
            head: 0,
            len: 0,
            dropped: 0,
            dropped_total: 0,
        }
    }

    /// Append a record, dropping the oldest to make room.
    pub fn push(&mut self, level: u8, text: &[u8]) {
        let text = &text[..text.len().min(MAX_RECORD_TEXT).min(N.saturating_sub(2))];
        let need = 2 + text.len();
        if need > N {
            return;
        }
        while N - self.len < need {
            self.drop_oldest();
        }
        self.put(text.len() as u8);
        self.put(level);
        for &b in text {
            self.put(b);
        }
    }

    /// Format and append a record (what [`link_log!`](crate::link_log) calls).
    pub fn push_fmt(&mut self, level: u8, args: fmt::Arguments<'_>) {
        let mut line = LineBuf {
            buf: [0; MAX_RECORD_TEXT],
            len: 0,
        };
        let _ = line.write_fmt(args);
        let n = line.len;
        self.push(level, &line.buf[..n]);
    }

    /// Take the oldest record (or a "dropped" notice first) as wire bytes into
    /// `out`; the record's length. Text that does not fit `out` is cut.
    pub fn pop_into(&mut self, out: &mut [u8]) -> Option<usize> {
        if out.is_empty() {
            return None;
        }
        if self.dropped > 0 {
            let mut line = LineBuf {
                buf: [0; MAX_RECORD_TEXT],
                len: 0,
            };
            let _ = write!(line, "{} log records dropped", self.dropped);
            self.dropped = 0;
            return Some(write_record(out, LEVEL_NOTICE, &line.buf[..line.len]));
        }
        if self.len == 0 {
            return None;
        }
        let n = self.take() as usize;
        let level = self.take();
        out[0] = level;
        let keep = n.min(out.len() - 1);
        for i in 0..n {
            let b = self.take();
            if i < keep {
                out[1 + i] = b;
            }
        }
        Some(1 + keep)
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0 && self.dropped == 0
    }

    /// Records held (walks the ring).
    pub fn len(&self) -> usize {
        let (mut at, mut left, mut n) = (self.head, self.len, 0);
        while left > 0 {
            let rec = 2 + self.buf[at] as usize;
            at = (at + rec) % N;
            left -= rec;
            n += 1;
        }
        n
    }

    /// Records dropped since boot.
    pub fn dropped_total(&self) -> u32 {
        self.dropped_total
    }

    fn drop_oldest(&mut self) {
        let n = self.buf[self.head] as usize;
        let rec = 2 + n;
        self.head = (self.head + rec) % N;
        self.len -= rec;
        self.dropped += 1;
        self.dropped_total += 1;
    }

    fn put(&mut self, b: u8) {
        self.buf[(self.head + self.len) % N] = b;
        self.len += 1;
    }

    fn take(&mut self) -> u8 {
        let b = self.buf[self.head];
        self.head = (self.head + 1) % N;
        self.len -= 1;
        b
    }
}

impl<const N: usize> Default for LogRing<N> {
    fn default() -> Self {
        Self::new()
    }
}

/// Format a log record into a [`LogRing`]:
/// `link_log!(ring, LEVEL_INFO, "fps {}", fps)`.
#[macro_export]
macro_rules! link_log {
    ($ring:expr, $level:expr, $($arg:tt)*) => {
        $ring.push_fmt($level, ::core::format_args!($($arg)*))
    };
}

/// Map a `log` crate level to the wire's level byte.
#[cfg(feature = "log")]
pub fn level_of(level: log::Level) -> u8 {
    match level {
        log::Level::Error => LEVEL_ERROR,
        log::Level::Warn => LEVEL_WARN,
        log::Level::Info => LEVEL_INFO,
        log::Level::Debug => LEVEL_DEBUG,
        log::Level::Trace => LEVEL_TRACE,
    }
}

/// Append a `log` crate record: the body of a firmware `log::Log::log`, which
/// wraps the ring in its own lock (a critical section on the board).
#[cfg(feature = "log")]
pub fn push_log_record<const N: usize>(ring: &mut LogRing<N>, record: &log::Record<'_>) {
    ring.push_fmt(
        level_of(record.level()),
        format_args!("{}: {}", record.target(), record.args()),
    );
}

fn write_record(out: &mut [u8], level: u8, text: &[u8]) -> usize {
    out[0] = level;
    let keep = text.len().min(out.len() - 1);
    out[1..1 + keep].copy_from_slice(&text[..keep]);
    1 + keep
}

/// A fixed line buffer that cuts at its capacity.
struct LineBuf {
    buf: [u8; MAX_RECORD_TEXT],
    len: usize,
}

impl Write for LineBuf {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let n = s.len().min(self.buf.len() - self.len);
        self.buf[self.len..self.len + n].copy_from_slice(&s.as_bytes()[..n]);
        self.len += n;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pop(r: &mut LogRing<64>) -> Option<(u8, alloc::vec::Vec<u8>)> {
        let mut out = [0u8; 64];
        let n = r.pop_into(&mut out)?;
        Some((out[0], out[1..n].to_vec()))
    }

    #[test]
    fn keeps_newest_and_reports_drops() {
        let mut r = LogRing::<64>::new();
        for i in 0..10 {
            crate::link_log!(r, LEVEL_INFO, "record number {i}");
        }
        let (lvl, text) = pop(&mut r).unwrap();
        assert_eq!(lvl, LEVEL_NOTICE);
        let dropped: u32 = core::str::from_utf8(&text)
            .unwrap()
            .split(' ')
            .next()
            .unwrap()
            .parse()
            .unwrap();
        let mut rest = alloc::vec::Vec::new();
        while let Some((_, t)) = pop(&mut r) {
            rest.push(alloc::string::String::from_utf8(t).unwrap());
        }
        assert_eq!(dropped as usize + rest.len(), 10);
        assert_eq!(rest.last().unwrap(), "record number 9");
        assert_eq!(r.dropped_total(), dropped);
    }

    #[test]
    fn cuts_to_the_output() {
        let mut r = LogRing::<64>::new();
        r.push(LEVEL_WARN, b"0123456789");
        let mut out = [0u8; 5];
        assert_eq!(r.pop_into(&mut out), Some(5));
        assert_eq!(&out, b"\x020123");
        assert!(r.is_empty());
    }
}
