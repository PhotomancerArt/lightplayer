# ADR: The board card, built in core, and one home page

- **Status:** Accepted (a direction: the roadmap's milestones build it, and
  amend this record with how where it says so)
- **Date:** 2026-10-08
- **Deciders:** Photomancer
- **Plan:** `lp2025/2026-10-06-1530-boards-and-projects-model` (the roadmap;
  this record is its groundwork step)
- **Supersedes:** section 2 ("Disconnect → disappear") of
  [2026-09-03-device-card-fixed-height-and-disconnect-disappears.md](2026-09-03-device-card-fixed-height-and-disconnect-disappears.md),
  and its zone table (section 1's rule, that a card never changes height,
  stands)
- **Amends:**
  [2026-07-24-runtime-pool.md](2026-07-24-runtime-pool.md) (going home no
  longer closes the lens),
  [2026-09-01-editor-lens-borrows-the-device-wire.md](2026-09-01-editor-lens-borrows-the-device-wire.md)
  (a card opens the lens in place),
  [2026-09-22-opening-a-board-adopts-its-project.md](2026-09-22-opening-a-board-adopts-its-project.md)
  (Connect binds or adopts; the address stays the home page until Edit),
  [2026-09-07-always-a-device-target-real-emu-sim.md](2026-09-07-always-a-device-target-real-emu-sim.md)
  (a stand-in board is a board card; its mark moves to the hardware bar)
- **Superseded by:** None
- **Spikes:** `spikes/one-home-page/index.html` (four rounds) and
  `spikes/board-card-stack/index.html` (five rounds; the page is the card as
  decided, with every ruling at its foot). `spikes/board-card/index.html` was
  the vocabulary round that fed the second.
- **Style:** `docs/style/ui.md` "The board card"; `docs/style/language.md`
  "Boards and the home page"

## Context

Studio greets you with three pages that each know part of the story: the
landing hero, `/devices` and `/projects`. A board and the project it plays
are on different pages, and nothing says which project is on which board.

The device card is `lpa-studio-web`'s `device_roster_card.rs`, 3,040 lines.
Its facts and offers already exist in core (`DeviceView` in `lpa-devices`, the
side maps of `DeviceRosterView`, the `devices/<board ref>/<verb>` offers), but
their arrangement into four zones is worked out in the web, where the app
agent cannot see it. Its Unlock button is built in the web
(`AccessCommand::LogIn`), against the agentic-control rule that every user
verb is an offer built in core
(`2026-10-01-agentic-control-offers-in-core.md`).

Watching a board already works. At load the page attaches every port it
was granted. It identifies the board and keeps the port open, and that link
carries the shared-wire conversations: the access-key sync, the Wi‑Fi
status read and the update's auto-start. It pulls the card's picture while
a card is on screen. The editor's lens is the full session. It takes the
wire exclusively, binds or adopts the project, and holds the board's
panel. There is one lens per tab, and opening it replaces the home page
with the editor.

Two `yona-ux` spikes converged on what the page and the card should be.
Yona's steer for building them: "minimize redoing things we have. for the
device cards, it should mostly be a reorganization, the core concepts
shouldn't be changing that much."

## Decision

### 1. One home page at `/`

- **It replaces the landing hero, `/devices` and `/projects`.** Those two
  addresses, and every link that names them, lead to `/`.
- **The sections, in order:**
  1. Online boards.
  2. Connect a board: three square buttons, USB · Bluetooth · Network. It is
     its own section, and looks the same to a newcomer.
  3. Offline boards.
  4. Other projects (the ones on no board).
  5. Your patterns.
  6. Then the examples, as today.
- **One card per board,** even when boards share a project.
- **Tabs** All · Boards · Projects · Patterns, with All first, until the
  account has groups.
- **A cards/list switch,** cards by default, remembered in the browser.
- **What leaves the page:**
  - the hero;
  - the logo pill;
  - the doors;
  - the chat box. The top bar's chat button stays.
- **"Board" is the UI noun.** A desktop server is not a board; revisit the
  noun when one exists.

### 2. The board card

The card's parts, top to bottom:

- **The picture.** Nothing ever covers it.
- **The status corner,** cut out of the picture's corner. It shows:
  - the worst notice's icon, or a blue dot when all is fine;
  - then the frame rate, or the picture's age.

  It opens its own details. The board's terminal lives there.
