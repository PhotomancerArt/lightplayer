# PLAYFUL Choker

A real piece: a single PCB choker that spells **PLAYFUL** in 73 2020-size
WS2812-class LEDs, driven by an ESP32-C6 on the wearer's battery.

- **Board / wiring:** XIAO ESP32-C6, data on `D10` (GPIO 18 on the default
  C6 profile), one channel, 73 lamps.
- **Brightness:** 10 % — it runs from a battery and sits an inch from a face.
  Nudge `brightness` in `fixture.json` (or the master fader in Studio).
- **Pattern:** one shader: bands of color sweeping along the word with crisp
  fronts between them, bent by a slow noise field and leaning back and forth
  so they cut the letters diagonally. Every lamp is the same Oklch lightness
  and chroma (`lpfn_oklch2rgb`), so only the hue moves and the letters read
  evenly. `scale` sets how many bands fit across the word and `frontWidth`
  how sharp the edge between two colors is.

## Mapping

`playful.map2d.json` is derived from the EasyEDA exports in the choker's
project folder: every `LEDn` position comes from the PCB (`PCB1.pcbdoc`, the
Altium ASCII export) and the chain from the netlist (`Netlist_*.tel`, WS2812
pin 3 in, pin 1 out). Doc units are millimetres from the board outline's
top-left, with the outline as the canvas. `playful-mapping.svg` draws the same
document with every lamp numbered in wire order and labelled with its PCB
designator, so a lamp on the board and a lamp in the file can be matched
without guessing.

Object order is wire order. Each object is one pen stroke — the strokes a pen
cannot join (the crossbars of the A and F, the three strokes of the Y) are
separate objects — so the resolver's even spacing along each polyline lands
within 0.93 mm of every pad. The chain, from the data-in pad:

| object | designators | stroke |
|---|---|---|
| `P` | LED2 3 4 5 78 6 7 8 9 10 11 12 13 | stem up, top bar, bowl, middle bar back |
| `L1` | LED14–21 | stem down, foot right |
| `A` | LED26–35 | left leg up, right leg down |
| `A-bar` | LED36 37 | crossbar right to left |
| `Y-stem` / `Y-left` / `Y-right` | LED38–40 / 41–43 / 44–46 | stem up to the fork, then each arm fork-outward |
| `F-lower` / `F-bar` / `F-upper` | LED47–49 / 50–51 / 52–56 | stem to the middle bar, the bar, stem on up and the top bar |
| `U` | LED57–69 | down, round the bottom, up |
| `L2` | LED70–77 | stem down, foot right |

The generator is `scripts/pcb-export-to-map2d.py`, and the table above is its
strokes table, `scripts/pcb-export-strokes/playful-choker.json`. The exports
are design files and stay in the choker's design folder, never in this repo.
Regenerate rather than hand-edit if the PCB changes, and check that the
committed files still match the exports after touching the generator:

```bash
just pcb-map2d playful-choker <design-folder>           # regenerate both files
just pcb-map2d playful-choker <design-folder> --check   # byte-identical, or fail
```
