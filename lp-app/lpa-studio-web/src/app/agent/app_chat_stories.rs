//! Stories for the app chat window (agentic-UI roadmap M5): the header
//! button and the drawer it opens, the home page's front door, the
//! not-configured state, a transcript with edit rows, the cards (a pending
//! connect, a pending flash over somebody's firmware, a done one), and the
//! hand-off into a shader's own agent.
//!
//! Fixed transcripts for deterministic PNGs. The drawer is drawn against
//! its story frame's right edge (`inline`) rather than the viewport's.

use dioxus::prelude::*;
use lpa_studio_core::{
    AgentProvider, BluetoothReach, OfferArgs, UiAgentAvailability, UiAgentCard, UiAgentCardState,
    UiAgentEditBatch, UiAgentEditLine, UiAgentEditOutcome, UiAgentModelView, UiAgentStatus,
    UiAgentToolRow, UiAgentTurn, UiAgentUsage, UiAppAgentView, UiOfferTree, add_device_offers,
    provider_guidance,
};
use lpa_studio_web_story_macros::story;

use super::agent_card_view_stories::{agent_card, foreign_flash};
use super::{AppChatButton, AppChatDrawer, use_provide_app_chat_chrome};
use crate::app::HomePage;
use crate::app::layout::site_chrome::{SiteChrome, SiteSection};
use crate::app::node::NodePane;
use crate::app::node::face_story_fixtures::shader_node_view;
use crate::core::OffersProvider;

#[story(
    description = "The header's chat button, beside the AI settings trigger (the sparkles: the assistant's own mark). TOP: at rest. MIDDLE: the drawer is closed while the assistant is still working — a pulsing working dot. BOTTOM: the drawer is closed and a card is waiting for the user's click (connect or flash a board) — a steady attention-orange dot (the \"needs a look\" colour, distinct from the working dot's pulsing yellow), and the tooltip says so. The button toggles the drawer; its open flag is web chrome, like a popover's."
)]
fn drawer_closed() -> Element {
    rsx! {
        div { class: "tw:grid tw:gap-3",
            ChromeFrame { pending_card: false, busy: false }
            ChromeFrame { pending_card: false, busy: true }
            ChromeFrame { pending_card: true, busy: false }
        }
    }
}

#[story(
    description = "The drawer open over the page's right edge, under the site chrome, from the header button (now pressed): the bar's right cluster stays reachable, so the same button closes it and the AI settings stay in reach. It overlays rather than reflows: the workbench is a full-height frame with docks of its own, and a drawer that pushed it would rearrange the room each time. The drawer stays open across navigation — it is mounted once, outside every route's body. An empty conversation says what to ask and that the assistant puts a button in the chat when a step needs the user's click."
)]
fn drawer_open() -> Element {
    rsx! {
        PageFrame { view: ready_view(Vec::new(), UiAgentStatus::Idle) }
    }
}

#[story(
    description = "The home page's front door (A3): with no project open, the landing's first ask is a big box under the slogan — \"What do you want your lights to do?\". It is the same session as the header's drawer: Ask sends and opens the drawer, where the conversation goes on. Before a provider is set up, Ask opens the drawer on the setup state and the words stay in the box."
)]
fn home_front_door() -> Element {
    rsx! {
        ChromeProvider {
            HomePage { app_agent: Some(ready_view(Vec::new(), UiAgentStatus::Idle)) }
        }
    }
}

#[story(
    description = "Not configured: the drawer before any AI provider is set up. Never a dead end — the one-click OpenRouter Connect leads (no key to paste), then the selected provider's own setup guidance and the pointer at Settings."
)]
fn not_configured() -> Element {
    let mut view = UiAppAgentView {
        availability: UiAgentAvailability::NeedsKey,
        setup: Some(provider_guidance(AgentProvider::OpenRouter)),
        ..UiAppAgentView::default()
    };
    view.model = model();
    rsx! {
        DrawerFrame { view }
    }
}

