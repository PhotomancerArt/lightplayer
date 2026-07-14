#!/usr/bin/env python3
"""Generate the control-flow torture corpus under lps-filetests/filetests/control/torture/.

Systematically enumerates control-flow shapes known to break structured-control-flow
lowering: if/else nested per-arm, branches in loops and loops in branches,
break/continue at nesting depth 1-2, early returns from nested blocks,
short-circuit && / || with side-effecting right operands, and ternaries nested
in branch conditions.

Every test function encodes its execution trace into an int accumulator via
`t = t * 10 + k` statements ("sites", each with a distinct digit k), so a wrong
path, a skipped merge point, or a re-ordered site changes the returned value.
Expected values in `// run:` directives are computed by a reference interpreter
in this script (exact int math; all programs are bounded and deterministic).

Usage:
    python3 lp-shader/scripts/gen-control-torture.py           # dry-run: list files
    python3 lp-shader/scripts/gen-control-torture.py --write   # write corpus + README
    python3 lp-shader/scripts/gen-control-torture.py --check   # diff against disk; exit 1 on drift
"""

from __future__ import annotations

import os
import sys
from dataclasses import dataclass, field

TORTURE_DIR = os.path.normpath(
    os.path.join(
        os.path.dirname(os.path.abspath(__file__)),
        "..",
        "lps-filetests",
        "filetests",
        "control",
        "torture",
    )
)

REGEN_CMD = "python3 lp-shader/scripts/gen-control-torture.py --write"

I32_MIN = -(2**31)
I32_MAX = 2**31 - 1

# ---------------------------------------------------------------------------
# AST
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class V:
    """Variable reference."""

    name: str


@dataclass(frozen=True)
class L:
    """Int literal."""

    value: int


@dataclass(frozen=True)
class B:
    """Binary op."""

    op: str
    lhs: object
    rhs: object


@dataclass(frozen=True)
class T:
    """Ternary (always emitted parenthesized)."""

    cond: object
    then: object
    els: object


@dataclass(frozen=True)
class Call:
    name: str
    args: tuple


@dataclass(frozen=True)
class Decl:
    """`int name = expr;`"""

    name: str
    expr: object


@dataclass(frozen=True)
class Asn:
    """`name = expr;`"""

    name: str
    expr: object


@dataclass(frozen=True)
class If:
    cond: object
    then: tuple
    els: tuple | None = None


@dataclass(frozen=True)
class For:
    """`for (int var = start; cond; var++) { body }`"""

    var: str
    start: int
    cond: object
    body: tuple


@dataclass(frozen=True)
class While:
    cond: object
    body: tuple


@dataclass(frozen=True)
class DoWhile:
    body: tuple
    cond: object


@dataclass(frozen=True)
class Break:
    pass


@dataclass(frozen=True)
class Continue:
    pass


@dataclass(frozen=True)
class Return:
    expr: object


@dataclass(frozen=True)
class FuncDef:
    ret_type: str  # "int" | "bool"
    name: str
    params: tuple  # of (type, name)
    body: tuple


# ---------------------------------------------------------------------------
# GLSL emitter
# ---------------------------------------------------------------------------

PREC = {
    "||": 1,
    "&&": 2,
    "==": 3,
    "!=": 3,
    "<": 4,
    "<=": 4,
    ">": 4,
    ">=": 4,
    "+": 5,
    "-": 5,
    "*": 6,
}


def emit_expr(e, min_prec: int = 0) -> str:
    if isinstance(e, V):
        return e.name
    if isinstance(e, L):
        return str(e.value)
    if isinstance(e, Call):
        args = ", ".join(emit_expr(a) for a in e.args)
        return f"{e.name}({args})"
    if isinstance(e, T):
        # Always parenthesize ternaries so nesting them in conditions is unambiguous.
        return (
            f"({emit_expr(e.cond)} ? {emit_expr(e.then)} : {emit_expr(e.els)})"
        )
    if isinstance(e, B):
        prec = PREC[e.op]
        s = f"{emit_expr(e.lhs, prec)} {e.op} {emit_expr(e.rhs, prec + 1)}"
        if prec < min_prec:
            return f"({s})"
        return s
    raise AssertionError(f"unknown expr node: {e!r}")


def emit_stmts(stmts, indent: int, out: list):
    pad = "    " * indent
    for s in stmts:
        if isinstance(s, Decl):
            out.append(f"{pad}int {s.name} = {emit_expr(s.expr)};")
        elif isinstance(s, Asn):
            out.append(f"{pad}{s.name} = {emit_expr(s.expr)};")
        elif isinstance(s, If):
            out.append(f"{pad}if ({emit_expr(s.cond)}) {{")
            emit_stmts(s.then, indent + 1, out)
            if s.els is not None:
                out.append(f"{pad}}} else {{")
                emit_stmts(s.els, indent + 1, out)
            out.append(f"{pad}}}")
        elif isinstance(s, For):
            out.append(
                f"{pad}for (int {s.var} = {s.start}; {emit_expr(s.cond)}; {s.var}++) {{"
            )
            emit_stmts(s.body, indent + 1, out)
            out.append(f"{pad}}}")
        elif isinstance(s, While):
            out.append(f"{pad}while ({emit_expr(s.cond)}) {{")
            emit_stmts(s.body, indent + 1, out)
            out.append(f"{pad}}}")
        elif isinstance(s, DoWhile):
            out.append(f"{pad}do {{")
            emit_stmts(s.body, indent + 1, out)
            out.append(f"{pad}}} while ({emit_expr(s.cond)});")
        elif isinstance(s, Break):
            out.append(f"{pad}break;")
        elif isinstance(s, Continue):
            out.append(f"{pad}continue;")
        elif isinstance(s, Return):
            out.append(f"{pad}return {emit_expr(s.expr)};")
        else:
            raise AssertionError(f"unknown stmt node: {s!r}")


def emit_func(f: FuncDef) -> str:
    params = ", ".join(f"{ty} {name}" for ty, name in f.params)
    lines = [f"{f.ret_type} {f.name}({params}) {{"]
    emit_stmts(f.body, 1, lines)
    lines.append("}")
    return "\n".join(lines)


# ---------------------------------------------------------------------------
# Reference interpreter (the oracle)
# ---------------------------------------------------------------------------


class _BreakSig(Exception):
    pass


class _ContinueSig(Exception):
    pass


class _ReturnSig(Exception):
    def __init__(self, value):
        self.value = value


