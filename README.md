# ripple

A tiny simulated world whose gentle soundscapes **emerge** from a few
axiomatic laws. You don't select a "rain sound". You set a world running —
water that flows, air that moves, matter that rings, creatures that sing —
and listen to what it does.

```sh
cargo run --release                 # the default world (glade)
cargo run --release -- shore        # a named world
cargo run --release -- list         # all worlds
cargo run --release -- render shore out.wav 60
```

## The idea

Most procedural audio is written top-down: *I want rain, so I write a rain
synthesiser* — an oscillator tuned to sound like a droplet, triggered on a
schedule that feels like rainfall. Every sound is authored separately and knows
what it is.

`ripple` is the inversion, taken as far down as it will go. There is no layer
of *stuff* here — no table of frequencies called "tin", no hand-picked noise
band called "wind", no valley drawn into the terrain for a brook to follow.
What is written down are **constants of substances and a handful of laws**;
the stuff itself — the ring of tin, the pitch of the wind, the very course of
the brook — is derived while you wait.

## Almost only atoms

- **A solid is a lattice of bonded point masses.** A substance is two numbers:
  how fast a bending wave runs through it, and how much of each flex it loses
  internally as heat. A body adds only geometry — a length, a thickness. Strike
  it and Newton's law is integrated node by node, sample by sample. Nobody
  tells a chime its overtones: a free steel bar rings at the free-bar ratios
  1 : 2.76 : 5.40 because that is what a stiff lattice with free ends does
  (there is a unit test that measures this from the rendered audio). The same
  steel, rolled thin and corrugated into a roof panel, pings briefly in the
  kilohertz — internal loss grows as frequency squared, so one loss constant
  makes the same metal sing for seconds down low and die in milliseconds up
  high. Wood differs from steel not in stiffness (their wave speeds are close)
  but in loss — the physical reason wood thuds. Even the click of contact is
  free: a sharp strike wakes the lattice's highest modes, which perish almost
  instantly.

- **A moving fluid is an eddy cascade, voiced by the Strouhal law.** A
  turbulent source keeps one physical datum: the size of its radiating eddies.
  Pitch is `f = St·v/l` with the same St ≈ 0.2 for every fluid in the world —
  wind through grass, water over stones, foam, flame. Faster flow, higher
  voice, by law rather than by ear.

- **A bubble is voiced by Minnaert's resonance.** The water reports only a
  physical fact — a bubble's radius; `f ≈ 3.26/r` turns it into pitch. The
  plink of rain on a pond *is* the ring of the little air bubble each drop
  entrains; churning water tears off bigger, deeper-voiced ones.

- **Terrain is made by tectonics and erosion, not drawn.** A preset raises a
  tilted block with random undulations and opens a spring. Then a *geological
  epoch* runs before the audible world starts: running water wears the bed
  down in proportion to how fast it runs (stream power), cutting concentrates
  the flow, which cuts deeper — and a channel is born wherever the water
  chose. The wear is what the ear later hears: the deepest cuts are scoured to
  bare rock and stay rough, so the brook churns exactly where the water dug.
  On the shore, the same law wears the shoal precisely where the waves break —
  the surf zone marks itself. Every seed grows a different valley.

- **A living thing drives a limit cycle** — `Critter`: an oscillator plus a
  behavioural clock. Its song is genetics, not passive physics, so it stays
  authored; what emerges is above the individual (see below).

Above them sits **the field** — a shallow sheet of water over terrain — and
**the medium**, a lumped model of the space around the listener. Two ears read
the medium and return stereo.

## A few axiomatic laws

1. **Gravity & flow.** Water accelerates down the gradient of its own surface
   and carries its momentum (the shallow-water equations).
2. **Elasticity.** Bonded matter resists bending and loses a fixed fraction of
   each flex internally; struck matter rings as the lattice integrates
   Newton's law. Accelerating surfaces radiate — that is how sound is made.
3. **Turbulent radiation (Strouhal).** Eddies of scale *l* in flow *v* radiate
   around `St·v/l`; radiated energy follows the flow's energy.
4. **Bubble resonance (Minnaert).** An air bubble of radius *r* in water rings
   at `3.26/r`.
5. **Stream power.** Running water erodes its bed in proportion to its speed —
   over an epoch, this writes the geography.
6. **Propagation.** Sound arrives again and again, softer and darker each time
   — the room answering back.
