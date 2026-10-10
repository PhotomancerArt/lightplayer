//! The device's Wi‑Fi popover: the detail card the Connections group's
//! Wi‑Fi row opens — and, on the board card, a section of the connection
//! bar's details (`in_card`: the section's divider, no frame of its own) —
//! three pages, the UX spike's 2B (`spikes/wifi-networks/index.html`,
//! chosen 2026-10-05).
//!
//! 1. **Networks** — the connected network first (bars, "Connected · ip"),
//!    the other saved ones with a word each, then "+ Connect to a network";
//!    the board's one Wi‑Fi switch in the header and the Cloud relay switch
//!    at the bottom. Nothing saved opens straight on page 2.
//! 2. **Connect to a network** — what the board hears, strongest first,
//!    and "Other network…". A firmware that cannot scan (every M5 image)
//!    makes this "Add a network by name": the name and password right here,
//!    and Connect reads Save.
//! 3. **The network** — its name (read-only when picked) and password, then
//!    Connect. After Connect the popover goes straight back to page 1,
//!    where the new network is in the list and its test runs in its row.
//!
//! A saved network's own page says how it stands, that its password is on
//! the board and can't be shown, and offers Change password (adding it
//! again) and Forget (two clicks).
//!
//! Which page shows is this component's state; every word comes from core
//! ([`lpa_studio_core::app::network::wifi_words`] through
//! [`UiDeviceWifi`]), and every press is a verb core publishes at
//! `devices/<board>/wifi/…`. The panel never builds an op. The password
//! field is a password input, its text leaves the form with the press, and
//! it is never echoed.

use dioxus::prelude::*;
use lpa_studio_core::app::network::wifi_words::{
    CANNOT_LIST, CLOUD_RELAY_HELP, PICK_A_NETWORK, label,
};
use lpa_studio_core::{
    HeardNetwork, NetworkCommand, OfferArgs, UiAction, UiDeviceWifi, UiOffer, UiWifiNetworkRow,
    UiWifiTest, WIFI_ENABLED_PARAM, WIFI_FORGET_SEGMENT, WIFI_NETWORK_PARAM, WIFI_PASSWORD_PARAM,
    WifiStepState, WifiTestNext, WifiTone, signal_bars, signal_word,
};

use super::access_fields::{HELP_CLASS, Switch, TEXT_LINK_CLASS};
use crate::base::{StudioIcon, StudioIconName};
use crate::core::action::{ActionButton, ActionButtonVariant};
use crate::core::offer::offer_params_form::{OfferPressButton, SecretField, pressed_or_refused};

/// Which page of the popover shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum WifiPage {
    /// Page 1: the saved networks.
    Networks,
    /// Page 2: what the board hears (or a typed name).
    Connect,
    /// Page 3: a network's name and password. `ssid: None` is "Other
    /// network…" (a typed name); `changing` is Change password.
    Form {
        ssid: Option<String>,
        changing: bool,
    },
    /// A saved network's own page.
    Network(String),
}

/// The popover body (the stories' subject too).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn WifiPanel(
    wifi: UiDeviceWifi,
    /// The verbs core publishes under `devices/<board>/wifi/` (in the app,
    /// read from the offer tree; in a story, built by core directly).
    offers: Vec<UiOffer>,
    on_action: EventHandler<UiAction>,
    /// A refresh, a scan, the in-row test dismissed.
    on_network: EventHandler<NetworkCommand>,
    /// Stories only: the page it opens on.
    #[props(default)]
    page_preview: Option<WifiPage>,
    /// Stories only: what the form starts holding.
    #[props(default)]
    args_preview: Option<OfferArgs>,
    /// Stories only: Forget starts armed.
    #[props(default)]
    forget_armed_preview: bool,
    /// Stories only: the in-row test to draw (a board reports only
    /// `connecting` while it tries; a later step is M6's to tell).
    #[props(default)]
    test_preview: Option<UiWifiTest>,
    /// Drawn as a section of a details card (the board card's connection
    /// details), not as a popover's whole body: its divider, no frame.
    #[props(default)]
    in_card: bool,
) -> Element {
    let mut page = use_signal(|| page_preview.clone().unwrap_or(WifiPage::Networks));
    let panel_class = panel_class(in_card);
    let args = use_signal(|| args_preview.clone().unwrap_or_default());
    let verbs = WifiVerbs::of(&offers);
    let shown = shown_page(&page.read(), &wifi);
    let device = wifi.device;

    if wifi.status.is_none() {
        let line = wifi.waiting_line().unwrap_or_default();
        return rsx! {
            div { class: panel_class,
                PageHead { title: label::WIFI.to_string() }
                p { class: HELP_CLASS, "{line}" }
            }
        };
    }

    let go = move |to: WifiPage| {
        if matches!(to, WifiPage::Connect) {
            on_network.call(NetworkCommand::Scan { device });
        }
        page.set(to);
    };
    let body = match &shown {
        WifiPage::Networks => {
            let test = test_preview.clone().or_else(|| wifi.test());
            networks_page(&wifi, test, &verbs, on_action, on_network, args, go)
        }
        WifiPage::Connect => connect_page(&wifi, &verbs, on_action, on_network, args, page, go),
        WifiPage::Form { ssid, changing } => form_page(
            &wifi,
            &verbs,
            ssid.clone(),
            *changing,
            on_action,
            args,
            page,
            go,
        ),
        WifiPage::Network(ssid) => network_page(
            &wifi,
            &verbs,
            ssid,
            on_action,
            on_network,
            args,
            go,
            forget_armed_preview,
        ),
    };
    rsx! {
        div { class: panel_class,
            {body}
            if wifi.writing {
                p { class: HELP_CLASS, "{label::WRITING}" }
            }
            if let Some(error) = wifi.error.clone() {
                p { class: "tw:m-0 tw:text-xs tw:leading-relaxed tw:text-status-error-foreground", "{error}" }
            }
        }
    }
}

