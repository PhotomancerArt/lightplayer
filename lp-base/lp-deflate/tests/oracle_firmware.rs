//! `lp_deflate::inflate` decodes what `miniz_oxide` encodes, on a real
//! firmware image, at every compression level miniz offers.
//!
//! The corpus is `lp-fw/bootloaders/esp32c6-bootloader-idf-v5.5.1.bin`, the
//! vendored ESP-IDF bootloader already in this repo (Apache-2.0, see
//! `lp-fw/bootloaders/README.md`) — real firmware, not a scratch path or a
//! synthetic stand-in.

const FIRMWARE: &[u8] =
    include_bytes!("../../../lp-fw/bootloaders/esp32c6-bootloader-idf-v5.5.1.bin");

#[test]
fn decodes_miniz_at_every_level_with_no_dictionary() {
    for level in [0u8, 1, 6, 9] {
        let compressed = miniz_oxide::deflate::compress_to_vec(FIRMWARE, level);
        let mut buf = vec![0u8; FIRMWARE.len()];
        let n = lp_deflate::inflate(&compressed, &mut buf, 0)
            .unwrap_or_else(|e| panic!("level {level}: inflate failed: {e:?}"));
        assert_eq!(n, FIRMWARE.len(), "level {level}: wrong output length");
        assert_eq!(&buf[..n], FIRMWARE, "level {level}: output mismatch");
    }
}

/// `inflate_with`, one `Tables` reused across every chunk (stored, fixed and
/// dynamic blocks alike): the same bytes as `inflate`, chunk by chunk.
#[test]
fn caller_owned_tables_reused_across_chunks_decode_the_same() {
    const CHUNK: usize = 4096;
    let mut tables = Box::new(lp_deflate::Tables::default());
    for (i, chunk) in FIRMWARE.chunks(CHUNK).enumerate() {
        let level = [0u8, 1, 6, 9][i % 4];
        let compressed = miniz_oxide::deflate::compress_to_vec(chunk, level);
        let mut buf = vec![0u8; chunk.len()];
        let n = lp_deflate::inflate_with(&compressed, &mut buf, 0, &mut tables)
            .unwrap_or_else(|e| panic!("chunk {i}: inflate_with failed: {e:?}"));
        assert_eq!(
            &buf[..n],
            chunk,
            "chunk {i} (level {level}): output mismatch"
        );
    }
}

/// The OTA shape itself: the image split into 4 KiB chunks, each compressed
/// (independently, no dictionary) and decoded into its own fresh slice —
/// the baseline the dictionary test in `preset_dictionary.rs` builds on.
#[test]
fn decodes_every_4kib_chunk_independently() {
    const CHUNK: usize = 4096;
    for (i, chunk) in FIRMWARE.chunks(CHUNK).enumerate() {
        let compressed = miniz_oxide::deflate::compress_to_vec(chunk, 6);
        let mut buf = vec![0u8; chunk.len()];
        let n = lp_deflate::inflate(&compressed, &mut buf, 0)
            .unwrap_or_else(|e| panic!("chunk {i}: inflate failed: {e:?}"));
        assert_eq!(&buf[..n], chunk, "chunk {i}: output mismatch");
    }
}
