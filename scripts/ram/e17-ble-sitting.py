#!/usr/bin/env python3
"""E17 (RAM research): one foreground driver for a silicon sitting with a
Bluetooth central.

It starts every process the sitting needs itself — the board's USB console
capture (`lp-cli link capture <port>`), a backgrounded Mac Chrome with a
scratch profile as the central (answered over CDP by
`spikes/ble-lab/scripts/cdp-central.mjs`), and for the pipe scenarios the
ble-lab page server and an `lp-cli link capture blepipe:` host — waits on
them with explicit timeouts, and kills them all on exit (normal, error or
signal). Nothing it starts outlives it.

Scenarios:

  studio   Studio (prod lightplayer.app, or --url) joins the board over
           Bluetooth and runs its card feed; the USB console records the
           heartbeat `memory`, `[radio-heap]`, `[ble]` lines. Phases: idle →
           connected/streaming → reload (disconnect) → after, repeated
           --cycles times. Writes marks.tsv beside the console.
  list     open the chooser on the page, print every device it offers, pick
           nothing.
  pipe     the frame pipe (`/pipe`) with an `lp-cli link capture blepipe:`
           host given --host-args (an update offer, requests…); the USB
           console records the board. Ends when the host exits or --secs.

Every phase boundary is a line in marks.tsv: wall time, seconds since the
start, the USB console's line count at that moment, the label. The parser
(`e17-ble-parse.py`) splits the console by those counts.
"""

import argparse
import json
import os
import re
import shlex
import signal
import subprocess
import sys
import time
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
CDP = REPO / "spikes/ble-lab/scripts/cdp-central.mjs"
BOARD_PORT = REPO / "scripts/emu/board-port.py"
LAB_SERVER = REPO / "spikes/ble-lab/server.py"

# Studio's home page: the Bluetooth square inside "Connect a board" (README).
STUDIO_BT = (
    '[...document.querySelectorAll("#home-connect-board button")]'
    '.find((b) => !b.disabled && b.innerText.trim() === "Bluetooth")?.click() ?? null'
)

CHILDREN = []  # Popen, each its own process group
PROFILES = []  # Chrome scratch profiles to pkill at exit
T0 = time.monotonic()


def log(msg):
    print(f"[e17 {time.monotonic() - T0:7.1f}s] {msg}", flush=True)


def spawn(argv, out, **kw):
    log(f"spawn {' '.join(shlex.quote(str(a)) for a in argv)[:300]}")
    f = open(out, "ab")
    p = subprocess.Popen([str(a) for a in argv], stdout=f, stderr=subprocess.STDOUT, start_new_session=True, **kw)
    CHILDREN.append(p)
    return p


def stop(p, sig=signal.SIGINT, wait=8):
    if p.poll() is not None:
        return p.returncode
    for s in (sig, signal.SIGTERM, signal.SIGKILL):
        try:
            os.killpg(p.pid, s)
        except ProcessLookupError:
            return p.returncode
        try:
            return p.wait(timeout=wait)
        except subprocess.TimeoutExpired:
            continue
    return p.returncode


def kill_chrome(profile):
    subprocess.run(["pkill", "-f", f"user-data-dir={profile}"], check=False)
    for _ in range(40):
        if subprocess.run(["pgrep", "-f", f"user-data-dir={profile}"], capture_output=True).returncode != 0:
            return
        time.sleep(0.25)
    subprocess.run(["pkill", "-9", "-f", f"user-data-dir={profile}"], check=False)


def cleanup():
    for p in reversed(CHILDREN):
        stop(p)
    for prof in PROFILES:
        kill_chrome(prof)
    log("cleanup done: every child stopped")


def on_signal(signum, _frame):
    log(f"signal {signum}: stopping")
    sys.exit(128 + signum)


