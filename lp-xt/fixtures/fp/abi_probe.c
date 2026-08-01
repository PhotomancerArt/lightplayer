/* The toolchain FP-ABI probe (M6 P4 §6).
 *
 * The question: which FRs does the esp toolchain treat as callee-saved?
 *
 * It matters because M5 compiles the f32 builtins with this toolchain at -O3
 * and M7 has to lay out a frame that survives calls into them — and the FR file
 * is FLAT. Unlike the AR file, whose preservation across a windowed call is free
 * by rotation, an FR value that must outlive a call has to be spilled by
 * somebody, and the ABI says who.
 *
 * The probe needs no hardware: compile, disassemble, and read which FRs are
 * stored to the frame before the call and reloaded after it. Six live values
 * across the call, so a toolchain with any callee-saved FRs at all has room to
 * use them.
 *
 * Run `abi_probe.sh` for the answer and the raw disassembly.
 */

extern float ext(float);

float probe(float a, float b, float c, float d, float e, float f) {
    float p = a * 2.0f;
    float q = b * 3.0f;
    float r = c * 5.0f;
    float s = d * 7.0f;
    float t = e * 11.0f;
    float u = f * 13.0f;
    /* Every one of q..u is live ACROSS this call. */
    float w = ext(p);
    return w + q + r + s + t + u;
}
