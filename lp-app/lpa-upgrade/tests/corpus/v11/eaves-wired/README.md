# Eaves, as the house is wired

The eaves project (`../eaves`) wired the way the house really is: the
controller sits at the corner where the two runs meet, so D10 feeds the
front run from that corner — its far end as drawn, so it runs reversed —
and D9 feeds the side run from the same corner. Both are patched by hand
(`flow: manual`), so neither moves when D10's count changes.

It is the truth the lab's piece is wired by when you open "Sean's eaves"
in the lab (plan `lp2025/2026-10-18-2337-explicit-patching-spike`, J2): the
canvas opens the plain eaves on auto, the piece shows the house, and the
walk patches the one to match the other. Written by the lab's own save.

`cargo test -p lab-mapping-fit-tests eaves_wired`