#[story(
    description = "A transcript after building Sean's project: two `edit_project` rows, expanded. Each row's one line says what the edits amount to; its list says each edit and how it went. The FIRST batch had one edit refused — the app said the fixture cannot hold 2500 LEDs — so its dot is red, its line says \"1 rejected\", and the refused edit is spelled out with the reason. The second batch fixed it and saved. The footer shows what OpenRouter reported charging (exact, no ~)."
)]
fn seans_project() -> Element {
    rsx! {
        DrawerFrame { view: ready_view(seans_transcript(), UiAgentStatus::Idle), tool_rows_expanded: true }
    }
}

#[story(
    description = "A pending connect card. The assistant pressed `devices/connect-usb`, which needs the browser's own click (the USB chooser), so its `act` became this card: the same Connect button the Devices page draws, and the user's click on it is what opens the chooser — the card dispatches the action inside the click, so the browser sees the gesture. Dismiss is the card's own action from core."
)]
fn connect_card_pending() -> Element {
    let card = connect_card(UiAgentCardState::Pending);
    rsx! {
        OffersProvider { offers: connect_offers(),
            DrawerFrame { view: ready_view(connect_transcript(card, false), UiAgentStatus::Idle) }
        }
    }
}

#[story(
    description = "A pending flash card, pre-filled. The board runs its factory demo, so flashing LightPlayer over it loses that firmware: the press is Lasting and only the user may make it. The card draws the Flash offer's own board pick, set to the board the assistant named and the name Sean asked for; he may change either, and the press arms first (error tint), acting on the second click."
)]
fn flash_card_pending() -> Element {
    let offer = foreign_flash();
    let card = agent_card(&offer);
    let mut offers = UiOfferTree::new();
    offers.publish(offer);
    let turns = vec![
        UiAgentTurn::User {
            text: "Put it on my XIAO, and call it Shelf lamp".to_string(),
        },
        act_row("tu_9", "asked you to click card c1: devices/new-3/flash"),
        UiAgentTurn::Card(card),
        UiAgentTurn::Assistant {
            text: "Your XIAO is still running its factory demo. Click **Flash** on the card — \
                   it replaces that demo with LightPlayer."
                .to_string(),
        },
    ];
    rsx! {
        OffersProvider { offers,
            DrawerFrame { view: ready_view(turns, UiAgentStatus::Idle) }
        }
    }
}

#[story(
    description = "A done card: Sean clicked Connect, the board came up, and the card settled to what the app said. The assistant heard the click on the next run (a line the user does not see: \"I clicked … on card c1\") and carried on."
)]
fn done_card() -> Element {
    let card = connect_card(UiAgentCardState::Done {
        outcome: "XIAO ESP32-C6 connected".to_string(),
    });
    rsx! {
        OffersProvider { offers: connect_offers(),
            DrawerFrame { view: ready_view(connect_transcript(card, true), UiAgentStatus::Idle) }
        }
    }
}

#[story(
    description = "The hand-off into a shader's own agent (A6). Sean asked the assistant to make the spiral turn slower; that is shader code, so the assistant pressed the node's `project/<node>/ask-agent` offer with his words as `request`. LEFT: the shader card, focused, its agent section open with the request typed in — not sent; Sean reads it and presses Send. RIGHT: the assistant's side of it."
)]
fn hand_off() -> Element {
    let mut card = shader_node_view(false, UiAgentStatus::Idle);
    card.focused = true;
    card.card_ui.agent_collapsed = false;
    card.card_ui.composer_draft = "make the spiral turn slower".to_string();
    card.card_ui.draft_seed = 1;
    let turns = vec![
        UiAgentTurn::User {
            text: "Can you make the spiral turn slower?".to_string(),
        },
        act_row(
            "tu_12",
            "pressed project/fyeah_sign.show/aurora.shader/ask-agent",
        ),
        UiAgentTurn::Assistant {
            text: "That's in the spiral's shader code, so I've handed it to that shader's own \
                   agent — your request is typed in on the Aurora card. Press **Send** there \
                   when it reads right."
                .to_string(),
        },
    ];
    rsx! {
        div { class: "tw:flex tw:flex-wrap tw:items-start tw:gap-6",
            div { class: "tw:w-[460px] tw:max-w-full",
                NodePane { view: card, on_action: move |_| {} }
            }
            DrawerFrame { view: ready_view(turns, UiAgentStatus::Idle) }
        }
    }
}