class Console:
    """The USB console file the capture writes, read as it grows."""

    def __init__(self, path):
        self.path = Path(path)

    def lines(self):
        try:
            return self.path.read_bytes().decode("utf-8", "replace").splitlines()
        except FileNotFoundError:
            return []

    def count(self):
        return len(self.lines())

    def wait(self, pattern, timeout, since=0, what=None):
        rx = re.compile(pattern)
        end = time.monotonic() + timeout
        while time.monotonic() < end:
            ls = self.lines()
            for i in range(since, len(ls)):
                if rx.search(ls[i]):
                    return i, ls[i]
            time.sleep(0.5)
        log(f"timeout ({timeout}s) waiting for {what or pattern}")
        return None, None


class Marks:
    def __init__(self, path, console):
        self.f = open(path, "a")
        self.console = console

    def mark(self, label):
        n = self.console.count()
        self.f.write(f"{time.strftime('%Y-%m-%dT%H:%M:%S')}\t{time.monotonic() - T0:.1f}\t{n}\t{label}\n")
        self.f.flush()
        log(f"MARK {label} (console line {n})")


def board_port(mac):
    r = subprocess.run(["python3", str(BOARD_PORT), mac], capture_output=True, text=True, timeout=20)
    return r.stdout.strip()


def start_usb(args, out):
    port = board_port(args.mac)
    if not port:
        raise SystemExit("no port for the board's MAC")
    cmd = [args.lpcli, "link", "capture", port, "--console", out / "usb.txt", "--seconds", str(args.usb_secs)]
    for r in args.usb_request or []:
        cmd += ["--request", r]
    p = spawn(cmd, out / "usb-lpcli.log")
    return p, Console(out / "usb.txt")


def launch_chrome(args, out, url):
    prof = Path(args.profile)
    prof.mkdir(parents=True, exist_ok=True)
    if prof not in PROFILES:
        PROFILES.append(prof)
    dap = prof / "DevToolsActivePort"
    if dap.exists():
        dap.unlink()
    log(f"chrome (background) → {url}")
    subprocess.run(
        ["open", "-g", "-n", "-a", "Google Chrome", "--args", "--remote-debugging-port=0",
         f"--user-data-dir={prof}", "--no-first-run", "--no-default-browser-check", url],
        check=True, timeout=30,
    )
    for _ in range(120):
        if dap.exists() and dap.read_text().strip():
            port = int(dap.read_text().splitlines()[0])
            log(f"chrome debugging port {port}")
            return port
        time.sleep(0.25)
    raise SystemExit("Chrome never wrote DevToolsActivePort")


def cdp(port, page, *cmd, timeout=90, out=None):
    argv = ["node", str(CDP), "--debug-port", str(port), "--page", page, *cmd]
    log(f"cdp {' '.join(shlex.quote(c) for c in cmd)[:200]}")
    try:
        r = subprocess.run(argv, capture_output=True, text=True, timeout=timeout)
    except subprocess.TimeoutExpired:
        log("cdp: timed out")
        return None, "timeout"
    if out:
        with open(out, "a") as f:
            f.write(f"$ {' '.join(cmd)}\n{r.stdout}{r.stderr}\n")
    if r.returncode != 0:
        log(f"cdp rc={r.returncode}: {r.stderr.strip()[:300]}")
        return None, r.stderr.strip()
    try:
        return json.loads(r.stdout.strip().splitlines()[-1]), None
    except (json.JSONDecodeError, IndexError):
        return r.stdout.strip(), None


def wait_page(port, page, timeout=60):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        v, err = cdp(port, page, "js", "document.readyState", timeout=15)
        if v == "complete":
            return True
        time.sleep(1)
    return False


def pick_id(listed, name_match):
    hits = [d for d in listed if name_match.lower() in (d.get("name") or "").lower()]
    if len(hits) == 1:
        return hits[0]["id"]
    log(f"chooser offered {listed}; {len(hits)} match '{name_match}'")
    return None


def wait_studio_ready(port, page, timeout=90):
    """Studio is a wasm app: wait until its Connect-a-board square exists."""
    expr = '!![...document.querySelectorAll("#home-connect-board button")].find((b) => !b.disabled && b.innerText.trim() === "Bluetooth")'
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        v, _ = cdp(port, page, "js", expr, timeout=15)
        if v is True:
            return True
        time.sleep(2)
    return False