/// The page that actually shows: nothing saved opens on Connect (unless an
/// add is on its way, whose network the list is about to hold), and a
/// network that is gone falls back to the list.
fn shown_page(page: &WifiPage, wifi: &UiDeviceWifi) -> WifiPage {
    let empty = wifi.nothing_saved() && !wifi.writing;
    let saved = |ssid: &str| {
        wifi.status
            .as_ref()
            .is_some_and(|status| status.network(ssid).is_some())
    };
    match page {
        WifiPage::Networks if empty => WifiPage::Connect,
        WifiPage::Network(ssid) if !saved(ssid) => {
            if empty {
                WifiPage::Connect
            } else {
                WifiPage::Networks
            }
        }
        page => page.clone(),
    }
}

// --- the pages ---------------------------------------------------------------

/// Page 1: the saved networks, then "+ Connect to a network", then the
/// Cloud relay switch.
fn networks_page(
    wifi: &UiDeviceWifi,
    test: Option<UiWifiTest>,
    verbs: &WifiVerbs,
    on_action: EventHandler<UiAction>,
    on_network: EventHandler<NetworkCommand>,
    mut args: Signal<OfferArgs>,
    mut go: impl FnMut(WifiPage) + Copy + 'static,
) -> Element {
    let rows = wifi.rows();
    let line = wifi.networks_line().filter(|_| test.is_none());
    let device = wifi.device;
    rsx! {
        PageHead { title: label::WIFI.to_string(), switch: verbs.enabled.clone(), on_action }
        if let Some(line) = line {
            p { class: SAY_CLASS, "{line}" }
        }
        div { class: "tw:grid tw:min-w-0",
            for row in rows {
                NetworkRowButton {
                    key: "{row.ssid}",
                    row: row.clone(),
                    on_open: move |ssid: String| go(WifiPage::Network(ssid)),
                }
                if let Some(test) = test.clone().filter(|test| test.ssid == row.ssid) {
                    TestCard {
                        test,
                        forget: verbs.forget(&row.slug),
                        on_action,
                        on_done: move |_| on_network.call(NetworkCommand::DismissTest { device }),
                        on_change_password: move |ssid: String| {
                            on_network.call(NetworkCommand::DismissTest { device });
                            args.set(OfferArgs::new().with(WIFI_NETWORK_PARAM, ssid.clone()));
                            go(WifiPage::Form { ssid: Some(ssid), changing: true });
                        },
                    }
                }
            }
        }
        button {
            class: WIDE_BUTTON_CLASS,
            r#type: "button",
            onclick: move |_| {
                args.set(OfferArgs::new());
                go(WifiPage::Connect);
            },
            StudioIcon { name: StudioIconName::Add, size: 13 }
            "{label::CONNECT_TO_A_NETWORK}"
        }
        RelaySwitch { offer: verbs.cloud_relay.clone(), line: wifi.relay_line(), on_action }
    }
}

