// test run

// ============================================================================
// Deeply nested structs (3+ levels): declaration, construction, path read/write,
// and a compact Outer/Middle/Inner/Coord chain.
//
// Hierarchy matches Box/Line/Point nesting from the roadmap; the top struct is
// named PanelGrid (not Layout) because `Layout` clashes with the GLSL keyword.
//
// Nested aggregate member assignment (e.g. panel_grid.header = box) is not
// lowered yet — tests use constructors or whole-struct assign where needed.
// ============================================================================

struct Point {
    float x;
    float y;
};

struct Line {
    Point start;
    Point end;
};

struct Box {
    Line top;
    Line bottom;
    Line left;
    Line right;
};

// Not named `Layout` — conflicts with GLSL `layout` qualifier keyword.
struct PanelGrid {
    Box header;
    Box content;
    Box footer;
};

// Compact builders for PanelGrid/Box (nested constructors get very long inline).
Line deep_mk_line(float ax, float ay, float bx, float by) {
    return Line(Point(ax, ay), Point(bx, by));
}

Box deep_mk_box(float tag) {
    return Box(
        deep_mk_line(0.0, 0.0 + tag, 1.0, 0.0 + tag),
        deep_mk_line(0.0, 1.0 + tag, 1.0, 1.0 + tag),
        deep_mk_line(0.0, 0.0 + tag, 0.0, 1.0 + tag),
        deep_mk_line(1.0, 0.0 + tag, 1.0, 1.0 + tag)
    );
}

PanelGrid deep_mk_panel_grid(float h, float c, float f) {
    return PanelGrid(deep_mk_box(h), deep_mk_box(c), deep_mk_box(f));
}

// --- PanelGrid hierarchy (4 struct levels: PanelGrid → Box → Line → Point) --

float test_deep_declare_panel_grid() {
    PanelGrid panel_grid;
    return 1.0;
}

// @unimplemented(jit.q32)
// run: test_deep_declare_panel_grid() ~= 1.0

float test_deep_construct_read_path_4level() {
    PanelGrid panel_grid = deep_mk_panel_grid(0.0, 0.0, 0.0);
    // panel_grid.header.top.start.x — four member hops before scalar (header, top, start, x).
    return panel_grid.header.top.start.x;
}

// @unimplemented(jit.q32)
// run: test_deep_construct_read_path_4level() ~= 0.0

float test_deep_path_read_tagged_header() {
    PanelGrid panel_grid = deep_mk_panel_grid(10.0, 20.0, 30.0);
    return panel_grid.header.top.start.y;
}

// @unimplemented(jit.q32)
// run: test_deep_path_read_tagged_header() ~= 10.0

float test_deep_path_read_content_bottom_end_x() {
    PanelGrid panel_grid = deep_mk_panel_grid(0.0, 5.0, 0.0);
    return panel_grid.content.bottom.end.x;
}

// @unimplemented(jit.q32)
// run: test_deep_path_read_content_bottom_end_x() ~= 1.0

float test_deep_path_write_content_top_end_y() {
    PanelGrid panel_grid = deep_mk_panel_grid(0.0, 0.0, 0.0);
    panel_grid.content.top.end.y = 100.0;
    return panel_grid.content.top.end.y;
}

// @unimplemented(jit.q32)
// run: test_deep_path_write_content_top_end_y() ~= 100.0

float test_deep_path_write_footer_left_start_x() {
    PanelGrid panel_grid = deep_mk_panel_grid(0.0, 0.0, 0.0);
    panel_grid.footer.left.start.x = -3.5;
    return panel_grid.footer.left.start.x;
}

// @unimplemented(jit.q32)
// run: test_deep_path_write_footer_left_start_x() ~= -3.5

// Same final shape as `panel_grid.header = replacement` after deep_mk_panel_grid(1,2,3).
// Nested aggregate member assign (`panel_grid.header = …`) is not lowered yet (phase 04).
float test_deep_partial_assign_header_via_constructor() {
    Box replacement = deep_mk_box(7.0);
    PanelGrid panel_grid =
        PanelGrid(replacement, deep_mk_box(2.0), deep_mk_box(3.0));
    return panel_grid.header.top.start.y;
}

// @unimplemented(jit.q32)
// run: test_deep_partial_assign_header_via_constructor() ~= 7.0

float test_deep_read_after_header_construct_and_write() {
    PanelGrid panel_grid =
        PanelGrid(deep_mk_box(2.0), deep_mk_box(0.0), deep_mk_box(0.0));
    panel_grid.header.right.end.x = 9.0;
    return panel_grid.header.right.end.x;
}

// @unimplemented(jit.q32)
// run: test_deep_read_after_header_construct_and_write() ~= 9.0

// --- Compact 4-level chain: Outer → Middle → Inner → Coord -------------------

struct Coord {
    float x;
};

struct Inner {
    Coord c;
};

struct Middle {
    Inner i;
};

struct Outer {
    Middle m;
};

float test_deep_chain_declare_outer() {
    Outer o;
    return 1.0;
}

// @unimplemented(jit.q32)
// run: test_deep_chain_declare_outer() ~= 1.0

float test_deep_chain_construct_read_4deep() {
    Outer o = Outer(Middle(Inner(Coord(5.0))));
    return o.m.i.c.x;
}

// @unimplemented(jit.q32)
// run: test_deep_chain_construct_read_4deep() ~= 5.0

float test_deep_chain_write_inner_coord() {
    Outer o = Outer(Middle(Inner(Coord(1.0))));
    o.m.i.c.x = 42.0;
    return o.m.i.c.x;
}

// @unimplemented(jit.q32)
// run: test_deep_chain_write_inner_coord() ~= 42.0

// `o.m = Middle(...)` hits nested aggregate assign (not phase 04); use whole-struct rebuild.
float test_deep_chain_rebuild_outer_with_middle() {
    Outer o = Outer(Middle(Inner(Coord(8.0))));
    return o.m.i.c.x;
}

// @unimplemented(jit.q32)
// run: test_deep_chain_rebuild_outer_with_middle() ~= 8.0

float test_deep_chain_whole_outer_assign() {
    Outer a = Outer(Middle(Inner(Coord(1.0))));
    Outer b = Outer(Middle(Inner(Coord(99.0))));
    a = b;
    return a.m.i.c.x;
}

// @unimplemented(jit.q32)
// run: test_deep_chain_whole_outer_assign() ~= 99.0

float test_deep_mixed_panel_grid_and_chain() {
    PanelGrid panel_grid = deep_mk_panel_grid(0.0, 0.0, 0.0);
    Outer o = Outer(Middle(Inner(Coord(3.0))));
    float a = panel_grid.header.top.start.x;
    float b = o.m.i.c.x;
    return a + b;
}

// @unimplemented(jit.q32)
// run: test_deep_mixed_panel_grid_and_chain() ~= 3.0