- **The name bar.** The board's name, its group or owner under it, and one
  primary action: Connect, Unlock, Install or Done. The action is a flush
  section of the bar with an icon before the word:
  - the link's icon before Connect;
  - a lock before Unlock;
  - ✓ before Done.

  The spectrum ring lights it on hover.
- **A stack of bars,** always in this order:
  - **Project:** what the board plays, and how many boards share it.
  - **Connection:** the link and its state. "USB · live", "also cloud",
    "direct only", "Offline · 2 weeks", "Sean is editing".
  - **Access:** what you can do and with which key. "You can edit · your
    account key", "Locked".
  - **Firmware:** the version alone, whose date is in it. When it is older
    than the newest, the bar turns blue and offers Update ("when it's
    back" if the board is offline).
  - **Hardware:** the board model. The LED count is the project's, in its
    details.

**The rules:**

- **The card is a fixed height.** It never changes height while it is on
  screen, which keeps the rule of the 2026-09-03 record.
- **No boxes in boxes.**
- **A notice tints its bar.**
- **A bar's action sits flush at its end.**
- **Details open as Studio's detail card,** merged with the bar that opened
  it.
- **Work in progress shows in the bar doing it.** The bar goes neutral, with
  Studio's spinner, the step and the percent, and the iridescent fill along
  its foot. When the work is done the bar is green for a few seconds. When it
  fails, the bar is striped and offers Retry. The picture and the corner
  don't change.
- **Nothing on today's card is lost.** Every fact and button moves into a
  bar's details. What goes:
  - the MAC and the firmware version shown twice;
  - the terminal on the card's face, which moves to the status corner's
    details.

**Offline boards are cards.** A board Studio remembers but cannot reach
shows as a card under Offline boards, with its last picture and that
picture's age. It no longer collapses into a line under the grid. This
supersedes section 2 of the 2026-09-03 record.

### 3. Built in core, drawn by a few generic pieces

The card is data, like the node cards. Core builds a card view model from
today's facts and offers:

- **a name bar:** the title, the place, and the primary offer;
- **bars,** each with a layer, an icon, a summary, an aside, a tone, an
  action offer, work in progress, and details.

The web draws them with a few generic pieces, reusing Studio's detail
card. The web works nothing out.

- **Every button is an offer** at `devices/<board ref>/<verb>`, pressed by
  path in core tests.
- **Unlock becomes an offer,** `devices/<board ref>/unlock`, and the web
  stops building `AccessCommand::LogIn`. That is a drop in
  `lint-web-actions`.
- **Core works out which board plays which project once,** for the card and
  the page alike. Today nothing can say which projects are on no board.

Yona: "these cards are the most important in the whole app."

### 4. Watched, connected, edit

A board on the home page is at one of three levels:

- **Watched.** The roster link the page already holds: it identifies the
  board, runs the shared-wire conversations and pulls the picture while
  the card is on screen. It reads no files and binds no project. The card
  shows the board's facts.
- **Connected.** Connect opens the editor's session (the lens) **in place,
  on the home page**. The card's bars become the board's panel: as many
  controls as fit at the card's height, drawn with Studio's existing panel
  widgets, then All controls. The play password is enough to connect.
- **Edit.** Edit is the project bar's action, for people who can edit. It
  goes to the editor on the session already open, and nothing reopens. Edit
  on a board that isn't connected connects it first. A project notice
  ("Send latest", "Show") takes Edit's place on the bar, and Edit moves
  into the project's details.

**One board is connected at a time.** Connect on a second board hands the
session over. Done closes the session, and the card shows its facts again.

**The primary action** says Connect on every board you can reach. Otherwise
it says:

- Unlock, on a locked board;
- Install, on a blank board;
- Done, on the connected board.

**The picture keeps moving while connected.** It comes from the lens's
session instead of the roster's feed. The picture's details name where it
comes from: the live link, the lens, the last one saved, or later the
cloud.

### 5. One tab holds each USB board

This is the direction; the milestone that builds it amends this record
with how.

- **One browser tab holds each USB board's port.**
- **Other tabs** show the board's last picture and say another tab has
  it. Connect there takes it over.
- **Someone else on a network board.** A C6's network slot is shared by
  the LAN and the relay. When another client already holds it, the card
  says so and offers the same take-over. The board allows a take-over
  only when the newcomer proves the holder's own key.

Take-over is not offered on the card. Studio finds out when you press
Connect, and offers it then.

### 6. No wire, persisted-format or firmware change

Building this (step 1 of the roadmap) opens no one-way door. These keep
their shapes:

- the registry;
- the saved frames;
- every `localStorage` key.

The cards/list switch adds a `localStorage` key. It is a per-browser
convenience.

## Consequences

- **`device_roster_card.rs` is deleted.** The card is mounted from one
  place on the home page. Its stories are re-pointed, and the walks that
  click its labels are updated in the same change as the labels.
- **The home page and an open lens coexist.** Today core hides the home
  view once a project loads, and going home closes the lens. Both
  change. The lens is a session the page can hold, not a page of its own.
  See the amendment to the runtime pool below.
- **The agent sees the card.** It reads the same name bar and bars the user
  does, and presses the same offers.
- **The roster's feed sends no picture over Bluetooth today** (it skips
  that link to spare airtime), so a Bluetooth card shows its last picture.
  Turning it on is a separate task, outside this record. The card shows
  whatever the feed sends.