class Interp:
    """Exact-int reference interpreter for the AST subset above.

    `&&` / `||` short-circuit exactly as GLSL requires; both frontends now
    lower side-effecting right operands to control flow (see
    docs/design/lpir/02-core-ops.md).
    """

    FUEL = 100_000

    def __init__(self, funcs: dict, global_decls: list):
        self.funcs = funcs
        self.global_decls = global_decls  # list of (type, name, init_int)

    def run(self, fn_name: str, args: tuple) -> int:
        self.fuel = self.FUEL
        self.globals = {name: init for _ty, name, init in self.global_decls}
        return self.call(fn_name, list(args))

    def call(self, fn_name: str, arg_values: list):
        fn = self.funcs[fn_name]
        assert len(arg_values) == len(fn.params), fn_name
        env = {name: val for (_ty, name), val in zip(fn.params, arg_values)}
        try:
            self.exec_block(fn.body, env)
        except _ReturnSig as r:
            return r.value
        raise AssertionError(f"{fn_name}: fell off the end without a return")

    def burn(self):
        self.fuel -= 1
        if self.fuel <= 0:
            raise AssertionError("fuel exhausted: generated program does not terminate")

    def check_i32(self, v):
        if isinstance(v, bool):
            return v
        assert I32_MIN <= v <= I32_MAX, f"i32 overflow in generated program: {v}"
        return v

    def store(self, name, value, env):
        if name in env:
            env[name] = value
        elif name in self.globals:
            self.globals[name] = value
        else:
            raise AssertionError(f"assignment to undeclared variable {name}")

    def eval(self, e, env):
        self.burn()
        if isinstance(e, V):
            if e.name in env:
                return env[e.name]
            return self.globals[e.name]
        if isinstance(e, L):
            return e.value
        if isinstance(e, Call):
            vals = [self.eval(a, env) for a in e.args]
            return self.call(e.name, vals)
        if isinstance(e, T):
            if self.eval(e.cond, env):
                return self.eval(e.then, env)
            return self.eval(e.els, env)
        if isinstance(e, B):
            op = e.op
            if op == "&&":
                return bool(self.eval(e.lhs, env)) and bool(self.eval(e.rhs, env))
            if op == "||":
                return bool(self.eval(e.lhs, env)) or bool(self.eval(e.rhs, env))
            a = self.eval(e.lhs, env)
            b = self.eval(e.rhs, env)
            if op == "+":
                return self.check_i32(a + b)
            if op == "-":
                return self.check_i32(a - b)
            if op == "*":
                return self.check_i32(a * b)
            if op == "<":
                return a < b
            if op == "<=":
                return a <= b
            if op == ">":
                return a > b
            if op == ">=":
                return a >= b
            if op == "==":
                return a == b
            if op == "!=":
                return a != b
            raise AssertionError(f"unknown op {op}")
        raise AssertionError(f"unknown expr node: {e!r}")

    def exec_block(self, stmts, env):
        for s in stmts:
            self.exec(s, env)

    def exec(self, s, env):
        self.burn()
        if isinstance(s, Decl):
            env[s.name] = self.eval(s.expr, env)
        elif isinstance(s, Asn):
            self.store(s.name, self.eval(s.expr, env), env)
        elif isinstance(s, If):
            if self.eval(s.cond, env):
                self.exec_block(s.then, env)
            elif s.els is not None:
                self.exec_block(s.els, env)
        elif isinstance(s, For):
            env[s.var] = s.start
            while self.eval(s.cond, env):
                self.burn()
                try:
                    self.exec_block(s.body, env)
                except _ContinueSig:
                    pass
                except _BreakSig:
                    break
                env[s.var] = self.check_i32(env[s.var] + 1)
        elif isinstance(s, While):
            while self.eval(s.cond, env):
                self.burn()
                try:
                    self.exec_block(s.body, env)
                except _ContinueSig:
                    continue
                except _BreakSig:
                    break
        elif isinstance(s, DoWhile):
            while True:
                self.burn()
                try:
                    self.exec_block(s.body, env)
                except _ContinueSig:
                    pass
                except _BreakSig:
                    break
                if not self.eval(s.cond, env):
                    break
        elif isinstance(s, Break):
            raise _BreakSig()
        elif isinstance(s, Continue):
            raise _ContinueSig()
        elif isinstance(s, Return):
            raise _ReturnSig(self.eval(s.expr, env))
        else:
            raise AssertionError(f"unknown stmt node: {s!r}")


# ---------------------------------------------------------------------------
# Test-file model
# ---------------------------------------------------------------------------


@dataclass
class TestFile:
    name: str  # e.g. "ifnest_d2_t.glsl"
    title: str
    notes: list  # extra comment lines
    globals: list = field(default_factory=list)  # (type, name, init_int)
    helpers: list = field(default_factory=list)  # FuncDef
    tests: list = field(default_factory=list)  # (FuncDef, [args tuples])

    def render(self) -> str:
        funcs = {f.name: f for f in self.helpers}
        for fn, _runs in self.tests:
            funcs[fn.name] = fn
        interp = Interp(funcs, self.globals)

        test_lines = []
        for fn, runs in self.tests:
            test_lines.append(emit_func(fn))
            test_lines.append("")
            for args in runs:
                expected = interp.run(fn.name, args)
                arg_str = ", ".join(str(a) for a in args)
                test_lines.append(f"// run: {fn.name}({arg_str}) == {expected}")
            test_lines.append("")

        lines = ["// test run", ""]
        lines.append("// " + "=" * 76)
        lines.append(f"// Control-flow torture: {self.title}")
        for n in self.notes:
            lines.append(f"// {n}" if n else "//")
        lines.append("//")
        lines.append("// GENERATED FILE - do not edit by hand.")
        lines.append(f"// Regenerate: {REGEN_CMD}")
        lines.append("// " + "=" * 76)
        lines.append("")
        for ty, name, init in self.globals:
            lines.append(f"{ty} {name} = {init};")
            lines.append("")
        for h in self.helpers:
            lines.append(emit_func(h))
            lines.append("")
        lines.extend(test_lines)
        return "\n".join(lines).rstrip() + "\n"


class SiteAlloc:
    """Allocates distinct trace-site digits 1..9 within one test function."""

    def __init__(self):
        self.next = 1

    def site(self) -> Asn:
        k = self.next
        self.next += 1
        assert k <= 9, "more than 9 trace sites in one function; split the test"
        return Asn("t", B("+", B("*", V("t"), L(10)), L(k)))


def gt0(name: str) -> B:
    return B(">", V(name), L(0))


def body(*stmts) -> tuple:
    return tuple(stmts)


def combos(n: int):
    """All 0/1 tuples of length n, in lexicographic order."""
    out = []
    for bits in range(2**n):
        out.append(tuple((bits >> (n - 1 - i)) & 1 for i in range(n)))
    return out


CHK = FuncDef(
    "bool",
    "chk",
    (("int", "k"), ("int", "v")),
    body(
        Asn("g_trace", B("+", B("*", V("g_trace"), L(10)), V("k"))),
        Return(B(">", V("v"), L(0))),
    ),
)

G_TRACE = ("int", "g_trace", 0)


def chk(k: int, v) -> Call:
    return Call("chk", (L(k), v))


# ---------------------------------------------------------------------------
# Axis 1: nested if/else, enumerated per arm (ifnest_*)
# ---------------------------------------------------------------------------

IFNEST_PARAMS = ["a", "b", "c"]


def build_ifnest_chain(depth: int, placements: str, sa: SiteAlloc, level: int = 0):
    """One if/else per level; the child if sits in the Then or Else arm per
    `placements[level]`, bracketed by pre/post sites. Leaf arms get sites."""
    cond = gt0(IFNEST_PARAMS[level])
    if level == depth - 1:
        return If(cond, body(sa.site()), body(sa.site()))
    child = build_ifnest_chain(depth, placements, sa, level + 1)
    nested_arm = body(sa.site(), child, sa.site())
    plain_arm = body(sa.site())
    if placements[level] == "T":
        return If(cond, nested_arm, plain_arm)
    return If(cond, plain_arm, nested_arm)


def build_ifnest_both(depth: int, sa: SiteAlloc, level: int = 0):
    """Full binary if/else tree: children in BOTH arms at every level."""
    cond = gt0(IFNEST_PARAMS[level])
    if level == depth - 1:
        return If(cond, body(sa.site()), body(sa.site()))
    return If(
        cond,
        body(build_ifnest_both(depth, sa, level + 1)),
        body(build_ifnest_both(depth, sa, level + 1)),
    )


