# ripple

A tiny simulated world whose gentle soundscapes emerge from moving water,
air, matter, and creatures. Set a world running and listen to what it does:
water finds a channel, wind brings a clapper into contact with a chime,
and an animal's call reaches its neighbours a moment later.

## Listen live

```sh
cargo run --release                     # open the world chooser in glade
cargo run --release -- shore            # start in a named world
cargo run --release -- brook --seed 42  # repeatable starting world
cargo run --release -- list             # list the eight worlds
```

The player uses your default audio output and needs an interactive terminal.
Worlds with geological preparation take a moment to start. When you choose
another world, the current audio continues while its terrain is prepared.

| Key | Action |
| --- | --- |
| `↑` / `↓` or `k` / `j` | Highlight a world |
| `Enter` | Play the highlighted world |
| `Space` | Pause or resume the simulation |
| `+` / `-` | Adjust listening volume |
| `r` | Restart the playing world with the same seed |
| `n` | Restart the playing world with a new seed |
| `q` / `Esc` | Quit |

The screen shows the seed, elapsed simulation time, volume, and output level.
Pause freezes the world. Restart begins its history again. A fresh seed is
chosen unless you supply `--seed <u32>`; the same seed and sample rate reproduce
a world's evolution. Water and terrain history persist while a world runs;
selecting or restarting a world creates a new simulation.

## The worlds

| World | What is happening |
| --- | --- |
| `glade` | A spring through a clearing, songbirds, leaves, and wind chimes |
| `brook` | Water finding and wearing its way down a rocky channel |
| `cozy-rain` | Rain on a corrugated steel roof, a swelling brook, and suspended chimes |
| `night-meadow` | Crickets and frogs responding to nearby calls, with a far owl |
| `shore` | Ocean swell over an eroded shoal, gulls, and sea wind |
| `hearth` | A campfire popping pockets against a wooden bar, crickets, and a breeze |
| `mountain` | Wind over a high ridge, leaves, and the odd bird |
| `storm` | Heavy rain, gusting wind, and a brook fed by runoff |

Every world uses the same engine. Its initial terrain, climate, materials,
and inhabitants determine which mechanisms are active.

## How the sound emerges

**Solids ring through their mechanics.** A body is a lattice of bonded point
masses. Its substance sets bending-wave speed and internal loss; its geometry
sets the shape. A strike wakes the lattice, whose motion produces pitch,
overtones, and decay. Presets choose chime pitches by calculating lengths to
cut, as a chime-maker would. They do not prescribe the resulting overtones.

**Wind causes chime encounters.** Drag moves suspended bars and a sail/clapper
assembly. Contact transfers momentum and excites the bending lattice. A chime
can miss a gust or strike repeatedly as its suspension moves; there is no
random strike schedule. The model tracks wind work, damping, collision loss,
and numerical correction work.

**Flow gives turbulence its voice.** Water and air share an eddy model with
pitch following the Strouhal relation, `f = St·v/l`: smaller eddies and faster
flow produce higher frequencies. Streams, foam, leaves, and flame provide
their own scales and driving motion.

**Bubbles can ring together.** An isolated bubble follows Minnaert's resonance,
approximately `f = 3.26/r` for radius in metres. A churning-water or breaking-wave
event entrains a compact packet of bubbles whose radii preserve its gas volume.
Their surrounding water couples their motion, producing collective modes,
including frequencies below the individual bubbles' resonances. Rain-drop
bubbles remain isolated.

**Terrain has a past.** A preset raises a tilted, uneven block and opens a
spring, or raises a beach under ocean swell. A geological preparation phase
lets water cut and scour the terrain before listening begins. During playback,
open ground retains some rainfall and releases it gradually. Water transports
sediment; erosion and deposition continue changing the bed. Present flow
depends on earlier weather and the terrain that flow has already changed.

**Creatures hear locally.** Each animal has a position, an authored song, and
a behavioural clock. An actual syllable onset sends a call along a path with
travel time and attenuation. A received call nudges another clock only if it
clears the hearing threshold and an estimate of water-and-wind masking.
Positive cricket coupling advances resting neighbours; negative frog coupling
delays them. The listener receives delayed, spatially placed voices through
the same path model. Coordination depends on audible communication; perfect
global synchrony is not guaranteed.

## Inspect and verify

The live player is the listening interface. Offline commands use the same
world engine for repeatable development checks:

```sh
cargo test
cargo build --release
./target/release/ripple render shore out.wav 30 --seed 12345
./target/release/ripple probe brook 20 --seed 12345
./target/release/ripple sync night-meadow 60 --seed 12345
```

`render` writes stereo, 16-bit WAV at 48 kHz and reports peak and RMS level.
It checks floating-point samples before conversion and fails on non-finite
audio or a peak outside PCM range. `probe` advances field physics and reports
flow, water, rain, and history. `sync` advances the audible world and reports
the first chorus's phase order, a measure of clock alignment from zero to
one, alongside emitted, heard, and masked call counts. Order alone does not
measure how many calls were heard.

Tests check lattice overtones, geological channel formation, passive contact
collisions and energy accounting, causal hearing and barriers, bubble modes
and stability, retained-water and sediment conservation, and live-player
pause/restart behaviour.

## Architecture

```text
src/
  main.rs       command-line entry point and seed handling
  tui.rs        world chooser, live audio, playback controls, and level meter
  audio.rs      checked offline WAV rendering
  dsp.rs        noise, filters, resonators, and reverb
  matter.rs     elastic mass-spring lattices and material constants
  voices.rs     turbulent fluid voices
  contacts.rs   suspended chime mechanics, collisions, and energy accounting
  bubbles.rs    coupled bubble packets and collective modes
  acoustics.rs  sound paths, travel time, attenuation, and barriers
  critters.rs   animal songs, behavioural clocks, and received calls
  field.rs      shallow-water flow, retained rain, sediment, and evolving bed
  sky.rs        wind, rain, and the turn of day
  world.rs      physics events, isolated rain bubbles, and the audible mix
  presets.rs    terrain, climate, materials, and inhabitants for eight worlds
```

The field and weather advance in short control blocks. Between those updates,
each audio sample reads the ringing lattices, bubble modes, turbulent voices,
and animals. The audio callback owns the running world; the UI builds and
retires worlds outside that callback. The architecture code tour in
`.tours/ripple-architecture.tour` follows these connections in the source.

## Model limits

These are small physical models with deliberate approximations. Chime
suspension uses small angles and a sheltered wind exposure; lattice vibration
does not feed back into the suspension. Hearing uses fixed positions, straight
paths, optional barrier attenuation, and an onset-level threshold rather than
frequency-specific ears or diffraction. The current worlds use open hearing
paths. Bubble packets have fixed geometry and linear fluid coupling, without
moving interfaces or a resolved free surface; their excitation strength is
not calibrated in joules.

Soil retention is a linear reservoir, and sediment erosion runs on an
accelerated timescale. The water solver is a small, damped shallow-water
approximation. Wind and flame drive a bandpass rendering of turbulence rather
than a resolved fluid field. Creature songs are authored, the fire uses a
lumped heat driver, and reverb approximates the surroundings' reflections.
Within those limits, the sounds come from the simulation's motion rather
than recordings.