/// Page 2: what the board hears, or — on a firmware that cannot scan — a
/// typed name and its password, right here.
fn connect_page(
    wifi: &UiDeviceWifi,
    verbs: &WifiVerbs,
    on_action: EventHandler<UiAction>,
    on_network: EventHandler<NetworkCommand>,
    mut args: Signal<OfferArgs>,
    page: Signal<WifiPage>,
    mut go: impl FnMut(WifiPage) + Copy + 'static,
) -> Element {
    let root = wifi.nothing_saved();
    let device = wifi.device;
    let head = if root {
        rsx! { PageHead { title: label::WIFI.to_string(), switch: verbs.enabled.clone(), on_action } }
    } else {
        let title = if wifi.can_connect() {
            label::CONNECT_TO_A_NETWORK
        } else {
            label::ADD_BY_NAME
        };
        rsx! { PageHead { title: title.to_string(), on_back: move |_| go(WifiPage::Networks) } }
    };
    let root_line = if root { wifi.networks_line() } else { None };
    if !wifi.can_connect() {
        return rsx! {
            {head}
            if let Some(line) = root_line {
                p { class: SAY_CLASS, "{line}" }
            }
            p { class: HELP_CLASS, "{CANNOT_LIST}" }
            NetworkForm {
                ssid: None,
                open: false,
                add: verbs.add.clone(),
                args,
                on_action,
                on_pressed: move |_| go(WifiPage::Networks),
            }
            if root {
                RelaySwitch { offer: verbs.cloud_relay.clone(), line: wifi.relay_line(), on_action }
            }
        };
    }
    let nearby = wifi.nearby();
    let scanning = wifi.scanning;
    let _ = page;
    rsx! {
        {head}
        if root {
            p { class: SAY_CLASS, "{PICK_A_NETWORK}" }
        }
        div { class: SECTION_HEAD_CLASS,
            span { "{label::NEARBY}" }
            span { class: "tw:ml-auto tw:inline-flex tw:items-center tw:gap-1 tw:normal-case tw:tracking-normal",
                if scanning {
                    span { class: "tw:animate-pulse tw:text-dim-foreground", "…" }
                } else {
                    button {
                        class: TEXT_LINK_CLASS,
                        r#type: "button",
                        onclick: move |_| on_network.call(NetworkCommand::Scan { device }),
                        StudioIcon { name: StudioIconName::Refresh, size: 11 }
                        " {label::REFRESH}"
                    }
                }
            }
        }
        div { class: "tw:grid tw:min-w-0",
            if scanning && nearby.is_empty() {
                p { class: "tw:px-3 tw:py-2 tw:text-[12px] tw:text-dim-foreground tw:animate-pulse", "{label::LOOKING_FOR_NETWORKS}" }
            }
            for heard in nearby {
                HeardRowButton {
                    key: "{heard.ssid}",
                    heard: heard.clone(),
                    on_pick: move |ssid: String| {
                        args.set(OfferArgs::new().with(WIFI_NETWORK_PARAM, ssid.clone()));
                        go(WifiPage::Form { ssid: Some(ssid), changing: false });
                    },
                }
            }
            button {
                class: ROW_BUTTON_CLASS,
                r#type: "button",
                onclick: move |_| {
                    args.set(OfferArgs::new());
                    go(WifiPage::Form { ssid: None, changing: false });
                },
                span { class: "tw:inline-flex tw:w-[22px] tw:flex-none tw:justify-center tw:text-subtle-foreground",
                    StudioIcon { name: StudioIconName::Add, size: 13 }
                }
                span { class: "tw:grid tw:min-w-0 tw:flex-1",
                    span { class: "tw:text-[13px] tw:font-semibold tw:text-subtle-foreground", "{label::OTHER_NETWORK}" }
                    span { class: ROW_SUB_CLASS, "{label::OTHER_NETWORK_SUB}" }
                }
            }
        }
        if root {
            RelaySwitch { offer: verbs.cloud_relay.clone(), line: wifi.relay_line(), on_action }
        }
    }
}

/// Page 3: the network's name and password, then Connect.
#[allow(clippy::too_many_arguments, reason = "one page's inputs, passed once")]
fn form_page(
    wifi: &UiDeviceWifi,
    verbs: &WifiVerbs,
    ssid: Option<String>,
    changing: bool,
    on_action: EventHandler<UiAction>,
    args: Signal<OfferArgs>,
    page: Signal<WifiPage>,
    mut go: impl FnMut(WifiPage) + Copy + 'static,
) -> Element {
    let title = match (&ssid, changing) {
        (Some(_), true) => label::CHANGE_PASSWORD.to_string(),
        (Some(ssid), false) => ssid.clone(),
        (None, _) => label::OTHER_NETWORK_TITLE.to_string(),
    };
    let back = match (&ssid, changing) {
        (Some(ssid), true) => WifiPage::Network(ssid.clone()),
        _ => WifiPage::Connect,
    };
    let open = ssid.as_deref().is_some_and(|ssid| wifi.heard_open(ssid)) && !changing;
    let _ = page;
    rsx! {
        PageHead { title, on_back: move |_| go(back.clone()) }
        NetworkForm {
            ssid,
            open,
            add: verbs.add.clone(),
            args,
            on_action,
            on_pressed: move |_| go(WifiPage::Networks),
        }
    }
}

