---
name: verify
description: Verify ripple changes by driving the CLI — render worlds to WAV and analyze them, run the probe/sync physics diagnostics. Use after changing any engine code (dsp, matter, voices, contacts, bubbles, acoustics, critters, field, world, presets).
---

# Verifying ripple

Verify engine changes with rendered audio, numeric analysis, and physics
probes. For player changes, also exercise the TUI in an interactive terminal.

## Build and surfaces

```sh
cargo test
cargo build --release                       # build FIRST; cargo test does not refresh this binary
./target/release/ripple list
./target/release/ripple render <world> out.wav <seconds> --seed 12345
./target/release/ripple probe <world> <seconds> --seed 12345
./target/release/ripple sync night-meadow <seconds> --seed 12345
```

All commands use the same engine: physical chime contacts, local hearing,
coupled churn bubbles, and live water/sediment history. Mechanisms need the
corresponding inhabitants, materials, and events to become active.

## What healthy looks like

- Render all eight worlds for at least ten seconds: finite, non-silent audio,
  peak below 1.0, and ample real-time headroom. `render` checks floating-point
  samples before PCM conversion. RMS depends on world, seed, and duration;
  compare repeatable runs before judging level changes.
- `probe brook`: flow settles rather than diverging; water entrains bubbles
  and transports sediment.
- `probe storm`: rain feeds surface and retained water; delayed release and
  evolving terrain affect subsequent flow.
- `probe shore`: the forced sea responds to its swell without unbounded growth.
  Individual breaking events depend on the evolving state, not a timer.
- `sync night-meadow`: order stays finite in [0, 1]. Local calls can coordinate
  animals without producing perfect global alignment. A fixed target such as
  order >0.9 is not a correctness criterion.
- `cargo test` covers lattice overtones, geological channels, passive contact
  and energy accounting, hearing causality and attenuation, collective bubble
  modes and stability, and retained-water/sediment conservation.

Use longer runs and multiple seeds if an apparent regression remains unclear.
Water budgets in controlled tests include rainfall and drainage; a world's
springs and imposed ocean boundary are additional external water sources.

## Player checks

Run the release player in a terminal with an audio output device. Check world
selection, pause/resume, volume, same-seed restart, new seed, narrow-terminal
layout, and quitting. Pause must freeze simulation time; selecting a world
must keep the current audio responsive while terrain is prepared. Exiting
must restore the cursor, normal terminal screen, and terminal input mode.

## Gotchas

- `render` keeps loose argument ordering: numbers mean seconds, known world
  names choose a world, and anything else is the output path.
- Worlds with geology (glade, brook, cozy-rain, storm, shore) have a terrain
  pre-roll. Each seed grows different terrain.
- Replay requires both the same seed and the same sample rate.
- Field history persists during a run; restarting creates a fresh world.
