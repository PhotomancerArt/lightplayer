# Needed for demo

- globals
- _out_ support for arguments as pointers for ABI/JIT
- trap recovery / stack recovery
- stack overflow trap
- cycle counting of some sort to prevent infinite loops

# Safety

- instruction counter limit trap
- output parameter support (for frexp/modf)

# Performance

- filetests should compile whole file, and only compile a single function when in detail mode

# Code size

- we switched to regalloc_algorithm=single_pass, can we remove a dependency due to this?

# Builtins

- pack/unpack functions frontend codegen
- integer bit functions frontend codegen
- floatBitsToInt/intBitsToFloat frontend codegen
