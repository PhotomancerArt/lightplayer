# Questions

Most questions have been answered in the previous planning session. This document captures those decisions:

## Answered Questions

1. ✅ **Should we create a single unified error formatting function that all error paths use?**
   - **Answer:** Yes, one unified function

2. ✅ **How should we handle the DEBUG mode check?**
   - **Answer:** Replace `show_full_output` boolean with an enum: `OutputMode { Summary, Detail, Debug }`
   - Pass this enum as a parameter to the unified error formatter

3. ✅ **Should the rerun commands include both `DEBUG=1` and non-DEBUG versions?**
   - **Answer:** Yes, show both commands with labels:
     ```
     Rerun just this test:
       scripts/glsl-filetests.sh filename:line
     
     Rerun with debugging:
       DEBUG=1 scripts/glsl-filetests.sh filename:line
     ```

4. ✅ **Should we extract the debug info formatting into separate functions?**
   - **Answer:** Keep one function (`format_debug_info`) to ensure consistent section headers. Reorder sections to match desired order.

5. ✅ **How should we handle cases where the executable is not available?**
   - **Answer:** Pass `None` for the executable parameter. Use wrapper functions to avoid passing lots of `None` values.

6. ✅ **Should the GLSL section always show the bootstrap code?**
   - **Answer:** Always show what was actually compiled (the isolated test code). Rename "bootstrap" → "test glsl".

7. ✅ **File structure approach?**
   - **Answer:** Total rewrite with semantic file organization. Separate summary and detail modes. One concept per file.

## Remaining Questions

None - ready to proceed with implementation.

