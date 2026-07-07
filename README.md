# ripple

A tiny simulated world whose gentle soundscapes **emerge** from a few
axiomatic laws. You don't select a "rain sound". You set a world running —
water that flows, air that moves, materials that ring, creatures that sing —
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

`ripple` is the inversion. Nothing here is a "rain sound." There is water in a
field, obeying one law of motion. When it rains, drops fall under the weather
and strike whatever is beneath them; a drop that lands on a tin roof makes the
roof ring, because the roof is a material with modes and the drop deposited
energy into it. A brook sounds like a brook because water is genuinely running
down a rocky channel and churning. Rain even swells the brook — the drops soak
into the field, raise the water, and the fuller channel runs louder. No part of
the program was told what a brook or a storm is supposed to sound like.

## Very few fundamental parts

Nearly every sound in a natural soundscape is one of three things:

- **A struck solid rings** — `Modal`. A real object (a roof panel, a rock, a
  glass chime, a shifting log) vibrates as a sum of decaying modes. Strike it
  and it rings; strike it many times and the rings superpose, because the
  response is linear. The *material* is nothing but which frequencies it has and
  how fast they fade. Rain-on-tin, a fire's pops, and wind chimes are all this
  one voice with different materials.

- **A moving fluid hisses and roars** — `Turbulence`. Turbulence is broadband
  noise; the faster the flow, the smaller and higher its eddies, the louder its
  energy. A single shaped noise band, driven by the local flow, is the honest
  voice of wind through leaves, water over rock, foam up a beach, and the airy
  body of a flame.

- **A living thing drives a limit cycle** — `Critter`. An oscillator plus a
  behavioural clock. On its own it just sings now and then.

Above them sits **the field** — a shallow sheet of water over terrain — and
**the medium**, a lumped model of the space around the listener (its countless
reflections collapsed into a reverb). Two ears read the medium and return
stereo.

That's the whole ontology. The eight worlds below are just different terrain,
weather, and populations poured into the *same* engine.

## A few axiomatic laws

1. **Gravity & flow.** Water accelerates down the gradient of its own surface
   and carries its momentum (the shallow-water equations). This single law makes
   rain pool, a spring feed a brook, and an ocean swell travel and shoal.
2. **Excitation coupling.** When matter is accelerated sharply — a drop lands, a
   gust shoves a chime, an ember bursts — it deposits energy into a resonator or
   the medium. Accelerating matter radiates; that is how sound is made.
3. **Turbulent radiation.** Moving fluid over roughness sheds broadband sound
   whose loudness and pitch follow the flow.
4. **Propagation.** Sound doesn't merely reach the ear; it arrives again and
   again, softer and darker each time — the room answering back.
5. **Thresholds & coupling.** Surface tension holds a drop until it detaches;
   heat builds until a pocket pops; a critter's clock winds until it sings, and
   a neighbour's song nudges its clock.

## What actually emerges

None of these were programmed as effects. You can watch them happen:

- **A brook finds its level.** `cargo run --release -- probe brook` runs the
  physics with no audio and prints the water. The spring fills the channel, flow
  settles to a steady churn, and ~24 air bubbles a second rise out of the fast
  water over the stones.
- **Rain swells the stream.** `probe storm` shows the field's water volume
  climbing as the downpour soaks in — the brook grows louder because it is
  fuller, not because anything turned it up.
- **A swell breaks on the shore.** `probe shore` shows the flow energy
  oscillating with the 7.5-second swell as waves travel up the beach, shoal in
  the shallows, and tip into foam.
- **A cricket chorus finds its rhythm.** `cargo run --release -- sync
  night-meadow` prints the flock's synchrony (the Kuramoto order parameter) each
  second. The crickets start scattered (~0.2) and, purely from gently kicking
  each other's clocks, lock into a breathing chorus (~0.9) within a couple of
  seconds — the same pulse-coupling that synchronises fireflies. The frogs are
  given the *opposite* coupling, so they push apart and take turns instead.

## The worlds

| World | What the simulation is |
| --- | --- |
| `glade` | a spring-fed brook over rock, songbirds and leaves in a daytime clearing |
| `brook` | just water finding its way down a rocky channel |
| `cozy-rain` | rain drumming on a tin roof, a brook the rain keeps fed, wind chimes |
| `night-meadow` | a synchronising cricket chorus, frogs taking turns, a far owl |
| `shore` | an ocean swell travelling up a beach and breaking, gulls, sea wind |
| `hearth` | a campfire — a heat engine popping embers over an airy rumble |
| `mountain` | wind over a high ridge, leaves, the odd bird |
| `storm` | a passing downpour: heavy rain, gusting wind, a brook swollen with runoff |

The weather wanders on its own (gusts, passing showers), so a world drifts
through moods the longer you leave it running.

## Architecture

```
src/
  main.rs      CLI: run / list / render, plus the probe & sync diagnostics
  audio.rs     live playback (cpal) and offline WAV render
  dsp.rs       primitive math: noise, filters, a resonant mode, the reverb
  voices.rs    the physical voices: Modal (struck solid), Turbulence (fluid)
  critters.rs  the living voice and its pulse-coupled Chorus
  field.rs     the shallow-water field and its one law of motion
  sky.rs       the slow drivers: wind, rainfall, the turn of day
  world.rs     assembles a world, steps the physics, reads it at the ears
  presets.rs   the eight worlds — terrain, weather, and who lives there
```

Two clocks turn. The physics advances in short control blocks (water flows,
weather drifts, drops land, embers pop). Between them, every audio sample is a
reading of the world as it stands — resonators still ringing, turbulence still
hissing — gathered at two ears.

## Honest notes

The water is a real (if small and gentle) shallow-water simulation. Wind and
heat are modelled as turbulent drivers rather than resolved flow fields —
faithful to how turbulence sounds, without the cost of resolving it. Creatures
are agents with built-in oscillators; what *emerges* among them is the chorus
dynamics — synchrony and call-and-response — not the individual voice. The
reverb is a lumped stand-in for the terrain's reflections rather than a resolved
acoustic space. Within those honest limits, every sound you hear is the world
moving, not a recording of one.

This is the bottom-up counterpart to its sibling `ripple-target`, which
synthesises the same family of soundscapes directly, one authored generator per
sound.