def ifnest_files() -> list:
    files = []

    def mk(name, fn_name, depth, node_builder, note):
        sa = SiteAlloc()
        node = node_builder(sa)
        fn = FuncDef(
            "int",
            fn_name,
            tuple(("int", p) for p in IFNEST_PARAMS[:depth]),
            body(Decl("t", L(0)), node, sa.site(), Return(V("t"))),
        )
        files.append(
            TestFile(
                name=name,
                title="nested if/else",
                notes=[
                    note,
                    "Trace: t = t * 10 + k at each site; the final digit is the",
                    "merge-point site after the outermost if.",
                ],
                tests=[(fn, combos(depth))],
            )
        )

    for placements in ["T", "E"]:
        mk(
            f"ifnest_d2_{placements.lower()}.glsl",
            f"test_ifnest_d2_{placements.lower()}",
            2,
            lambda sa, p=placements: build_ifnest_chain(2, p, sa),
            f"Depth-2 if/else chain; inner if/else nested in the "
            f"{'then' if placements == 'T' else 'else'} arm (shape {placements}).",
        )
    for placements in ["TT", "TE", "ET", "EE"]:
        mk(
            f"ifnest_d3_{placements.lower()}.glsl",
            f"test_ifnest_d3_{placements.lower()}",
            3,
            lambda sa, p=placements: build_ifnest_chain(3, p, sa),
            f"Depth-3 if/else chain; nesting arms per level = {placements} "
            f"(T = then arm, E = else arm).",
        )
    mk(
        "ifnest_d2_both.glsl",
        "test_ifnest_d2_both",
        2,
        lambda sa: build_ifnest_both(2, sa),
        "Depth-2 full binary if/else tree: child if/else in BOTH arms.",
    )
    mk(
        "ifnest_d3_both.glsl",
        "test_ifnest_d3_both",
        3,
        lambda sa: build_ifnest_both(3, sa),
        "Depth-3 full binary if/else tree: child if/else in BOTH arms (8 leaves).",
    )

    # if-without-else nesting.
    def noelse_d2():
        sa = SiteAlloc()
        fn = FuncDef(
            "int",
            "test_ifnest_d2_noelse",
            (("int", "a"), ("int", "b")),
            body(
                Decl("t", L(0)),
                If(gt0("a"), body(sa.site(), If(gt0("b"), body(sa.site())), sa.site())),
                sa.site(),
                Return(V("t")),
            ),
        )
        return TestFile(
            name="ifnest_d2_noelse.glsl",
            title="nested if without else",
            notes=["Depth-2 if-in-if where neither if has an else arm."],
            tests=[(fn, combos(2))],
        )

    def noelse_d3():
        sa = SiteAlloc()
        fn = FuncDef(
            "int",
            "test_ifnest_d3_noelse",
            (("int", "a"), ("int", "b"), ("int", "c")),
            body(
                Decl("t", L(0)),
                If(
                    gt0("a"),
                    body(
                        sa.site(),
                        If(gt0("b"), body(sa.site(), If(gt0("c"), body(sa.site())))),
                        sa.site(),
                    ),
                ),
                sa.site(),
                Return(V("t")),
            ),
        )
        # Mixed: else-less if inside an if/else's else arm.
        sa2 = SiteAlloc()
        fn2 = FuncDef(
            "int",
            "test_ifnest_d3_noelse_in_else",
            (("int", "a"), ("int", "b"), ("int", "c")),
            body(
                Decl("t", L(0)),
                If(
                    gt0("a"),
                    body(sa2.site()),
                    body(
                        sa2.site(),
                        If(gt0("b"), body(If(gt0("c"), body(sa2.site())), sa2.site())),
                        sa2.site(),
                    ),
                ),
                sa2.site(),
                Return(V("t")),
            ),
        )
        return TestFile(
            name="ifnest_d3_noelse.glsl",
            title="nested if without else, depth 3",
            notes=[
                "Depth-3 else-less if chains, plus an else-less chain nested",
                "inside an if/else's else arm.",
            ],
            tests=[(fn, combos(3)), (fn2, combos(3))],
        )

    files.append(noelse_d2())
    files.append(noelse_d3())

    # else-if chains with nested if/else in the arms.
    def chain_d2():
        sa = SiteAlloc()
        inner = lambda: If(gt0("q"), body(sa.site()), body(sa.site()))  # noqa: E731
        fn = FuncDef(
            "int",
            "test_ifnest_chain_d2",
            (("int", "p"), ("int", "q")),
            body(
                Decl("t", L(0)),
                If(
                    B("==", V("p"), L(0)),
                    body(inner()),
                    (
                        If(
                            B("==", V("p"), L(1)),
                            body(inner()),
                            body(inner()),
                        ),
                    ),
                ),
                sa.site(),
                Return(V("t")),
            ),
        )
        runs = [(p, q) for p in (0, 1, 2) for q in (0, 1)]
        return TestFile(
            name="ifnest_chain_d2.glsl",
            title="else-if chain with nested if/else in every arm",
            notes=["3-way else-if chain on p; each arm holds an if/else on q."],
            tests=[(fn, runs)],
        )

    def chain_d3():
        sa = SiteAlloc()
        fn = FuncDef(
            "int",
            "test_ifnest_chain_d3",
            (("int", "p"), ("int", "q")),
            body(
                Decl("t", L(0)),
                If(
                    B("==", V("p"), L(0)),
                    # arm 0: inner 3-way else-if chain on q
                    (
                        If(
                            B("==", V("q"), L(0)),
                            body(sa.site()),
                            (
                                If(
                                    B("==", V("q"), L(1)),
                                    body(sa.site()),
                                    body(sa.site()),
                                ),
                            ),
                        ),
                    ),
                    (
                        If(
                            B("==", V("p"), L(1)),
                            body(sa.site()),
                            # arm 2: if/else on q
                            body(If(gt0("q"), body(sa.site()), body(sa.site()))),
                        ),
                    ),
                ),
                sa.site(),
                Return(V("t")),
            ),
        )
        runs = [(p, q) for p in (0, 1, 2) for q in (0, 1, 2)]
        return TestFile(
            name="ifnest_chain_d3.glsl",
            title="else-if chain nested inside an else-if chain arm",
            notes=["Outer 3-way chain on p; arm 0 holds another 3-way chain on q."],
            tests=[(fn, runs)],
        )

    files.append(chain_d2())
    files.append(chain_d3())
    return files


# ---------------------------------------------------------------------------
# Axis 2: branches inside loops / loops inside branches (loopif_*, ifloop_*, mix_*)
# ---------------------------------------------------------------------------

LOOP_KINDS = ["for", "while", "dowhile"]


def make_loop(kind: str, var: str, bound, body_stmts: tuple):
    """A loop running var = 0 .. bound-1 (bound >= 1 for dowhile)."""
    if kind == "for":
        return (For(var, 0, B("<", V(var), bound), body_stmts),)
    if kind == "while":
        return (
            Decl(var, L(0)),
            While(
                B("<", V(var), bound),
                body_stmts + (Asn(var, B("+", V(var), L(1))),),
            ),
        )
    if kind == "dowhile":
        return (
            Decl(var, L(0)),
            DoWhile(
                body_stmts + (Asn(var, B("+", V(var), L(1))),),
                B("<", V(var), bound),
            ),
        )
    raise AssertionError(kind)


def loopif_files() -> list:
    files = []
    for kind in LOOP_KINDS:
        # form: plain if
        sa = SiteAlloc()
        s_taken, s_always = sa.site(), sa.site()
        fn_if = FuncDef(
            "int",
            f"test_loopif_{kind}_if",
            (("int", "p"),),
            body(
                Decl("t", L(0)),
                *make_loop(kind, "i", L(3), body(If(B("<", V("i"), V("p")), body(s_taken)), s_always)),
                sa.site(),
                Return(V("t")),
            ),
        )
        files.append(
            TestFile(
                name=f"loopif_{kind}_if.glsl",
                title=f"if inside {kind} loop",
                notes=[f"3-iteration {kind} loop; if taken while i < p."],
                tests=[(fn_if, [(p,) for p in range(4)])],
            )
        )

        # form: if/else
        sa = SiteAlloc()
        s_then, s_else = sa.site(), sa.site()
        fn_ifelse = FuncDef(
            "int",
            f"test_loopif_{kind}_ifelse",
            (("int", "p"),),
            body(
                Decl("t", L(0)),
                *make_loop(kind, "i", L(3), body(If(B("<", V("i"), V("p")), body(s_then), body(s_else)))),
                sa.site(),
                Return(V("t")),
            ),
        )
        files.append(
            TestFile(
                name=f"loopif_{kind}_ifelse.glsl",
                title=f"if/else inside {kind} loop",
                notes=[f"3-iteration {kind} loop; then-arm while i < p, else-arm after."],
                tests=[(fn_ifelse, [(p,) for p in range(4)])],
            )
        )

        # form: else-if chain
        sa = SiteAlloc()
        s0, s1, s2 = sa.site(), sa.site(), sa.site()
        chain = If(
            B("<", V("i"), V("p")),
            body(s0),
            (If(B("==", V("i"), V("p")), body(s1), body(s2)),),
        )
        fn_chain = FuncDef(
            "int",
            f"test_loopif_{kind}_chain",
            (("int", "p"),),
            body(
                Decl("t", L(0)),
                *make_loop(kind, "i", L(3), body(chain)),
                sa.site(),
                Return(V("t")),
            ),
        )
        files.append(
            TestFile(
                name=f"loopif_{kind}_chain.glsl",
                title=f"else-if chain inside {kind} loop",
                notes=[f"3-iteration {kind} loop; 3-way chain on i vs p."],
                tests=[(fn_chain, [(p,) for p in range(4)])],
            )
        )
    return files


