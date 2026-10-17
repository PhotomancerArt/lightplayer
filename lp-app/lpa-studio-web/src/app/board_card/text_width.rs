//! About how wide the card's words are: a guess from their letters, for
//! the two places the card decides what gives way before a value is cut —
//! the name bar's primary (its word drops to its verb alone,
//! [`super::name_bar`]) and the rows' kind words ([`super::stack_bar`]).
//! CSS can't measure a word against its neighbours, so the card works out
//! the width it needs and a container query's arithmetic (`100cqw` against
//! that width) makes the step. Measured against the stories to within a
//! few px; each use adds a margin so a guess never costs a letter.

/// About how wide `text` is at the primary's 12.5 px extrabold, in tenths
/// of a px: narrow letters, wide ones, capitals and the rest. Scale it for
/// another size or weight ([`at_percent`]).
pub(crate) fn text_tenths(text: &str) -> usize {
    text.chars()
        .map(|glyph| match glyph {
            'i' | 'j' | 'l' | 'r' | 't' | 'f' | 'I' | ' ' | '.' | ',' | '-' | '\'' | '·' => 40,
            'm' | 'w' | 'M' | 'W' => 105,
            'A'..='Z' => 85,
            _ => GLYPH_TENTHS,
        })
        .sum()
}

/// `tenths` at `percent` of the primary's type (a row's 11.5 px is 86 %).
pub(crate) fn at_percent(tenths: usize, percent: usize) -> usize {
    tenths * percent / 100
}

/// An ordinary letter at 12.5 px extrabold, in tenths of a px.
pub(crate) const GLYPH_TENTHS: usize = 75;

#[cfg(test)]
mod tests {
    use super::*;

    /// Measured on the stories (Chrome, 2026-10-17): "Install firmware" is
    /// 97 px at the primary's type, "Porch lights" 77 px at the name's.
    #[test]
    fn a_guess_lands_near_the_measured_widths() {
        let install = text_tenths("Install firmware") / 10;
        assert!((90..=100).contains(&install), "{install}");
        let porch = at_percent(text_tenths("Porch lights"), 106) / 10;
        assert!((74..=80).contains(&porch), "{porch}");
        assert!(text_tenths("mmmm") > text_tenths("iiii"));
    }
}
