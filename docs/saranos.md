# Saranos — the operating system

**Role: a *what* — the system's identity.** What Saranos is, what each layer is called,
and where the boundaries fall. The technical contracts are
[architecture.md](architecture.md); the windowing system is
[emca.md](emca.md); the dated decisions that produced these names are in
[design.md](design.md)'s log (2026-08-31, sharpened twice on 2026-09-01).

## What Saranos is

Not a barebones Unix reimagined, but **a whole operating system with its own
semantics, user interface and artifacts** (Christine, 2026-08-31). It is what
the system boots into on every surface — the browser page, the macOS app, the
iPadOS app.

## The three layers

**Saranos is the operating system** — the whole thing, and what someone would
say they are running. **IPNX is the kernel and the userspace** — the wasm side.
**emca is the windowing and UI system**, and it spans both sides by
construction: `emca` the program is a guest, the surface that renders its tree
is the host's.

| Apple | here | |
|---|---|---|
| macOS | **Saranos** | the operating system: **host and wasm together** |
| Darwin | **IPNX** | the kernel and the userspace — the wasm side |
| Aqua | **emca** | the windowing and UI system — a half on each side |

## Why Saranos needs a name of its own

**Saranos is a symbiosis, and that is why it needs its own name.** It
encompasses the host side — the Rust host under wasmtime, the browser runtime,
the surface — *and* the wasm side. Neither exists without the other: the kernel
is wasm and cannot run without a host to give it workers, memory and a screen;
the host has nothing to do without the kernel. IPNX names only the wasm half,
which is exactly why a second name was needed rather than a qualifier.

## The names

Saranos is Sanskrit *śaraṇa*, refuge — Christine's reading: a refuge from the complexities of the modern
computing environment, a refuge for the PERSON, which is why it names the
system someone uses rather than the kernel underneath. A process also runs in
a refuge bounded by what it was given; one word, both layers.

emca is acme backwards. Note the symmetry that forced the
layering: XNU is "X is Not Unix" and IPNX is "IP is Not UNIX" — the same joke,
so the layer above wanted a human name rather than a second acronym, exactly as
Darwin did. **Dated entries in the records keep the words they were written
with**; only present-tense statements of what the system *is* carry these names.
