#!/usr/bin/env python3
"""Disposable TimeProduct M2 migration sweep (plan P5).

One table drives everything: per unique GLSL body, the converted source, the
consumed-slot edits every def sharing that body receives, and the uniform
bindings the lps-probe A/B oracle must supply to reproduce the original.

    python3 scratch/time-sweep/sweep.py apply    # rewrite glsl + defs
    python3 scratch/time-sweep/sweep.py cases    # emit oracle cases.json
    python3 scratch/time-sweep/sweep.py ledger   # rebuild the ledger table

Deleted in P9. No tests, no polish — the ledger is the deliverable.
"""

from __future__ import annotations

import json
import math
import os
import shutil
import struct
import subprocess
import sys
from collections import OrderedDict

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
HERE = os.path.dirname(os.path.abspath(__file__))
GLSL = os.path.join(HERE, "glsl")
SHIMS = os.path.join(HERE, "shims")

# The oracle t-grid from the phase file.
T_GRID = [0.0, 0.7, 3.333, 17.9, 100.1]
THRESHOLD = 1e-5


def f32(x: float) -> float:
    return struct.unpack("<f", struct.pack("<f", x))[0]


# Every converted body that folds a `sin(time * k)` onto a phasor multiplies
# the uniform back up by a TAU literal. The phasor period must be derived
# from the SAME f32 constant the shader spells, or the skipped whole cycles
# would not be whole.
TAU_8 = f32(6.2831853)  # "6.2831853" and "6.28318530718" round to this f32


def rad(k: float, tau: float = TAU_8) -> float:
    """Period of `sin(time * k)` — one cycle of the shader's own TAU."""
    return tau / k


def cyc(rate_hz: float) -> float:
    """Period of a term that already counts whole cycles per second."""
    return 1.0 / rate_hz


# --- uniform descriptors -------------------------------------------------


def ph(name, period, label, desc, offset=0.0, bind=None):
    return {
        "name": name,
        "kind": "phasor",
        "period": period,
        "offset": offset,
        "label": label,
        "desc": desc,
        "bind": bind,
    }


def sec(name, label, desc):
    return {"name": name, "kind": "seconds", "label": label, "desc": desc}


# --- the conversion table -------------------------------------------------
#
# glsl:    converted body in scratch/time-sweep/glsl/ (None = body unchanged)
# targets: every .glsl path that receives it (byte-identical by construction)
# defs:    every node def that gets the slot edits
# drop:    consumed slots retired by the conversion
# unbind:  authored `bindings` entries retired by the conversion
# consts:  uniforms the oracle holds fixed on BOTH sides (their authored
#          defaults), so the A/B isolates the timebase rewrite
# shim:    (orig, conv) oracle sources when the real body has no render()

