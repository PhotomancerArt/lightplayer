//! A little-endian cursor over one message's bytes. Every read is checked:
//! running out is `None`, never a panic. What is left after the fields a
//! reader knows is ignored (the additive rule), so there is no "expect end".

pub(crate) struct WireReader<'a> {
    bytes: &'a [u8],
}

impl<'a> WireReader<'a> {
    pub(crate) fn new(bytes: &'a [u8]) -> Self {
        Self { bytes }
    }

    pub(crate) fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        if self.bytes.len() < n {
            return None;
        }
        let (head, rest) = self.bytes.split_at(n);
        self.bytes = rest;
        Some(head)
    }

    pub(crate) fn u8(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }

    pub(crate) fn u16(&mut self) -> Option<u16> {
        let b = self.take(2)?;
        Some(u16::from_le_bytes([b[0], b[1]]))
    }

    pub(crate) fn u32(&mut self) -> Option<u32> {
        let b = self.take(4)?;
        Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub(crate) fn array<const N: usize>(&mut self) -> Option<[u8; N]> {
        let mut out = [0u8; N];
        out.copy_from_slice(self.take(N)?);
        Some(out)
    }

    /// Everything not yet read: a chunk's payload, a manifest's JSON.
    pub(crate) fn rest(self) -> &'a [u8] {
        self.bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_little_endian_and_runs_out_without_panicking() {
        let mut r = WireReader::new(&[1, 0x34, 0x12, 0x78, 0x56, 0x34, 0x12, 9]);
        assert_eq!(r.u8(), Some(1));
        assert_eq!(r.u16(), Some(0x1234));
        assert_eq!(r.u32(), Some(0x1234_5678));
        assert_eq!(r.u32(), None, "one byte left is not a u32");
        assert_eq!(r.array::<1>(), Some([9]));
        assert_eq!(r.u8(), None);
        assert!(r.rest().is_empty());
    }
}