/// A saved network's page: how it stands, its password line, Change
/// password and Forget.
#[allow(clippy::too_many_arguments, reason = "one page's inputs, passed once")]
fn network_page(
    wifi: &UiDeviceWifi,
    verbs: &WifiVerbs,
    ssid: &str,
    on_action: EventHandler<UiAction>,
    on_network: EventHandler<NetworkCommand>,
    mut args: Signal<OfferArgs>,
    mut go: impl FnMut(WifiPage) + Copy + 'static,
    forget_armed_preview: bool,
) -> Element {
    let (line, tone) = wifi
        .network_line(ssid)
        .unwrap_or_else(|| (String::new(), WifiTone::Plain));
    let password = wifi.password_line(ssid).unwrap_or_default();
    let refused = wifi.password_refused(ssid);
    let slug = wifi
        .rows()
        .into_iter()
        .find(|row| row.ssid == ssid)
        .map(|row| row.slug);
    let forget = slug.and_then(|slug| verbs.forget(&slug));
    let owned = ssid.to_string();
    let device = wifi.device;
    rsx! {
        PageHead { title: ssid.to_string(), on_back: move |_| go(WifiPage::Networks) }
        p { class: "{SAY_CLASS} {tone_class(tone)}", "{line}" }
        p { class: HELP_CLASS, "{password}" }
        div { class: "tw:flex tw:min-w-0 tw:flex-wrap tw:items-start tw:gap-2",
            button {
                class: if refused { PRIMARY_BUTTON_CLASS } else { SMALL_BUTTON_CLASS },
                r#type: "button",
                onclick: move |_| {
                    on_network.call(NetworkCommand::DismissTest { device });
                    args.set(OfferArgs::new().with(WIFI_NETWORK_PARAM, owned.clone()));
                    go(WifiPage::Form { ssid: Some(owned.clone()), changing: true });
                },
                "{label::CHANGE_PASSWORD}"
            }
            if let Some(forget) = forget {
                div { class: "tw:ml-auto",
                    OfferPressButton {
                        offer: forget,
                        args: OfferArgs::new(),
                        variant: ActionButtonVariant::Quiet,
                        armed_preview: forget_armed_preview,
                        on_action,
                    }
                }
            }
        }
    }
}

// --- the pieces --------------------------------------------------------------

/// The Wi‑Fi verbs core published, by name.
#[derive(Clone)]
struct WifiVerbs {
    add: Option<UiOffer>,
    enabled: Option<UiOffer>,
    cloud_relay: Option<UiOffer>,
    forgets: Vec<UiOffer>,
}

impl WifiVerbs {
    fn of(offers: &[UiOffer]) -> Self {
        let verb = |name: &str| {
            offers
                .iter()
                .find(|offer| offer.path.last() == Some(name))
                .cloned()
        };
        Self {
            add: verb("add"),
            enabled: verb("enabled"),
            cloud_relay: verb("cloud-relay"),
            forgets: offers
                .iter()
                .filter(|offer| {
                    offer
                        .path
                        .owner()
                        .is_some_and(|owner| owner.last() == Some(WIFI_FORGET_SEGMENT))
                })
                .cloned()
                .collect(),
        }
    }

    /// The forget verb of the saved network whose slug is `slug`.
    fn forget(&self, slug: &str) -> Option<UiOffer> {
        self.forgets
            .iter()
            .find(|offer| offer.path.last() == Some(slug))
            .cloned()
    }
}

