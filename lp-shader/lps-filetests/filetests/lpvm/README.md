# LPVM backend-specific filetests

Tests in this directory exercise behavior tied to a specific LPVM backend or ABI
(e.g. native RV32 calls, spill pressure), rather than general GLSL spec coverage.

They complement the main `filetests/` tree and are run with the same harness
(`lps-filetests` targets such as `rv32n.q32`).
