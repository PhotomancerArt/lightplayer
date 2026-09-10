// The shim's page chrome: the picker that stands where Chrome's chooser was,
// and the dev banner that says out loud what this page is.
//
// Both exist ONLY under `?emu=<url>` (M3), both are DOM, and both belong to
// the page rather than to Studio. That placement is the plan's premise, not a
// convenience:
//
//  * The picker is `navigator.serial.requestPort()`'s answer. Under the shim
//    there is no browser chooser to show, and `requestPort` is called from
//    exactly one place — `browser_esp32_device_controller.js:41` — which must
//    not change (the inviolable invariant). So the page draws the chooser at
//    the same seam Chrome draws its own, and the call site never learns that
//    anything is different. Studio's own Devices picker is a different thing
//    with a different meaning (the device roster); giving it a second meaning
//    is exactly the change this plan says must not happen.
//
//  * The banner is honesty at PAGE level (PD12 / OQ2). Under `?emu=` a device
//    card is deliberately indistinguishable from a real board's — that is the
//    claim being tested — so the place to be honest is the page, once, naming
//    the shim and the URL behind it. A card-level dress would be a behaviour
//    difference, and the sibling's mode A already owns honest dress.
//
// Neither file below this one imports this one: `virtual_serial.js` takes the
// picker as a plain `(candidates) => Promise<boardId | null>` callback and has
// no DOM in it at all, which is what lets the conformance suite run the whole
// polyfill with no page chrome at all.

const PALETTE = {
  ink: "#e8edf3",
  dim: "#93a1b0",
  edge: "#2a3340",
  panel: "#161b22",
  raised: "#1e242d",
  accent: "#7aa2f7",
  warn: "#f0b429",
};

const FONT = "12px/1.45 ui-monospace, SFMono-Regular, Menlo, monospace";

/// The chooser. Returns `(candidates) => Promise<boardId | null>`, which is
/// the shape `createBus({ picker })` wants: a board id, or null for "closed
/// with nothing" — which the polyfill turns into `NotFoundError`, the same
/// rejection Chrome gives a cancelled chooser.
export function createPicker({ backingUrl = "" } = {}) {
  return (candidates) => choose(candidates, backingUrl);
}

function choose(candidates, backingUrl) {
  return new Promise((resolve) => {
    const overlay = document.createElement("div");
    overlay.id = "lp-emu-picker";
    overlay.setAttribute("role", "dialog");
    overlay.setAttribute("aria-modal", "true");
    overlay.setAttribute("aria-label", "Choose an emulated board");
    style(overlay, {
      position: "fixed",
      inset: "0",
      zIndex: "10001",
      display: "grid",
      placeItems: "center",
      background: "rgba(6, 9, 13, 0.72)",
      font: FONT,
      color: PALETTE.ink,
    });

    const panel = document.createElement("div");
    style(panel, {
      width: "min(460px, calc(100vw - 32px))",
      background: PALETTE.panel,
      border: `1px solid ${PALETTE.edge}`,
      borderRadius: "10px",
      boxShadow: "0 18px 48px rgba(0, 0, 0, 0.55)",
      overflow: "hidden",
    });

    const header = document.createElement("div");
    style(header, { padding: "14px 16px 10px", borderBottom: `1px solid ${PALETTE.edge}` });
    const title = document.createElement("div");
    title.textContent = `${location.host} wants to connect to a serial port`;
    style(title, { fontSize: "13px", fontWeight: "600" });
    const subtitle = document.createElement("div");
    // The picker says what it is. The card it leads to will not, by design.
    subtitle.textContent = `Emulated boards on ${backingUrl || "the emulator"}`;
    style(subtitle, { marginTop: "4px", color: PALETTE.dim });
    header.append(title, subtitle);

    const list = document.createElement("div");
    list.id = "lp-emu-picker-list";
    style(list, { maxHeight: "50vh", overflowY: "auto", padding: "6px" });

    let settled = false;
    const finish = (value) => {
      if (settled) {
        return;
      }
      settled = true;
      document.removeEventListener("keydown", onKey, true);
      overlay.remove();
      resolve(value);
    };

    for (const board of candidates) {
      list.append(row(board, () => finish(board.boardId)));
    }

    const footer = document.createElement("div");
    style(footer, {
      display: "flex",
      justifyContent: "flex-end",
      gap: "8px",
      padding: "10px 12px",
      borderTop: `1px solid ${PALETTE.edge}`,
    });
    const cancel = document.createElement("button");
    cancel.type = "button";
    cancel.id = "lp-emu-picker-cancel";
    cancel.textContent = "Cancel";
    style(cancel, buttonStyle());
    cancel.addEventListener("click", () => finish(null));
    footer.append(cancel);

    const onKey = (event) => {
      if (event.key === "Escape") {
        event.stopPropagation();
        finish(null);
      }
    };
    document.addEventListener("keydown", onKey, true);
    overlay.addEventListener("click", (event) => {
      if (event.target === overlay) {
        finish(null);
      }
    });

    panel.append(header, list, footer);
    overlay.append(panel);
    document.body.append(overlay);
    cancel.focus();
  });
}

