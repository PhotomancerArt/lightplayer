# circular-display-walk — the circular display, from nothing

**The walk project for the mapping rebuild's G3 (Studio's Graph, by hand)
and M5 (the board walk).** Open it in Studio's Graph and map Yona's
circular display from nothing: delete the starter strip, draw a circle,
give it rings of 24, 16, 12 and 8 lamps and a centre lamp, start the
fixture on D0, then reverse ring 3 and move it to D1.

It is a new board's project exactly as Studio generates it (the XIAO
ESP32-C6's setup flow, `generate_board_project`: clock, the meteor
playlist, a 256-lamp starter strip, one output), with one change: the
output has the display's **five ports of 64 lamps (D0 to D4) and no
name**, so the first port you choose names it (`1`, lean Q96).

The core journey `the_circular_display_from_nothing`
(`lp-app/lpa-studio-core/src/app/studio/studio_graph_journeys_tests.rs`)
presses the same steps as offers, starting from these same bytes, and
saves `circular_display_from_nothing.map2d.json` beside it.
`the_walk_project_is_the_from_nothing_journeys` checks every file here
(this README aside) is what Studio generates; re-write them with
`LPA_STUDIO_CORE_BLESS=1 cargo test -p lpa-studio-core --lib
the_walk_project_is_the_from_nothing_journeys` when the generator moves.

Until the starter strip is deleted and a start is chosen, the fixture
starts on output `1`, which has no name yet: the Graph shows that as a
problem, which is the point of the walk's first steps.

The finished display, as soldered (two ports), is
[`circular-display-6`](../circular-display-6/).
