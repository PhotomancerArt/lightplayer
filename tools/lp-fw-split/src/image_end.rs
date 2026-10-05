//! Where a flashed image may end: on a 4 KiB flash-sector boundary, padded
//! with `0xFF`.
//!
//! A flasher erases whole sectors, so the padding is the same bytes the
//! erase leaves; it changes nothing on the chip. What it removes is a
//! partial last write. espflash 3.3.0's `write_bin_to_flash` hands its stub
//! the image's raw length, and the stub dropped the last 254 bytes of a
//! packaged split image whose length (`0x2F55FE`) was not a multiple of 4:
//! the engine's last functions read back `0xFF` and the engine faulted on
//! every boot (`docs/defects/2026-10-05-the-host-flasher-dropped-the-split-images-last-bytes.md`).
//! A sector is a multiple of every unit a flasher works in (a word, the ESP
//! image's 16 bytes, a 256-byte program page, a 1 KiB write block, the
//! erase), so an image that ends on one leaves no partial unit anywhere.

/// The unit every flashed image's end is aligned to: one flash sector.
pub const IMAGE_END_ALIGN: usize = 0x1000;

/// Pad `image` with `0xFF` to the next [`IMAGE_END_ALIGN`] boundary.
/// `start` is the flash offset the image is written at, which must itself
/// be sector-aligned (a flasher's erase starts there).
pub fn pad_image_end(image: &mut Vec<u8>, start: u32) {
    debug_assert_eq!(start as usize % IMAGE_END_ALIGN, 0);
    image.resize(image.len().next_multiple_of(IMAGE_END_ALIGN), 0xff);
}

/// Whether an image written at `start` ends on the boundary.
pub fn image_end_is_aligned(start: u32, len: usize) -> bool {
    (start as usize + len) % IMAGE_END_ALIGN == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn padding_reaches_the_next_sector_with_erased_bytes() {
        for len in [1usize, 3, 4, 254, 0xfff, 0x1000, 0x1001, 0x2F_55FE] {
            let mut image = vec![0xa5u8; len];
            pad_image_end(&mut image, 0);
            assert!(image_end_is_aligned(0, image.len()), "{len:#x}");
            assert!(image.len() - len < IMAGE_END_ALIGN, "{len:#x}");
            assert!(image[len..].iter().all(|b| *b == 0xff), "{len:#x}");
            assert!(image[..len].iter().all(|b| *b == 0xa5), "{len:#x}");
        }
    }

    #[test]
    fn an_aligned_image_is_left_alone() {
        let mut image = vec![7u8; 0x3000];
        pad_image_end(&mut image, 0x1_0000);
        assert_eq!(image.len(), 0x3000);
    }

    #[test]
    fn the_76959e7a4_image_was_not_aligned() {
        // The packaged image the bench C6 lost its last 254 bytes of.
        assert!(!image_end_is_aligned(0, 0x2F_55FE));
    }
}
