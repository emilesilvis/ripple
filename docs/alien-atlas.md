# Alien world library

Run `cargo run --release -- atlas --seed 12345`. The first visit explores 24
candidates, starts playing the first accepted world, and keeps exploring in the
background. Later visits reopen your saved collection. The seed controls search
proposals; the initial library also affects which parents and winners it finds.

## Listen and explore

| Key | Action |
| --- | --- |
| Arrows / `j` / `k` | Highlight a world |
| `Enter` | Listen to the highlighted world |
| `f` | Toggle its saved favourite marker |
| `Tab` | Cycle Atlas, Favourites, and All saved |
| `g` | Explore 24 global candidates, mixing fresh starts and mutations |
| `e` | Explore 12 neighbours of the highlighted world; listen to the best accepted neighbour when finished |
| `p` | Revisit the highlighted world's parent, if present in this library |
| `x` | Stop exploration; completed discoveries remain saved |
| `Space` | Pause playback and freeze its simulation |
| `+` / `-` | Adjust listening volume |
| `r` | Restart the playing world |
| `q` / `Esc` | Quit and stop exploration |

Browsing, pausing and volume remain responsive during exploration. A separate
bounded worker prepares requested playback while current audio continues. Pause
affects the listening simulation; use `x` to stop the independent search.

Every accepted candidate is saved automatically. Atlas shows the current best
candidate in each occupied sound region; All saved also contains earlier
candidates. Improving an archive cell never deletes its previous world or removes
a favourite. Nearby exploration keeps locally promising variations even when
they do not beat the global cell champion.

## What is actually simulated

These worlds run `World`, not a parallel imitation of the Earth engine. A saved
recipe specifies climate, terrain shape, springs and swell, fluid eddy scales,
solid material properties and geometry, population anatomy/behaviour, and space.

| Shared mechanism | What an alien world supplies |
| --- | --- |
| Water and sediment history | An initially uncarved 16-by-28 terrain grid, a geological preparation period, rainfall, water sources and drains; history remains active during listening |
| Coupled bubbles | Actual churn and breaking events excite the same packets of eight interacting bubbles |
| Physical contact | Two to five material lattices hang in the same passive suspension/contact solver; encounters follow wind-driven motion |
| Local hearing | One to three generated populations, with two to five inhabitants each, use the same finite-delay, distance, barrier and masking calculations |
| Fluid sound | Discovered eddy scale, reference speed and resonance feed the shared Strouhal-law renderer |
| Solid sound | Discovered wave speed, internal loss, dimensions and geometry feed the shared bending-lattice solver |

Population spectra are derived from the eigenmodes of generated mass-and-spring
organs. Their ratios feed the existing population voice renderer; the runtime
voice remains an oscillator/envelope approximation, not a solved biological
vocal tract. Phrase timing, coupling and geometry are generated within explicit
bounds. No cricket, frog, bird or other named species constructor is called.

Not every mechanism makes an audible event in every short excerpt. Quiet wind
can move a suspension without causing a collision, and a barrier or environmental
noise can prevent a neighbour from hearing. These outcomes remain causal.

The worlds share the Earth engine's kinds of physical building blocks and its
approximations. They are not literal atomic simulations, new chemistry, or evolved
organisms. Gravity, water laws, bubble laws and the contact solver remain shared.
The listener's stereo aperture is a bounded channel-mixing approximation; resolved
population paths still use actual positions and travel time.

## A diverse archive

Every candidate is rendered at 16 kHz: three seconds of startup, followed by a
twelve-second measurement window. Audio analysis measures:

- Spectral centroid (brightness).
- Normalized spectral entropy (tonal through diffuse texture).
- Variation of 100 ms RMS levels (temporal motion).
- Side-channel energy fraction (stereo width).

The four measurements locate a world in one of 108 cells: four brightness bands
and three bands for each other descriptor. Labels such as “deep / tonal /
drifting / focused” describe the measured result; they do not choose presets.
The archive is an application of [MAP-Elites](https://arxiv.org/abs/1504.04909):
keep promising examples across a space of behaviours. Empty regions accept a
valid candidate; occupied regions replace their champion only when its score
improves. Half the global proposals are fresh seeds; the other half mutate a
uniformly selected occupied region. Mutation changes environment, materials,
topology of vocal organs, populations, and space.

The score combines a Sethares-style roughness estimate from up to 24 rendered
spectral peaks, high-frequency energy, large peak/RMS ratios, and frequent large
level rises. This is a deliberately limited calmness proxy. It is not a validated
preference model, and a short excerpt cannot describe everything a slowly evolving
world will do. Stereo analysis and spectral descriptors are approximations.

## Calmness and output bounds

The recipe permits positive materials, damped bodies, moderate weather, bounded
population sizes, soft syllable envelopes and slow changes. The shared world's
master saturation bounds output at 0.9. A fixed listening gain no greater than
0.5/0.9 and a convex stereo observation mix keep a discovered source's digital
peak at or below 0.5, before the player's volume control. Analysis can reduce
gain toward RMS 0.035; it never raises gain beyond that bound. Later playback
keeps this constant gain, so quiet passages stay quiet.

This is an output bound, not a proof of a global mechanical-energy ceiling for
the combined fluid/contact/population engine. Shared physics tests separately
check stability, contact energy accounting and water/sediment conservation.
Neither the bounds nor the scoring guarantee subjective peacefulness.

## Files and offline use

```sh
# Choose a library directory.
cargo run --release -- atlas my-worlds

# Search without an audio device or interactive terminal.
cargo run --release -- scout 24 my-worlds --seed 12345

# Export one saved world. The output file must not already exist.
cargo run --release -- render-alien my-worlds/worlds/<id>.world alien.wav 30
```

Each `.world` file contains a `ripple-world-v2` version tag, the complete physical
recipe and seed, a parent identity, measured descriptors, diagnostic counts, and
listening gain. Floating-point values round-trip exactly. Replay requires the
same model implementation and playback sample rate. Future changes to synthesis
laws require a new model version or a migration; existing files are never silently
reinterpreted as another version. The old `ripple-alien-v1` lineage format still
opens through `discover` and uses its original resonator engine.

World files are published atomically without overwriting an existing file.
Favourites are independent marker files. Copying the library directory preserves
the collection and favourites. Invalid or unsupported world files are reported
and skipped, not repaired or deleted. An interrupted search leaves completed
worlds available on the next visit.

## Verification

Tests cover synthetic audio descriptor behaviour, per-region selection,
favourite survival after replacement and reopening, immutable saves, invalid
recipes, exact recipe/audio replay, and cancellation. The longer integration
audit renders generated worlds, checks breadth across measured regions,
observes all four shared mechanisms, and tests longer 48 kHz playback bounds.
Real terminal checks cover browsing, playback, search/cancel, nearby exploration,
favourites, reloading, pause, and narrow layouts. The Earth-world regression
checks exercise the unchanged preset assembly and shared runtime.