/// A site chrome bar carrying the chat button.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ChromeFrame(pending_card: bool, busy: bool) -> Element {
    let open = use_signal(|| false);
    rsx! {
        div { class: "tw:max-w-[1000px] tw:border tw:border-dashed tw:border-border-muted tw:px-4 tw:pt-3",
            SiteChrome { section: SiteSection::Home,
                AppChatButton { open, pending_card, busy }
            }
        }
    }
}

/// A page-sized frame: the chrome on top, the drawer open over its right
/// edge.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn PageFrame(view: UiAppAgentView) -> Element {
    let open = use_signal(|| true);
    let draft = use_signal(String::new);
    rsx! {
        div { class: "tw:relative tw:h-[760px] tw:w-[1100px] tw:max-w-full tw:overflow-hidden tw:border tw:border-dashed tw:border-border-muted tw:px-4 tw:pt-3",
            SiteChrome { section: SiteSection::Projects,
                AppChatButton { open }
            }
            p { class: "tw:m-0 tw:mt-24 tw:max-w-md tw:text-sm tw:text-dim-foreground",
                "(the page under the drawer stays where it is)"
            }
            // Under the chrome, as the product's drawer sits.
            div { class: "tw:absolute tw:inset-x-0 tw:bottom-0 tw:top-[53px]",
                AppChatDrawer { view, open, draft, inline: true, on_action: move |_| {}, on_connect: move |_| {} }
            }
        }
    }
}

/// The drawer alone, at its real width and a page's height.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn DrawerFrame(
    view: UiAppAgentView,
    #[props(default = false)] tool_rows_expanded: bool,
) -> Element {
    let open = use_signal(|| true);
    let draft = use_signal(String::new);
    rsx! {
        div { class: "tw:relative tw:h-[760px] tw:w-[440px] tw:max-w-full",
            AppChatDrawer {
                view,
                open,
                draft,
                tool_rows_expanded,
                inline: true,
                on_action: move |_| {},
                on_connect: move |_| {},
            }
        }
    }
}

/// The web app's app-chat chrome, which the front door needs.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ChromeProvider(children: Element) -> Element {
    use_provide_app_chat_chrome();
    children
}

fn model() -> UiAgentModelView {
    UiAgentModelView {
        effective: Some("z-ai/glm-5.3".to_string()),
        options: Vec::new(),
        loading: false,
    }
}

fn ready_view(turns: Vec<UiAgentTurn>, status: UiAgentStatus) -> UiAppAgentView {
    let usage = if turns.is_empty() {
        UiAgentUsage::default()
    } else {
        UiAgentUsage {
            input_tokens: 4_210,
            output_tokens: 2_310,
            cache_read_tokens: 14_200,
            cost_micro_usd: Some(41_800),
            ..UiAgentUsage::default()
        }
    };
    UiAppAgentView {
        availability: UiAgentAvailability::Ready,
        setup: None,
        status,
        estimated_cost: usage.cost_micro_usd.map(|_| "$0.04".to_string()),
        turns,
        usage,
        model: model(),
    }
}