function row(board, onPick) {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "lp-emu-picker-board";
  button.dataset.boardId = board.boardId;
  style(button, {
    display: "block",
    width: "100%",
    textAlign: "left",
    padding: "10px 12px",
    margin: "2px 0",
    border: "1px solid transparent",
    borderRadius: "7px",
    background: "transparent",
    color: PALETTE.ink,
    font: FONT,
    cursor: "pointer",
  });
  button.addEventListener("mouseenter", () => {
    button.style.background = PALETTE.raised;
    button.style.borderColor = PALETTE.edge;
  });
  button.addEventListener("mouseleave", () => {
    button.style.background = "transparent";
    button.style.borderColor = "transparent";
  });
  button.addEventListener("click", onPick);

  const name = document.createElement("div");
  // The same label Studio's own `labelForPort` builds for a native-USB ESP32,
  // with the board id after it — the emulator's ids are the thing a person
  // picking between two boards actually reads.
  name.textContent = `ESP32 Serial (303a:1001) — ${board.boardId}`;
  style(name, { fontWeight: "600" });

  const detail = document.createElement("div");
  detail.textContent = [
    board.mac ? `MAC ${board.mac}` : null,
    board.chip,
    board.flash ? `flash ${board.flash}` : null,
    board.granted ? "granted" : null,
  ]
    .filter(Boolean)
    .join("  ·  ");
  style(detail, { marginTop: "3px", color: PALETTE.dim });

  button.append(name, detail);
  return button;
}

