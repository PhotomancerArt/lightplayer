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

Take-over is not a button on the card. Studio knows from the holder's
claim; Connect is itself the take-over, and its tint is the warning (see
the Amendment 2026-10-09: how a port gets one holder tab).

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

- **`2026-09-09-studio-device-stack-over-a-virtual-serial-port`** (note
  dated 2026-10-09): a second page may install with `?emu-second-tab=1`,
  holding a board's bytes and not its cable, for the one-tab-holds-a-board
  walk. The door's one client per endpoint and rule 1 are unchanged.

## Amendment 2026-10-09: how the page is built

Section 1 stands. This is how the home page is built (plan
`lp2025/2026-10-08-2050-one-home-page`), and where the build departs from
the spike. It is not a new decision.

- **The page's data is core's.** `UiHomeView.sections` holds it: the online
  and offline boards, the Connect a board offers, Other projects, Projects,
  Your patterns (as `prj…` uids), and `newcomer` (no board of any kind and no
  library project of any kind: a first visit, with no tabs, no switch, and
  Connect a board first with its one hint). `UiHomeTab::shows` is the tab
  rule, the one table of which tab shows which section. The web draws and
  decides nothing about which board or project sits in which section.
- **The sections come from the board↔project join** (`BoardProjects`). A
  project is under Other projects when no board plays it, and a project an
  offline board plays is on a board. The Online/Offline split is the
  roster's own status.
- **The Projects tab lists every library project,** so a board's project,
  even an offline board's, keeps Rename, Duplicate, Download and Delete and
  still opens on a sim, each saying which boards play it ("On Desk C6").
  **Other projects** (the All tab) lists only the unattached.
- **Tabs are view state, not offers.** So are the cards/list switch and the
  "Unlocking your boards" fold: no `UiAction`, not in the offer tree. The
  switch is remembered in `localStorage` under `lp.home.view.v1` (`"cards"`
  or `"list"`; anything else reads as cards; every access is inside a
  try/catch). The tab is not remembered.
- **The top bar has no Devices or Projects tab;** the logo is Home's tab.
  `/devices`, `/projects`, `/home`, and a bare `/device` or junk under it,
  parse as Home and heal to `/` with the page's query kept (every flag, not
  `?on=`). The route, site-section and page variants for the two are gone,
  and "Back to devices" is "Back home".
- **`StudioShell`'s no-editor arm draws the same page,** because that arm
  also serves a cold `/device/<uid>` load (the cards are the connect
  evidence) and a lens detaching.
- **`BoardCardSlot` is the one place a board's card is mounted,** keyed by
  the section entry, so the board card (the roadmap's card milestone) swaps
  one body.

**Deliberate differences from the spike:**

- no stage (Q28);
- the tabs are All · Boards · Projects · Patterns, not group tabs (Q31);
- Network opens an inline address row under the squares, as the spike drew
  it; the vision's Q12 said "a small sheet", so the page's visual gate asks;
- patterns draw as cards;
- "Unlocking your boards" is a closed fold under the boards;
- a newcomer sees one quiet project add row (Other projects is only New ·
  Import · Paste);
- the sign-in line reads "Sign in to unlock your boards from any browser."
  because what an account keeps today is the key that unlocks a board in any
  browser; the account's own board list is step 2.

## Amendment 2026-10-09: how the card is built

Sections 2 and 3 stand. This is how the card was built (plan
`lp2025/2026-10-08-2050-the-board-card`), with the director's rulings on
where this record and the build disagreed. It is not a new decision.

- **Unlock is not a drop in `lint-web-actions`** (§3 said it was). That
  ratchet does not count access commands, and today's card was not in it.
  The proof that the web no longer builds `AccessCommand::LogIn` is a core
  test that presses `devices/<board ref>/unlock` by path, and
  `git grep "AccessCommand::LogIn" lp-app/lpa-studio-web/src` coming back
  empty. Widening the ratchet to access commands is separate work.
- **Edit is the interim primary.** Until Connect (the lens in the card) and
  Done land, the primary on a ready board that runs a project is **Edit**,
  a new offer, `devices/<board ref>/edit` (`RuntimeOp::OpenDeviceLens`). It
  is never "Connect" meaning the editor. When Connect lands it becomes the
  primary and Edit moves to the project bar, as §2 says. Today's "Open in
  editor" was a link, so a cmd-click to a new tab is lost with it.
- **The project bar's empty words are "Nothing on it yet",** with "Add a
  project" as its action, not "Nothing loaded".
- **A board reached through lightplayer.app reads "Wi‑Fi via
  lightplayer.app"** on its connection bar, the link's own label, with the
  cloud icon (not "Cloud · live").
- **"also cloud" / "direct only" show only once the board has said** its
  Cloud relay setting. Before that the connection bar says nothing about
  the cloud, so an unread board is never called "also cloud".
- **The hardware bar of a stand-in reads "Emulated XIAO ESP32-C6"** (or
  "Simulated …") with "in this tab" as its aside; its speed or tier is in
  the details.
- **The LED count is left out.** Nothing the card reads knows a project's
  LED count (and today's card never showed it), so the hardware details do
  not carry it yet.
- **The firmware bar says the version alone, whatever it is:** a release's
  `2026.10.08-9`, a dev build's `dev 5eb70a7`, or the label the board said
  hello with.
- **The layout question is the firmware bar's details,** opened by core
  (`raised`) while the question is open, holding the question, its
  Download backup, Cancel and Continue. The page-level overlay is gone.
  Continue acts on one press there: the details are the question.
- **A bar's Done and Failed are watched in studio core** (`ActivityEnds`,
  read off the device journal), with no change to the device model: Done
  shows for about three seconds, Failed until the next activity replaces
  it.
- **One popover at a time.** A pick in a bar's details (the project pick,
  the board pick) is a row there; pressing it closes the details and opens
  the picker over the same bar.

