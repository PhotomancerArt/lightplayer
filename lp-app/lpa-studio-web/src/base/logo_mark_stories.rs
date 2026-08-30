//! Brand asset stories — the durable PNG mint.
//!
//! Every `screenshot` story here is a published brand asset (bare capture,
//! `lg` only): the brand lockup, app-icon tiles, mono forms, the mark size
//! sheet. Change `logo_mark.rs` and CI re-mints every PNG under
//! `story-images/base__logo-mark__*`. The capture harness freezes CSS
//! animations before mount, so animated treatments capture their canonical
//! first frame (rainbow gradient at rest, play triangle on the brand green).
//!
//! Design record: `spikes/lightplayer-logo/index.html` (PR #304).

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;

use crate::base::{LogoLockup, LogoMark, LogoStacked};

#[story(
    screenshot,
    description = "The brand lockup: white datasheet-fine chip, full-rainbow wordmark, triangle cycling with it (brand green when frozen). One treatment everywhere — app chrome and readme hero alike."
)]
fn lockup() -> Element {
    dark_stage(rsx! { LogoLockup { size: 110 } })
}

#[story(
    screenshot,
    description = "Mono forms: ink-on-light for print, white-on-black, one-color brand green."
)]
fn lockup_mono() -> Element {
    rsx! {
        div { class: "tw:flex tw:flex-col tw:gap-4 tw:p-8 tw:bg-terminal",
            div {
                class: "tw:flex tw:justify-center tw:rounded-lg tw:p-8",
                style: "background:#f4f2ec;color:#14181d",
                LogoLockup { size: 44, mono: true }
            }
            div {
                class: "tw:flex tw:justify-center tw:rounded-lg tw:p-8",
                style: "background:#000;color:#fffaf0",
                LogoLockup { size: 44, mono: true }
            }
            div {
                class: "tw:flex tw:justify-center tw:rounded-lg tw:bg-card tw:p-8 tw:text-[#5fe08b]",
                LogoLockup { size: 44, mono: true }
            }
        }
    }
}

#[story(
    screenshot,
    description = "Stacked/square form for app-icon tiles, social cards, and splash surfaces."
)]
fn stacked() -> Element {
    dark_stage(rsx! { LogoStacked { size: 120 } })
}

#[story(
    screenshot,
    description = "App-icon tiles at 128px: dark, brand-green, and black treatments. Icon-only rule: the play triangle is the animated element (frozen here on the brand green)."
)]
fn app_icons() -> Element {
    rsx! {
        div { class: "tw:flex tw:items-center tw:justify-center tw:gap-10 tw:bg-terminal tw:p-10",
            span {
                class: "tw:flex tw:items-center tw:justify-center",
                style: "width:128px;height:128px;border-radius:29px;background:linear-gradient(160deg,#1d232a,#0c1114);color:#fffaf0",
                LogoMark { size: 88, animated: true }
            }
            span {
                class: "tw:flex tw:items-center tw:justify-center",
                style: "width:128px;height:128px;border-radius:29px;background:linear-gradient(160deg,#8ae7bd,#57c391);color:#0c1114",
                LogoMark { size: 88 }
            }
            span {
                class: "tw:flex tw:items-center tw:justify-center",
                style: "width:128px;height:128px;border-radius:29px;background:#000;color:#fffaf0",
                LogoMark { size: 88, animated: true }
            }
        }
    }
}

#[story(
    screenshot,
    description = "Mark size sheet: 16 (favicon) / 22 (top bar) / 32 / 56 / 128, plus the compact animated form. White on terminal."
)]
fn mark_sizes() -> Element {
    rsx! {
        div { class: "tw:flex tw:items-center tw:justify-center tw:gap-8 tw:bg-terminal tw:p-10 tw:text-strong",
            LogoMark { size: 16 }
            LogoMark { size: 22 }
            LogoMark { size: 32 }
            LogoMark { size: 56 }
            LogoMark { size: 128, animated: true }
        }
    }
}

fn dark_stage(inner: Element) -> Element {
    rsx! {
        div { class: "tw:flex tw:items-center tw:justify-center tw:bg-terminal tw:p-12",
            {inner}
        }
    }
}
