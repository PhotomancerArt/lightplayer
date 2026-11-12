# LightPlayer intent

LightPlayer is a lighting control and visualization system for building interactive light art that scales from microcontrollers running 100s of LEDs to Burning Man-scale art with ~100k LEDs.

It is written in rust and the main targets are ESP32 microcontrollers.

GLSL Shaders are the main source of visual data and processing.
On embedded, they run using a custom JIT.

The visualization engine is a demand-driven dataflow graph with output as the demand roots and shaders and inputs as the leaves.

## Intent history

I started LightPlayer in November 2025 as an experiment in AI-driven software engineering.

It grew rapidly as agentic tools improved and has powered several small pieces of LED art.
I did most of the development using chat-powered approaches.

In November 2026, I became disillusioned with the process because I did not feel that I understood or owned the [theory of the software](https://gwern.net/doc/cs/algorithm/1985-naur.pdf) anymore.
This was evidenced by poor experiences I had using the Studio UX, which was the most agentic part of the user-facing code.

Coinciding with similar feelings I had at work, I wrote [human-driven-development](https://github.com/Yona-Appletree/human-driven-development) as a process to help people build software while retaining ownership of the theory.

I decided to adopt HDD as the process for LightPlayer as an experiment both in the process and also in adopting the process for a brownfield project.

## Links

- [human-driven-development](https://github.com/Yona-Appletree/human-driven-development).
