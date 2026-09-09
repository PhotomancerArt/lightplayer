//! The medium: a frame one participant hands over, offered to every other
//! participant a stated number of guest cycles later.
//!
//! # Why this is here and not in the chip crate
//!
//! This crate holds no chip numbers (see the crate docs), and a medium that
//! carries bytes between machines needs none: not a register offset, not an
//! interrupt source, not a frame format. It does not know that the frame it
//! carries is an 802.11 vendor-specific action frame, that the sender armed
//! it by writing a descriptor pointer into a radio register, or that the
//! receiver will be told about it with an interrupt. It knows a byte string,
//! who handed it over, and at which guest cycle — which is the whole of what
//! "two machines can hear each other" means. The chip crate reads the bytes
//! out of one machine's RAM and writes them into another's; this module is
//! only the queue in between.
//!
//! It is the sibling of [`crate::pins`] for frames: one shared state on the
//! outside of the machines, with the chip layer driving both ends, so neither
//! machine ever sees the other.
//!
//! # What is modelled, and what is not
//!
//! Modelled: **byte delivery on a perfect medium.** Every frame reaches every
//! other participant, in the order it was sent, exactly once, after one
//! stated latency. Nothing is lost, nothing is corrupted, nothing collides.
//!
//! Not modelled, and deliberately not: the PHY, the channel, collisions,
//! retries, RSSI, timing windows, encryption, and any notion of range. A
//! participant that is "out of range" of another does not exist here; there
//! is one medium and everybody on it hears everybody else.
//!
//! The **latency is one stated constant in guest cycles**, chosen by the
//! caller ([`PerfectAir::new`]) and never derived from a frame's own length:
//! deriving it per frame would be a PHY model, and no PHY is modelled. It
//! must not be zero — a receiver that could see a frame in the same cycle it
//! was armed would let the pair's two machines communicate inside one
//! quantum, which is not a thing radios do and not a thing a lockstep runner
//! can reproduce. Wall time never enters (plan PD5): everything here is in
//! guest cycles, so two runs of the same participants sending the same bytes
//! at the same cycles produce byte-identical deliveries.
//!
//! # Determinism
//!
//! The delivery path has no map iteration, no clock and no thread. Each
//! participant's queue is a `VecDeque` filled in send order, so
//! [`Air::take_due`] returns frames in the order they were handed over and in
//! the same order on every run.

use alloc::collections::VecDeque;
use alloc::vec::Vec;
use lp_emu_core::sched::Cycles;

/// Which participant on the medium: an index assigned by whoever built the
/// [`PerfectAir`], stable for the life of the run.
///
/// Not an address. The medium never looks inside a frame, so it has no idea
/// what the sender calls itself on the wire; this is only "the first machine"
/// and "the second machine".
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ParticipantId(pub usize);

impl ParticipantId {
    pub const fn index(self) -> usize {
        self.0
    }
}

impl core::fmt::Display for ParticipantId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "#{}", self.0)
    }
}

/// A frame on the medium: the bytes, who sent them, and when.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AirFrame {
    /// Who handed it over. Never the participant it is delivered to.
    pub from: ParticipantId,
    /// The guest cycle of the handover, on the *sender's* clock. In a
    /// lockstep run the two clocks are the same clock, which is the reason
    /// lockstep exists.
    pub at: Cycles,
    /// When the medium is willing to offer it: `at + latency`.
    pub due: Cycles,
    /// The frame, verbatim.
    pub bytes: Vec<u8>,
}

/// A medium frames travel on.
///
/// Two calls, both driven from outside the machines: one participant hands a
/// frame over, and every other participant asks what has become deliverable.
pub trait Air {
    /// One participant hands the medium a frame at guest cycle `at`.
    ///
    /// A frame from a participant the medium does not know is dropped and
    /// counted ([`PerfectAir::dropped`] on the concrete type) rather than
    /// panicking: an emulator that aborted mid-run would tell you less than
    /// one that says so at the end.
    fn send(&mut self, from: ParticipantId, at: Cycles, bytes: &[u8]);

    /// Everything deliverable to `to` at or before `now`, in send order.
    ///
    /// Each frame is returned exactly once, to each participant but its
    /// sender. A frame whose `due` is past `now` stays queued.
    fn take_due(&mut self, to: ParticipantId, now: Cycles) -> Vec<AirFrame>;
}