def ifloop_files() -> list:
    files = []
    for kind in LOOP_KINDS:
        # loop in then arm
        sa = SiteAlloc()
        s_pre, s_iter, s_post, s_else = sa.site(), sa.site(), sa.site(), sa.site()
        fn_then = FuncDef(
            "int",
            f"test_ifloop_{kind}_then",
            (("int", "p"),),
            body(
                Decl("t", L(0)),
                If(
                    gt0("p"),
                    body(s_pre, *make_loop(kind, "i", V("p"), body(s_iter)), s_post),
                    body(s_else),
                ),
                sa.site(),
                Return(V("t")),
            ),
        )
        # loop in else arm
        sa2 = SiteAlloc()
        s2_then, s2_pre, s2_iter, s2_post = sa2.site(), sa2.site(), sa2.site(), sa2.site()
        fn_else = FuncDef(
            "int",
            f"test_ifloop_{kind}_else",
            (("int", "p"),),
            body(
                Decl("t", L(0)),
                If(
                    B("==", V("p"), L(0)),
                    body(s2_then),
                    body(s2_pre, *make_loop(kind, "i", V("p"), body(s2_iter)), s2_post),
                ),
                sa2.site(),
                Return(V("t")),
            ),
        )
        # loops in both arms
        sa3 = SiteAlloc()
        s3_then_iter, s3_else_iter = sa3.site(), sa3.site()
        fn_both = FuncDef(
            "int",
            f"test_ifloop_{kind}_both",
            (("int", "p"),),
            body(
                Decl("t", L(0)),
                If(
                    gt0("p"),
                    body(*make_loop(kind, "i", L(2), body(s3_then_iter))),
                    body(*make_loop(kind, "j", L(3), body(s3_else_iter))),
                ),
                sa3.site(),
                Return(V("t")),
            ),
        )
        files.append(
            TestFile(
                name=f"ifloop_{kind}.glsl",
                title=f"{kind} loop nested inside branch arms",
                notes=[
                    f"{kind} loop in the then arm, the else arm, and both arms.",
                    "Loop bound comes from the branch-selecting parameter where possible.",
                ],
                tests=[
                    (fn_then, [(p,) for p in range(4)]),
                    (fn_else, [(p,) for p in range(4)]),
                    (fn_both, [(0,), (1,)]),
                ],
            )
        )
    return files


def mix_files() -> list:
    files = []

    # for { if { while } else { site } }
    sa = SiteAlloc()
    s_inner, s_else = sa.site(), sa.site()
    fn1 = FuncDef(
        "int",
        "test_mix_for_if_while",
        (("int", "p"),),
        body(
            Decl("t", L(0)),
            For(
                "i",
                0,
                B("<", V("i"), L(2)),
                body(
                    If(
                        B("<", V("i"), V("p")),
                        body(
                            Decl("j", L(0)),
                            While(
                                B("<", V("j"), L(2)),
                                body(s_inner, Asn("j", B("+", V("j"), L(1)))),
                            ),
                        ),
                        body(s_else),
                    ),
                ),
            ),
            sa.site(),
            Return(V("t")),
        ),
    )
    # while { if { for } }
    sa2 = SiteAlloc()
    s2_inner, s2_skip = sa2.site(), sa2.site()
    fn2 = FuncDef(
        "int",
        "test_mix_while_if_for",
        (("int", "p"),),
        body(
            Decl("t", L(0)),
            Decl("i", L(0)),
            While(
                B("<", V("i"), L(3)),
                body(
                    If(
                        B("==", V("i"), V("p")),
                        body(For("j", 0, B("<", V("j"), L(2)), body(s2_inner))),
                        body(s2_skip),
                    ),
                    Asn("i", B("+", V("i"), L(1))),
                ),
            ),
            sa2.site(),
            Return(V("t")),
        ),
    )
    files.append(
        TestFile(
            name="mix_loop_if_loop.glsl",
            title="loop-in-branch-in-loop mixes (for/while)",
            notes=["for{if{while}else{..}} and while{if{for}..} shapes."],
            tests=[(fn1, [(p,) for p in range(3)]), (fn2, [(p,) for p in range(4)])],
        )
    )

    # dowhile { if { for } } and for { if { dowhile } }
    sa3 = SiteAlloc()
    s3_inner, s3_skip = sa3.site(), sa3.site()
    fn3 = FuncDef(
        "int",
        "test_mix_dowhile_if_for",
        (("int", "p"),),
        body(
            Decl("t", L(0)),
            Decl("i", L(0)),
            DoWhile(
                body(
                    If(
                        B("<", V("i"), V("p")),
                        body(For("j", 0, B("<", V("j"), L(2)), body(s3_inner))),
                        body(s3_skip),
                    ),
                    Asn("i", B("+", V("i"), L(1))),
                ),
                B("<", V("i"), L(3)),
            ),
            sa3.site(),
            Return(V("t")),
        ),
    )
    sa4 = SiteAlloc()
    s4_inner, s4_else = sa4.site(), sa4.site()
    fn4 = FuncDef(
        "int",
        "test_mix_for_if_dowhile",
        (("int", "p"),),
        body(
            Decl("t", L(0)),
            For(
                "i",
                0,
                B("<", V("i"), L(2)),
                body(
                    If(
                        B("==", V("i"), V("p")),
                        body(
                            Decl("j", L(0)),
                            DoWhile(
                                body(s4_inner, Asn("j", B("+", V("j"), L(1)))),
                                B("<", V("j"), L(2)),
                            ),
                        ),
                        body(s4_else),
                    ),
                ),
            ),
            sa4.site(),
            Return(V("t")),
        ),
    )
    files.append(
        TestFile(
            name="mix_dowhile_if_for.glsl",
            title="loop-in-branch-in-loop mixes (do-while)",
            notes=["dowhile{if{for}..} and for{if{dowhile}else{..}} shapes."],
            tests=[(fn3, [(p,) for p in range(4)]), (fn4, [(p,) for p in range(3)])],
        )
    )
    return files


# ---------------------------------------------------------------------------
# Axis 3: break / continue at depth 1-2 (brk_*, cont_*, brkcont_*)
# ---------------------------------------------------------------------------


def brk_file(kind: str) -> TestFile:
    # d1, guard in then arm: break when i == p
    sa = SiteAlloc()
    s_a, s_b = sa.site(), sa.site()
    fn_then = FuncDef(
        "int",
        f"test_brk_{kind}_d1_then",
        (("int", "p"),),
        body(
            Decl("t", L(0)),
            *make_loop(
                kind, "i", L(4), body(s_a, If(B("==", V("i"), V("p")), body(Break())), s_b)
            ),
            sa.site(),
            Return(V("t")),
        ),
    )
    # d1, break in else arm
    sa2 = SiteAlloc()
    s2_a = sa2.site()
    fn_else = FuncDef(
        "int",
        f"test_brk_{kind}_d1_else",
        (("int", "p"),),
        body(
            Decl("t", L(0)),
            *make_loop(
                kind,
                "i",
                L(4),
                body(If(B("!=", V("i"), V("p")), body(s2_a), body(Break()))),
            ),
            sa2.site(),
            Return(V("t")),
        ),
    )
    # d2: break in inner loop exits inner only
    sa3 = SiteAlloc()
    s3_outer, s3_inner, s3_after = sa3.site(), sa3.site(), sa3.site()
    inner = make_loop(
        kind, "j", L(2), body(If(B("==", V("j"), V("p")), body(Break())), s3_inner)
    )
    fn_d2 = FuncDef(
        "int",
        f"test_brk_{kind}_d2_inner",
        (("int", "p"),),
        body(
            Decl("t", L(0)),
            *make_loop(kind, "i", L(2), body(s3_outer, *inner, s3_after)),
            sa3.site(),
            Return(V("t")),
        ),
    )
    # d2: break guarded by an if nested two deep
    sa4 = SiteAlloc()
    s4_iter = sa4.site()
    fn_guard = FuncDef(
        "int",
        f"test_brk_{kind}_d2_guard",
        (("int", "p"), ("int", "q")),
        body(
            Decl("t", L(0)),
            *make_loop(
                kind,
                "i",
                L(4),
                body(
                    If(
                        B(">=", V("i"), V("p")),
                        body(If(gt0("q"), body(Break()))),
                    ),
                    s4_iter,
                ),
            ),
            sa4.site(),
            Return(V("t")),
        ),
    )
    return TestFile(
        name=f"brk_{kind}.glsl",
        title=f"break in {kind} loops",
        notes=[
            "break guarded in then arm / else arm at depth 1; break in the inner",
            "loop of a nested pair (must exit inner only); break behind a",
            "depth-2 if guard.",
        ],
        tests=[
            (fn_then, [(p,) for p in range(5)]),
            (fn_else, [(p,) for p in range(5)]),
            (fn_d2, [(p,) for p in range(3)]),
            (fn_guard, [(p, q) for p in (0, 2, 4) for q in (0, 1)]),
        ],
    )