/// The page-level dev banner. Names the shim and the backing URL, lists the
/// boards the bus is holding, and carries the one control a person watching
/// needs that Studio deliberately cannot offer: the cable.
///
/// `attach`/`detach` are here rather than in Studio because they are the
/// EMULATOR's verbs — a cable going in and out — and because the control
/// channel admits one client per board (the door answers a second with 409),
/// and that client is this page. Pressing them dispatches `connect` /
/// `disconnect` on the bus, which is the same edge Chrome fires on a replug.
export function installDevBanner({ bus, backingUrl, facade = null }) {
  const banner = document.createElement("div");
  banner.id = "lp-emu-banner";
  style(banner, {
    position: "fixed",
    left: "12px",
    bottom: "12px",
    zIndex: "10000",
    maxWidth: "min(520px, calc(100vw - 24px))",
    padding: "8px 10px",
    background: PALETTE.panel,
    border: `1px solid ${PALETTE.edge}`,
    borderLeft: `3px solid ${PALETTE.warn}`,
    borderRadius: "8px",
    boxShadow: "0 10px 30px rgba(0, 0, 0, 0.45)",
    font: FONT,
    color: PALETTE.ink,
  });

  const head = document.createElement("div");
  style(head, { display: "flex", alignItems: "baseline", gap: "8px" });

  const label = document.createElement("span");
  label.textContent = "EMULATED";
  style(label, { color: PALETTE.warn, fontWeight: "700", letterSpacing: "0.06em" });

  const said = document.createElement("span");
  said.id = "lp-emu-banner-text";
  said.textContent = "navigator.serial in this page is a shim";
  style(said, { color: PALETTE.dim });

  const toggle = document.createElement("button");
  toggle.type = "button";
  toggle.id = "lp-emu-banner-toggle";
  toggle.textContent = "hide";
  style(toggle, { ...buttonStyle(), marginLeft: "auto", padding: "1px 6px" });

  head.append(label, said, toggle);

  const url = document.createElement("div");
  url.id = "lp-emu-banner-url";
  url.textContent = backingUrl;
  style(url, { marginTop: "4px", color: PALETTE.accent, wordBreak: "break-all" });

  const boards = document.createElement("div");
  boards.id = "lp-emu-banner-boards";
  style(boards, { marginTop: "6px", display: "grid", gap: "4px" });

  const body = document.createElement("div");
  body.append(url, boards);

  toggle.addEventListener("click", () => {
    const hidden = body.style.display === "none";
    body.style.display = hidden ? "" : "none";
    toggle.textContent = hidden ? "hide" : "show";
  });

  const render = () => {
    boards.textContent = "";
    for (const board of bus.describeBoards()) {
      boards.append(boardRow(bus, board, render));
    }
  };
  render();

  // A re-enumeration mints a new port object, so the rows are stale after one.
  const target = facade ?? bus;
  target.addEventListener("connect", render);
  target.addEventListener("disconnect", render);
  // …and so is "closed" the moment Studio opens a port. `boardstate` is the
  // bus's own signal for that, never a Web Serial event (see `noteState`).
  bus.addEventListener("boardstate", render);

  banner.append(head, body);
  document.body.append(banner);
  return { element: banner, refresh: render };
}

function boardRow(bus, board, refresh) {
  const row = document.createElement("div");
  row.className = "lp-emu-banner-board";
  row.dataset.boardId = board.boardId;
  style(row, { display: "flex", alignItems: "center", gap: "8px" });

  const name = document.createElement("span");
  name.textContent = board.boardId;
  style(name, { fontWeight: "600" });

  // A detached board says so where a plugged-in one says whether an
  // application holds it open: with the cable out there is no port to be open.
  const state = board.attached === false ? "detached" : board.open ? "open" : "closed";
  const detail = document.createElement("span");
  detail.textContent = [board.mac, state].filter(Boolean).join("  ·  ");
  style(detail, { color: board.attached === false ? PALETTE.warn : PALETTE.dim });

  const cable = document.createElement("button");
  cable.type = "button";
  cable.className = "lp-emu-banner-detach";
  cable.dataset.boardId = board.boardId;
  cable.textContent = "detach";
  cable.disabled = board.attached === false;
  style(cable, { ...buttonStyle(), marginLeft: "auto" });

  const plug = document.createElement("button");
  plug.type = "button";
  plug.className = "lp-emu-banner-attach";
  plug.dataset.boardId = board.boardId;
  plug.textContent = "attach";
  plug.disabled = board.attached !== false;
  style(plug, buttonStyle());

  const run = async (button, work) => {
    button.disabled = true;
    try {
      await work();
    } catch (error) {
      console.warn(`[emu] ${button.textContent} ${board.boardId}:`, error);
    } finally {
      button.disabled = false;
      refresh();
    }
  };
  cable.addEventListener("click", () => run(cable, () => bus.detach(board.boardId)));
  plug.addEventListener("click", () => run(plug, () => bus.attach(board.boardId)));

  row.append(name, detail, cable, plug);
  return row;
}

function buttonStyle() {
  return {
    padding: "2px 8px",
    border: `1px solid ${PALETTE.edge}`,
    borderRadius: "5px",
    background: PALETTE.raised,
    color: PALETTE.ink,
    font: FONT,
    cursor: "pointer",
  };
}

function style(element, declarations) {
  Object.assign(element.style, declarations);
}