fn seans_transcript() -> Vec<UiAgentTurn> {
    let applied = |detail: &str| UiAgentEditOutcome::Applied {
        detail: detail.to_string(),
    };
    let first = UiAgentEditBatch {
        lines: vec![
            line(
                "create_node",
                "Playlist",
                None,
                None,
                None,
                applied("playlist"),
            ),
            line(
                "import_pattern",
                "spiral",
                Some("playlist"),
                None,
                None,
                applied("spiral"),
            ),
            line(
                "import_pattern",
                "embers",
                Some("playlist"),
                None,
                None,
                applied("embers"),
            ),
            line(
                "import_pattern",
                "tide",
                Some("playlist"),
                None,
                None,
                applied("tide"),
            ),
            line(
                "import_pattern",
                "aurora",
                Some("playlist"),
                None,
                None,
                applied("aurora"),
            ),
            line(
                "set",
                "fixture",
                None,
                Some("count"),
                Some("2500"),
                UiAgentEditOutcome::Rejected {
                    reason: "a fixture holds at most 1024 lamps".to_string(),
                },
            ),
            line(
                "set",
                "output",
                None,
                Some("ports[0].endpoint"),
                Some("\"ws281x:local:D6\""),
                applied("set"),
            ),
            line(
                "set_target",
                "seeed/xiao-esp32-c6",
                None,
                None,
                None,
                applied("set"),
            ),
        ],
        saved: false,
        save_error: None,
    };
    let second = UiAgentEditBatch {
        lines: vec![line(
            "set",
            "fixture",
            None,
            Some("count"),
            Some("250"),
            applied("set"),
        )],
        saved: true,
        save_error: None,
    };
    vec![
        UiAgentTurn::User {
            text: "I have a XIAO ESP32-C6 with a strip of 250 LEDs on D6. Make it cycle a few \
                   calm patterns."
                .to_string(),
        },
        UiAgentTurn::Assistant {
            text: "I'll build a playlist of four calm patterns for your strip.".to_string(),
        },
        edit_row(
            "tu_1",
            "a playlist of four calm patterns on the XIAO's D6",
            first,
        ),
        UiAgentTurn::Assistant {
            text: "I typed 2500 instead of 250 — fixing that.".to_string(),
        },
        edit_row("tu_2", "250 LEDs, then save", second),
        UiAgentTurn::Assistant {
            text: "Done: your project cycles Spiral, Embers, Tide and Aurora on 250 LEDs on D6, \
                   and it's saved. Want me to put it on the board?"
                .to_string(),
        },
    ]
}

fn line(
    verb: &str,
    target: &str,
    within: Option<&str>,
    path: Option<&str>,
    value: Option<&str>,
    outcome: UiAgentEditOutcome,
) -> UiAgentEditLine {
    UiAgentEditLine {
        verb: verb.to_string(),
        target: target.to_string(),
        within: within.map(str::to_string),
        path: path.map(str::to_string),
        value: value.map(str::to_string),
        outcome,
    }
}

fn edit_row(id: &str, note: &str, edits: UiAgentEditBatch) -> UiAgentTurn {
    UiAgentTurn::Tool(UiAgentToolRow {
        note: Some(note.to_string()),
        done: true,
        edits: Some(edits),
        ..UiAgentToolRow::started(id).for_tool("edit_project")
    })
}

fn act_row(id: &str, headline: &str) -> UiAgentTurn {
    UiAgentTurn::Tool(UiAgentToolRow {
        done: true,
        headline: Some(headline.to_string()),
        ..UiAgentToolRow::started(id).for_tool("act")
    })
}

/// The add slot's offers, `devices/connect-usb` among them, in a browser
/// with Web Serial.
fn connect_offers() -> UiOfferTree {
    let mut offers = UiOfferTree::new();
    for offer in add_device_offers(true, BluetoothReach::Ready) {
        offers.publish(offer);
    }
    offers
}

/// The card the assistant's press of `devices/connect-usb` became.
fn connect_card(state: UiAgentCardState) -> UiAgentCard {
    let offer = connect_offers()
        .iter()
        .find(|offer| offer.path.to_string() == "devices/connect-usb")
        .cloned()
        .expect("the add slot publishes connect-usb");
    let mut card = UiAgentCard::new(
        "c1",
        offer.action.clone(),
        "so I can put your project on the XIAO",
    )
    .for_offer(offer.path.clone(), OfferArgs::new());
    card.state = state;
    card
}

fn connect_transcript(card: UiAgentCard, done: bool) -> Vec<UiAgentTurn> {
    let mut turns = vec![
        UiAgentTurn::User {
            text: "Yes, put it on the board — it's plugged in.".to_string(),
        },
        act_row("tu_5", "asked you to click card c1: devices/connect-usb"),
        UiAgentTurn::Card(card),
        UiAgentTurn::Assistant {
            text: "Click **Connect a board via USB** on the card and pick your XIAO in the \
                   browser's list."
                .to_string(),
        },
    ];
    if done {
        // The resumed run hears the click as a hidden line; the user sees
        // the card settle, then the assistant carry on.
        turns.push(UiAgentTurn::Assistant {
            text: "It's connected. Sending your project to it now.".to_string(),
        });
    }
    turns
}
