#!/usr/bin/env python3
"""SPIKE (claude/s3-fluid-spike): the look check for the fluid variants.

    python3 spikes/fluid/look.py metrics <dump-dir> <grid>            # the rubric table
    python3 spikes/fluid/look.py video <dump-dir> <grid> <out.mp4> base v1 v2 ...
    python3 spikes/fluid/look.py sheet <dump-dir> <grid> <out.png> base v1 v2 ...

<dump-dir> is what `cargo run -p lpc-engine --release --example fluid_look`
wrote: <grid>/<variant>.f32, per step r,g,b over the n×n interior.

A fluid is chaotic: two solvers that differ in the last bit drift apart in a
few seconds and never meet again. So the rubric compares the *character* of
the picture over the whole run (brightness, detail, motion, checkerboard),
and the frames only over the first two seconds, before the drift.
"""
import sys, os, subprocess
import numpy as np
from PIL import Image, ImageDraw

STEP_HZ = 25


def load(d, n, name):
    a = np.fromfile(os.path.join(d, str(n), name + ".f32"), dtype="<f4")
    return a.reshape(-1, 3, n, n)


def display(a):
    """What the LEDs get: clamp to 0..1, 8 bits (gamma off in the project)."""
    return np.clip(a, 0, 1) * 255.0


def character(a):
    x = display(a)
    late = x[STEP_HZ * 4:]  # past the start-up transient
    bright = late.mean()
    gx = np.abs(np.diff(late, axis=3)).mean()
    gy = np.abs(np.diff(late, axis=2)).mean()
    detail = (gx + gy) / 2
    aniso = gx / gy
    motion = np.abs(np.diff(late, axis=0)).mean()
    n = a.shape[-1]
    chk = np.indices((n, n)).sum(axis=0) % 2 * 2 - 1  # ±1 checkerboard
    # Per-frame, per-channel projection onto the checkerboard, as 8-bit units.
    checker = np.abs((late * chk).mean(axis=(2, 3))).mean() * 2
    # Banding: share of lit cells (> 1/255) whose value repeats a neighbour's
    # exactly (stalled, flat runs).
    lit = late > 1.0
    flat = (np.diff(late, axis=3) == 0) & lit[..., 1:]
    banding = flat.sum() / max(lit.sum(), 1)
    return dict(bright=bright, detail=detail, motion=motion, checker=checker, banding=banding, aniso=aniso)


def early_diff(a, b):
    k = STEP_HZ * 2
    return np.abs(display(a[:k]) - display(b[:k])).mean(), np.abs(display(a[:k]) - display(b[:k])).max()


def verdict(row, base):
    """The rubric's numeric half, calibrated by eye on the sheets (see the
    report): it must call f32 and one-sweep-from-source a twin, 2 sweeps "a
    little worse" at 32×32, and 1 sweep, warm-started pressure and a velocity
    diffusion started from zero "different".

    twin            same brightness and detail (±5 %), motion (±8 %), no
                    direction bias (±3 %), checkerboard ≤ 1.25×, flat runs
                    (blocky, harder edges) ≤ 1.3× today's
    a little worse  ±10 %, bias ±5 %, checkerboard ≤ 1.6×, flat runs ≤ 2.2×
    different       anything past that
    Tracking (`run`, `early`) is reported, not graded: a fluid drifts.
    """
    if row["exact"]:
        return "identical"
    r = lambda k: abs(row[k] / base[k] - 1)
    chk = row["checker"] / max(base["checker"], 1e-9)
    flat = row["banding"] / max(base["banding"], 1e-9)
    if max(r("bright"), r("detail")) <= 0.05 and r("motion") <= 0.08 and r("aniso") <= 0.03 \
            and chk <= 1.25 and flat <= 1.3:
        return "twin"
    if max(r("bright"), r("detail"), r("motion")) <= 0.10 and r("aniso") <= 0.05 \
            and chk <= 1.6 and flat <= 2.2:
        return "a little worse"
    return "different"


