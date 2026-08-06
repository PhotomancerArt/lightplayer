# What's a shader?

```embed hero-preview example=examples/plasma
```

That's a shader. It's running right now, in your browser, on a
simulated LED board — nothing was installed, nothing was flashed. What
you're watching is the classic "plasma" pattern being computed, color
by color, sixty-ish times a second.

If you've used WLED, you already know the word *effect*: you scroll a
list, pick one, and it runs. A shader plays the same role — it's the
thing that decides what your LEDs do — with one difference that changes
everything: **a shader isn't a menu item. It's a friendly little
program, and you can open it.**

We'll get there in three steps, and you can touch everything along the
way.

## First, the part you already know

Here are the plasma shader's knobs. Drag them.

```embed panel sim=disc,grid mode=interactive
```

Familiar, right? Speed and scale, just like a WLED effect's sliders.
The sim above responds live as you drag — go ahead and make a mess.
(The Reset button in the corner puts everything back the way it was.
You can't break anything on this page, which is rather the point of a
simulator.)

So far, LightPlayer looks like WLED with different paint. Here's where
the floor opens.

## The reveal

Those knobs aren't settings we hard-coded for you. Each one is a line
in the shader itself. This is the entire plasma program — about fifteen
lines — with the line behind the **Scale** knob highlighted:

```embed code-figure src=plasma-shader
```

That violet highlight isn't decoration. In Studio, violet always means
*bound* — a value wired up so something else can drive it. The shader's
author wrote `scale` into the math, declared "this is a knob," and
LightPlayer did the rest: the slider you dragged a moment ago exists
*because of that line*.

Which means the effects list isn't a wall anymore. Want a knob WLED
never gave you? Add a line. Want the plasma to breathe instead of
scroll? Change the math. Anyone can do this — the [editor](#/docs/guide)
is built for it, and shaders this size are genuinely small.

## One effect, any shape

Here's the same shader — the exact same fifteen lines, byte for byte —
running on two different physical layouts at once. The disc you've been
watching, and a 16×16 grid:

```embed sim-canvas sim=disc view=map
```

```embed sim-canvas sim=grid view=map
```

Notice they're both still listening to the knobs above. In WLED, 2D
setups live in a separate world with their own effect list. Here
there's no separate world: a shader computes color *at a position*, and
a **mapping** tells LightPlayer where your LEDs actually are — strip,
ring, grid, or a dome you soldered at 2 a.m. Write the effect once and
it runs on every shape you own.

(How mappings work is its own good question — that page is on its way.)

## Make it yours

Reading about it only gets you so far. Open this very shader in the
real editor — same plasma, same knobs, plus everything else Studio can
do:

```embed open-in-studio example=examples/plasma
```

It lands in your projects, it's yours, and you can always reset it.
Break it with confidence.

## Where next

- [Brightness and smooth fades](#/docs/brightness-and-smooth-fades) —
  what those output settings actually do to your colors.
- [The guide](#/docs/guide) — the map of everything else.