/// A page's header: back (or the Wi‑Fi glyph), the title, and the board's
/// Wi‑Fi switch on the root page.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn PageHead(
    title: String,
    #[props(default)] on_back: Option<EventHandler<()>>,
    #[props(default)] switch: Option<UiOffer>,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    rsx! {
        div { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-2 tw:text-status-neutral-foreground",
            if let Some(on_back) = on_back {
                button {
                    class: "tw:-ml-1.5 tw:inline-flex tw:flex-none tw:cursor-pointer tw:appearance-none tw:rounded tw:border-0 tw:bg-transparent tw:p-1 tw:text-subtle-foreground tw:hover:bg-white/5 tw:hover:text-strong-foreground ux-focus-ring",
                    r#type: "button",
                    title: "Back",
                    aria_label: "Back",
                    onclick: move |_| on_back.call(()),
                    StudioIcon { name: StudioIconName::Back, size: 15 }
                }
            } else {
                span { class: "tw:inline-flex tw:flex-none", StudioIcon { name: StudioIconName::Wifi, size: 15 } }
            }
            h3 { class: "tw:m-0 tw:min-w-0 tw:truncate tw:text-sm tw:font-bold tw:text-strong-foreground", "{title}" }
            if let (Some(offer), Some(on_action)) = (switch, on_action) {
                span { class: "tw:ml-auto tw:inline-flex",
                    OfferSwitch { offer, on_action }
                }
            }
        }
    }
}

/// A switch offer's switch: it presses the offer with the new state.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn OfferSwitch(offer: UiOffer, on_action: EventHandler<UiAction>) -> Element {
    let Some(param) = offer.params().first().cloned() else {
        return rsx! {};
    };
    let on = param.default_value().is_some_and(|value| value == "true");
    let locked = !offer.is_enabled();
    rsx! {
        Switch {
            on,
            label: capitalized(&param.label),
            locked,
            on_toggle: move |next: bool| {
                if let Ok(action) = offer.press(&OfferArgs::new().with(WIFI_ENABLED_PARAM, next.to_string())) {
                    on_action.call(action);
                }
            },
        }
    }
}

/// The Cloud relay switch at the foot of a root page, and under it whether
/// the board reached lightplayer.app (`line`, the board's relay state in
/// words).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn RelaySwitch(
    offer: Option<UiOffer>,
    line: Option<(&'static str, WifiTone)>,
    on_action: EventHandler<UiAction>,
) -> Element {
    let Some(offer) = offer else {
        return rsx! {};
    };
    rsx! {
        div { class: "tw:mt-0.5 tw:flex tw:min-h-9 tw:min-w-0 tw:items-center tw:gap-2.5 tw:border-t tw:border-border-muted tw:pt-2",
            div { class: "tw:grid tw:min-w-0 tw:flex-1 tw:gap-0.5",
                span { class: "tw:text-[13px] tw:font-semibold tw:text-strong-foreground", "Cloud relay" }
                span { class: HELP_CLASS, "{CLOUD_RELAY_HELP}" }
                if let Some((words, tone)) = line {
                    span { class: "{HELP_CLASS} {tone_class(tone)}", "{words}" }
                }
            }
            OfferSwitch { offer, on_action }
        }
    }
}

/// One saved network's row: bars, its name and word, a chevron to its page.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn NetworkRowButton(row: UiWifiNetworkRow, on_open: EventHandler<String>) -> Element {
    let ssid = row.ssid.clone();
    let name_class = if row.in_use {
        "tw:truncate tw:text-[13px] tw:font-semibold tw:text-status-good-foreground"
    } else {
        "tw:truncate tw:text-[13px] tw:font-semibold tw:text-strong-foreground"
    };
    rsx! {
        button {
            class: ROW_BUTTON_CLASS,
            r#type: "button",
            onclick: move |_| on_open.call(ssid.clone()),
            SignalBars { rssi: row.rssi, in_use: row.in_use }
            span { class: "tw:grid tw:min-w-0 tw:flex-1",
                span { class: name_class, "{row.ssid}" }
                span { class: "{ROW_SUB_CLASS} {tone_class(row.tone)}", "{row.word}" }
            }
            span { class: "tw:inline-flex tw:flex-none tw:text-dim-foreground",
                StudioIcon { name: StudioIconName::Collapsed, size: 13 }
            }
        }
    }
}

/// One network the board hears: bars, its name and signal, a lock when it
/// asks for a password.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn HeardRowButton(heard: HeardNetwork, on_pick: EventHandler<String>) -> Element {
    let ssid = heard.ssid.clone();
    let sub = if heard.secure {
        signal_word(heard.rssi).to_string()
    } else {
        format!("{} · {}", signal_word(heard.rssi), label::OPEN)
    };
    rsx! {
        button {
            class: ROW_BUTTON_CLASS,
            r#type: "button",
            onclick: move |_| on_pick.call(ssid.clone()),
            SignalBars { rssi: Some(heard.rssi), in_use: false }
            span { class: "tw:grid tw:min-w-0 tw:flex-1",
                span { class: "tw:truncate tw:text-[13px] tw:font-semibold tw:text-strong-foreground", "{heard.ssid}" }
                span { class: ROW_SUB_CLASS, "{sub}" }
            }
            if heard.secure {
                span { class: "tw:inline-flex tw:flex-none tw:text-subtle-foreground", aria_label: "needs a password",
                    StudioIcon { name: StudioIconName::AccessLocked, size: 12 }
                }
            }
        }
    }
}

