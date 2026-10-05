# ripple

A tiny simulated world whose gentle soundscapes emerge from moving water,
air, matter, and creatures. Set a world running and listen to what it does:
water finds a channel, wind brings a clapper into contact with a chime,
and an animal's call reaches its neighbours a moment later.

## Listen live

```sh
cargo run --release                     # play glade with its automatic visualization
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
| `l` | Open or close the live event log |
| `v` | Switch between the automatic visualization and the browser |
| `q` / `Esc` | Quit |

The screen shows the seed, elapsed simulation time, volume, and output level.
It also shows a live activity summary: recent event rate, calling inhabitants,
heard connections, contacts/bubbles/rain, water and sediment state, and a
20-second activity graph. These observations follow the playing world. The
original A/B player shows the audible candidate's force activity and energy.
Pause freezes the world. Restart begins its history again. A fresh seed is
chosen unless you supply `--seed <u32>`; the same seed and sample rate reproduce
a world's evolution. Water and terrain history persist while a world runs;
selecting or restarting a world creates a new simulation.

Press **`l` in any player** to watch timestamped calls, hearing, contacts, rain,
bubbles, and periodic weather/water readings as the simulation runs. `Tab`
filters the log; `h` holds the view while audio continues; arrows or Page Up/Down
scroll; `End` returns to the live tail. `Space` pauses the simulation itself.
The view keeps the latest 2,048 records and reports any missed events.
See [live events](docs/live-events.md) for coverage and structured export.

## See the simulation

The visualization opens automatically when you play a soundscape. Water and
rain, chimes, calls, and fire appear when they belong to that world; wind has
a simple strength indicator. All creature populations appear together. The
original A/B discovery shows the resonances contributing to the current mix.
There are no visualization settings or views to choose.

Press **`v`** or an arrow key to browse, then **`Enter`** to play another world.
`Space` pauses both sound and motion; `+` / `-` changes the listening volume.
`l` opens the event log and returns to the picture when pressed again.
The picture also adapts to small terminals.

The display follows the same simulation that produces the audio. Spatial
parts retain their own local coordinates; fire and wind are simple activity
symbols, rather than invented fluid simulations. See
[visualization details](docs/visualization.md) for the rendering rules.

## Discover alien soundscapes

```sh
cargo run --release -- atlas --seed 12345
```

The alien world library discovers initial conditions for the **same world engine**
as the eight Earth worlds: evolving water and sediment, flow-driven turbulence,
coupled bubbles, physical contact between suspended bodies, and populations
that hear one another. Materials, geometry, weather, fluid scales and population
behaviour vary. Population spectra come from generated mass-and-spring organs;
there are no named Earth species or musical tuning scales in this generator.

An empty library starts exploring automatically. Use arrows and `Enter` to listen,
`f` to favourite, `Tab` to browse Atlas/Favourites/All saved, `g` to explore more,
and `e` to explore nearby variations of the highlighted world. Accepted worlds
save automatically in `discoveries/atlas`; later searches preserve old discoveries
and favourites. Pause, volume, replay and quit use the normal player controls.

The search measures rendered brightness, texture, motion and stereo width,
then retains a calmness-proxy winner in each occupied sound region. It seeks a
collection of different sounds, rather than a single global winner. Those
measurements encourage variety; human listening still decides what feels alien
and peaceful. See [the alien world library guide](docs/alien-atlas.md) for details.

The original small resonator A/B experiment and its saved lineages remain available:

```sh
cargo run --release -- discover --seed 12345
cargo run --release -- discover discoveries/<saved-file>.alien
```

This smaller listening mode generates networks of masses and springs, then
searches structural mutations for a less rough, varied spectrum. Their voices
follow the resulting resonances; there are no species presets or musical note
lists. Both the parent and discovered descendant obey shared energy limits,
soft excitation, sparse activity, high-frequency damping, and slow weather.
Those rules bias the result toward calm listening; they do not guarantee
subjective pleasantness.

It starts with the automatically selected descendant. Use `a`/`b` to compare,
`e` to keep the audible candidate and search its descendants, and `s` to save
the pair and lineage in `discoveries/`. Pause, volume, replay, new seed, and
quit use the normal player controls. See [the discovery guide](docs/alien-discovery.md)
for the laws, listening-level matching, save format, and limits.

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
./target/release/ripple trace night-meadow 10 --seed 12345 > events.jsonl
```

`render` writes stereo, 16-bit WAV at 48 kHz and reports peak and RMS level.
It checks floating-point samples before conversion and fails on non-finite
audio or a peak outside PCM range. `probe` advances field physics and reports
flow, water, rain, and history. `sync` advances the audible world and reports
the first chorus's phase order, a measure of clock alignment from zero to
one, alongside emitted, heard, and masked call counts. Order alone does not
measure how many calls were heard.

`trace` renders without an audio device and writes one event per JSON line to
stdout, using simulation time at 48 kHz. It also accepts a saved `.world` path.
It runs as fast as the simulation allows; the `l` view follows live playback.

Tests check lattice overtones, geological channel formation, passive contact
collisions and energy accounting, causal hearing and barriers, bubble modes
and stability, retained-water and sediment conservation, and live-player
pause/restart behaviour.

## Architecture

```text
src/
  main.rs       command-line entry point and seed handling
  tui.rs        world chooser, live audio, playback controls, and level meter
  alien.rs      bounded resonator networks, structural search, and saved lineages
  alien/worlds.rs    discovered physical recipes assembled with the shared World
  alien/analysis.rs rendered audio descriptors and calmness proxy
  alien/atlas.rs    diverse archive, immutable world files, and favourites
  tui/library.rs   browsing, playback, and background exploration
  tui/event_view.rs scrolling live event log, filtering, and held views
  tui/activity.rs  rolling activity, heard connections, and state summaries
  tui/visualization.rs automatic soundscape composition
  visual.rs     bounded read-only snapshots of simulation state
  events.rs     bounded event records and offline JSONL traces
  world/assembly.rs shared physical assembly interfaces and diagnostics
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