7. **Thresholds & coupling.** Surface tension holds a drop until it detaches;
   heat builds until a pocket pops; a critter's clock winds until it sings,
   and a neighbour's song nudges its clock.

## What actually emerges

None of these were programmed as effects. You can watch — or test — them:

- **A chime's voice.** `cargo test` includes a test that strikes a steel bar
  cut for 400 Hz and measures, from the audio, a 400 Hz fundamental with
  overtones at ×2.76 and ×5.40 — the free-bar signature, nowhere written.
- **A brook digs its own bed.** `cargo test` also builds the `brook` world
  from a bumpy unmarked slope and asserts that the epoch scoured a rocky
  channel and that the spring's water audibly churns along it. Run
  `cargo run --release -- probe brook` to watch it fill and settle (~24
  air bubbles a second rise out of the fast water).
- **Rain swells the stream.** `probe storm` shows the field's water volume
  climbing as the downpour soaks in — the brook grows louder because it is
  fuller, not because anything turned it up.
- **The sea finds its surf.** `probe shore` shows the flow energy oscillating
  with the 7.5-second swell; the epoch's waves have already worn the shoal
  rough exactly where they break.
- **A cricket chorus finds its rhythm.** `cargo run --release -- sync
  night-meadow` prints the flock's synchrony (the Kuramoto order parameter)
  each second: scattered (~0.2) to locked (~0.9) within seconds, purely from
  gently kicking each other's clocks — the same pulse-coupling that
  synchronises fireflies. The frogs get the *opposite* coupling, so they take
  turns instead.

## The worlds

| World | What the simulation is |
| --- | --- |
| `glade` | a spring carving through a clearing, songbirds and leaves by day |
| `brook` | just water finding — and cutting — its way down a slope |
| `cozy-rain` | rain drumming on a corrugated steel sheet, a swelling brook, steel bars hung in the gusts |
| `night-meadow` | a synchronising cricket chorus, frogs taking turns, a far owl |
| `shore` | an ocean swell breaking on the shoal it wore rough itself, gulls, sea wind |
| `hearth` | a campfire — a heat engine popping pockets against a real wooden bar |
| `mountain` | wind over a high ridge, leaves, the odd bird |
| `storm` | a passing downpour: heavy rain, gusting wind, a brook swollen with runoff |

The weather wanders on its own, and every seed uplifts different terrain, so
no two runs of a world are the same place.

## Architecture

```
src/
  main.rs      CLI: run / list / render, plus the probe & sync diagnostics
  audio.rs     live playback (cpal) and offline WAV render
  dsp.rs       primitive math: noise, filters, the reverb
  matter.rs    solids from atoms: mass-spring lattices, Newton's law, two
               constants per substance — pitch, overtones, click and decay
               all emergent (unit-tested against free-bar theory)
  voices.rs    the fluid voice: an eddy field voiced by the Strouhal law
  critters.rs  the living voice and its pulse-coupled Chorus
  field.rs     the shallow-water field, its one law of motion, and the
               geological epoch that carves terrain by stream power
  sky.rs       the slow drivers: wind, rainfall, the turn of day
  world.rs     assembles a world, steps the physics, reads it at the ears
  presets.rs   the eight worlds — tectonics, climate, and who lives there
```

Two clocks turn. The physics advances in short control blocks (water flows,
weather drifts, drops land, embers pop). Between them, every audio sample is a
reading of the world as it stands — lattices still ringing, turbulence still
hissing — gathered at two ears.

## Honest notes

The water is a real (if small and gentle) shallow-water simulation, and the
struck solids are real (if small) elastic lattices. Wind and heat are modelled
as turbulent drivers rather than resolved flow fields; the eddy *scales* are
authored (a leaf edge, a whistling crack), but their voices follow one law.
The bandpass rendering of a cascade, and the radiated-power makeup that goes
with it, are the honest cheap stand-in for unresolved turbulence. Creatures
are agents with built-in oscillators; their individual songs are genes, not
physics — what emerges among them is the chorus dynamics. The fire's heat is a
lumped random flare, though its pops strike a genuine wooden bar. The chime-
maker still *tunes*: presets pick pitches, but only the way a human does — by
inverting the bending law to choose a length to cut; the bar's overtones,
click and decay are the lattice's own business. The reverb is a lumped
stand-in for the terrain's reflections. Within those honest limits, every
sound you hear is the world moving, not a recording of one.