/// Four signal bars, lit to the strength; dim when there is none.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn SignalBars(rssi: Option<i8>, in_use: bool) -> Element {
    let lit = rssi.map_or(0, signal_bars);
    let on = if in_use {
        "tw:bg-status-good-foreground"
    } else {
        "tw:bg-status-neutral-foreground"
    };
    let title = rssi.map_or_else(String::new, |rssi| format!("{rssi} dBm"));
    rsx! {
        span { class: "tw:inline-flex tw:h-3.5 tw:w-[22px] tw:flex-none tw:items-end tw:gap-0.5", title: "{title}",
            for (n, height) in [(1u8, "tw:h-1"), (2, "tw:h-[7px]"), (3, "tw:h-2.5"), (4, "tw:h-[13px]")] {
                span {
                    key: "{n}",
                    class: "tw:block tw:w-[3.5px] tw:rounded-[1px] {height} {bar_fill(n <= lit, on)}",
                }
            }
        }
    }
}

/// The just-added network's test, in its row (2B): the steps, then the
/// outcome and what it offers next.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn TestCard(
    test: UiWifiTest,
    forget: Option<UiOffer>,
    on_action: EventHandler<UiAction>,
    on_done: EventHandler<()>,
    on_change_password: EventHandler<String>,
) -> Element {
    let steps = test.steps();
    let result = test.result();
    let ssid = test.ssid.clone();
    rsx! {
        div { class: "tw:mb-2 tw:ml-8 tw:grid tw:gap-1.5 tw:rounded-lg tw:border tw:border-border tw:bg-white/[0.02] tw:px-2.5 tw:py-2",
            for (n, step) in steps.into_iter().enumerate() {
                div { key: "{n}", class: "tw:flex tw:min-w-0 tw:items-center tw:gap-2 tw:text-xs {step_class(step.state)}",
                    span { class: "tw:inline-flex tw:h-4 tw:w-4 tw:flex-none tw:items-center tw:justify-center",
                        match step.state {
                            WifiStepState::Done => rsx! { StudioIcon { name: StudioIconName::StepComplete, size: 12 } },
                            WifiStepState::Bad => rsx! { StudioIcon { name: StudioIconName::Cancel, size: 12 } },
                            WifiStepState::Now => rsx! { span { class: "tw:h-2 tw:w-2 tw:animate-pulse tw:rounded-full tw:bg-status-good-foreground" } },
                            WifiStepState::Todo => rsx! { span { class: "tw:h-1.5 tw:w-1.5 tw:rounded-full tw:bg-white/20" } },
                        }
                    }
                    span { class: "tw:min-w-0 tw:truncate", "{step.label}" }
                }
            }
            if let Some(note) = test.relay_note {
                p { class: "tw:text-xs tw:leading-relaxed tw:text-status-warning-foreground", "{note}" }
            }
            if let Some(result) = result {
                div { class: "tw:grid tw:gap-2 tw:text-xs tw:leading-relaxed tw:text-muted-foreground",
                    span {
                        b { class: if result.good { "tw:text-status-good-foreground" } else { "tw:text-strong-foreground" }, "{result.headline}" }
                        " · {result.body}"
                    }
                    div { class: "tw:flex tw:flex-wrap tw:items-start tw:justify-end tw:gap-2",
                        match result.next {
                            WifiTestNext::Done => rsx! {
                                button { class: SMALL_BUTTON_CLASS, r#type: "button", onclick: move |_| on_done.call(()), "{label::DONE}" }
                            },
                            WifiTestNext::RemoveOrChangePassword => rsx! {
                                if let Some(forget) = forget.clone() {
                                    ActionButton {
                                        action: pressed_or_refused(&forget, &OfferArgs::new()).with_label(label::REMOVE),
                                        running: false,
                                        variant: ActionButtonVariant::Quiet,
                                        on_action,
                                    }
                                }
                                button {
                                    class: PRIMARY_BUTTON_CLASS,
                                    r#type: "button",
                                    onclick: move |_| on_change_password.call(ssid.clone()),
                                    "{label::CHANGE_PASSWORD}"
                                }
                            },
                        }
                    }
                }
            }
        }
    }
}

/// The network's name and password, and Connect (or Save). Pressing it
/// sends the add, clears the form, and goes back to the list.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn NetworkForm(
    /// The picked network (read-only); `None` is a typed name.
    ssid: Option<String>,
    /// The board heard it as open: no password field.
    open: bool,
    add: Option<UiOffer>,
    args: Signal<OfferArgs>,
    on_action: EventHandler<UiAction>,
    on_pressed: EventHandler<()>,
) -> Element {
    let Some(add) = add else {
        return rsx! {};
    };
    let mut args = args;
    let current = args.read().clone();
    let typed_name = current
        .get(WIFI_NETWORK_PARAM)
        .unwrap_or_default()
        .to_string();
    let password = current
        .get(WIFI_PASSWORD_PARAM)
        .unwrap_or_default()
        .to_string();
    rsx! {
        label { class: "tw:grid tw:min-w-0 tw:gap-1",
            span { class: FIELD_LABEL_CLASS, "{label::NETWORK_FIELD}" }
            if let Some(ssid) = ssid.clone() {
                input { class: "{INPUT_CLASS} tw:opacity-75", value: "{ssid}", readonly: true, tabindex: "-1" }
            } else {
                input {
                    class: INPUT_CLASS,
                    value: "{typed_name}",
                    placeholder: "Network name",
                    spellcheck: "false",
                    autocomplete: "off",
                    autocapitalize: "off",
                    oninput: move |event| args.write().insert(WIFI_NETWORK_PARAM, event.value()),
                }
            }
        }
        if open {
            p { class: HELP_CLASS, "{lpa_studio_core::app::network::wifi_words::OPEN_NETWORK}" }
        } else {
            SecretField {
                label: label::PASSWORD_FIELD.to_string(),
                placeholder: if ssid.is_none() { "empty if open".to_string() } else { String::new() },
                value: password,
                on_input: move |text: String| args.write().insert(WIFI_PASSWORD_PARAM, text),
            }
        }
        div { class: "tw:flex tw:min-w-0 tw:justify-end",
            OfferPressButton {
                offer: add,
                args: current,
                variant: ActionButtonVariant::Outline,
                // An empty name speaks for itself; a broken rule is said.
                hide_refusal: ssid.is_none() && typed_name.is_empty(),
                on_action: move |action| {
                    on_action.call(action);
                    // The password leaves the form with the press.
                    args.set(OfferArgs::new());
                    on_pressed.call(());
                },
            }
        }
    }
}

