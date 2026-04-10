## Register Allocation Improvements

### Interval Representation

Currently using simple (start, end) intervals in linear scan. Future improvements:

- **Segmented intervals**: Support holes in live ranges for better allocation
- **Use positions**: Track specific use points for better spill heuristics
- **Interval splitting**: Split at call boundaries to use callee-saved registers for second half
- **Coalescing**: Eliminate redundant moves between intervals

## Memory Improvements

pub clobbered: BTreeSet<PhysReg>, // todo: PRegSet??
vec use in general in lpvm-native
