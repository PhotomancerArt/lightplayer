//! A board's own name: the one rule every end applies before it stores,
//! sends or advertises one, so Studio, the server and the Bluetooth name
//! agree on what a name is.

use alloc::string::String;

/// The longest name a board keeps, in characters. Far more than Bluetooth
/// can advertise (that cut is the radio's, at its own byte budget); enough
/// for any name a person types into a card.
pub const DEVICE_NAME_MAX_CHARS: usize = 64;

/// `raw` as a board's name: control characters dropped, every run of
/// whitespace one space, trimmed, and cut to [`DEVICE_NAME_MAX_CHARS`]
/// characters. `None` when nothing is left (a blank name clears the board's).
pub fn clean_device_name(raw: &str) -> Option<String> {
    let mut name = String::new();
    let mut pending_space = false;
    let mut chars = 0usize;
    for c in raw.chars() {
        if c.is_whitespace() {
            pending_space = !name.is_empty();
            continue;
        }
        if c.is_control() {
            continue;
        }
        let needed = if pending_space { 2 } else { 1 };
        if chars + needed > DEVICE_NAME_MAX_CHARS {
            break;
        }
        if pending_space {
            name.push(' ');
            chars += 1;
            pending_space = false;
        }
        name.push(c);
        chars += 1;
    }
    (!name.is_empty()).then_some(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_name_is_kept() {
        assert_eq!(
            clean_device_name("Flower-Viatrix").as_deref(),
            Some("Flower-Viatrix")
        );
    }

    #[test]
    fn whitespace_collapses_and_controls_go() {
        assert_eq!(
            clean_device_name("  Porch \t\n sign\u{7}  ").as_deref(),
            Some("Porch sign")
        );
    }

    #[test]
    fn a_blank_name_is_none() {
        assert_eq!(clean_device_name(""), None);
        assert_eq!(clean_device_name(" \t "), None);
    }

    #[test]
    fn a_long_name_is_cut_on_a_character() {
        let long = "é".repeat(100);
        let cut = clean_device_name(&long).unwrap();
        assert_eq!(cut.chars().count(), DEVICE_NAME_MAX_CHARS);
    }
}