CONVERSIONS = [
    dict(
        key="fast",
        cls="periodic",
        glsl="fast.glsl",
        targets=["examples/fast/shader.glsl"],
        defs=["examples/fast/shader.json"],
        uniforms=[ph("phase", 1.0, "Phase", "Cycle position (0-1) over the phasor period")],
        note="`mod(time, 1.0)` IS the phasor: one 1 s ramp, read straight out.",
    ),
    dict(
        key="fiber-headband",
        cls="periodic",
        glsl="fiber-headband.glsl",
        targets=["examples/fiber-headband/shader.glsl"],
        defs=["examples/fiber-headband/shader.json"],
        uniforms=[ph("phase", cyc(0.12), "Phase", "Rainbow cycle position (0-1)")],
        note="`fract(time*0.12 + led*0.5)` -> `fract(phase + led*0.5)`.",
    ),
    dict(
        key="rocaille",
        cls="periodic",
        glsl="rocaille.glsl",
        targets=["examples/rocaille/shader.glsl"],
        defs=["examples/rocaille/shader.json"],
        uniforms=[ph("cycle", cyc(0.05), "Cycle", "Pattern cycle position (0-1)")],
        note="`mod(time*0.05*TAU, TAU)` == `TAU*fract(time*0.05)`; exact for ANY "
        "TAU literal because the shader's own constant is both the scale and "
        "the modulus.",
    ),
    dict(
        key="quad-strips",
        cls="periodic",
        glsl="quad-strips.glsl",
        targets=[
            "projects/test/quad-strips/shader.glsl",
            "projects/test/quad-strips-1fix/shader.glsl",
            "projects/test/quad-strips-v3/shader.glsl",
            "projects/test/quad-gamma-v3/shader.glsl",
            "projects/test/quad-gamma-full/shader.glsl",
            "projects/test/quad-equal100-v3/shader.glsl",
            "projects/test/quad60-v3/shader.glsl",
        ],
        defs=[
            "projects/test/quad-strips/shader.json",
            "projects/test/quad-strips-1fix/shader.json",
            "projects/test/quad-strips-v3/shader.json",
            "projects/test/quad-gamma-v3/shader.json",
            "projects/test/quad-gamma-full/shader.json",
            "projects/test/quad-equal100-v3/shader.json",
            "projects/test/quad60-v3/shader.json",
        ],
        uniforms=[ph("phase", cyc(0.05), "Phase", "Chase cycle position (0-1)")],
        note="Per-band rates 0.25/0.35/0.45/0.55 Hz are whole multiples of "
        "0.05 Hz: one 20 s phasor, `phase*(5+2*band)`.",
    ),
    dict(
        key="penta-strands",
        cls="periodic",
        glsl="penta-strands.glsl",
        targets=["projects/test/penta-strands-v3/shader.glsl"],
        defs=["projects/test/penta-strands-v3/shader.json"],
        uniforms=[ph("phase", cyc(0.05), "Phase", "Chase cycle position (0-1)")],
        note="Same family as quad-*, five bands (0.25..0.65 Hz over 0.05 Hz).",
    ),
    dict(
        key="plasma",
        cls="periodic (driven)",
        glsl="plasma.glsl",
        targets=["examples/plasma/shader.glsl"],
        defs=["examples/plasma/shader.json"],
        drop=["speed"],
        unbind=["speed"],
        consts={"speed": 1.0, "scale": 1.0},
        uniforms=[
            ph(
                "phase",
                100.0,
                "Phase",
                "Plasma base cycle position (0-1); every field rides a whole "
                "multiple of it",
                bind="bus:speed",
            )
        ],
        note="All five rates (0.13/0.09/0.11/0.15/0.05) are whole multiples of "
        "0.01 Hz -> one base phasor. `speed` uniform retired; the phasor's "
        "period is driven by the retyped `bus:speed` config channel.",
    ),
    dict(
        key="smoke-project",
        cls="periodic",
        glsl="smoke-project.glsl",
        targets=["lp-fw/fw-browser/www/smoke-project/shader.glsl"],
        defs=["lp-fw/fw-browser/www/smoke-project/shader.json"],
        uniforms=[
            ph("wavePhaseA", rad(2.1), "Wave A", "Horizontal wave cycle position (0-1)"),
            ph("wavePhaseB", rad(1.7), "Wave B", "Vertical wave cycle position (0-1)"),
            ph("crossPhase", rad(1.3), "Cross", "Cross-wave cycle position (0-1)"),
            ph("huePhase", cyc(0.08), "Hue", "Palette cycle position (0-1)"),
        ],
        note="Four incommensurate rates -> four phasors. TAU literal tightened "
        "from 6.28318 to 6.2831853 so the skipped cycles really are whole "
        "(6.28318 drifts 5e-6 rad/cycle, and this body runs 33 cycles by "
        "t=100 s).",
    ),
    dict(
        key="basic2",
        cls="periodic",
        glsl="basic2.glsl",
        targets=["examples/basic2/shader.glsl"],
        defs=["examples/basic2/shader.json"],
        uniforms=[
            ph("panPhase", rad(0.3), "Pan", "Pan oscillation cycle position (0-1)"),
            ph("scalePhase", rad(0.7), "Zoom", "Zoom oscillation cycle position (0-1)"),
            ph("huePhase", rad(1.0), "Hue", "Hue rotation cycle position (0-1)"),
        ],
        orig_override="basic2-orig.glsl",
        note="Worley is sampled at a time-independent coordinate, so nothing "
        "here is unbounded: three phasors and no seconds. `worley_demo` moved "
        "above `render` — the committed order is call-before-declaration, "
        "which the naga front end rejects outright (the oracle baseline "
        "carries the same reorder so the A/B stays like-for-like).",
    ),
    dict(
        key="basic",
        cls="both",
        glsl="basic.glsl",
        targets=["examples/basic/shader.glsl"],
        defs=["examples/basic/shader.json"],
        uniforms=[
            sec("time", "Time", "Seconds from the scope's time product (noise advance)"),
            ph("palettePhase01", 25.0, "Palette", "Palette cycle position (0-1)"),
            ph("panPhase", rad(0.3), "Pan", "Pan oscillation cycle position (0-1)"),
            ph("scalePhase", rad(0.7), "Zoom", "Zoom oscillation cycle position (0-1)"),
        ],
        note="Split: palette/pan/zoom -> phasors; `prsd_demo` keeps raw seconds "
        "(psrdnoise alpha + a mod-1 hue walk are tangled in one argument).",
    ),
    dict(
        key="perf",
        cls="both",
        glsl="perf.glsl",
        targets=[
            "examples/perf/baseline/shader.glsl",
            "examples/perf/fastmath/shader.glsl",
        ],
        defs=[
            "examples/perf/baseline/shader.json",
            "examples/perf/fastmath/shader.json",
        ],
        uniforms=[
            sec("time", "Time", "Seconds from the scope's time product (noise advance)"),
            ph("palettePhase01", 25.0, "Palette", "Palette cycle position (0-1)"),
            ph("panPhase", rad(0.3), "Pan", "Pan oscillation cycle position (0-1)"),
            ph("scalePhase", rad(0.7), "Zoom", "Zoom oscillation cycle position (0-1)"),
        ],
        note="Same split as basic. `mod(time,5)` and `mod(time*0.2,5)` both fold "
        "onto the one 25 s phasor (5 and 25 are whole multiples of it).",
    ),
    dict(
        key="button-idle",
        cls="both",
        glsl="button-idle.glsl",
        targets=[
            "examples/button-playlist/idle.glsl",
            "examples/button-sign/idle.glsl",
        ],
        defs=[
            "examples/button-playlist/idle.json",
            "examples/button-sign/idle.json",
        ],
        uniforms=[
            sec("time", "Time", "Seconds from the scope's time product (fbm scroll)"),
            ph("wavePhase", rad(0.35), "Wave", "Wave cycle position (0-1)"),
            ph("palettePhase", 25.0, "Palette", "Palette cycle position (0-1)"),
        ],
        note="fbm coordinate scroll stays seconds; the wave and the palette walk "
        "become phasors.",
    ),
    dict(
        key="fyeah-attract",
        cls="periodic (driven)",
        glsl="fyeah-attract.glsl",
        targets=["examples/fyeah-button/attract.glsl"],
        defs=["examples/fyeah-button/attract.json"],
        drop=["speed"],
        consts={"speed": 2.0},
        uniforms=[
            ph("wheelPhase", cyc(0.115 * 2.0), "Wheel", "Wheel rotation cycle position (0-1)"),
            ph(
                "paletteCycle",
                cyc(0.055 * 2.0 / 3.0),
                "Palette",
                "Palette walk cycle position (0-1)",
            ),
        ],
        note="`speed` was a LOCAL knob here (no bus binding), so its default 2.0 "
        "is baked into the two periods and the uniform is retired. The two "
        "rates share no useful base (69:11), so they get one phasor each.",
    ),
    dict(
        key="fyeah-idle",
        cls="both (driven)",
        glsl="fyeah-idle.glsl",
        targets=["examples/fyeah-sign/idle.glsl"],
        defs=["examples/fyeah-sign/idle.json"],
        drop=["speed"],
        unbind=["speed"],
        consts={"speed": 1.0, "glow": 0.5},
        uniforms=[
            sec("time", "Time", "Seconds from the scope's time product (noise scroll)"),
            ph("zoomPhase", rad(0.32), "Zoom", "Zoom oscillation cycle position (0-1)"),
            ph("driftPhase", rad(0.18), "Drift", "Drift oscillation cycle position (0-1)"),
            ph("bandPhase", rad(0.85), "Bands", "Band sweep cycle position (0-1)"),
            ph("breathPhase", rad(0.75), "Breath", "Breath cycle position (0-1)"),
            ph("paletteCycle", 18.0, "Palette", "Palette cycle position (0-1)"),
        ],
        note="Five wrapped terms -> five phasors; the psrdnoise scroll keeps "
        "seconds. `speed` multiplied ALL of them, and a period channel drives "
        "one rate only, so the knob is retired at its default 1.0 (see the "
        "deviations section).",
    ),
    dict(
        key="fyeah-idle-plain",
        cls="both",
        glsl="fyeah-idle-plain.glsl",
        targets=["projects/test/fyeah-sign/idle.glsl"],
        defs=["projects/test/fyeah-sign/idle.json"],
        uniforms=[
            sec("time", "Time", "Seconds from the scope's time product (noise scroll)"),
            ph("zoomPhase", rad(0.32), "Zoom", "Zoom oscillation cycle position (0-1)"),
            ph("driftPhase", rad(0.18), "Drift", "Drift oscillation cycle position (0-1)"),
            ph("bandPhase", rad(0.85), "Bands", "Band sweep cycle position (0-1)"),
            ph("breathPhase", rad(0.75), "Breath", "Breath cycle position (0-1)"),
            ph("paletteCycle", 18.0, "Palette", "Palette cycle position (0-1)"),
        ],
        note="The knob-free sibling of fyeah-idle; same split.",
    ),
    dict(
        key="fluid-compute",
        cls="periodic",
        glsl="fluid-compute.glsl",
        targets=["examples/fluid/compute.glsl"],
        defs=["examples/fluid/compute.json"],
        unbind=["time"],
        uniforms=[
            ph("wave_a", rad(0.31), "Emitter A swing", "Cycle position (0-1)"),
            ph("wave_a2", rad(0.31 * 0.73), "Emitter A rise", "Cycle position (0-1)"),
            ph(
                "wave_b2",
                rad(0.23 * 0.81),
                "Emitter B swing",
                "Cycle position (0-1)",
                offset=(2.1 * 0.81) / TAU_8,
            ),
            ph(
                "wave_b",
                rad(0.23),
                "Emitter B rise",
                "Cycle position (0-1)",
                offset=2.1 / TAU_8,
            ),
            ph(
                "wave_c",
                rad(0.19),
                "Emitter C swing",
                "Cycle position (0-1)",
                offset=4.2 / TAU_8,
            ),
            ph(
                "wave_c2",
                rad(0.19 * 0.67),
                "Emitter C rise",
                "Cycle position (0-1)",
                offset=(4.2 * 0.67) / TAU_8,
            ),
            ph("wave_breathe", rad(0.18), "Breathe", "Cycle position (0-1)"),
        ],
        shims=[
            ("fluid-a", "fluid-a-orig.glsl", "fluid-a-conv.glsl",
             ["wave_a", "wave_a2", "wave_b2", "wave_b"]),
            ("fluid-b", "fluid-b-orig.glsl", "fluid-b-conv.glsl",
             ["wave_c", "wave_c2", "wave_breathe"]),
        ],
        note="Seven standalone `sin(time*k + c)` terms -> seven phasors, each "
        "constant folded into `phase_offset`. Compute body: oracled through "
        "two render() shims.",
    ),
    dict(
        key="meteor-sim",
        cls="unbounded",
        glsl=None,
        targets=["examples/meteor/sim.glsl"],
        defs=["examples/meteor/sim.json"],
        unbind=["time"],
        uniforms=[sec("time", "Time", "Seconds from the scope's time product")],
        note="Sanctioned integrator: `dt = time - prev_time`. Slot retyped to "
        "`seconds`, GLSL untouched, `speed` stays a plain f32 that scales dt.",
    ),
    dict(
        key="events-a",
        cls="unbounded",
        glsl=None,
        targets=["examples/events/event_a.glsl"],
        defs=["examples/events/event_a.json"],
        unbind=["time"],
        uniforms=[sec("time", "Time", "Seconds from the scope's time product")],
        note="Monotone counter `uint(time*2)` feeds an event sequence number; a "
        "wrapped phase would replay ids. Slot retyped, GLSL untouched.",
    ),
    dict(
        key="events-b",
        cls="unbounded",
        glsl=None,
        targets=["examples/events/event_b.glsl"],
        defs=["examples/events/event_b.json"],
        unbind=["time"],
        uniforms=[sec("time", "Time", "Seconds from the scope's time product")],
        note="Same as event_a with different moduli.",
    ),
]