## Amendment 2026-10-09: how a port gets one holder tab (M5)

Section 5 stands. This is how it was built (plan
`lp2025/2026-10-08-2330-one-tab-holds-a-board`), and where the build
departs from the section. It is not a new decision.

The acceptance, in Yona's words of 2026-10-09:

> One tab of this browser holds each board, over USB or Wi‑Fi. Other tabs
> show its last picture and that another tab has it, and Connect there
> takes it over. When someone else holds a board's Wi‑Fi connection, the
> card says "Someone else connected" and offers Retry; taking a board over
> from another person is sharing's question (step 3).

- **The key.** The browser gives no serial number before a port opens
  (Web Serial's `getInfo()` is vendor and product only), so the board's
  lock cannot be named before its port is open. The lock is
  `lp-board:usb:<vid>:<pid>:<mac>` (lowercase hex, the MAC without
  colons), taken **after the hello** has said the MAC. The OS's own
  exclusive `open()` is the mutex; the lock is the holder's name and its
  liveness (a crashed tab's lock vanishes). The first design, a lock named
  by the board before the port opens, was not possible. Two others were
  weighed and left: a pre-open lock keyed by vendor:product and the port's
  index in `getPorts()` (the order is unspecified, and a wrong index lets
  go of the wrong board), and WebUSB serial numbers (a second permission
  prompt, and nothing for the CH340 bridge).
- **The order.** Claim: open the port, hello, take the lock, announce.
  Release: close the port, release the lock, announce.
- **The channel.** Tabs of one browser tell each other on a
  `BroadcastChannel` named `lp-board-holds`. Each message is a JSON
  envelope with `v: 1`, the sender's tab id and one note (`holds`, `gone`,
  `ask`, `answer`, `who`). A message of another version, malformed text,
  or the tab's own echo is ignored: lightplayer.app redeploys daily and a
  tab can live for days, so a stale tab and a fresh one share the channel.
  Nothing is persisted, and there is no compatibility code.
- **Take-over.** One offer, `devices/<board ref>/take-over`, drawn as
  **Connect**. It is Routine when the holder only watches, **Undoable**
  when the holder has the editor open (or has not said), and disabled with
  its reason ("Busy in the other tab: <what it is doing>") when the holder
  is flashing, updating or pushing. The asker waits 5 s for an answer
  ("That tab didn't answer", with Retry). The holder closes its editor,
  writes its last picture, closes the port, releases the lock, then says
  so. The offer names the board by its MAC and never pairs ports: the
  asker opens the ports it was refused, and the one that opens is that
  board.
- **No tab opens a port on its own.** When a holder lets go or dies, the
  other tabs' fact clears and the card offers the ordinary Connect.
  Closing a tab must not make another tab grab a port that a flashing tool
  was about to use.
- **The network slot.** The board takes its one network slot silently for
  a newcomer that proves the holder's key, and closes the holder with an
  ordinary close; it refuses anyone else. Every tab of one browser
  presents the same keys, so the slot is held the same way (the lock
  `lp-board:net:<mac>`), the offer exists only where the holder is another
  tab of this browser, and a tab that hears another tab take its network
  board closes its own session by request instead of redialling. This
  replaces section 5's third bullet, which read the firmware as offering a
  take-over. A refusal from anyone else stays **Someone else connected**,
  with Retry and nothing more; taking a board over from another person is
  sharing's question (vision Q20). An older redial loop between two
  clients with one key (a tab and lp-cli, or two browsers) is filed
  (`docs/defects/2026-10-09-two-clients-with-one-key-take-a-boards-network-slot-from-each-other.md`).
- **Pictures.** A tab that holds a board writes its picture to the
  library; another tab's card shows it, dimmed, with its age. A newer
  sidecar frame replaces one that is not live, and a holder writes a final
  frame when it lets go.
- **Held boards are Online.** A board another tab holds is plugged in and
  running, so it sits under Online boards (`split_roster` counts the fact
  as connected), not Offline boards.
- **What the emulator proves, and does not.** `just walk-two-tabs-emu`
  walks two tabs of one headless Chrome against one emulated board: the
  hold, the card, the take-over both ways, and a holder that closes. The
  shim's `?emu-second-tab=1` gives the second page the board's bytes and
  not its cable (the door admits one `/control` client). It does **not**
  prove Chrome's real exclusive `open()` across tabs (the door's
  one-client rule stands in for it), how a long-hidden holder behaves
  (both pages are visible), or a taker with a cable. The network take-over
  is proved by core tests with scripted LAN and relay sources, not by a
  two-tab Wi‑Fi walk. No claim here is a hardware claim. The check on a
  real Chrome and a leased board is queued, and it does not block merging.
- **The chooser path, found by the walk.** A port granted at load is read
  against the other tabs' claims and attached without being opened. A port
  picked in the chooser is registered without that gate (it may be a
  different board), so a tab that picks a port another tab holds asks for
  it once, is refused, and reads the refusal against the claims after the
  identify deadline. The card shows a second "new device" card for the
  board until then. Filed in `docs/defects/`; no behaviour changed here.
  *Fixed 2026-10-10:* the refusal is read against the claims when it
  arrives, so the port sits on the board's card at once; and a take-over
  whose holder let go opens every port of its kind the tab could not open,
  once each, including a port whose refusal is heard after the release. A
  holder that lets go by itself still opens nothing here. The pick still
  asks for the port (the chooser stays ungated); see
  `docs/defects/2026-10-09-a-chooser-pick-of-a-held-usb-port-is-not-gated.md`.