/// `label` with its first letter capitalized (core's labels are lower case:
/// "cloud relay" reads "Cloud relay").
fn capitalized(label: &str) -> String {
    let mut chars = label.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// One bar's fill: lit (`on`) or dim.
fn bar_fill(lit: bool, on: &'static str) -> &'static str {
    if lit { on } else { "tw:bg-white/15" }
}

fn tone_class(tone: WifiTone) -> &'static str {
    match tone {
        WifiTone::Plain => "",
        WifiTone::Good => "tw:text-status-good-foreground",
        WifiTone::Warn => "tw:text-status-warning-foreground",
    }
}

fn step_class(state: WifiStepState) -> &'static str {
    match state {
        WifiStepState::Done => "tw:text-muted-foreground",
        WifiStepState::Now => "tw:font-semibold tw:text-strong-foreground",
        WifiStepState::Bad => "tw:font-semibold tw:text-status-error-foreground",
        WifiStepState::Todo => "tw:text-dim-foreground",
    }
}

const PANEL_CLASS: &str = "tw:grid tw:min-w-0 tw:gap-2 tw:px-3 tw:pt-2.5 tw:pb-3";

/// [`PANEL_CLASS`] as a section of a details card: the section's divider
/// above it, and still no frame of its own (no box in a box).
const IN_CARD_CLASS: &str = "tw:grid tw:min-w-0 tw:gap-2 tw:border-0 tw:border-t tw:border-solid tw:border-border-muted tw:px-3 tw:pt-2.5 tw:pb-3 tw:first:border-t-0";

/// The panel's classes: a popover's whole body, or a section of a details
/// card.
fn panel_class(in_card: bool) -> &'static str {
    match in_card {
        true => IN_CARD_CLASS,
        false => PANEL_CLASS,
    }
}

const SAY_CLASS: &str = "tw:m-0 tw:text-[12.5px] tw:leading-snug tw:text-muted-foreground";

const ROW_BUTTON_CLASS: &str = "tw:-mx-1 tw:flex tw:w-[calc(100%+8px)] tw:min-w-0 tw:cursor-pointer tw:appearance-none tw:items-center tw:gap-2.5 tw:rounded-md tw:border-0 tw:bg-transparent tw:px-1 tw:py-1.5 tw:text-left tw:hover:bg-white/5 ux-focus-ring";

