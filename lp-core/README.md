# LightPlayer Application

LightPlayer is a framework for building and deploying interactive visualization applications
using GLSL primarily for addressable LED control on embedded systems.

It employs a client-server architecture, where the server runs the visualization on
a headless machine (e.g. an esp32-c6 or raspberry pi), and the client communicates with
the server via a network or serial connection.

The server portion is portable, and is designed to be runnable from any OS for
development, a headless host for larger deployments, or as bare-metal firmware
on an embedded system.

On machines without OpenGL support, such as embedded systems, the shaders are compiled to native
code using the bespoke LightPlayer GLSL compiler which is built on a fork of Cranelift.

# Workspace

This workspace contains the main LightPlayer application, including the device specific firmware,
engine, server, and clients.

It exists within cranelift temporarily during active development of the compiler, because managing
multiple repositories is too cumbersome.
