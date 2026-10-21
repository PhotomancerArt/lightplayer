// A control command in flight when its channel closes (or never answers) is
// rejected, and the bus's `detach` still reaches `unplug`.
//
// `node --test lp-app/lpa-link/tests/js/` — `just lpa-link-js-test`. No
// browser, no door: a scripted WebSocket stands in for the control channel.
// The defect: `emulator_port.js` kept a command waiting forever when the
// door closed the channel under it (it does when a board stops), so a page
// `detach` never reached `unplug` and the door's client stayed open.
import assert from "node:assert/strict";
import { test } from "node:test";

import { EmulatorPort } from "../../../lpa-studio-web/public/lpa-link/emulator_port.js";
import { createBus } from "../../../lpa-studio-web/public/lpa-link/virtual_serial.js";

test("a command in flight when the control channel closes is rejected, naming channel and command", async () => {
  const { port, sockets } = await connect();
  const pending = port.command("state");
  const outcome = pending.then(() => "resolved", (error) => error);
  sockets.control.serverClose();
  const error = await outcome;
  assert.ok(error instanceof Error, "the command must reject, not stay pending");
  assert.equal(error.channelLost, true);
  assert.match(error.message, /control channel closed/);
  assert.match(error.message, /`state`/);
  assert.match(error.message, /board-a/);
});

test("every command waiting on the channel is rejected, not only the first", async () => {
  const { port, sockets } = await connect();
  const a = port.command("state").catch((e) => e);
  const b = port.command("pins").catch((e) => e);
  sockets.control.serverClose();
  const [ea, eb] = await Promise.all([a, b]);
  assert.match(ea.message, /`state`/);
  assert.match(eb.message, /`pins`/);
  await assert.rejects(port.command("state"), /control channel is not open/);
});

test("a command that gets no reply is rejected after the bounded wait, and the channel is closed", async () => {
  const { port, sockets } = await connect({ commandTimeoutMs: 20 });
  const error = await port.command("state").then(() => null, (e) => e);
  assert.ok(error, "the command must reject");
  assert.equal(error.channelLost, true);
  assert.match(error.message, /got no reply in 20 ms/);
  assert.equal(sockets.control.readyState, 3, "the door's client is released");
});

test("a reply that arrives in time settles the command and disarms its timer", async () => {
  const { port, sockets } = await connect({ commandTimeoutMs: 50 });
  const pending = port.command("state");
  sockets.control.reply("ok us=1");
  assert.equal(await pending, "ok us=1");
  await new Promise((resolve) => setTimeout(resolve, 100));
  assert.equal(sockets.control.readyState, 1, "no stale timer closed the channel");
});

test("the door's own err reply is still a plain rejection, not a lost channel", async () => {
  const { port, sockets } = await connect();
  const pending = port.command("detach").catch((e) => e);
  sockets.control.reply("err not attached");
  const error = await pending;
  assert.notEqual(error.channelLost, true);
  assert.equal(sockets.control.readyState, 1);
});

test("bus.detach with the control channel closing under the verb still unplugs the port", async () => {
  const { bus, port, sockets } = await connectBus();
  const disconnects = [];
  bus.addEventListener("disconnect", (event) => disconnects.push(event.detail.port));
  sockets.control.onSend = (line) => {
    if (line === "detach") {
      sockets.control.serverClose();
    }
  };
  const detached = await bus.detach("board-a");
  assert.equal(detached, port);
  assert.equal(port.dead, true, "the port died the way an unplugged one does");
  assert.deepEqual(disconnects, [port]);
});

test("bus.detach with a door `err` reply leaves the port plugged and rejects", async () => {
  const { bus, port, sockets } = await connectBus();
  sockets.control.onSend = (line) => {
    if (line === "detach") {
      sockets.control.reply("err refused");
    }
  };
  await assert.rejects(bus.detach("board-a"), /refused/);
  assert.notEqual(port.dead, true);
});

// --- a scripted door ------------------------------------------------------

class FakeSocket {
  static instances = null;

  constructor(url) {
    this.url = String(url);
    this.readyState = 0;
    this.listeners = new Map();
    this.onSend = null;
    const channel = /\/(bytes|control)$/.exec(new URL(this.url).pathname)?.[1];
    FakeSocket.instances[channel] = this;
    queueMicrotask(() => {
      this.readyState = 1;
      this.dispatch("open");
    });
  }

  addEventListener(type, callback, options = {}) {
    const set = this.listeners.get(type) ?? new Set();
    set.add({ callback, once: Boolean(options.once) });
    this.listeners.set(type, set);
  }

  dispatch(type, event = {}) {
    for (const entry of [...(this.listeners.get(type) ?? [])]) {
      if (entry.once) {
        this.listeners.get(type).delete(entry);
      }
      entry.callback(event);
    }
  }

  send(line) {
    this.onSend?.(String(line));
  }

  reply(line) {
    this.dispatch("message", { data: line });
  }

  /// The door closes the channel: what it does when a board stops.
  serverClose() {
    this.readyState = 3;
    this.dispatch("close");
  }

  /// Our own `close()`.
  close() {
    if (this.readyState !== 3) {
      this.serverClose();
    }
  }
}

async function connect({ commandTimeoutMs } = {}) {
  FakeSocket.instances = {};
  const port = await EmulatorPort.connect(
    "http://door.invalid/",
    "board-a",
    { id: "board-a" },
    { WebSocketImpl: FakeSocket, commandTimeoutMs },
  );
  return { port, sockets: FakeSocket.instances };
}

async function connectBus() {
  FakeSocket.instances = {};
  let port = null;
  const bus = await createBus("http://door.invalid/", {
    boards: ["board-a"],
    backing: {
      describe: () => "fake door",
      listBoards: async () => [{ id: "board-a" }],
      connect: async (boardId, board) => {
        const emulator = await EmulatorPort.connect(
          "http://door.invalid/",
          boardId,
          board,
          { WebSocketImpl: FakeSocket },
        );
        return emulator;
      },
    },
  });
  port = bus.newestPortFor("board-a");
  return { bus, port, sockets: FakeSocket.instances };
}
