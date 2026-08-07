# What's a shader?

```embed sim-canvas sim=main view=product
```

```embed panel sim=main mode=interactive
```

That's a shader: a friendly little program that decides what your LEDs
do. This one is the classic "plasma," and it's running right now, in
your browser, on a simulated board. The knobs are its controls — drag
them and the picture answers.

If you're coming from WLED, here's the one-sentence version: a shader
is what an *effect* would be if you could open it.

## Any shape you've got

A shader draws a picture. It doesn't know or care where your LEDs are —
a **mapping** tells LightPlayer where each one actually sits, and the
picture gets projected onto them. Same shader, same knobs, two very
different shapes, live:

```embed sim-canvas sim=main view=map fixture=disc
```

```embed sim-canvas sim=main view=map fixture=grid
```

Both of these are fixtures in one project, fed by the one shader above.
Strip, ring, matrix, dome — write the effect once and point it at every
shape you own. (In WLED, 2D lives in a separate world with its own
effect list. Here there's no separate world.)

## Now edit it

This is the whole program — about fifteen lines — and it's live. Change
something and watch the sims above follow:

```embed editor sim=main
```

Some things to try:

- **Slow it down:** the numbers `13.0`, `9.0`, `11.0`, `15.0` are how
  fast each wave rides the base cycle. Halve one.
- **Meet a knob:** the `scale` on line 3 *is* the Scale knob above —
  the shader declared it, LightPlayer built the slider. (That's what
  violet means everywhere in Studio: *bound* — wired so something else
  can drive it.)
- **Repaint it:** pick a different palette from the panel above — the
  colors are a *value* now, not code. Or squeeze more of the ramp into
  the picture: change `vec2(hue, 0.0)` on the last line to
  `vec2(hue * 2.0, 0.0)`.

Breaking it is fine — a typo shows its error right in the editor, the
LEDs keep their last good frame, and **Reset** puts everything back the
way it was. That's what simulators are for.

## Make it yours

The page you're on is a sandbox; the real editor is one click away —
same project, plus everything else Studio can do:

```embed open-in-studio example=examples/plasma-duo
```

It lands in your projects and it's yours to keep, break, and rebuild.

## For shader engineers

You don't need any of this yet — come back when you want the machinery.

**The dialect.** Shaders are GLSL with one entry point:
`vec4 render(vec2 pos)` returns the color at `pos`, called for every
position each frame. Inputs arrive as `layout(binding = N) uniform`
declarations. No `main()`, no varyings, no version pragma.

**Knobs are declared, not built.** The shader's sidecar (`shader.json`)
lists what it consumes: a plain value (`scale`, with min/max/default)
or a phasor (a cycle position the clock advances). Anything bound to a
channel shows up on the panel automatically — declaring an input *is*
publishing a knob:

```embed code-figure src=plasma-shader
```

**Deterministic math.** Shaders run in fixed-point by default, compiled
on the device itself — the same frames render in your browser's sim and
on the chip. That's also why the plasma rides one `phase` cycle with
whole-number multiples: exactness survives the wrap.

**The library.** The standard GLSL functions are there — trig, `exp`
family, `mix`, `clamp`, `smoothstep`, vector ops — plus extras like
`hsv2rgb` and `hash`, and `sampler2D` textures. Your own helper
functions work as you'd expect, which means portable snippet libraries
like [lygia](https://lygia.xyz) are a good hunting ground.

## Where next

- [Brightness and smooth fades](#/docs/brightness-and-smooth-fades) —
  what the output settings do to your colors.
- [The guide](#/docs/guide) — the map of everything else.