# Authored `"time": 0` value lines that no longer parse against the retyped
# product slots (P4's red list).
VALUE_LINE_FILES = [
    "examples/button-playlist/playlist.json",
    "examples/button-sign/playlist.json",
    "examples/fyeah-button/playlist.json",
    "examples/fyeah-sign/playlist.json",
    "projects/test/fyeah-sign/playlist.json",
    "examples/fluid/fluid.json",
]


# --- apply ---------------------------------------------------------------


def read_json(path):
    with open(path) as f:
        return json.load(f, object_pairs_hook=OrderedDict)


def write_json(path, data):
    with open(path, "w") as f:
        json.dump(data, f, indent=2)
        f.write("\n")


def slot_def(u):
    d = OrderedDict()
    if u["kind"] == "phasor":
        d["kind"] = "phasor"
        d["value"] = "f32"
        d["phasor"] = OrderedDict(
            period_seconds=round(u["period"], 7),
            waveform="ramp",
            phase_offset=round(u["offset"], 7),
        )
        if u.get("bind"):
            d["default_bind"] = u["bind"]
    else:
        d["kind"] = "seconds"
        d["value"] = "f32"
    d["default"] = 0
    d["label"] = u["label"]
    d["description"] = u["desc"]
    return d


def apply_all():
    for c in CONVERSIONS:
        if c["glsl"]:
            src = os.path.join(GLSL, c["glsl"])
            body = open(src).read()
            for t in c["targets"]:
                with open(os.path.join(ROOT, t), "w") as f:
                    f.write(body)
        for dpath in c["defs"]:
            p = os.path.join(ROOT, dpath)
            d = read_json(p)
            consumed = d.get("consumed", OrderedDict())
            for name in ["time"] + list(c.get("drop", [])):
                consumed.pop(name, None)
            for u in c["uniforms"]:
                consumed[u["name"]] = slot_def(u)
            d["consumed"] = consumed
            bindings = d.get("bindings")
            if bindings:
                for name in c.get("unbind", []):
                    bindings.pop(name, None)
                if not bindings:
                    d.pop("bindings", None)
            write_json(p, d)
        print(f"applied {c['key']}: {len(c['targets'])} glsl, {len(c['defs'])} defs")

    # Textual, not a JSON round-trip: these files carry unrelated authored
    # numbers (fluid's 0.00003 viscosity) that re-serialization would churn.
    for path in VALUE_LINE_FILES:
        p = os.path.join(ROOT, path)
        lines = open(p).read().splitlines(keepends=True)
        kept = [l for l in lines if l.strip().rstrip(",") != '"time": 0']
        if len(kept) != len(lines):
            # The dropped line may have been the last member of its object.
            for i in range(len(kept) - 1):
                if kept[i].rstrip().endswith(",") and kept[i + 1].strip().startswith("}"):
                    kept[i] = kept[i].rstrip()[:-1] + "\n"
            open(p, "w").write("".join(kept))
            json.loads("".join(kept))
            print(f"dropped authored `time` value line from {path}")

    # Byte-identity check for shared bodies.
    for c in CONVERSIONS:
        if len(c["targets"]) > 1:
            digests = {
                open(os.path.join(ROOT, t), "rb").read() for t in c["targets"]
            }
            assert len(digests) == 1, f"{c['key']} targets diverged"