def metrics(d, n):
    names = sorted(f[:-4] for f in os.listdir(os.path.join(d, str(n))) if f.endswith(".f32"))
    names.remove("base")
    names.insert(0, "base")
    ref = load(d, n, "base")
    base = character(ref)
    print(f"grid {n}×{n}, {ref.shape[0]} steps at {STEP_HZ} Hz; character over steps {STEP_HZ*4}..; "
          f"early = first {2} s, mean/max |Δ| in 8-bit units")
    print(f"{'variant':22} {'bright':>7} {'detail':>7} {'motion':>7} {'checker':>8} {'aniso':>7} {'banding':>8} "
          f"{'early':>6} {'run':>6}  verdict")
    for name in names:
        a = ref if name == "base" else load(d, n, name)
        c = character(a)
        c["exact"] = np.array_equal(a, ref)
        c["early"], c["emax"] = early_diff(a, ref)
        k = min(a.shape[0], ref.shape[0])
        c["run"] = np.abs(display(a[:k]) - display(ref[:k])).mean()
        rel = lambda k: f"{c[k] / base[k] * 100:6.1f}%" if name != "base" else f"{c[k]:7.2f}"
        print(f"{name:22} {rel('bright'):>7} {rel('detail'):>7} {rel('motion'):>7} {rel('checker'):>8} {rel('aniso'):>7} "
              f"{c['banding']*100:7.2f}% {c['early']:6.2f} {c['run']:6.2f}  {verdict(c, base)}")


def panel(frame, size, label):
    img = np.clip(frame, 0, 1).transpose(1, 2, 0) * 255
    im = Image.fromarray(img.astype(np.uint8), "RGB").transpose(Image.FLIP_TOP_BOTTOM)
    im = im.resize((size, size), Image.BILINEAR)
    out = Image.new("RGB", (size, size + 22), (16, 16, 16))
    out.paste(im, (0, 22))
    ImageDraw.Draw(out).text((4, 4), label, fill=(230, 230, 230))
    return out


def compose(arrs, labels, k, size):
    panels = [panel(a[k], size, f"{l}  t={k/STEP_HZ:4.1f}s") for a, l in zip(arrs, labels)]
    cols = min(len(panels), 4)
    rows = (len(panels) + cols - 1) // cols
    w, h = panels[0].size
    sheet = Image.new("RGB", (cols * (w + 6), rows * (h + 6)), (0, 0, 0))
    for i, p in enumerate(panels):
        sheet.paste(p, ((i % cols) * (w + 6), (i // cols) * (h + 6)))
    return sheet


def video(d, n, out, names, size=240):
    arrs = [load(d, n, x) for x in names]
    steps = min(a.shape[0] for a in arrs)
    first = compose(arrs, names, 0, size)
    w, h = first.size
    w, h = w + w % 2, h + h % 2
    ff = subprocess.Popen(["ffmpeg", "-y", "-loglevel", "error", "-f", "rawvideo", "-pix_fmt", "rgb24",
                           "-s", f"{w}x{h}", "-r", str(STEP_HZ), "-i", "-", "-pix_fmt", "yuv420p",
                           "-c:v", "libx264", "-crf", "18", out], stdin=subprocess.PIPE)
    for k in range(steps):
        im = Image.new("RGB", (w, h))
        im.paste(compose(arrs, names, k, size), (0, 0))
        ff.stdin.write(im.tobytes())
    ff.stdin.close()
    ff.wait()


def sheet(d, n, out, names, size=160):
    arrs = [load(d, n, x) for x in names]
    times = [2, 10, 20, 29]
    ims = [compose(arrs, names, t * STEP_HZ, size) for t in times]
    w, h = ims[0].size
    s = Image.new("RGB", (w, h * len(ims) + 8 * len(ims)), (40, 40, 40))
    for i, im in enumerate(ims):
        s.paste(im, (0, i * (h + 8)))
    s.save(out)


if __name__ == "__main__":
    cmd, d, n = sys.argv[1], sys.argv[2], int(sys.argv[3])
    if cmd == "metrics":
        metrics(d, n)
    elif cmd == "video":
        video(d, n, sys.argv[4], sys.argv[5:])
    elif cmd == "sheet":
        sheet(d, n, sys.argv[4], sys.argv[5:])
