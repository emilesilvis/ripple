# Physical visualization

Run `cargo run --release -- brook --seed 42`, then press `v`.
The same mode is available in `atlas` and `discover`. A terminal of at least
80 columns by 20 rows shows the maps; larger terminals give finer spatial
resolution. Smaller windows show a resize hint and retain playback controls.

| Control | Action |
| --- | --- |
| `v` | Open the visualization or return to the player |
| `Tab` / `Shift-Tab` | Next / previous view |
| `1`–`5` | Water / Bed / Flow / Chimes / Hearing |
| `[` / `]` | Previous / next hearing population |
| `l` | Open the live event log |
| `Space`, `+` / `-`, `r`, `q` | Pause, volume, restart, quit |
| `a` / `b` in `discover` | Select the parent / descendant |

## What is being shown

The water grid is a plan view, with positive x to the right and positive y
downwards. Its cell size and domain dimensions come from the solver. Maps
preserve physical aspect approximately, assuming terminal characters are twice
as tall as they are wide. A reduced map averages the covered solver cells;
there is no invented intermediate terrain or motion. A `+` is an actual rain
impact held for 0.5 simulation seconds. `R` marks roof coverage.

Water depth has fixed thresholds in metres: `.` below 0.0001, `,` below 0.001,
`:` below 0.01, `~` below 0.05, `=` below 0.2, `O` below 0.5, `#` below 1, and
`@` at or above 1. Very thin films are grouped with dry ground for display.
The bed's scale is its current minimum and maximum, printed in metres. It
includes geological preparation and subsequent erosion and deposition.
The water, retained water and suspended sediment totals use the full field,
not the reduced picture.

Flow arrows use the signed mean of opposite face fluxes. These are the
virtual-pipe solver's internal flow quantities, not a calibrated velocity
field in m/s. Opposing fluxes can cancel. No tracer particles are advected.

Chimes have a separate local horizontal coordinate system, in metres. The
view has a fixed extent of -0.16 to +0.16 m on both axes. `C` is the clapper;
bar labels follow the zero-based indices used in the log (modulo ten for
larger assemblies). Discs use actual collision radii, `+` marks suspension
anchors, and yellow indicates contact in that snapshot. Off-screen centres
are counted. The panel reports mechanical energy, wind work, contact count
and the energy-balance residual. It shows the low-frequency small-angle
suspension, not the audio-rate bending lattice. Brief contacts can fall between
snapshots; the cumulative count still includes them.

Hearing scenes are shown independently, using their actual positions,
listener, sound speed and any barrier. `o` is resting, `*` is an active
syllable at the source, and `L` is the listener. A green dotted connection
records an actual heard arrival during the previous 0.5 simulation seconds;
it is not a traveling wave or a continuously open communication channel.
Source calls can precede audible listener output by their modeled propagation
delay. Masked calls do not produce heard connections. At low resolution,
several animals or lines can occupy the same terminal cell.

The original A/B discovery has no spatial habitat. Its view reports each
mode's natural frequency and instantaneous energy `(z² + v²)/2`, in model
units, for the selected candidate. A full bar is the shared 0.025 total-energy
ceiling, so the scale stays fixed. Both candidates continue evolving; the
actual smoothed audio blend is displayed during switches. Small terminals
explicitly report how many modes fit.

## Fidelity and runtime

The audio thread owns the world. Ten times per second it tries to update a
preallocated snapshot. If the UI is copying a snapshot, the audio thread
skips that update without waiting. Rendering and terminal I/O happen on the UI
thread. A busy display may skip frames; it never advances an independent
simulation or interpolates motion. The displayed timestamp belongs to the
snapshot, in simulation time, not the DAC's physical playback clock. Sound
output buffering adds device-dependent latency. The water and weather state
is the latest audio control-block state (32 samples per block).

Pause freezes both state and event-mark age. Restart/world replacement uses
generation-tagged snapshots and event records, so a new world cannot inherit
old marks. While another world is being prepared, the display follows the
world still playing. Event marks use the live log's bounded observation
stream; missed records are reported. Holding or filtering the log does not
hold or filter the physical view.

Display storage is bounded at 4,096 field cells, 65 suspended bodies,
1,024 animals, eight populations and 24 resonator modes. This covers all
current named and generated worlds. Future larger models report omissions;
an incomplete field is not drawn as a complete map.

Weather and fire remain dimensionless model-driver readings. Bubbles and
bubble packets have active counts without invented map coordinates. The
engine does not specify a joint transform between terrain, chimes and hearing
scenes, a resolved wind/flame field, or acoustic pressure across the landscape.
The display makes none of those claims. Existing physical approximations and
uncalibrated quantities remain as described in the README's model limits.