- **Two of the vision's step-2 states are words, not yet facts:**
  - "also cloud" / "direct only" read the board's own Cloud relay setting;
  - "Sean is editing" needs the account.

  Until the account keeps the board list, the card says only what this
  browser knows.

## Alternatives considered

- **A light, panel-only connection per board, with several connected at
  once.** It would show many panels at once, but it needs a second kind of
  session beside the lens, and the pool holds one session
  (`SESSION_CAPACITY = 1`). It is later, not rejected.
- **Keep the three pages and restyle today's card.** The agent still could
  not see the card, and the derivations would stay in 3,000 lines of web
  code.
- **A stage for the connected board** (home-page spike, round 3). The card
  carries the panel, so nothing needs a stage. Dropped.
- **Take-over on the card itself** (the vision's first wording). Most of the
  time the other holder is you, in another tab. Offering take-over when
  you press Connect says the same thing without a permanent button.

## Follow-ups

These are the roadmap's other milestones, in `plan.md`:

- **The card:** the core view model, the web pieces, Unlock as an offer;
  delete the old card.
- **The home page.**
- **Connected:** the lens in place, the panel at card size, hand-over.
- **One tab holds a board:** amends section 5 with how.
- **Step 2, your boards follow you:** the account's board list, best link,
  groups, the cloud badge as a fact.
- **Pictures through the cloud:** relay protocol 2; it amends
  `2026-10-06-cloud-relay.md`.
- **Step 3, sharing:** its own ADR for remote access.
- **Editing without the board:** how a project, or an offline board's
  project, starts on an emulated or simulated board.

## Amendments to other records

Each amended record carries a reciprocal note dated 2026-10-08.

- **`2026-09-03-device-card-fixed-height-and-disconnect-disappears`:**
  - Section 1's rule stands: no card changes height while on screen.
  - Its zone table is replaced by the name bar and the bars.
  - Section 2 is superseded: an offline board is a card, with its last
    picture.
  - Section 3 (auto-name, never a MAC) stands.
- **`2026-07-24-runtime-pool`:** the web's route→Home policy no longer
  detaches the lens when the board was connected from the home page. The
  lens and the home page coexist; Done is what detaches. Capacity stays
  one.
- **`2026-09-01-editor-lens-borrows-the-device-wire`:** Connect on a card
  takes the same exclusive borrow, in place. A card verb that needs the
  wire still closes the lens first.
- **`2026-09-22-opening-a-board-adopts-its-project`:** Connect binds or
  adopts the board's project exactly as an open does. The address stays the
  home page while connected; Edit goes to the project's address.
- **`2026-09-07-always-a-device-target-real-emu-sim`:** a stand-in
  (simulated or emulated) board is a board card on the home page, and
  "Start a board here" stays in Connect a board. The runtime band's words
  move to the hardware bar. How a project starts on a stand-in from the
  home page is open (the roadmap's "Editing without the board").