def cont_file(kind: str) -> TestFile:
    files_tests = []
    # d1: continue when i == p. For while/dowhile the increment must precede
    # continue to keep the loop terminating - that is exactly the interesting
    # edge (continue jumps to the condition, not the top of the body).
    sa = SiteAlloc()
    s_a, s_b = sa.site(), sa.site()
    if kind == "for":
        loop = make_loop(
            "for",
            "i",
            L(3),
            body(s_a, If(B("==", V("i"), V("p")), body(Continue())), s_b),
        )
    else:
        inner_body = body(
            s_a,
            Asn("i", B("+", V("i"), L(1))),
            If(B("==", V("i"), V("p")), body(Continue())),
            s_b,
        )
        if kind == "while":
            loop = (Decl("i", L(0)), While(B("<", V("i"), L(3)), inner_body))
        else:
            loop = (Decl("i", L(0)), DoWhile(inner_body, B("<", V("i"), L(3))))
    fn_d1 = FuncDef(
        "int",
        f"test_cont_{kind}_d1",
        (("int", "p"),),
        body(Decl("t", L(0)), *loop, sa.site(), Return(V("t"))),
    )
    files_tests.append((fn_d1, [(p,) for p in range(4)]))

    # d1: continue in else arm
    sa2 = SiteAlloc()
    s2_then, s2_b = sa2.site(), sa2.site()
    if kind == "for":
        loop2 = make_loop(
            "for",
            "i",
            L(3),
            body(If(B("==", V("i"), V("p")), body(s2_then), body(Continue())), s2_b),
        )
    else:
        inner_body2 = body(
            Asn("i", B("+", V("i"), L(1))),
            If(B("==", V("i"), V("p")), body(s2_then), body(Continue())),
            s2_b,
        )
        if kind == "while":
            loop2 = (Decl("i", L(0)), While(B("<", V("i"), L(3)), inner_body2))
        else:
            loop2 = (Decl("i", L(0)), DoWhile(inner_body2, B("<", V("i"), L(3))))
    fn_d1e = FuncDef(
        "int",
        f"test_cont_{kind}_d1_else",
        (("int", "p"),),
        body(Decl("t", L(0)), *loop2, sa2.site(), Return(V("t"))),
    )
    files_tests.append((fn_d1e, [(p,) for p in range(4)]))

    # d2: continue in inner loop affects inner only
    sa3 = SiteAlloc()
    s3_outer, s3_inner, s3_after = sa3.site(), sa3.site(), sa3.site()
    if kind == "for":
        inner3 = make_loop(
            "for",
            "j",
            L(2),
            body(If(B("==", V("j"), V("p")), body(Continue())), s3_inner),
        )
    else:
        inner_body3 = body(
            Asn("j", B("+", V("j"), L(1))),
            If(B("==", V("j"), V("p")), body(Continue())),
            s3_inner,
        )
        if kind == "while":
            inner3 = (Decl("j", L(0)), While(B("<", V("j"), L(2)), inner_body3))
        else:
            inner3 = (Decl("j", L(0)), DoWhile(inner_body3, B("<", V("j"), L(2))))
    fn_d2 = FuncDef(
        "int",
        f"test_cont_{kind}_d2_inner",
        (("int", "p"),),
        body(
            Decl("t", L(0)),
            *make_loop("for", "i", L(2), body(s3_outer, *inner3, s3_after)),
            sa3.site(),
            Return(V("t")),
        ),
    )
    files_tests.append((fn_d2, [(p,) for p in range(4)]))

    return TestFile(
        name=f"cont_{kind}.glsl",
        title=f"continue in {kind} loops",
        notes=[
            "continue guarded in then arm / else arm at depth 1; continue in the",
            "inner loop of a nested pair (must re-test the inner condition only).",
            "For while/do-while the induction increment precedes the continue,",
            "exercising the continue-to-condition edge.",
        ],
        tests=files_tests,
    )


def brkcont_files() -> list:
    files = [brk_file(k) for k in LOOP_KINDS] + [cont_file(k) for k in LOOP_KINDS]

    # Mixed: continue in outer for, break in inner while - and the reverse.
    sa = SiteAlloc()
    s_pre, s_inner, s_post = sa.site(), sa.site(), sa.site()
    fn1 = FuncDef(
        "int",
        "test_cont_outer_brk_inner",
        (("int", "p"), ("int", "q")),
        body(
            Decl("t", L(0)),
            For(
                "i",
                0,
                B("<", V("i"), L(2)),
                body(
                    If(B("==", V("i"), V("p")), body(Continue())),
                    s_pre,
                    Decl("j", L(0)),
                    While(
                        B("<", V("j"), L(2)),
                        body(
                            If(B("==", V("j"), V("q")), body(Break())),
                            s_inner,
                            Asn("j", B("+", V("j"), L(1))),
                        ),
                    ),
                    s_post,
                ),
            ),
            sa.site(),
            Return(V("t")),
        ),
    )
    sa2 = SiteAlloc()
    s2_pre, s2_inner = sa2.site(), sa2.site()
    fn2 = FuncDef(
        "int",
        "test_brk_outer_cont_inner",
        (("int", "p"), ("int", "q")),
        body(
            Decl("t", L(0)),
            Decl("i", L(0)),
            While(
                B("<", V("i"), L(3)),
                body(
                    Asn("i", B("+", V("i"), L(1))),
                    If(B("==", V("i"), V("p")), body(Break())),
                    s2_pre,
                    For(
                        "j",
                        0,
                        B("<", V("j"), L(2)),
                        body(If(B("==", V("j"), V("q")), body(Continue())), s2_inner),
                    ),
                ),
            ),
            sa2.site(),
            Return(V("t")),
        ),
    )
    files.append(
        TestFile(
            name="brkcont_mixed.glsl",
            title="break and continue split across nested loop levels",
            notes=[
                "continue in the outer loop with break in the inner loop, and the",
                "reverse; each must bind to its own loop.",
            ],
            tests=[
                (fn1, [(p, q) for p in (0, 1, 2) for q in (0, 1, 2)]),
                (fn2, [(p, q) for p in (1, 2, 4) for q in (0, 1, 2)]),
            ],
        )
    )

    # Both break and continue in the same loop body.
    sa3 = SiteAlloc()
    s3_a, s3_b = sa3.site(), sa3.site()
    fn3 = FuncDef(
        "int",
        "test_brkcont_same_for",
        (("int", "p"), ("int", "q")),
        body(
            Decl("t", L(0)),
            For(
                "i",
                0,
                B("<", V("i"), L(4)),
                body(
                    If(B("==", V("i"), V("p")), body(Continue())),
                    s3_a,
                    If(B("==", V("i"), V("q")), body(Break())),
                    s3_b,
                ),
            ),
            sa3.site(),
            Return(V("t")),
        ),
    )
    sa4 = SiteAlloc()
    s4_a, s4_b = sa4.site(), sa4.site()
    fn4 = FuncDef(
        "int",
        "test_brkcont_same_while",
        (("int", "p"), ("int", "q")),
        body(
            Decl("t", L(0)),
            Decl("i", L(0)),
            While(
                B("<", V("i"), L(4)),
                body(
                    Asn("i", B("+", V("i"), L(1))),
                    If(B("==", V("i"), V("p")), body(Continue())),
                    s4_a,
                    If(B("==", V("i"), V("q")), body(Break())),
                    s4_b,
                ),
            ),
            sa4.site(),
            Return(V("t")),
        ),
    )
    files.append(
        TestFile(
            name="brkcont_same_loop.glsl",
            title="break and continue in the same loop body",
            notes=["continue at i == p then break at i == q in one body (for and while)."],
            tests=[
                (fn3, [(p, q) for p in (0, 2, 5) for q in (1, 3, 5)]),
                (fn4, [(p, q) for p in (1, 3, 5) for q in (2, 4, 5)]),
            ],
        )
    )
    return files


# ---------------------------------------------------------------------------
# Axis 4: early returns from nested blocks (ret_*)
# ---------------------------------------------------------------------------