/// The perfect medium: one stated latency, no losses, no PHY.
///
/// Participants are fixed at construction. `send` fans a frame out into one
/// queue per other participant, so delivery order is the send order by
/// construction and nothing in the path depends on a hash.
#[derive(Debug)]
pub struct PerfectAir {
    latency: Cycles,
    /// One queue per participant, in participant order. Never a map.
    queues: Vec<VecDeque<AirFrame>>,
    sent: u64,
    delivered: u64,
    dropped: u64,
}

impl PerfectAir {
    /// A medium with `participants` participants and a latency of `latency`
    /// guest cycles.
    ///
    /// `latency` is the caller's stated constant and this module does not
    /// choose it — the chip crate does, because "how many cycles is 672 µs"
    /// is a chip number. A latency of zero is refused by
    /// [`Self::try_new`]; this constructor panics on one, because a zero
    /// latency is a construction bug and not a run-time condition.
    pub fn new(participants: usize, latency: Cycles) -> Self {
        Self::try_new(participants, latency)
            .expect("PerfectAir: the latency must be at least one guest cycle")
    }

    /// As [`Self::new`], but `None` for a zero latency.
    pub fn try_new(participants: usize, latency: Cycles) -> Option<Self> {
        if latency == 0 {
            return None;
        }
        Some(Self {
            latency,
            queues: (0..participants).map(|_| VecDeque::new()).collect(),
            sent: 0,
            delivered: 0,
            dropped: 0,
        })
    }

    /// The stated latency, in guest cycles. A transcript sidecar states this.
    pub const fn latency(&self) -> Cycles {
        self.latency
    }

    /// How many participants are on the medium.
    pub fn participants(&self) -> usize {
        self.queues.len()
    }

    /// Frames handed over, deliveries made, and frames dropped because the
    /// sender was not a participant.
    pub const fn sent(&self) -> u64 {
        self.sent
    }

    pub const fn delivered(&self) -> u64 {
        self.delivered
    }

    pub const fn dropped(&self) -> u64 {
        self.dropped
    }

    /// Frames queued for `to` that are not deliverable yet.
    pub fn pending(&self, to: ParticipantId) -> usize {
        self.queues.get(to.index()).map_or(0, VecDeque::len)
    }

    /// The earliest cycle at which anything is deliverable to anyone, or
    /// `None` when the medium is empty. A runner may use it to bound a
    /// quantum; the lockstep runner does not need to, because its quantum is
    /// already no larger than the latency.
    pub fn next_due(&self) -> Option<Cycles> {
        self.queues
            .iter()
            .filter_map(|q| q.front().map(|f| f.due))
            .min()
    }
}

impl Air for PerfectAir {
    fn send(&mut self, from: ParticipantId, at: Cycles, bytes: &[u8]) {
        if from.index() >= self.queues.len() {
            self.dropped += 1;
            return;
        }
        self.sent += 1;
        let due = at.saturating_add(self.latency);
        for to in 0..self.queues.len() {
            if to == from.index() {
                // A frame is never delivered back to its sender. A radio does
                // not hear itself, and a machine that did would answer its own
                // broadcast.
                continue;
            }
            self.queues[to].push_back(AirFrame {
                from,
                at,
                due,
                bytes: bytes.to_vec(),
            });
        }
    }

    fn take_due(&mut self, to: ParticipantId, now: Cycles) -> Vec<AirFrame> {
        let Some(queue) = self.queues.get_mut(to.index()) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        // The queue is in send order and every frame carries the same
        // latency, so `due` is non-decreasing along it: the first frame that
        // is not due stops the walk.
        while queue.front().is_some_and(|f| f.due <= now) {
            out.push(queue.pop_front().expect("checked"));
        }
        self.delivered += out.len() as u64;
        out
    }
}

/// The socket form of the medium: the same frames, over a stream, between
/// machines in different processes.
///
/// # Auditable only, never a gate
///
/// A socket pair is **not** byte-identical: two processes interleave however
/// the operating system schedules them, and a frame lands wherever the
/// receiver happens to be. Nothing that has to replay may run this way. The
/// deterministic form is [`super::PerfectAir`] under
/// `lp-emu-esp32c6`'s lockstep runner, in one process and one thread, and it
/// is the only form a validation configuration, a transcript or a CI job ever
/// uses. This is for watching two machines talk.
///
/// # The framing, and why it is not `M!`
///
/// `lpc_wire::json::to_serial_line` is the repo's only `M!` framer, and the
/// emulator never uses it — the MIT fence (`just lint-emu-fence`) is what
/// keeps that true. So this has its own, and it is as small as a framing can
/// be: a magic, a length, the sender's guest cycle, the sender's seat, and
/// the frame.
///
/// ```text
///   offset  size  field
///   0       4     magic, the ASCII bytes "LPA1"
///   4       4     length, little-endian u32: how many bytes follow
///   8       8     at, little-endian u64: the sender's guest cycle
///   16      2     from, little-endian u16: the sender's ParticipantId
///   18      n     the frame, verbatim
/// ```
///
/// `length` counts `at`, `from` and the frame — everything after itself — so
/// a reader that has the first eight bytes knows exactly how much more to
/// wait for. Little-endian throughout, because every machine this runs
/// between is.
pub mod wire {
    use super::{AirFrame, ParticipantId};
    use alloc::vec::Vec;
    use lp_emu_core::sched::Cycles;

