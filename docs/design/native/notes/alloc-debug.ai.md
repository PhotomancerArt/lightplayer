== Allocation trace for test_native_call_in_loop ===

```
v3
v3  v1
    v1  v4    <--- leave a gap on columns for clarity, so we don't assign another reg to a column until there is a blank row
v2  v1  v4    <--- but we reuse column 1 here for v2 so the display doesn't get very wide
v2  v1
v2  v1
```

```
             vinst  lpir inst
v2           [  0] [  0] IConst32  v2 = 1  # live(v2), v2 = x7
v2           [  1] [  1] BrIf      !v2, 1  # expire(v2), free x7
    v3       [  2] [  2] IConst32  v3 = 327680  # live(v3), v3 = x7
v4  v3       [  3] [  3] Call      v4 = native_branch_helper(v0, v3)  # expire(v3), free_reg(x7), live(v4), assign_reg(v4, x7)
v4      v1   [  4] [  4] Mov32     v1 = v4  # expire(v4), free x7, live(v1), v1 = x7
        v1   [  5] [  1] Br        Label(2)
        v1   [  6] [  5] Label(1)
```