def ret_files() -> list:
    files = []

    # returns from nested if/else arms
    sa = SiteAlloc()
    fn_ifnest = FuncDef(
        "int",
        "test_ret_ifnest_d2",
        (("int", "a"), ("int", "b")),
        body(
            Decl("t", L(0)),
            If(
                gt0("a"),
                body(
                    sa.site(),
                    If(gt0("b"), body(Return(V("t"))), body(sa.site())),
                    sa.site(),
                ),
                body(If(gt0("b"), body(sa.site()), body(Return(L(-1))))),
            ),
            sa.site(),
            Return(V("t")),
        ),
    )
    sa2 = SiteAlloc()
    fn_ifnest3 = FuncDef(
        "int",
        "test_ret_ifnest_d3",
        (("int", "a"), ("int", "b"), ("int", "c")),
        body(
            Decl("t", L(0)),
            If(
                gt0("a"),
                body(
                    sa2.site(),
                    If(
                        gt0("b"),
                        body(If(gt0("c"), body(Return(V("t"))), body(sa2.site()))),
                        body(sa2.site()),
                    ),
                    sa2.site(),
                ),
            ),
            sa2.site(),
            Return(V("t")),
        ),
    )
    files.append(
        TestFile(
            name="ret_ifnest.glsl",
            title="early return from nested if/else arms",
            notes=[
                "Returns from a depth-2 then arm, a depth-2 else arm (distinct",
                "constant), and a depth-3 leaf; fall-through paths must still run",
                "the post-if sites.",
            ],
            tests=[(fn_ifnest, combos(2)), (fn_ifnest3, combos(3))],
        )
    )

    # return from inside each loop kind
    for kind in LOOP_KINDS:
        sa = SiteAlloc()
        s_a, s_b = sa.site(), sa.site()
        fn = FuncDef(
            "int",
            f"test_ret_{kind}",
            (("int", "p"),),
            body(
                Decl("t", L(0)),
                *make_loop(
                    kind,
                    "i",
                    L(3),
                    body(s_a, If(B("==", V("i"), V("p")), body(Return(V("t")))), s_b),
                ),
                sa.site(),
                Return(V("t")),
            ),
        )
        sa2 = SiteAlloc()
        s2_then = sa2.site()
        fn_else = FuncDef(
            "int",
            f"test_ret_{kind}_else",
            (("int", "p"),),
            body(
                Decl("t", L(0)),
                *make_loop(
                    kind,
                    "i",
                    L(3),
                    body(
                        If(B("!=", V("i"), V("p")), body(s2_then), body(Return(V("t")))),
                    ),
                ),
                sa2.site(),
                Return(V("t")),
            ),
        )
        files.append(
            TestFile(
                name=f"ret_{kind}.glsl",
                title=f"early return out of a {kind} loop",
                notes=[
                    "Return from the then arm mid-iteration and from an else arm;",
                    "the post-loop site must not run on returning paths.",
                ],
                tests=[
                    (fn, [(p,) for p in range(4)]),
                    (fn_else, [(p,) for p in range(4)]),
                ],
            )
        )

    # return from the inner loop of a nested pair
    sa = SiteAlloc()
    s_outer, s_inner = sa.site(), sa.site()
    fn_nested = FuncDef(
        "int",
        "test_ret_nested_loop",
        (("int", "p"), ("int", "q")),
        body(
            Decl("t", L(0)),
            For(
                "i",
                0,
                B("<", V("i"), L(3)),
                body(
                    s_outer,
                    Decl("j", L(0)),
                    While(
                        B("<", V("j"), L(2)),
                        body(
                            If(
                                B("&&", B("==", V("i"), V("p")), B("==", V("j"), V("q"))),
                                body(Return(V("t"))),
                            ),
                            s_inner,
                            Asn("j", B("+", V("j"), L(1))),
                        ),
                    ),
                ),
            ),
            sa.site(),
            Return(V("t")),
        ),
    )
    files.append(
        TestFile(
            name="ret_nested_loop.glsl",
            title="early return from the inner loop of a nested pair",
            notes=["Return fires when (i, j) == (p, q); both loops unwind at once."],
            tests=[(fn_nested, [(p, q) for p in (0, 1, 2, 3) for q in (0, 1)])],
        )
    )

    # return from a loop nested in a branch
    sa = SiteAlloc()
    s_iter, s_else = sa.site(), sa.site()
    fn_lb = FuncDef(
        "int",
        "test_ret_loop_in_if",
        (("int", "p"), ("int", "q")),
        body(
            Decl("t", L(0)),
            If(
                gt0("p"),
                body(
                    For(
                        "i",
                        0,
                        B("<", V("i"), L(3)),
                        body(If(B("==", V("i"), V("q")), body(Return(V("t")))), s_iter),
                    ),
                ),
                body(s_else),
            ),
            sa.site(),
            Return(V("t")),
        ),
    )
    sa2 = SiteAlloc()
    s2_iter, s2_then = sa2.site(), sa2.site()
    fn_lb2 = FuncDef(
        "int",
        "test_ret_dowhile_in_else",
        (("int", "p"), ("int", "q")),
        body(
            Decl("t", L(0)),
            If(
                gt0("p"),
                body(s2_then),
                body(
                    Decl("i", L(0)),
                    DoWhile(
                        body(
                            If(B("==", V("i"), V("q")), body(Return(V("t")))),
                            s2_iter,
                            Asn("i", B("+", V("i"), L(1))),
                        ),
                        B("<", V("i"), L(3)),
                    ),
                ),
            ),
            sa2.site(),
            Return(V("t")),
        ),
    )
    files.append(
        TestFile(
            name="ret_loop_in_if.glsl",
            title="early return from a loop nested inside a branch",
            notes=["for-in-then and do-while-in-else with a conditional return inside."],
            tests=[
                (fn_lb, [(p, q) for p in (0, 1) for q in (0, 2, 3)]),
                (fn_lb2, [(p, q) for p in (0, 1) for q in (0, 2, 3)]),
            ],
        )
    )
    return files


# ---------------------------------------------------------------------------
# Axis 5: short-circuit && / || with side-effecting right operands (sc_*)
# ---------------------------------------------------------------------------


def sc_reset() -> Asn:
    return Asn("g_trace", L(0))


