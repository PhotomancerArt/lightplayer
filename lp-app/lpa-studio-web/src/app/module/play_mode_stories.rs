//! Play-mode stories (M2 UX spike, gate G2 question 4).
//!
//! `docs/design/panel.md` P12: play mode renders **panels only** — the root
//! module's panel, recursively presenting its nested module groups (R8) —
//! with no faces, no children, no wiring, no authoring surfaces at all.
//! Reset and auto-save stay, because "anything play mode can do, an end
//! user is allowed to do" and the scarf scenario (P10) is an end user's.
//!
//! Two widths, because the phone case is the real one: the installation
//! controller in E7 and the 4 a.m. dimmer in P10 are both phones.

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;

use super::PlayModeSurface;
use super::module_fixtures::held_root_face;

#[story(
    description = "Play mode, desktop: the root module's panel alone — its own controls in a row, then the two effect groups as bordered clusters SIDE BY SIDE — over a slim output banner. No sublabels, no hero-dominant preview, no children, no wiring, no card chrome. This is the surface the minimalism was for. Reset and auto-save stay because they are end-user gestures."
)]
fn desktop() -> Element {
    let face = held_root_face();
    rsx! {
        div { class: "tw:h-[560px] tw:w-full tw:max-w-[900px] tw:overflow-auto tw:border tw:border-border",
            PlayModeSurface {
                panel: face.panel,
                preview: face.preview,
                auto_save: face.auto_save,
                on_panel: move |_| {},
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "Play mode, phone width (375px): the same panel wrapping to one column. This is the surface the design docs' two hardest scenarios live on — dimming an LED scarf from a phone at 4 a.m., and driving an installation's XY pad."
)]
fn mobile() -> Element {
    let face = held_root_face();
    rsx! {
        div { class: "tw:h-[720px] tw:w-[375px] tw:overflow-auto tw:border tw:border-border",
            PlayModeSurface {
                panel: face.panel,
                preview: face.preview,
                auto_save: face.auto_save,
                on_panel: move |_| {},
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "Density without collapse: the same panel at tablet width, where the two effect boxes no longer fit side by side and the flex row wraps them into a stack. Wrapping is the density mechanism — there is no fold-away — so compare against play-mode/desktop, where the same two boxes sit next to each other."
)]
fn groups_wrapped() -> Element {
    let face = held_root_face();
    rsx! {
        div { class: "tw:h-[560px] tw:w-[430px] tw:overflow-auto tw:border tw:border-border",
            PlayModeSurface {
                panel: face.panel,
                preview: face.preview,
                auto_save: face.auto_save,
                on_panel: move |_| {},
                on_action: move |_| {},
            }
        }
    }
}
