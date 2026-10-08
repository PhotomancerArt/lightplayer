// The relay lane's cloud for `walk-ota-emu --relay` (OTA M8 PR C): what
// stands in for lightplayer.app.
//
// - **A real lp-cloud-server** (mem store, dev sign-in), on a
//   `scripts/dev-port.sh` port, whose base URL is the walk's own Studio
//   origin (dev sign-in needs a localhost one).
// - **One origin, as on lightplayer.app.** The page's `/api`, `/auth` and
//   `/relay` are forwarded to it from the walk's Studio server
//   (`forwardHttp`, `forwardUpgrade`), so the signed-in session's cookie
//   rides the relay's browser leg as it does in production.
// - **The board's device leg through a forward the walk can cut.** The
//   emulated LAN's uplink carries `lightplayer.app:80` to `devicePort`, a
//   TCP forward in front of the server: `cutDeviceLeg` drops every
//   connection on it, which is the relay dropping the board mid-update (the
//   hub then ends the board's sessions, 4410, and the page redials).
// - **The account's key**, as Studio installs it on a board over USB
//   (`AccessAdd`, kind `account`, at edit): `accessAdd` is that request's
//   JSON, for the walk's chip seeding. Made-up account only.

import { spawn, execFileSync } from "node:child_process";
import { pbkdf2Sync } from "node:crypto";
import { appendFileSync } from "node:fs";
import { request as httpRequest } from "node:http";
import { connect as netConnect, createServer as createNetServer } from "node:net";

/// The walk's made-up account.
export const WALK_EMAIL = "walk-relay@example.com";
/// The cloud API's version (`lpc_cloud_api::CLOUD_API_VERSION`).
const CLOUD_API_VERSION = 5;

