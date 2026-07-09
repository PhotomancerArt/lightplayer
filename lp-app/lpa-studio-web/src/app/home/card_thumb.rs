//! Placeholder card thumbnails.
//!
//! The thumbnail source is swappable by design (roadmap M4): today a
//! deterministic gradient seeded by the package identity with the name's
//! initial; a cached rendered frame (and later a live GPU gallery) replaces
//! this component's body without touching the cards.

use dioxus::prelude::*;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn CardThumb(
    seed: String,
    label: String,
    #[props(default = false)] muted: bool,
) -> Element {
    let (hue_a, hue_b) = thumb_hues(&seed);
    let (saturation, lightness) = if muted { (12, 16) } else { (42, 22) };
    let style = format!(
        "background: linear-gradient(135deg, hsl({hue_a} {saturation}% {lightness}%), hsl({hue_b} {}% {}%));",
        saturation + 12,
        lightness + 10,
    );
    let initial = label
        .chars()
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_default();
    let initial_class = if muted {
        "tw:text-2xl tw:font-extrabold tw:text-white/20"
    } else {
        "tw:text-2xl tw:font-extrabold tw:text-white/40"
    };

    rsx! {
        div {
            class: "tw:flex tw:h-24 tw:w-full tw:items-center tw:justify-center tw:rounded-t-md",
            style: "{style}",
            span { class: initial_class, "{initial}" }
        }
    }
}

/// Two stable hues from the seed (uid or name): FNV-1a, split into two
/// angles far enough apart to read as a gradient.
fn thumb_hues(seed: &str) -> (u16, u16) {
    let mut hash: u32 = 0x811c_9dc5;
    for byte in seed.bytes() {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    let hue_a = (hash % 360) as u16;
    let hue_b = ((hash >> 16) % 360) as u16;
    (hue_a, hue_b)
}

#[cfg(test)]
mod tests {
    use super::thumb_hues;

    #[test]
    fn hues_are_stable_and_seed_dependent() {
        assert_eq!(thumb_hues("prj_a"), thumb_hues("prj_a"));
        assert_ne!(thumb_hues("prj_a"), thumb_hues("prj_b"));
        let (a, b) = thumb_hues("prj_a");
        assert!(a < 360 && b < 360);
    }
}
