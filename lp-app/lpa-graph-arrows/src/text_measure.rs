//! How wide a string is: the one thing about text the library can't know.
//!
//! The page measures with the browser's fonts; tests and the gallery use
//! [`AverageCharWidth`], which is exact for a monospace font and close
//! enough for a sans one. Any `Fn(&str, f64) -> f64` is a measure too.

pub trait TextMeasure {
    /// The width of `text` set at `font_size`, in the caller's units.
    fn width(&self, text: &str, font_size: f64) -> f64;
}

impl<F: Fn(&str, f64) -> f64> TextMeasure for F {
    fn width(&self, text: &str, font_size: f64) -> f64 {
        self(text, font_size)
    }
}

/// Every character the same width: `em` × the font size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AverageCharWidth {
    pub em: f64,
}

impl Default for AverageCharWidth {
    /// 0.6 em: Menlo, SF Mono and JetBrains Mono are all within a hair.
    fn default() -> Self {
        AverageCharWidth { em: 0.6 }
    }
}

impl TextMeasure for AverageCharWidth {
    fn width(&self, text: &str, font_size: f64) -> f64 {
        text.chars().count() as f64 * self.em * font_size
    }
}
