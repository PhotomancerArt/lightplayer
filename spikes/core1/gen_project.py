#!/usr/bin/env python3
"""SPIKE (core-1 render split): generate an N-lane x L-LED measurement project.

skirt-620's shape (one Map2d fixture + one WS281x output per lane, direct
sampling of one shader at 32x32), scaled. Usage:

    gen_project.py <out_dir> <lanes> <leds_per_lane> [shader.glsl]

The default shader is skirt-620's (basic's psrdnoise noise).
"""
import json, math, os, shutil, sys

REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
SKIRT = os.path.join(REPO, "projects/test/skirt-620")
PINS = ["D0", "D1", "D2", "D3", "D4", "D5", "D8", "D9", "D10", "D6", "D7"]

def main():
    out, lanes, per = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
    shader = sys.argv[4] if len(sys.argv) > 4 else os.path.join(SKIRT, "shader.glsl")
    os.makedirs(out, exist_ok=True)
    for f in ("clock.json", "shader.json"):
        shutil.copy(os.path.join(SKIRT, f), out)
    shutil.copy(shader, os.path.join(out, "shader.glsl"))
    total = lanes * per
    json.dump({"format": 11, "name": f"Core1 {lanes}x{per} ({total})"},
              open(os.path.join(out, "project.json"), "w"), indent=2)
    fixture_tpl = json.load(open(os.path.join(SKIRT, "fixture1.json")))
    output_tpl = json.load(open(os.path.join(SKIRT, "output1.json")))
    nodes = {"clock": {"ref": "./clock.json"}, "shader": {"ref": "./shader.json"}}
    cols = 8
    rows = math.ceil(per / cols)
    assert cols * rows == per, "leds per lane must be a multiple of 8"
    width = 100.0 / lanes
    pitch = min((width - 2) / cols, 96.0 / rows)
    for k in range(lanes):
        n = k + 1
        m = {"format": 1, "sample_diameter": 2.0, "canvas": [0.0, 0.0, 100.0, 100.0],
             "objects": [{"name": f"lane-{n}", "shape": {"grid": {
                 "origin": [k * width + 1.0, 2.0], "cols": cols, "rows": rows,
                 "pitch": round(pitch, 4)}}}]}
        json.dump(m, open(os.path.join(out, f"fixture{n}.map2d.json"), "w"), indent=2)
        fx = json.loads(json.dumps(fixture_tpl))
        fx["bindings"]["output"]["target"] = f"bus:control.out/ch{n}"
        fx["mapping"]["source"] = f"fixture{n}.map2d.json"
        json.dump(fx, open(os.path.join(out, f"fixture{n}.json"), "w"), indent=2)
        op = json.loads(json.dumps(output_tpl))
        op["ports"]["0"]["endpoint"] = f"ws281x:local:{PINS[k]}"
        op["bindings"]["input"]["source"] = f"bus:control.out/ch{n}"
        json.dump(op, open(os.path.join(out, f"output{n}.json"), "w"), indent=2)
        nodes[f"fixture{n}"] = {"ref": f"./fixture{n}.json"}
        nodes[f"output{n}"] = {"ref": f"./output{n}.json"}
    json.dump({"kind": "Module", "nodes": dict(sorted(nodes.items()))},
              open(os.path.join(out, "module.json"), "w"), indent=2)
    print(out, total)

if __name__ == "__main__":
    main()
