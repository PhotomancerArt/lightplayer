//! What [`crate::inflate`] can fail with. Every variant is a value the
//! decoder reaches by inspecting its input; none of them is a panic path.

/// Why [`crate::inflate`] stopped before producing a full output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// The input ended inside a block (a bit, length or code was needed but
    /// no more bytes remained).
    Truncated,
    /// A block type of 3, a stored block whose length and its one's
    /// complement disagree, an over-subscribed Huffman code, a dynamic-block
    /// header past its limits, or a literal/length/distance code with no
    /// match in the active table.
    Corrupt,
    /// The output buffer (`buf[start..]`) filled up before the stream ended.
    NoRoom,
    /// A back-reference's distance reaches before the start of `buf`, i.e.
    /// further back than everything decoded so far plus the preset
    /// dictionary.
    FarBack,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variants_are_copy_and_comparable() {
        let a = Error::Truncated;
        let b = a;
        assert_eq!(a, b);
        assert_ne!(Error::Corrupt, Error::NoRoom);
    }
}