def join_studio(args, out, port, console, marks, cycle):
    if not wait_studio_ready(port, args.page):
        log("Studio's Bluetooth square never appeared")
        return False
    dev = args.id
    if not dev:
        listed, err = cdp(port, args.page, "list", "--click-expr", STUDIO_BT, "--timeout-ms", "10000",
                          timeout=40, out=out / "cdp.log")
        if not listed:
            return False
        (out / "chooser.json").write_text(json.dumps(listed, indent=1))
        dev = pick_id(listed, args.name_match)
        if not dev:
            return False
        args.id = dev
        time.sleep(2)
    since = console.count()
    marks.mark(f"c{cycle}:join")
    res, err = cdp(port, args.page, "join", "--id", dev, "--click-expr", STUDIO_BT, "--timeout-ms", "45000",
                   timeout=80, out=out / "cdp.log")
    if not res:
        return False
    i, line = console.wait(r"\[ble\] link\d+: notifications on", 40, since, "link open")
    if i is None:
        return False
    marks.mark(f"c{cycle}:link-open")
    return True


def scenario_studio(args, out):
    usb, console = start_usb(args, out)
    marks = Marks(out / "marks.tsv", console)
    i, _ = console.wait(r"\[link\] up|heartbeat", 40, 0, "USB link up")
    if i is None:
        return 2
    marks.mark("usb-up")
    log(f"idle {args.idle}s (no central)")
    time.sleep(args.idle)
    marks.mark("idle-end")
    port = launch_chrome(args, out, args.url)
    if not wait_page(port, args.page):
        log("page never loaded")
        return 3
    rc = 0
    for c in range(1, args.cycles + 1):
        if not join_studio(args, out, port, console, marks, c):
            marks.mark(f"c{c}:join-failed")
            rc = 4
            break
        cdp(port, args.page, "shot", str(out / f"c{c}-connected.png"), timeout=30)
        log(f"streaming {args.stream}s (Studio's card feed)")
        half = args.stream // 2
        time.sleep(half)
        cdp(port, args.page, "shot", str(out / f"c{c}-streaming.png"), timeout=30)
        time.sleep(args.stream - half)
        marks.mark(f"c{c}:stream-end")
        since = console.count()
        if args.disconnect == "reload":
            cdp(port, args.page, "js", "location.reload(); 1", timeout=20)
        else:
            kill_chrome(Path(args.profile))
        i, _ = console.wait(r"\[ble\] link\d+: disconnected", 60, since, "disconnected")
        marks.mark(f"c{c}:disconnected" if i is not None else f"c{c}:disconnect-unseen")
        log(f"after {args.after}s")
        time.sleep(args.after)
        marks.mark(f"c{c}:after-end")
        if args.disconnect == "kill" and c < args.cycles:
            port = launch_chrome(args, out, args.url)
            wait_page(port, args.page)
        elif args.disconnect == "reload":
            wait_page(port, args.page)
    stop(usb)
    marks.mark("end")
    return rc


def scenario_list(args, out):
    port = launch_chrome(args, out, args.url)
    wait_page(port, args.page)
    press = ["--click-expr", STUDIO_BT] if "lightplayer" in args.url or args.studio else []
    if press:
        wait_studio_ready(port, args.page)
    listed, err = cdp(port, args.page, "list", *press, "--timeout-ms", "10000", timeout=40, out=out / "cdp.log")
    log(f"chooser: {listed or err}")
    (out / "chooser.json").write_text(json.dumps(listed, indent=1))
    return 0 if listed else 1


