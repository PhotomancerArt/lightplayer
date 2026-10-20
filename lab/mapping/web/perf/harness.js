// The lab page's timing harness: paste into the page's console (or inject
// it with a browser tool), then `await labPerf.suite("radiance-dome")`.
//
// It drives the page with synthetic input, one event per animation frame,
// the way a hand on a trackpad does, and reports the time between painted
// frames (rAF timestamps): a page that keeps up shows ~16.7 ms on a 60 Hz
// display; anything above that is work the page could not fit in a frame.
// "open" is the time from choosing a project to its first painted frame.
//
// Not a gate: a desk measurement, in a visible tab (a hidden one throttles
// rAF to nothing).
window.labPerf = (() => {
  const raf = () => new Promise((r) => requestAnimationFrame(r));
  const canvas = () => document.querySelector(".canvas");
  const box = () => canvas().getBoundingClientRect();

  function stats(ts) {
    const s = [...ts].sort((a, b) => a - b);
    const q = (p) => s[Math.min(s.length - 1, Math.floor(p * s.length))];
    const r = (x) => Math.round(x * 10) / 10;
    return { n: s.length, median: r(q(0.5)), p90: r(q(0.9)), max: r(s[s.length - 1]) };
  }

  // One event per frame for `n` frames; the gaps between painted frames.
  async function drive(n, step) {
    const gaps = [];
    let last = await raf();
    for (let i = 0; i < n; i++) {
      step(i);
      const t = await raf();
      gaps.push(t - last);
      last = t;
    }
    // The last event's frame.
    await raf();
    return stats(gaps);
  }

  function move(x, y, buttons = 0) {
    const b = box();
    window.dispatchEvent(
      new MouseEvent("mousemove", { clientX: b.left + x, clientY: b.top + y, buttons, bubbles: true }),
    );
  }

  function wheel(x, y, dx, dy, ctrl = false) {
    const b = box();
    canvas().dispatchEvent(
      new WheelEvent("wheel", {
        clientX: b.left + x, clientY: b.top + y, deltaX: dx, deltaY: dy,
        deltaMode: 0, ctrlKey: ctrl, bubbles: true, cancelable: true,
      }),
    );
  }

  function press(x, y, button = 0, type = "mousedown") {
    const b = box();
    const target = type === "mousedown" ? canvas() : window;
    target.dispatchEvent(
      new MouseEvent(type, {
        clientX: b.left + x, clientY: b.top + y, button,
        buttons: type === "mousedown" ? 1 << (button === 1 ? 2 : button) : 0, bubbles: true,
      }),
    );
  }

  // Time from an action to its painted frame.
  async function timed(action) {
    await raf();
    const t0 = performance.now();
    action();
    await raf();
    await raf();
    return Math.round((performance.now() - t0) * 10) / 10;
  }

  async function open(key) {
    const sel = document.querySelector(".fixture-picker select");
    return timed(() => {
      sel.value = key;
      sel.dispatchEvent(new Event("change", { bubbles: true }));
    });
  }

  async function idle(n) {
    return drive(n, () => {});
  }

  async function hover(n) {
    const b = box();
    return drive(n, (i) => {
      const t = i / n;
      move(b.width * (0.2 + 0.6 * t), b.height * (0.3 + 0.4 * Math.sin(t * 6.28)));
    });
  }

  async function pan(n) {
    const b = box();
    return drive(n, (i) => wheel(b.width / 2, b.height / 2, i % 2 ? 6 : -6, 3));
  }

  async function zoom(n, inward = true) {
    const b = box();
    return drive(n, () => wheel(b.width / 2, b.height / 2, 0, inward ? -8 : 8, true));
  }

  async function click(x, y) {
    return timed(() => {
      press(x, y, 0, "mousedown");
      press(x, y, 0, "mouseup");
    });
  }

  async function treeClick(label) {
    const row = [...document.querySelectorAll(".tree .row")].find(
      (r) => r.textContent.trim() === label,
    );
    if (!row) return null;
    return timed(() => row.click());
  }

  async function key(k, shiftKey = false) {
    return timed(() => {
      const lab = document.querySelector(".lab");
      lab.dispatchEvent(new KeyboardEvent("keydown", { key: k, shiftKey, bubbles: true }));
      lab.dispatchEvent(new KeyboardEvent("keyup", { key: k, shiftKey, bubbles: true }));
    });
  }

  // The whole walk on one project, at the whole view and zoomed in.
  async function suite(project, { frames = 60, repeat = null } = {}) {
    const out = { project };
    out.open = await open(project);
    out.elements = document.querySelectorAll(".canvas *").length;
    out.idle = await idle(30);
    out.hoverWhole = await hover(frames);
    out.panWhole = await pan(frames);
    out.zoomIn = await zoom(25, true);
    out.elementsZoomed = document.querySelectorAll(".canvas *").length;
    out.hoverZoomed = await hover(frames);
    out.panZoomed = await pan(frames);
    out.zoomOut = await zoom(25, false);
    out.fit = await key("!", true);
    const b = box();
    out.clickSelect = await click(b.width / 2, b.height / 2);
    out.animateSelected = await idle(frames);
    if (repeat) {
      out.selectRepeat = await treeClick(repeat);
      out.animateRepeat = await idle(frames);
      out.hoverWithRepeat = await hover(frames);
    }
    out.escape = await key("Escape");
    return out;
  }

  return { raf, drive, stats, move, wheel, press, timed, open, idle, hover, pan, zoom, click, treeClick, key, suite };
})();
"labPerf ready";
