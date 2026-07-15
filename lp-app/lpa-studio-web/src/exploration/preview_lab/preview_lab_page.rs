//! Dev-only preview lab page (PoC A, GPU-preview discovery M1).
//!
//! Reached at `#/preview-lab` in `stories`-feature builds only; never linked
//! from product navigation. Spawns N live preview cards, each backed by a
//! full browser firmware runtime, and shows per-card and aggregate frame
//! cost so the CPU-path scaling envelope can be measured.

use std::cell::RefCell;
use std::rc::Rc;

use dioxus::prelude::*;

use crate::exploration::preview_lab_config::{LabConfig, LabProject, LabTier};

use super::lab_runner::{LabRun, LabView, canvas_element_id, run_lab};

pub fn should_show_preview_lab() -> bool {
    location_hash().is_some_and(|hash| hash.starts_with("#/preview-lab"))
}

#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn PreviewLabPage() -> Element {
    let initial = use_hook(|| {
        location_hash()
            .and_then(|hash| LabConfig::parse_hash(&hash))
            .unwrap_or_default()
    });
    let mut config = use_signal(|| initial.clone());
    let view = use_signal(LabView::default);
    let mut run = use_signal(|| None::<Rc<RefCell<LabRun>>>);

    let start = move |_| {
        if let Some(active) = run.read().as_ref() {
            active.borrow_mut().request_stop();
        }
        let next = Rc::new(RefCell::new(LabRun::new(config.read().clone())));
        run.set(Some(Rc::clone(&next)));
        spawn(run_lab(next, view));
    };
    let stop = move |_| {
        if let Some(active) = run.read().as_ref() {
            active.borrow_mut().request_stop();
        }
    };

    // Automation entry: `#/preview-lab?...&autostart=1` starts on mount.
    use_hook(move || {
        if initial.autostart {
            let next = Rc::new(RefCell::new(LabRun::new(initial.clone())));
            run.set(Some(Rc::clone(&next)));
            spawn(run_lab(next, view));
        }
    });

    let current = view.read().clone();
    let card_count = config.read().cards as usize;
    let size = config.read().size;
    let agg = &current.aggregate;
    let memory_line = current
        .worker_wasm_memory_bytes
        .iter()
        .enumerate()
        .map(|(w, bytes)| format!("w{w}: {:.1} MB", bytes / (1024.0 * 1024.0)))
        .collect::<Vec<_>>()
        .join("  ");
    let js_heap_line = current
        .js_heap_bytes
        .map(|bytes| format!("{:.1} MB", bytes / (1024.0 * 1024.0)))
        .unwrap_or_else(|| "n/a".to_string());

    rsx! {
        main { class: "tw:mx-auto tw:grid tw:w-[min(1520px,100%)] tw:gap-4 tw:p-6",
            header { class: "tw:grid tw:gap-1",
                h1 { class: "tw:m-0 tw:text-xl tw:font-bold tw:text-strong-foreground",
                    "Preview Lab (PoC A)"
                }
                p { class: "tw:m-0 tw:text-sm tw:text-muted-foreground",
                    "Dev-only CPU-path preview scaling measurement. Each card is a full browser firmware runtime; pixels arrive over transferable ArrayBuffers."
                }
            }

            section { class: "tw:flex tw:flex-wrap tw:items-end tw:gap-4 tw:rounded-md tw:border tw:border-border tw:bg-card tw:p-4",
                ConfigChoice {
                    label: "Cards",
                    choices: LabConfig::CARD_CHOICES.to_vec(),
                    selected: config.read().cards,
                    onselect: move |value| config.write().cards = value,
                }
                ConfigChoice {
                    label: "Workers",
                    choices: LabConfig::WORKER_CHOICES.to_vec(),
                    selected: config.read().workers,
                    onselect: move |value| config.write().workers = value,
                }
                ConfigChoice {
                    label: "FPS",
                    choices: LabConfig::FPS_CHOICES.to_vec(),
                    selected: config.read().fps,
                    onselect: move |value| config.write().fps = value,
                }
                ConfigChoice {
                    label: "Size",
                    choices: LabConfig::SIZE_CHOICES.to_vec(),
                    selected: config.read().size,
                    onselect: move |value| config.write().size = value,
                }
                div { class: "tw:grid tw:gap-1",
                    span { class: "tw:text-xs tw:font-bold tw:uppercase tw:text-subtle-foreground", "Project" }
                    div { class: "tw:flex tw:gap-1",
                        for project in LabProject::ALL {
                            button {
                                class: choice_class(config.read().project == project),
                                r#type: "button",
                                onclick: move |_| config.write().project = project,
                                "{project.key()}"
                            }
                        }
                    }
                }
                div { class: "tw:grid tw:gap-1",
                    span { class: "tw:text-xs tw:font-bold tw:uppercase tw:text-subtle-foreground", "Tier" }
                    div { class: "tw:flex tw:gap-1",
                        for tier in LabTier::ALL {
                            button {
                                class: choice_class(config.read().tier == tier),
                                r#type: "button",
                                onclick: move |_| config.write().tier = tier,
                                "{tier.key()}"
                            }
                        }
                    }
                }
                div { class: "tw:ml-auto tw:flex tw:gap-2",
                    button {
                        class: "tw:rounded-sm tw:border tw:border-accent-border tw:bg-card-raised tw:px-4 tw:py-1.5 tw:text-sm tw:font-bold tw:text-strong-foreground",
                        r#type: "button",
                        onclick: start,
                        "Start"
                    }
                    button {
                        class: "tw:rounded-sm tw:border tw:border-border-strong tw:bg-card-raised tw:px-4 tw:py-1.5 tw:text-sm tw:text-soft-foreground",
                        r#type: "button",
                        onclick: stop,
                        "Stop"
                    }
                }
            }

            section { class: "tw:grid tw:gap-1 tw:rounded-md tw:border tw:border-border tw:bg-card tw:p-4 tw:font-mono tw:text-xs tw:text-soft-foreground",
                div { class: "tw:text-sm tw:font-bold tw:text-strong-foreground", "phase: {current.phase}  ·  elapsed: {current.elapsed_s:.0}s" }
                div {
                    "total fps: {agg.total_fps:.1}  ·  per-frame ms — tick: {agg.mean_tick_ms:.2}  render: {agg.mean_render_ms:.2}  transport: {agg.mean_transport_ms:.2}  present: {agg.mean_present_ms:.2}"
                }
                div {
                    "est worker cores: {agg.est_worker_cores:.2}  ·  est present cores: {agg.est_present_cores:.3}  ·  dropped: {agg.total_dropped}  ·  errors: {agg.total_errors}"
                }
                div { "wasm memory  {memory_line}  ·  js heap: {js_heap_line}" }
                if !current.notes.is_empty() {
                    div { class: "tw:text-warning-foreground",
                        for note in current.notes.iter() {
                            div { "{note}" }
                        }
                    }
                }
            }

            section { class: "tw:flex tw:flex-wrap tw:gap-3",
                for index in 0..card_count {
                    {
                        let card = current.cards.get(index);
                        let status = card.map(|c| c.status.clone()).unwrap_or_else(|| "idle".to_string());
                        let tier = card.map(|c| c.tier.clone()).unwrap_or_else(|| "…".to_string());
                        let tier_reason = card.and_then(|c| c.tier_reason.clone());
                        let stat_line = card
                            .map(|c| {
                                format!(
                                    "{:.1} fps  t {:.1} r {:.1} x {:.1} p {:.2}",
                                    c.stats.achieved_fps,
                                    c.stats.mean_tick_ms,
                                    c.stats.mean_render_ms,
                                    c.stats.mean_transport_ms,
                                    c.stats.mean_present_ms,
                                )
                            })
                            .unwrap_or_default();
                        let error = card.and_then(|c| c.error.clone());
                        let canvas_id = canvas_element_id(current.generation, index);
                        rsx! {
                            div {
                                key: "{canvas_id}",
                                class: "tw:grid tw:gap-1 tw:rounded-md tw:border tw:border-border tw:bg-card tw:p-2",
                                canvas {
                                    id: "{canvas_id}",
                                    width: "{size}",
                                    height: "{size}",
                                    style: "width: 128px; height: 128px; image-rendering: pixelated; background: #000;",
                                }
                                div { class: "tw:w-[128px] tw:font-mono tw:text-[0.6rem] tw:leading-tight tw:text-subtle-foreground",
                                    div { class: "tw:flex tw:items-center tw:gap-1",
                                        span { "#{index} {status}" }
                                        span { class: tier_badge_class(&tier), "{tier}" }
                                    }
                                    div { "{stat_line}" }
                                    if let Some(reason) = tier_reason {
                                        div { class: "tw:text-warning-foreground tw:break-words",
                                            "gpu→cpu: {reason}"
                                        }
                                    }
                                    if let Some(error) = error {
                                        div { class: "tw:text-error-foreground tw:break-words", "{error}" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ConfigChoice(
    label: &'static str,
    choices: Vec<u32>,
    selected: u32,
    onselect: EventHandler<u32>,
) -> Element {
    rsx! {
        div { class: "tw:grid tw:gap-1",
            span { class: "tw:text-xs tw:font-bold tw:uppercase tw:text-subtle-foreground", "{label}" }
            div { class: "tw:flex tw:gap-1",
                for choice in choices {
                    button {
                        class: choice_class(selected == choice),
                        r#type: "button",
                        onclick: move |_| onselect.call(choice),
                        "{choice}"
                    }
                }
            }
        }
    }
}

/// Visible tier badge (fidelity-tiers ADR: which tier a card runs on is
/// user-visible state, never inferred).
fn tier_badge_class(tier: &str) -> &'static str {
    match tier {
        "gpu" => {
            "tw:rounded-sm tw:border tw:border-accent-border tw:px-1 tw:font-bold tw:uppercase tw:text-strong-foreground"
        }
        "cpu" => {
            "tw:rounded-sm tw:border tw:border-border-strong tw:px-1 tw:font-bold tw:uppercase tw:text-muted-foreground"
        }
        _ => "tw:px-1 tw:text-muted-foreground",
    }
}

fn choice_class(active: bool) -> &'static str {
    if active {
        "tw:rounded-sm tw:border tw:border-accent-border tw:bg-card-raised tw:px-2 tw:py-1 tw:text-xs tw:font-bold tw:text-strong-foreground"
    } else {
        "tw:rounded-sm tw:border tw:border-border-strong tw:bg-card tw:px-2 tw:py-1 tw:text-xs tw:text-muted-foreground"
    }
}

fn location_hash() -> Option<String> {
    web_sys::window()
        .map(|window| window.location())
        .and_then(|location| location.hash().ok())
}