def scenario_pipe(args, out):
    usb, console = start_usb(args, out) if not args.no_usb else (None, Console(out / "usb.txt"))
    marks = Marks(out / "marks.tsv", console)
    lab_port = int(subprocess.run([str(REPO / "scripts/dev-port.sh"), "ble-lab-e17"], capture_output=True,
                                  text=True, timeout=20).stdout.strip())
    pipe_port = int(subprocess.run([str(REPO / "scripts/dev-port.sh"), "ble-pipe-e17"], capture_output=True,
                                   text=True, timeout=20).stdout.strip())
    env = dict(os.environ, BLE_LAB_PORT=str(lab_port))
    spawn(["python3", "-u", LAB_SERVER], out / "lab-server.log", env=env, cwd=REPO)
    host_cmd = [args.lpcli, "link", "capture", f"blepipe:{pipe_port}", "--console", out / "ble.txt",
                "--seconds", str(args.secs), *shlex.split(args.host_args or "")]
    host = spawn(host_cmd, out / "ble-host.log", cwd=REPO)
    time.sleep(2)
    marks.mark("start")
    url = f"http://localhost:{lab_port}/pipe?ws=ws://127.0.0.1:{pipe_port}"
    page = f"localhost:{lab_port}/pipe"
    port = launch_chrome(args, out, url)
    wait_page(port, page)
    dev = args.id
    if not dev:
        listed, err = cdp(port, page, "list", "--timeout-ms", "10000", timeout=40, out=out / "cdp.log")
        (out / "chooser.json").write_text(json.dumps(listed, indent=1))
        dev = pick_id(listed or [], args.name_match)
        if not dev:
            return 4
        time.sleep(2)
    res, err = cdp(port, page, "join", "--id", dev, "--timeout-ms", "45000", timeout=80, out=out / "cdp.log")
    marks.mark("joined" if res else "join-failed")
    if not res:
        return 4
    end = time.monotonic() + args.secs
    resets = list(args.reset_at or [])
    while time.monotonic() < end and host.poll() is None:
        time.sleep(1)
    marks.mark(f"host-exit rc={host.poll()}")
    v, _ = cdp(port, page, "js", "pipe.S", timeout=20, out=out / "cdp.log")
    (out / "pipe-S.json").write_text(json.dumps(v, indent=1))
    if usb:
        stop(usb)
    marks.mark("end")
    return 0 if host.poll() == 0 else 5


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("scenario", choices=["studio", "list", "pipe"])
    ap.add_argument("--out", required=True)
    ap.add_argument("--lpcli", required=True)
    ap.add_argument("--mac", default="14:C1:9F:E6:54:90")
    ap.add_argument("--profile", required=True, help="Chrome scratch profile dir")
    ap.add_argument("--url", default="https://lightplayer.app/")
    ap.add_argument("--page", default="lightplayer.app")
    ap.add_argument("--studio", action="store_true")
    ap.add_argument("--id", default=None, help="chooser device id (else chosen by --name-match)")
    ap.add_argument("--name-match", default="Meteor")
    ap.add_argument("--idle", type=int, default=60)
    ap.add_argument("--stream", type=int, default=120)
    ap.add_argument("--after", type=int, default=60)
    ap.add_argument("--cycles", type=int, default=1)
    ap.add_argument("--disconnect", choices=["reload", "kill"], default="reload")
    ap.add_argument("--usb-secs", type=int, default=560)
    ap.add_argument("--usb-request", action="append")
    ap.add_argument("--no-usb", action="store_true")
    ap.add_argument("--secs", type=int, default=500, help="pipe: the host's --seconds and the wait bound")
    ap.add_argument("--host-args", default="")
    ap.add_argument("--reset-at", type=float, action="append")
    ap.add_argument("--deadline", type=int, default=590, help="hard bound on the whole run")
    args = ap.parse_args()
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    for s in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP, signal.SIGALRM):
        signal.signal(s, on_signal)
    signal.alarm(args.deadline)
    try:
        rc = {"studio": scenario_studio, "list": scenario_list, "pipe": scenario_pipe}[args.scenario](args, out)
    finally:
        cleanup()
    log(f"exit {rc}")
    return rc


if __name__ == "__main__":
    sys.exit(main())