/// Start the cloud for a Studio served at `studioOrigin`. `binary` is a
/// built `lp-cloud-server`; its output goes to `log`. The device-leg forward
/// listens on `deviceBind:devicePort` (loopback and any free port by
/// default; a desk board on the LAN needs the Mac's address and the port its
/// `LP_RELAY_HOST` image dials).
export async function startRelayCloud({
  root,
  binary,
  studioOrigin,
  log,
  deadlineMs = 180_000,
  deviceBind = "127.0.0.1",
  devicePort = 0,
}) {
  const port = Number(execFileSync("scripts/dev-port.sh", ["walk-ota-relay-cloud"], { cwd: root, encoding: "utf8" }).trim());
  const origin = `http://127.0.0.1:${port}`;
  let text = "";
  const child = spawn(binary, [], {
    cwd: root,
    env: {
      ...process.env,
      LP_CLOUD_PORT: String(port),
      LP_CLOUD_BASE_URL: studioOrigin,
      LP_CLOUD_STORE: "mem",
      LP_CLOUD_BLOBS: "mem",
      LP_CLOUD_DEV_AUTH: "1",
      RUST_LOG: "info,lp_cloud_server::relay=debug",
    },
    stdio: ["ignore", "pipe", "pipe"],
  });
  for (const stream of [child.stdout, child.stderr]) {
    stream.on("data", (chunk) => {
      text += chunk.toString();
      appendFileSync(log, chunk);
    });
  }
  const deviceLegs = new Set();
  const forward = createNetServer((socket) => {
    const upstream = netConnect(port, "127.0.0.1");
    const pair = { socket, upstream };
    deviceLegs.add(pair);
    const drop = () => {
      deviceLegs.delete(pair);
      socket.destroy();
      upstream.destroy();
    };
    socket.on("error", drop);
    upstream.on("error", drop);
    socket.on("close", drop);
    upstream.on("close", drop);
    socket.pipe(upstream);
    upstream.pipe(socket);
  });
  await new Promise((resolve) => forward.listen(devicePort, deviceBind, resolve));
  const stop = () => {
    for (const pair of deviceLegs) {
      pair.socket.destroy();
      pair.upstream.destroy();
    }
    forward.close();
    child.kill();
  };

  const deadline = Date.now() + deadlineMs;
  for (;;) {
    try {
      if ((await fetch(`${origin}/healthz`)).ok) break;
    } catch {
      /* not up yet */
    }
    if (Date.now() > deadline || child.exitCode !== null) {
      stop();
      throw new Error(`lp-cloud-server never answered at ${origin} (its log: ${log})`);
    }
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  const login = await fetch(`${origin}/auth/dev?email=${encodeURIComponent(WALK_EMAIL)}`, { redirect: "manual" });
  const cookie = (login.headers.get("set-cookie") ?? "").match(/lp_session=([^;]+)/)?.[1];
  if (!cookie) {
    stop();
    throw new Error(`the dev sign-in gave no session (${login.status})`);
  }
  const access = await accountAccess(origin, cookie);
  return {
    origin,
    port,
    cookie,
    devicePort: forward.address().port,
    accessAdd: accessAdd(access),
    log: () => text,
    /// Drop every board's device leg now: how many there were.
    cutDeviceLeg() {
      const count = deviceLegs.size;
      for (const pair of [...deviceLegs]) {
        pair.socket.destroy();
        pair.upstream.destroy();
      }
      deviceLegs.clear();
      return count;
    },
    stop,
  };
}

/// Forward one plain request (`/api`, `/auth/…`, a `/relay/…` that is not an
/// upgrade) to the cloud at `port`.
export function forwardHttp(request, response, port) {
  const upstream = httpRequest(
    { host: "127.0.0.1", port, method: request.method, path: request.url, headers: request.headers },
    (reply) => {
      response.writeHead(reply.statusCode ?? 502, reply.headers);
      reply.pipe(response);
    },
  );
  upstream.on("error", (error) => {
    response.writeHead(502);
    response.end(String(error));
  });
  request.pipe(upstream);
}

/// Forward one WebSocket upgrade (the relay's browser leg) to the cloud at
/// `port`, byte for byte.
export function forwardUpgrade(request, socket, head, port) {
  const upstream = netConnect(port, "127.0.0.1", () => {
    const lines = [`${request.method} ${request.url} HTTP/1.1`];
    for (let at = 0; at < request.rawHeaders.length; at += 2) {
      lines.push(`${request.rawHeaders[at]}: ${request.rawHeaders[at + 1]}`);
    }
    upstream.write(`${lines.join("\r\n")}\r\n\r\n`);
    if (head?.length) upstream.write(head);
    upstream.pipe(socket);
    socket.pipe(upstream);
  });
  const drop = () => {
    socket.destroy();
    upstream.destroy();
  };
  upstream.on("error", drop);
  socket.on("error", drop);
}

/// The relay id of a board whose MAC is `mac` (`02:4c:50:…`).
export function relayId(mac) {
  return mac.replaceAll(":", "").toLowerCase();
}

/// The signed-in account's key (`GetAccountAccess`).
async function accountAccess(origin, cookie) {
  const response = await fetch(`${origin}/api`, {
    method: "POST",
    headers: { "content-type": "application/json", cookie: `lp_session=${cookie}` },
    body: JSON.stringify({ version: CLOUD_API_VERSION, request: "getAccountAccess" }),
  });
  const reply = await response.json();
  const info = reply?.result?.Ok?.accountAccessInfo;
  if (!info?.keySecret || !info?.keySalt) {
    throw new Error(`GetAccountAccess gave no account key: ${JSON.stringify(reply).slice(0, 200)}`);
  }
  return info;
}

/// The `AccessAdd` request Studio sends a board over USB for the account's
/// key: kind `account`, at edit, `K = PBKDF2-SHA256(secret, salt, 1)` (as
/// `lpc_access::SecretEntry::from_password`).
function accessAdd(info) {
  const salt = Buffer.from(info.keySalt, "base64");
  const k = pbkdf2Sync(Buffer.from(info.keySecret, "base64"), salt, 1, 32, "sha256");
  return JSON.stringify({
    accessAdd: {
      entry: {
        label: "Walk's account",
        kind: "account",
        tier: "edit",
        salt: salt.toString("base64"),
        iterations: 1,
        k: k.toString("base64"),
      },
    },
  });
}