# --- oracle cases ---------------------------------------------------------


def wrap_unit(x):
    frac = x - math.floor(x)
    return 0.0 if frac >= 1.0 else frac


def phasor_value(u, t):
    period = f32(round(u["period"], 7))
    phase = wrap_unit(t / period) if period > 0 else 0.0
    return f32(wrap_unit(phase + f32(round(u["offset"], 7))))


def git_show(path):
    return subprocess.check_output(["git", "show", f"HEAD:{path}"], cwd=ROOT).decode()


def build_cases():
    cases = []
    for c in CONVERSIONS:
        if c["glsl"] is None:
            continue  # body untouched: the A/B is the identity
        consts = c.get("consts", {})
        shims = c.get("shims")
        if shims:
            for sid, orig, conv, names in shims:
                sel = [u for u in c["uniforms"] if u["name"] in names]
                cases.append(
                    dict(
                        id=f"{c['key']}/{sid}",
                        original=open(os.path.join(SHIMS, orig)).read(),
                        converted=open(os.path.join(SHIMS, conv)).read(),
                        steps=[
                            dict(
                                t=t,
                                original={"time": t},
                                converted={u["name"]: phasor_value(u, t) for u in sel},
                            )
                            for t in T_GRID
                        ],
                    )
                )
            continue
        cases.append(
            dict(
                id=c["key"],
                original=(
                    open(os.path.join(SHIMS, c["orig_override"])).read()
                    if c.get("orig_override")
                    else git_show(c["targets"][0])
                ),
                converted=open(os.path.join(GLSL, c["glsl"])).read(),
                steps=[
                    dict(
                        t=t,
                        original=dict({"time": t}, **consts),
                        converted=dict(
                            {
                                u["name"]: (t if u["kind"] == "seconds" else phasor_value(u, t))
                                for u in c["uniforms"]
                            },
                            **{k: v for k, v in consts.items() if k not in c.get("drop", [])},
                        ),
                    )
                    for t in T_GRID
                ],
            )
        )
    return cases