def sc_files() -> list:
    files = []

    def sc_binop_file(op: str, tag: str) -> TestFile:
        fn = FuncDef(
            "int",
            f"test_sc_{tag}",
            (("int", "a"), ("int", "b")),
            body(
                sc_reset(),
                Decl("r", T(B(op, chk(1, V("a")), chk(2, V("b"))), L(1), L(2))),
                Return(B("+", B("*", V("g_trace"), L(10)), V("r"))),
            ),
        )
        # Right operand behind a call already evaluated once on the left.
        fn_dup = FuncDef(
            "int",
            f"test_sc_{tag}_chain3",
            (("int", "a"), ("int", "b"), ("int", "c")),
            body(
                sc_reset(),
                Decl(
                    "r",
                    T(
                        B(op, B(op, chk(1, V("a")), chk(2, V("b"))), chk(3, V("c"))),
                        L(1),
                        L(2),
                    ),
                ),
                Return(B("+", B("*", V("g_trace"), L(10)), V("r"))),
            ),
        )
        return TestFile(
            name=f"sc_{tag}.glsl",
            title=f"short-circuit {op} with side-effecting right operand",
            notes=[
                "chk(k, v) appends digit k to g_trace and returns v > 0, so the",
                "result exposes exactly which operands were evaluated and in what",
                "order. Wrongly evaluating a skipped operand changes the value.",
            ],
            globals=[G_TRACE],
            helpers=[CHK],
            tests=[
                (fn, combos(2)),
                (fn_dup, combos(3)),
            ],
        )

    files.append(sc_binop_file("&&", "and"))
    files.append(sc_binop_file("||", "or"))

    # precedence mixes
    fn_mix1 = FuncDef(
        "int",
        "test_sc_and_or",
        (("int", "a"), ("int", "b"), ("int", "c")),
        body(
            sc_reset(),
            Decl(
                "r",
                T(
                    B("||", B("&&", chk(1, V("a")), chk(2, V("b"))), chk(3, V("c"))),
                    L(1),
                    L(2),
                ),
            ),
            Return(B("+", B("*", V("g_trace"), L(10)), V("r"))),
        ),
    )
    fn_mix2 = FuncDef(
        "int",
        "test_sc_or_and",
        (("int", "a"), ("int", "b"), ("int", "c")),
        body(
            sc_reset(),
            Decl(
                "r",
                T(
                    B("||", chk(1, V("a")), B("&&", chk(2, V("b")), chk(3, V("c")))),
                    L(1),
                    L(2),
                ),
            ),
            Return(B("+", B("*", V("g_trace"), L(10)), V("r"))),
        ),
    )
    fn_mix3 = FuncDef(
        "int",
        "test_sc_and_grouped_or",
        (("int", "a"), ("int", "b"), ("int", "c")),
        body(
            sc_reset(),
            Decl(
                "r",
                T(
                    B("&&", chk(1, V("a")), B("||", chk(2, V("b")), chk(3, V("c")))),
                    L(1),
                    L(2),
                ),
            ),
            Return(B("+", B("*", V("g_trace"), L(10)), V("r"))),
        ),
    )
    files.append(
        TestFile(
            name="sc_chain.glsl",
            title="mixed && / || chains with side effects",
            notes=[
                "a && b || c, a || b && c, and a && (b || c): the skip set depends",
                "on precedence and grouping.",
            ],
            globals=[G_TRACE],
            helpers=[CHK],
            tests=[(fn_mix1, combos(3)), (fn_mix2, combos(3)), (fn_mix3, combos(3))],
        )
    )

    # (a && b) || (c && d)
    fn_nested = FuncDef(
        "int",
        "test_sc_nested_groups",
        (("int", "a"), ("int", "b"), ("int", "c"), ("int", "d")),
        body(
            sc_reset(),
            Decl(
                "r",
                T(
                    B(
                        "||",
                        B("&&", chk(1, V("a")), chk(2, V("b"))),
                        B("&&", chk(3, V("c")), chk(4, V("d"))),
                    ),
                    L(1),
                    L(2),
                ),
            ),
            Return(B("+", B("*", V("g_trace"), L(10)), V("r"))),
        ),
    )
    files.append(
        TestFile(
            name="sc_nested.glsl",
            title="(a && b) || (c && d) with side effects",
            notes=["All 16 input combinations; each has a distinct evaluation trace."],
            globals=[G_TRACE],
            helpers=[CHK],
            tests=[(fn_nested, combos(4))],
        )
    )

    # short-circuit inside an if condition
    sa = SiteAlloc()
    s_then, s_else = sa.site(), sa.site()
    fn_if_and = FuncDef(
        "int",
        "test_sc_in_if_and",
        (("int", "a"), ("int", "b")),
        body(
            Decl("t", L(0)),
            sc_reset(),
            If(B("&&", chk(1, V("a")), chk(2, V("b"))), body(s_then), body(s_else)),
            Return(B("+", B("*", V("g_trace"), L(10)), V("t"))),
        ),
    )
    sa2 = SiteAlloc()
    s2_then, s2_else = sa2.site(), sa2.site()
    fn_if_or = FuncDef(
        "int",
        "test_sc_in_if_or",
        (("int", "a"), ("int", "b")),
        body(
            Decl("t", L(0)),
            sc_reset(),
            If(B("||", chk(1, V("a")), chk(2, V("b"))), body(s2_then), body(s2_else)),
            Return(B("+", B("*", V("g_trace"), L(10)), V("t"))),
        ),
    )
    files.append(
        TestFile(
            name="sc_in_if.glsl",
            title="short-circuit operators as if conditions",
            notes=["The branch taken and the evaluation trace must both be right."],
            globals=[G_TRACE],
            helpers=[CHK],
            tests=[(fn_if_and, combos(2)), (fn_if_or, combos(2))],
        )
    )

    # short-circuit inside a while condition (side effects run once per test)
    fn_while = FuncDef(
        "int",
        "test_sc_in_while",
        (("int", "p"),),
        body(
            sc_reset(),
            Decl("i", L(0)),
            While(chk(1, B("-", V("p"), V("i"))), body(Asn("i", B("+", V("i"), L(1))))),
            Return(B("+", B("*", V("g_trace"), L(10)), V("i"))),
        ),
    )
    fn_while_and = FuncDef(
        "int",
        "test_sc_in_while_and",
        (("int", "p"),),
        body(
            sc_reset(),
            Decl("i", L(0)),
            While(
                B("&&", B("<", V("i"), L(3)), chk(2, B("-", V("p"), V("i")))),
                body(Asn("i", B("+", V("i"), L(1)))),
            ),
            Return(B("+", B("*", V("g_trace"), L(10)), V("i"))),
        ),
    )
    files.append(
        TestFile(
            name="sc_in_while.glsl",
            title="side-effecting while conditions",
            notes=[
                "The condition call must run exactly once per test, including the",
                "final failing test; the RHS of && must be skipped once i reaches 3.",
            ],
            globals=[G_TRACE],
            helpers=[CHK],
            tests=[(fn_while, [(p,) for p in range(4)]), (fn_while_and, [(p,) for p in range(5)])],
        )
    )

    # short-circuit inside a ternary condition
    fn_tern = FuncDef(
        "int",
        "test_sc_in_ternary",
        (("int", "a"), ("int", "b")),
        body(
            sc_reset(),
            Decl("r", T(B("&&", chk(1, V("a")), chk(2, V("b"))), L(7), L(8))),
            Return(B("+", B("*", V("g_trace"), L(10)), V("r"))),
        ),
    )
    fn_tern_or = FuncDef(
        "int",
        "test_sc_in_ternary_or",
        (("int", "a"), ("int", "b")),
        body(
            sc_reset(),
            Decl("r", T(B("||", chk(1, V("a")), chk(2, V("b"))), L(7), L(8))),
            Return(B("+", B("*", V("g_trace"), L(10)), V("r"))),
        ),
    )
    files.append(
        TestFile(
            name="sc_in_ternary.glsl",
            title="short-circuit operators as ternary conditions",
            notes=["(chk && chk) ? 7 : 8 and the || variant."],
            globals=[G_TRACE],
            helpers=[CHK],
            tests=[(fn_tern, combos(2)), (fn_tern_or, combos(2))],
        )
    )
    return files


# ---------------------------------------------------------------------------
# Axis 6: ternaries nested in branch conditions (terncond_*)
# ---------------------------------------------------------------------------


def terncond_files() -> list:
    files = []

    sa = SiteAlloc()
    s_then, s_else = sa.site(), sa.site()
    fn_if = FuncDef(
        "int",
        "test_terncond_if",
        (("int", "p"), ("int", "a"), ("int", "b")),
        body(
            Decl("t", L(0)),
            If(B(">", T(gt0("p"), V("a"), V("b")), L(0)), body(s_then), body(s_else)),
            sa.site(),
            Return(V("t")),
        ),
    )
    sa2 = SiteAlloc()
    s2_then, s2_else = sa2.site(), sa2.site()
    fn_if_both = FuncDef(
        "int",
        "test_terncond_if_both_sides",
        (("int", "p"), ("int", "q")),
        body(
            Decl("t", L(0)),
            If(
                B(">", T(gt0("p"), L(3), L(1)), T(gt0("q"), L(2), L(0))),
                body(s2_then),
                body(s2_else),
            ),
            sa2.site(),
            Return(V("t")),
        ),
    )
    files.append(
        TestFile(
            name="terncond_if.glsl",
            title="ternary inside an if condition",
            notes=["if ((p > 0 ? a : b) > 0) and a comparison with ternaries on both sides."],
            tests=[
                (fn_if, [(p, a, b) for p in (0, 1) for a in (0, 1) for b in (0, 1)]),
                (fn_if_both, combos(2)),
            ],
        )
    )

    # ternary in loop conditions
    sa = SiteAlloc()
    s_iter = sa.site()
    fn_for = FuncDef(
        "int",
        "test_terncond_for",
        (("int", "p"),),
        body(
            Decl("t", L(0)),
            For("i", 0, B("<", V("i"), T(gt0("p"), L(2), L(3))), body(s_iter)),
            sa.site(),
            Return(V("t")),
        ),
    )
    sa2 = SiteAlloc()
    s2_iter = sa2.site()
    fn_while = FuncDef(
        "int",
        "test_terncond_while",
        (("int", "p"), ("int", "q")),
        body(
            Decl("t", L(0)),
            Decl("i", L(0)),
            While(
                B("<", V("i"), T(gt0("p"), V("q"), L(2))),
                body(s2_iter, Asn("i", B("+", V("i"), L(1)))),
            ),
            sa2.site(),
            Return(V("t")),
        ),
    )
    sa3 = SiteAlloc()
    s3_iter = sa3.site()
    fn_dowhile = FuncDef(
        "int",
        "test_terncond_dowhile",
        (("int", "p"),),
        body(
            Decl("t", L(0)),
            Decl("i", L(0)),
            DoWhile(
                body(s3_iter, Asn("i", B("+", V("i"), L(1)))),
                B("<", V("i"), T(gt0("p"), L(3), L(1))),
            ),
            sa3.site(),
            Return(V("t")),
        ),
    )
    files.append(
        TestFile(
            name="terncond_loop.glsl",
            title="ternary inside loop conditions",
            notes=[
                "The loop bound itself is a ternary; it is re-evaluated on every",
                "iteration test.",
            ],
            tests=[
                (fn_for, [(0,), (1,)]),
                (fn_while, [(p, q) for p in (0, 1) for q in (0, 1, 3)]),
                (fn_dowhile, [(0,), (1,)]),
            ],
        )
    )

    # nested ternaries as conditions
    sa = SiteAlloc()
    s_then, s_else = sa.site(), sa.site()
    fn_nested = FuncDef(
        "int",
        "test_terncond_nested",
        (("int", "a"), ("int", "b"), ("int", "c")),
        body(
            Decl("t", L(0)),
            If(
                B(">", T(B(">", T(gt0("a"), V("b"), V("c")), L(0)), L(1), L(0)), L(0)),
                body(s_then),
                body(s_else),
            ),
            sa.site(),
            Return(V("t")),
        ),
    )
    sa2 = SiteAlloc()
    s2_then, s2_else = sa2.site(), sa2.site()
    fn_bool_arms = FuncDef(
        "int",
        "test_terncond_bool_arms",
        (("int", "a"), ("int", "b"), ("int", "c")),
        body(
            Decl("t", L(0)),
            If(
                T(gt0("a"), B(">", V("b"), L(0)), B(">", V("c"), L(0))),
                body(s2_then),
                body(s2_else),
            ),
            sa2.site(),
            Return(V("t")),
        ),
    )
    files.append(
        TestFile(
            name="terncond_nested.glsl",
            title="nested ternaries as branch conditions",
            notes=[
                "A ternary whose condition is itself built from a ternary, and a",
                "ternary with boolean arms used directly as an if condition.",
            ],
            tests=[(fn_nested, combos(3)), (fn_bool_arms, combos(3))],
        )
    )

    # ternary condition with side-effecting arms
    fn_se = FuncDef(
        "int",
        "test_terncond_sideeffect",
        (("int", "a"), ("int", "b"), ("int", "c")),
        body(
            Decl("t", L(0)),
            sc_reset(),
            If(
                T(chk(1, V("a")), chk(2, V("b")), chk(3, V("c"))),
                body(Asn("t", L(7))),
                body(Asn("t", L(8))),
            ),
            Return(B("+", B("*", V("g_trace"), L(10)), V("t"))),
        ),
    )
    files.append(
        TestFile(
            name="terncond_sideeffect.glsl",
            title="side-effecting ternary as an if condition",
            notes=[
                "if ((chk(a) ? chk(b) : chk(c))): exactly two chk calls run per",
                "test and the trace exposes which.",
            ],
            globals=[G_TRACE],
            helpers=[CHK],
            tests=[(fn_se, combos(3))],
        )
    )
    return files


