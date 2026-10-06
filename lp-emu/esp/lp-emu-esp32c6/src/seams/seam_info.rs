//! `--seams-info <image>`: what a flash image's seam tables say, without
//! running it.

use lp_emu_esp_common::seam::{self, seam_impl};

/// Every table in `image` (each candidate, its identity, version and
/// entries; each mismatch, with its identity only), then this emulator's own
/// identity and implementations.
pub fn seams_info(image: &[u8]) -> String {
    format!(
        "{}\nemulator seam abi {:016x}; implementations: {}",
        seam::scan(image),
        lp_seam::SEAM_ABI_ID,
        seam_impl::known_atoms()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_blank_image_says_it_has_no_table_and_names_the_emulator() {
        let text = seams_info(&[0xff; 4096]);
        assert!(text.contains("no seam table"), "{text}");
        assert!(text.contains("emulator seam abi"), "{text}");
        assert!(text.contains("led=fast"), "{text}");
    }
}