def write_cases():
    cases = build_cases()
    out = os.path.join(HERE, "cases.json")
    with open(out, "w") as f:
        json.dump(dict(threshold=THRESHOLD, cases=cases), f, indent=1)
    print(f"{len(cases)} oracle cases -> {out}")


# --- ledger ---------------------------------------------------------------


def ledger():
    results = {}
    rpath = os.path.join(HERE, "oracle-results.json")
    if os.path.exists(rpath):
        results = {r["id"]: r for r in json.load(open(rpath))["results"]}
    renders = {}
    rrpath = os.path.join(HERE, "render-check.json")
    if os.path.exists(rrpath):
        renders = json.load(open(rrpath))

    lines = []
    for c in CONVERSIONS:
        ids = [f"{c['key']}/{s[0]}" for s in c["shims"]] if c.get("shims") else [c["key"]]
        worst = []
        for i in ids:
            r = results.get(i)
            worst.append(f"{i.split('/')[-1]}: {r['max_abs_diff']:.3g}" if r else "n/a")
        uni = ", ".join(
            f"{u['name']}={u['period']:.5g}s" if u["kind"] == "phasor" else f"{u['name']}=seconds"
            for u in c["uniforms"]
        )
        rc = renders.get(c["key"], {})
        lines.append(
            dict(
                key=c["key"],
                cls=c["cls"],
                defs=len(c["defs"]),
                targets=len(c["targets"]),
                uniforms=uni,
                worst="; ".join(worst),
                note=c["note"],
                render=rc,
            )
        )
    with open(os.path.join(HERE, "ledger-data.json"), "w") as f:
        json.dump(lines, f, indent=1)
    for l in lines:
        print(f"{l['key']:<18} {l['cls']:<20} {l['worst']}")


if __name__ == "__main__":
    cmd = sys.argv[1] if len(sys.argv) > 1 else "apply"
    if cmd == "apply":
        apply_all()
    elif cmd == "cases":
        write_cases()
    elif cmd == "ledger":
        ledger()
    else:
        raise SystemExit(f"unknown command {cmd}")