# ---------------------------------------------------------------------------
# Corpus assembly + README
# ---------------------------------------------------------------------------


def build_corpus() -> list:
    files = []
    files += ifnest_files()
    files += loopif_files()
    files += ifloop_files()
    files += mix_files()
    files += brkcont_files()
    files += ret_files()
    files += sc_files()
    files += terncond_files()
    names = [f.name for f in files]
    assert len(names) == len(set(names)), "duplicate file names in corpus"
    return files


README = """# Control-flow torture corpus

Generated regression corpus for structured-control-flow lowering. Motivated by
the 2026-07-08 nested-if/else LPIR-interpreter bug: control-flow bug classes
are combinatorial, so this directory systematically enumerates the shapes
instead of keeping one hand-written example per category.

**GENERATED FILES — do not edit by hand.** Regenerate with:

```bash
{regen}
```

The generator is deterministic (pure enumeration, no randomness). Expected
values are computed by a reference integer interpreter inside the generator;
all programs are bounded and terminate. Verify all backends agree before
committing:

```bash
scripts/filetests.sh --target rv32n.q32,rv32c.q32,wasm.q32 control/torture/
```

## Trace encoding

Every test function threads an int accumulator through distinct "sites":
`t = t * 10 + k` with a per-site digit `k`. The returned value is therefore a
base-10 execution trace — a wrong branch, a skipped merge point, an extra loop
iteration, or a re-ordered side effect each produce a different value.
Short-circuit tests use a global `g_trace` mutated by a helper `chk(k, v)`,
so the value also proves which operands were (not) evaluated, in order.

## Short-circuit `&&` / `||`

GLSL requires `&&` and `||` to short-circuit, and both frontends lower
side-effecting right operands to control flow (naga glsl-in via the
third_party/naga fork; lps-glsl natively). Expected values in this corpus
are the GLSL-correct short-circuit results. The corpus previously carried
`@broken(rv32n.q32) @broken(rv32c.q32) @broken(wasm.q32)` on every directive
whose value differed under the old eager lowering; those were removed when
the lowering was fixed. Ternary conditions and arms also evaluate lazily
(see `terncond_sideeffect.glsl`).

## Enumeration axes

| Prefix        | Axis                                                              |
| ------------- | ----------------------------------------------------------------- |
| `ifnest_*`    | if/else nested in then vs else arms to depth 3: chain shapes (`d3_te` = child in Then arm at depth 1, Else arm at depth 2), full binary trees (`both`), else-less chains (`noelse`), else-if chains (`chain`) |
| `loopif_*`    | branches inside loops: {{for, while, dowhile}} x {{if, if/else, else-if chain}} |
| `ifloop_*`    | loops inside branches: {{for, while, dowhile}} in then / else / both arms |
| `mix_*`       | loop-in-branch-in-loop, mixed loop kinds                          |
| `brk_*`       | break at depth 1 (then/else guard) and depth 2 (inner loop only, nested guard) per loop kind |
| `cont_*`      | continue, same enumeration; while/do-while variants exercise the continue-to-condition edge |
| `brkcont_*`   | break and continue mixed across nesting levels / in one body      |
| `ret_*`       | early returns from nested ifs, from each loop kind, from inner loops of nested pairs, from loops inside branches |
| `sc_*`        | short-circuit `&&`/`||` whose right operand calls a global-mutating function: bare ops, precedence chains, nested groups, and as if/while/ternary conditions |
| `terncond_*`  | ternaries nested in branch conditions: if conditions, loop bounds, nested ternaries, side-effecting arms |

Each file holds one enumerated shape with `// run:` directives covering every
(reachable) combination of the branch-selecting parameters, so file names are
regular and machine-friendly — the metamorphic fuzzing harness
(compiler-robustness roadmap M4) seeds from this corpus.
""".format(regen=REGEN_CMD)


def main() -> int:
    mode = "list"
    for arg in sys.argv[1:]:
        if arg == "--write":
            mode = "write"
        elif arg == "--check":
            mode = "check"
        elif arg in ("-h", "--help"):
            print(__doc__)
            return 0
        else:
            print(f"unknown argument: {arg}", file=sys.stderr)
            return 2

    files = build_corpus()
    rendered = {f.name: f.render() for f in files}
    rendered["README.md"] = README

    if mode == "list":
        print(f"Would write {len(rendered)} files to {TORTURE_DIR}:")
        for name in sorted(rendered):
            print(f"  {name}")
        print("\nRun with --write to write, --check to diff against disk.")
        return 0

    if mode == "write":
        os.makedirs(TORTURE_DIR, exist_ok=True)
        stale = [
            n
            for n in os.listdir(TORTURE_DIR)
            if (n.endswith(".glsl") or n == "README.md") and n not in rendered
        ]
        for name in sorted(rendered):
            path = os.path.join(TORTURE_DIR, name)
            with open(path, "w", encoding="utf-8") as fh:
                fh.write(rendered[name])
            print(f"wrote {os.path.relpath(path)}")
        for name in stale:
            print(f"WARNING: stale file not in generator output: {name}", file=sys.stderr)
        print(f"\n{len(rendered)} files written to {TORTURE_DIR}")
        return 0

    # --check
    bad = []
    for name in sorted(rendered):
        path = os.path.join(TORTURE_DIR, name)
        if not os.path.exists(path):
            bad.append(f"missing: {name}")
            continue
        with open(path, encoding="utf-8") as fh:
            if fh.read() != rendered[name]:
                bad.append(f"differs: {name}")
    if bad:
        print("torture corpus is out of sync with the generator:", file=sys.stderr)
        for b in bad:
            print(f"  {b}", file=sys.stderr)
        print(f"\nRegenerate with: {REGEN_CMD}", file=sys.stderr)
        return 1
    print(f"torture corpus in sync ({len(rendered)} files)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
