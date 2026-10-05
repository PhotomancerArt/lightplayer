//! A raw-deflate stream (RFC 1951) that only copies: one fixed-Huffman
//! block of back-references `distance` bytes back. Decoded with a preset
//! dictionary of at least `distance` bytes, it yields the dictionary's
//! bytes at that distance — so a board that hands `lp_deflate::inflate` the
//! wrong dictionary decodes the wrong bytes, and the test sees it.
//!
//! Written from RFC 1951 §3.2.5 and §3.2.6 (the fixed codes and the
//! length/distance tables); test-only.

/// `(symbol, base, extra bits)` for lengths (§3.2.5).
const LENGTHS: [(u16, u16, u8); 29] = [
    (257, 3, 0),
    (258, 4, 0),
    (259, 5, 0),
    (260, 6, 0),
    (261, 7, 0),
    (262, 8, 0),
    (263, 9, 0),
    (264, 10, 0),
    (265, 11, 1),
    (266, 13, 1),
    (267, 15, 1),
    (268, 17, 1),
    (269, 19, 2),
    (270, 23, 2),
    (271, 27, 2),
    (272, 31, 2),
    (273, 35, 3),
    (274, 43, 3),
    (275, 51, 3),
    (276, 59, 3),
    (277, 67, 4),
    (278, 83, 4),
    (279, 99, 4),
    (280, 115, 4),
    (281, 131, 5),
    (282, 163, 5),
    (283, 195, 5),
    (284, 227, 5),
    (285, 258, 0),
];

/// `(code, base, extra bits)` for distances (§3.2.5).
const DISTANCES: [(u16, u16, u8); 30] = [
    (0, 1, 0),
    (1, 2, 0),
    (2, 3, 0),
    (3, 4, 0),
    (4, 5, 1),
    (5, 7, 1),
    (6, 9, 2),
    (7, 13, 2),
    (8, 17, 3),
    (9, 25, 3),
    (10, 33, 4),
    (11, 49, 4),
    (12, 65, 5),
    (13, 97, 5),
    (14, 129, 6),
    (15, 193, 6),
    (16, 257, 7),
    (17, 385, 7),
    (18, 513, 8),
    (19, 769, 8),
    (20, 1025, 9),
    (21, 1537, 9),
    (22, 2049, 10),
    (23, 3073, 10),
    (24, 4097, 11),
    (25, 6145, 11),
    (26, 8193, 12),
    (27, 12289, 12),
    (28, 16385, 13),
    (29, 24577, 13),
];

struct Bits {
    out: Vec<u8>,
    acc: u32,
    n: u32,
}

impl Bits {
    fn put(&mut self, value: u32, bits: u32) {
        self.acc |= value << self.n;
        self.n += bits;
        while self.n >= 8 {
            self.out.push(self.acc as u8);
            self.acc >>= 8;
            self.n -= 8;
        }
    }

    /// A Huffman code: its bits go most significant first.
    fn put_code(&mut self, code: u32, bits: u32) {
        let rev = code.reverse_bits() >> (32 - bits);
        self.put(rev, bits);
    }

    fn put_litlen(&mut self, sym: u16) {
        match sym {
            256..=279 => self.put_code(u32::from(sym - 256), 7),
            280..=287 => self.put_code(0xC0 + u32::from(sym - 280), 8),
            _ => unreachable!("only lengths and end-of-block are emitted"),
        }
    }

    fn finish(mut self) -> Vec<u8> {
        if self.n > 0 {
            self.out.push(self.acc as u8);
        }
        self.out
    }
}

fn find(table: &[(u16, u16, u8)], value: u16) -> (u16, u16, u8) {
    *table
        .iter()
        .rev()
        .find(|(_, base, _)| *base <= value)
        .expect("in range")
}

/// `len` bytes, each copied from `distance` bytes back.
pub fn copy_stream(len: usize, distance: u16) -> Vec<u8> {
    assert!(len >= 3, "a copy is at least 3 bytes");
    let mut b = Bits {
        out: Vec::new(),
        acc: 0,
        n: 0,
    };
    b.put(1, 1); // BFINAL
    b.put(1, 2); // BTYPE = 01, fixed Huffman
    let mut left = len;
    while left > 0 {
        let mut l = left.min(258);
        if (1..3).contains(&(left - l)) {
            l -= 3;
        }
        let (sym, base, extra) = find(&LENGTHS, l as u16);
        b.put_litlen(sym);
        b.put(u32::from(l as u16 - base), u32::from(extra));
        let (code, base, extra) = find(&DISTANCES, distance);
        b.put_code(u32::from(code), 5);
        b.put(u32::from(distance - base), u32::from(extra));
        left -= l;
    }
    b.put_litlen(256);
    b.finish()
}
