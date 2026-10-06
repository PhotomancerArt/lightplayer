//! The seam identity: a compiler-computed hash of the seam declarations.
//!
//! [`abi_id`] is FNV-1a 64 over the declaration text with every ASCII
//! whitespace byte removed, and with a string literal's line continuation
//! (`\` immediately before a newline) removed too, so reflowing a `doc:`
//! string is a whitespace edit and not an ABI change.
//!
//! Why whitespace is stripped: `stringify!`'s spacing between tokens is a
//! pretty-printer decision and has changed between rustc versions. The C6
//! firmware and the emulator build on the same pinned nightly, so they agree
//! today either way; the Xtensa firmwares build on the esp toolchain's rustc,
//! which is where the stripping earns its keep (plan M7 re-checks it).
//!
//! Why doc comments are spelled `doc: "…"` and not `///`: a `///` comment
//! reaches `stringify!` as an attribute, and how an attribute is rendered
//! (`#[doc = "…"]` or `/// …`) is exactly the kind of non-whitespace
//! difference two rustc versions can disagree on. A string literal is
//! rendered as its source text.

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// FNV-1a 64 over `text`, skipping whitespace and line continuations.
pub const fn abi_id(text: &str) -> u64 {
    let bytes = text.as_bytes();
    let mut hash = FNV_OFFSET;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        let continuation =
            b == b'\\' && i + 1 < bytes.len() && (bytes[i + 1] == b'\n' || bytes[i + 1] == b'\r');
        if !continuation && !b.is_ascii_whitespace() {
            hash ^= b as u64;
            hash = hash.wrapping_mul(FNV_PRIME);
        }
        i += 1;
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whitespace_and_line_continuations_do_not_count() {
        let a = abi_id("seam 0x0001 led { doc: \"one two\" }");
        assert_eq!(a, abi_id("seam  0x0001\n\tled{doc:\"one two\"}"));
        assert_eq!(a, abi_id("seam 0x0001 led { doc: \"one \\\n      two\" }"));
    }

    #[test]
    fn a_token_or_doc_text_edit_does_count() {
        let a = abi_id("seam 0x0001 led { doc: \"one two\" }");
        assert_ne!(a, abi_id("seam 0x0002 led { doc: \"one two\" }"));
        assert_ne!(a, abi_id("seam 0x0001 led { doc: \"one too\" }"));
        assert_ne!(a, abi_id("seam 0x0001 led { doc: \"one two\" } x"));
    }

    #[test]
    fn it_is_fnv1a_64() {
        // The published FNV-1a 64 vectors.
        assert_eq!(abi_id(""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(abi_id("a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(abi_id("foobar"), 0x8594_4171_f739_67e8);
    }
}
