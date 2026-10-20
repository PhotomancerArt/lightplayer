//! A byte size, in the unit that keeps the number short (P15): "512 B",
//! "1.5 KB", "38 KB", "4.2 MB". Binary steps (1 KB = 1024 B), as Studio has
//! always sized things; one decimal below ten of a unit, none from ten on,
//! and a whole number reads whole ("4 KB", not "4.0 KB").
//!
//! The one byte formatter (P17): an asset's body, a board's files, a link's
//! traffic, the engine's download. Before it there were four, and 1536 B
//! read "1 KB" in one and "1.5 KB" in the others.

/// The units after bytes, each 1024 of the one before.
const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];

/// `bytes` in the unit that keeps the number short.
pub fn byte_size(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64 / 1024.0;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    // Rounded first, so 10 239 B (9.999 KB) reads "10 KB", not "10.0 KB".
    let tenths = (value * 10.0).round() / 10.0;
    if tenths < 10.0 && tenths.fract() != 0.0 {
        format!("{tenths:.1} {}", UNITS[unit])
    } else {
        format!("{value:.0} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_read_in_the_unit_that_keeps_them_short() {
        assert_eq!(byte_size(0), "0 B");
        assert_eq!(byte_size(512), "512 B");
        assert_eq!(byte_size(1023), "1023 B");
        assert_eq!(byte_size(1024), "1 KB");
        assert_eq!(byte_size(1536), "1.5 KB");
        assert_eq!(byte_size(4096), "4 KB");
        assert_eq!(byte_size(10_239), "10 KB");
        assert_eq!(byte_size(38 * 1024), "38 KB");
        assert_eq!(byte_size(720_896), "704 KB");
        assert_eq!(byte_size(4_404_019), "4.2 MB");
        assert_eq!(byte_size(1_363_149), "1.3 MB");
        assert_eq!(byte_size(64 * 1024 * 1024), "64 MB");
        assert_eq!(byte_size(3 * 1024 * 1024 * 1024), "3 GB");
    }

    /// Past the last unit the number grows rather than the unit.
    #[test]
    fn terabytes_are_the_last_unit() {
        assert_eq!(byte_size(2048 * 1024 * 1024 * 1024 * 1024), "2048 TB");
    }
}
