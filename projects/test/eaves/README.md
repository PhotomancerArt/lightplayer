# Eaves

A test project for Sean's eave lights: two runs of an ordinary strip
along a roofline, each turning its corner on a short
jumper (a path gap: wire, no lamps). The first run fills D10 (its port's
count is 60, the run's lamps), so the second follows on to D9 (the
patcher's wrap to the next port; until project format 12 it was auto-flow). It is the
plasma pattern with its mapping and output swapped.

The Graph opens it baked in (`lpa_graph_files::embedded::EAVES`): its
story `graph_eaves_front_reversed`, the `seans_eaves` journey in
`studio_graph_journeys_tests.rs`, and `lpa-graph-files`'s `tests/journeys.rs`
(with `../eaves-wired`) and `tests/save.rs`. It came from the retired mapping
lab (`lab/mapping/projects/`, moved 2026-10-28), where it was the fit
project for plan M5.
