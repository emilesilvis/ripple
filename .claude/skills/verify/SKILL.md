---
name: verify
description: Verify ripple changes by driving the CLI — render worlds to WAV and analyze them, run the probe/sync physics diagnostics. Use after changing any engine code (dsp, matter, voices, field, world, presets).
---

# Verifying ripple

ripple is a CLI that runs/renders simulated soundscapes. There is no way to
"hear" audio in a session, so verification = **render to WAV + numeric
analysis** plus the built-in **physics probes**.

## Build & surfaces

```sh
cargo build --release                      # rebuild FIRST — cargo test does NOT refresh target/release/ripple
./target/release/ripple list               # all worlds
./target/release/ripple render <world> out.wav <secs>   # prints peak + RMS dBFS
./target/release/ripple probe <world> <secs>   # physics only: flow energy/speed, water volume, rain, bubbles/s
./target/release/ripple sync night-meadow <secs>  # Kuramoto order of the cricket chorus
```

## What healthy looks like

- `probe brook`: flow_e settles ~0.05–0.3, water volume plateaus, ~24 bubbles/s.
- `probe storm`: water volume climbs steadily (rain feeds the field).
- `probe shore`: water ~25–40 and *oscillating* with the 7.5 s swell; if it
  climbs without bound the sea/terrain is in disequilibrium (a past bug).
- `sync night-meadow`: order rises from ~0.2 to >0.9 within a few seconds.
- `render` each world ~10 s: RMS ≈ 0.09–0.24, peak < 1.0, no NaN. Renders are
  seeded randomly and climates wander — RMS varies run to run; use 20 s+ and
  multiple runs before judging a level change.
- 30 s render should take ~3 s wall clock (≥10x real-time headroom for live cpal playback).

## WAV analysis

A stdlib-only analyzer (RMS/peak/NaN + Goertzel spectral peaks) exists at the
scratchpad from earlier sessions; recreate as needed. Useful spectral check:
the lowest chime bar is tuned to C4 (261.6 Hz) and its *emergent* second mode
should appear near ×2.73–2.79 (≈715–730 Hz) in chime-bearing worlds
(glade, cozy-rain).

## Gotchas

- `render` argument parsing is positional-loose: numbers → seconds, known
  world names → world, anything else → output path.
- Worlds with geology (glade, brook, cozy-rain, storm, shore) run a ~1 s
  terrain pre-roll at build; each seed grows different terrain.