const ROW_SUB_CLASS: &str = "tw:truncate tw:text-[11px] tw:text-dim-foreground";

const SECTION_HEAD_CLASS: &str = "tw:flex tw:items-center tw:gap-2 tw:text-[10px] tw:font-extrabold tw:uppercase tw:tracking-[0.08em] tw:text-dim-foreground";

const WIDE_BUTTON_CLASS: &str = "tw:inline-flex tw:w-full tw:cursor-pointer tw:appearance-none tw:items-center tw:justify-center tw:gap-1.5 tw:rounded-md tw:border tw:border-status-neutral-border tw:bg-status-neutral-bg tw:px-3 tw:py-1.5 tw:text-xs tw:font-bold tw:text-status-neutral-foreground tw:hover:text-strong-foreground ux-focus-ring";

const SMALL_BUTTON_CLASS: &str = "tw:inline-flex tw:cursor-pointer tw:appearance-none tw:items-center tw:gap-1.5 tw:rounded-md tw:border tw:border-status-neutral-border tw:bg-status-neutral-bg tw:px-2.5 tw:py-1.5 tw:text-xs tw:font-bold tw:text-status-neutral-foreground tw:hover:text-strong-foreground ux-focus-ring";

const PRIMARY_BUTTON_CLASS: &str = "tw:inline-flex tw:cursor-pointer tw:appearance-none tw:items-center tw:gap-1.5 tw:rounded-md tw:border tw:border-status-good-border tw:bg-status-good-bg tw:px-2.5 tw:py-1.5 tw:text-xs tw:font-bold tw:text-strong-foreground ux-focus-ring";

const FIELD_LABEL_CLASS: &str =
    "tw:text-[10.5px] tw:font-bold tw:tracking-[0.02em] tw:text-dim-foreground";

const INPUT_CLASS: &str = "tw:min-w-0 tw:appearance-none tw:rounded-md tw:border tw:border-border-strong tw:bg-terminal tw:px-2.5 tw:py-2 tw:text-[13px] tw:text-strong-foreground";

#[cfg(test)]
mod tests {
    use super::*;
    use lpa_studio_core::{DeviceId, NetworkStatus, SavedNetworkInfo, StationState};

    fn wifi(saved: &[&str], writing: bool) -> UiDeviceWifi {
        UiDeviceWifi {
            status: Some(NetworkStatus {
                wifi: true,
                cloud_relay: true,
                networks: saved
                    .iter()
                    .map(|ssid| SavedNetworkInfo {
                        ssid: ssid.to_string(),
                        has_password: true,
                        hidden: false,
                        last: None,
                    })
                    .collect(),
                station: StationState::Unsupported,
                relay: lpc_wire::RelayState::Off,
            }),
            writing,
            ..UiDeviceWifi::new(DeviceId(1), true)
        }
    }

    #[test]
    fn nothing_saved_opens_on_connect_unless_an_add_is_on_its_way() {
        assert_eq!(
            shown_page(&WifiPage::Networks, &wifi(&[], false)),
            WifiPage::Connect
        );
        assert_eq!(
            shown_page(&WifiPage::Networks, &wifi(&[], true)),
            WifiPage::Networks
        );
        assert_eq!(
            shown_page(&WifiPage::Networks, &wifi(&["lp-walk-net"], false)),
            WifiPage::Networks
        );
    }

    /// Inside a details card the panel is a section of it: the section's
    /// divider, and no frame of its own — no border box, no ground, no
    /// rounding.
    #[test]
    fn inside_a_details_card_the_panel_carries_no_frame() {
        let class = panel_class(true);
        assert!(class.contains("tw:border-t"), "{class}");
        assert!(!class.contains("rounded"), "{class}");
        assert!(!class.contains("tw:bg-"), "{class}");
        assert!(!class.contains("shadow"), "{class}");
        assert_eq!(panel_class(false), PANEL_CLASS);
    }

    #[test]
    fn a_switch_label_reads_as_a_name() {
        assert_eq!(capitalized("cloud relay"), "Cloud relay");
        assert_eq!(capitalized("Wi‑Fi"), "Wi‑Fi");
    }

    #[test]
    fn a_forgotten_networks_page_falls_back_to_the_list() {
        let page = WifiPage::Network("gone".to_string());
        assert_eq!(
            shown_page(&page, &wifi(&["lp-walk-net"], false)),
            WifiPage::Networks
        );
        assert_eq!(shown_page(&page, &wifi(&[], false)), WifiPage::Connect);
    }
}