    /// The four bytes every frame starts with.
    pub const MAGIC: [u8; 4] = *b"LPA1";
    /// Magic and length: what a reader needs before it knows the rest.
    pub const HEADER_LEN: usize = 8;
    /// `at` and `from`, the fixed part of a frame's payload.
    pub const PAYLOAD_PREFIX_LEN: usize = 10;
    /// The largest frame this codec will encode or accept. An 802.11 frame
    /// is under two kilobytes and the RX ring's buffers are 1,700 bytes; the
    /// cap is here so a bad length on a socket cannot ask for a huge
    /// allocation, not because anything needs to be this big.
    pub const MAX_FRAME_LEN: usize = 4_096;

    /// Why a byte string was not a frame.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum WireError {
        /// Not enough bytes yet. Read more and try again — this is the
        /// ordinary answer on a stream, not a failure.
        Incomplete,
        /// The first four bytes were not [`MAGIC`]: the stream is not this
        /// protocol, or it has lost sync. A reader should close rather than
        /// hunt for the next magic.
        BadMagic,
        /// A length past [`MAX_FRAME_LEN`], or shorter than the fixed
        /// prefix.
        BadLength { length: u32 },
    }

    impl core::fmt::Display for WireError {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            match self {
                WireError::Incomplete => f.write_str("air wire: incomplete frame"),
                WireError::BadMagic => f.write_str("air wire: bad magic; the stream is not LPA1"),
                WireError::BadLength { length } => {
                    write!(f, "air wire: refusing a frame of length {length}")
                }
            }
        }
    }

    /// One frame on the wire. See the module docs for the layout.
    pub fn encode(from: ParticipantId, at: Cycles, bytes: &[u8]) -> Vec<u8> {
        let length = (PAYLOAD_PREFIX_LEN + bytes.len()) as u32;
        let mut out = Vec::with_capacity(HEADER_LEN + length as usize);
        out.extend_from_slice(&MAGIC);
        out.extend_from_slice(&length.to_le_bytes());
        out.extend_from_slice(&at.to_le_bytes());
        out.extend_from_slice(&(from.index() as u16).to_le_bytes());
        out.extend_from_slice(bytes);
        out
    }

    /// The first frame in `buf`, and how many bytes it took.
    ///
    /// `due` on the returned frame is `at`: the wire carries what the sender
    /// handed over, and the receiving side's own air decides when it is due.
    pub fn decode(buf: &[u8]) -> Result<(AirFrame, usize), WireError> {
        if buf.len() < HEADER_LEN {
            return Err(WireError::Incomplete);
        }
        if buf[..4] != MAGIC {
            return Err(WireError::BadMagic);
        }
        let length = u32::from_le_bytes(buf[4..8].try_into().expect("checked"));
        let payload = length as usize;
        if payload < PAYLOAD_PREFIX_LEN || payload > PAYLOAD_PREFIX_LEN + MAX_FRAME_LEN {
            return Err(WireError::BadLength { length });
        }
        if buf.len() < HEADER_LEN + payload {
            return Err(WireError::Incomplete);
        }
        let at = Cycles::from_le_bytes(buf[8..16].try_into().expect("checked"));
        let from = u16::from_le_bytes(buf[16..18].try_into().expect("checked"));
        let bytes = buf[HEADER_LEN + PAYLOAD_PREFIX_LEN..HEADER_LEN + payload].to_vec();
        Ok((
            AirFrame {
                from: ParticipantId(usize::from(from)),
                at,
                due: at,
                bytes,
            },
            HEADER_LEN + payload,
        ))
    }

    /// Every complete frame at the front of `buf`, removing them from it and
    /// leaving any partial tail in place. The shape a stream reader wants.
    pub fn drain(buf: &mut Vec<u8>) -> Result<Vec<AirFrame>, WireError> {
        let mut out = Vec::new();
        let mut taken = 0;
        loop {
            match decode(&buf[taken..]) {
                Ok((frame, used)) => {
                    out.push(frame);
                    taken += used;
                }
                Err(WireError::Incomplete) => break,
                Err(other) => return Err(other),
            }
        }
        buf.drain(..taken);
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: ParticipantId = ParticipantId(0);
    const B: ParticipantId = ParticipantId(1);
    const C: ParticipantId = ParticipantId(2);

    #[test]
    fn a_frame_is_offered_only_after_the_stated_latency() {
        let mut air = PerfectAir::new(2, 100);
        air.send(A, 1_000, b"hello");
        assert!(air.take_due(B, 1_099).is_empty(), "not due one cycle early");
        assert_eq!(air.pending(B), 1);
        let got = air.take_due(B, 1_100);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].bytes, b"hello");
        assert_eq!(got[0].from, A);
        assert_eq!(got[0].at, 1_000);
        assert_eq!(got[0].due, 1_100);
        assert!(air.take_due(B, 9_999).is_empty(), "delivered exactly once");
    }

    #[test]
    fn a_frame_is_never_delivered_back_to_its_sender() {
        let mut air = PerfectAir::new(2, 10);
        air.send(A, 0, b"mine");
        assert!(air.take_due(A, 1_000_000).is_empty());
        assert_eq!(air.take_due(B, 1_000_000).len(), 1);
    }

    #[test]
    fn send_order_is_delivery_order() {
        let mut air = PerfectAir::new(2, 10);
        for n in 0..8u8 {
            air.send(A, u64::from(n) * 3, &[n]);
        }
        let got = air.take_due(B, 1_000);
        let seen: Vec<u8> = got.iter().map(|f| f.bytes[0]).collect();
        assert_eq!(seen, (0..8u8).collect::<Vec<_>>());
        // And a partial take keeps the tail in order.
        let mut air = PerfectAir::new(2, 10);
        for n in 0..8u8 {
            air.send(A, u64::from(n) * 3, &[n]);
        }
        let first = air.take_due(B, 19); // due = at + 10, so n = 0..3
        assert_eq!(
            first.iter().map(|f| f.bytes[0]).collect::<Vec<_>>(),
            vec![0, 1, 2, 3]
        );
        let rest = air.take_due(B, 1_000);
        assert_eq!(
            rest.iter().map(|f| f.bytes[0]).collect::<Vec<_>>(),
            vec![4, 5, 6, 7]
        );
    }

    #[test]
    fn three_participants_each_hear_the_other_two() {
        let mut air = PerfectAir::new(3, 5);
        air.send(A, 0, b"a");
        air.send(B, 1, b"b");
        air.send(C, 2, b"c");
        let heard = |air: &mut PerfectAir, who| {
            air.take_due(who, 1_000)
                .into_iter()
                .map(|f| (f.from, f.bytes))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            heard(&mut air, A),
            vec![(B, b"b".to_vec()), (C, b"c".to_vec())]
        );
        assert_eq!(
            heard(&mut air, B),
            vec![(A, b"a".to_vec()), (C, b"c".to_vec())]
        );
        assert_eq!(
            heard(&mut air, C),
            vec![(A, b"a".to_vec()), (B, b"b".to_vec())]
        );
        assert_eq!(air.sent(), 3);
        assert_eq!(air.delivered(), 6);
    }

    #[test]
    fn the_same_traffic_delivers_identically_twice() {
        let run = || {
            let mut air = PerfectAir::new(3, 64);
            let mut log = Vec::new();
            for step in 0..16u64 {
                if step % 3 == 0 {
                    air.send(A, step * 32, &[b'a', step as u8]);
                }
                if step % 5 == 0 {
                    air.send(C, step * 32, &[b'c', step as u8]);
                }
                for who in [A, B, C] {
                    for f in air.take_due(who, step * 32) {
                        log.push((who.index(), f.from.index(), f.at, f.due, f.bytes));
                    }
                }
            }
            log
        };
        assert_eq!(run(), run());
        assert!(!run().is_empty());
    }

    #[test]
    fn a_zero_latency_is_refused_and_an_unknown_sender_is_counted() {
        assert!(PerfectAir::try_new(2, 0).is_none());
        let mut air = PerfectAir::new(2, 1);
        air.send(ParticipantId(7), 0, b"nobody");
        assert_eq!(air.dropped(), 1);
        assert_eq!(air.sent(), 0);
        assert!(air.take_due(B, 1_000).is_empty());
        assert!(air.take_due(ParticipantId(7), 1_000).is_empty());
    }

    /// G1-5's first half: the codec round-trips, and says which of the three
    /// things went wrong when one does.
    #[test]
    fn the_wire_codec_round_trips_and_names_its_refusals() {
        use super::wire;
        let frame = [0xd0u8, 0x00, 0xff, 0xff, 0x18, 0xfe, 0x34];
        let bytes = wire::encode(B, 165_826_944, &frame);
        assert_eq!(&bytes[..4], b"LPA1");
        assert_eq!(bytes.len(), wire::HEADER_LEN + 10 + frame.len());
        let (got, used) = wire::decode(&bytes).unwrap();
        assert_eq!(used, bytes.len());
        assert_eq!(got.from, B);
        assert_eq!(got.at, 165_826_944);
        assert_eq!(got.bytes, frame);

        // A short read is Incomplete at every prefix, never a false frame.
        for n in 0..bytes.len() {
            assert_eq!(
                wire::decode(&bytes[..n]),
                Err(wire::WireError::Incomplete),
                "prefix of {n} bytes"
            );
        }
        // A stream that is not ours is refused rather than resynchronised.
        let mut wrong = bytes.clone();
        wrong[1] = b'X';
        assert_eq!(wire::decode(&wrong), Err(wire::WireError::BadMagic));
        // And a length nothing could hold.
        let mut huge = bytes.clone();
        huge[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(
            wire::decode(&huge),
            Err(wire::WireError::BadLength { length: u32::MAX })
        );

        // `drain` takes whole frames and leaves a partial tail alone.
        let mut stream = Vec::new();
        stream.extend_from_slice(&wire::encode(A, 1, b"one"));
        stream.extend_from_slice(&wire::encode(B, 2, b"two"));
        let tail = wire::encode(C, 3, b"three");
        stream.extend_from_slice(&tail[..tail.len() - 2]);
        let got = wire::drain(&mut stream).unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].bytes, b"one");
        assert_eq!(got[1].bytes, b"two");
        assert_eq!(stream.len(), tail.len() - 2, "the partial frame is kept");
        stream.extend_from_slice(&tail[tail.len() - 2..]);
        let got = wire::drain(&mut stream).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].bytes, b"three");
        assert!(stream.is_empty());
    }

    /// G1-5's second half: two sockets in one process, one frame across
    /// localhost, decoded by the same codec that encoded it.
    ///
    /// Not `#[ignore]`d: it binds `127.0.0.1:0` (the kernel picks the port,
    /// so two of these can never collide) and both ends are in this process,
    /// so there is nothing here to be flaky about. If it ever does flake on
    /// this box, mark it `#[ignore]` **with the reason in a comment here**
    /// rather than deleting it.
    #[test]
    fn one_frame_crosses_a_localhost_socket() {
        use super::wire;
        use std::io::{Read, Write};
        use std::net::{TcpListener, TcpStream};

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let sent = wire::encode(A, 165_826_944, b"\xd0\x00\xff\xff\x18\xfe\x34");

        let writer = std::thread::spawn({
            let sent = sent.clone();
            move || {
                let mut s = TcpStream::connect(addr).expect("connect");
                s.write_all(&sent).expect("write");
                s.flush().expect("flush");
            }
        });

        let (mut server, _) = listener.accept().expect("accept");
        let mut buf = Vec::new();
        let mut chunk = [0u8; 64];
        while buf.len() < sent.len() {
            let n = server.read(&mut chunk).expect("read");
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&chunk[..n]);
        }
        writer.join().expect("writer");

        let got = wire::drain(&mut buf).expect("decode");
        assert_eq!(got.len(), 1, "one frame in, one frame out");
        assert_eq!(got[0].from, A);
        assert_eq!(got[0].at, 165_826_944);
        assert_eq!(got[0].bytes, b"\xd0\x00\xff\xff\x18\xfe\x34");
        assert!(buf.is_empty());
    }

    #[test]
    fn next_due_is_the_earliest_queued_delivery() {
        let mut air = PerfectAir::new(2, 100);
        assert_eq!(air.next_due(), None);
        air.send(A, 40, b"x");
        air.send(B, 10, b"y");
        assert_eq!(air.next_due(), Some(110), "B's frame was handed over first");
        assert_eq!(air.latency(), 100);
        assert_eq!(air.participants(), 2);
    }
}
