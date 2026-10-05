# Automatic visualization

Play any world, an atlas discovery, or the original A/B experiment. A picture
appears automatically and adapts to the systems present in the soundscape.
There are no tabs, layer settings, or population selectors. Quiet passages
keep the same layout so the picture does not jump between subjects.

`Space` pauses; `+` / `-` adjusts volume; `v` or Up/Down opens the browser.
`Enter` plays the chosen world and returns to its picture. `l` opens the event
log; press it again to return. Replay and A/B comparison retain their usual
keys. Small terminals get a compact composition rather than a resize prompt.

## What you see

- **Water and rain:** the actual water grid, with depth shown by color and
  texture, sparse flow arrows, and bright marks at recent rain impacts.
  Dry ground is shaded by bed elevation; horizontal strokes mark roof coverage.
- **Chimes:** the real suspension positions and collision radii. Contact
  lights a body yellow. A solid marker identifies the clapper; `o` marks each bar.
- **Calls:** fixed source positions light up during syllables. Brief dotted
  connections show calls that were actually heard. All populations are shown
  together in separate little spaces; their coordinates are not combined.
- **Fire:** a warm glow follows the model's heat driver. Actual pops brighten
  it. The glow is a symbol for activity, not a resolved flame simulation.
- **Wind:** a quiet line grows brighter along its length with the current
  air driver. It also covers wind-driven leaves, without inventing leaf motion.
- **A/B discovery:** resonance bars respond to modal energies, weighted by
  the actual audio crossfade. Frequencies determine horizontal position; height
  is proportional to the square root of energy with a fixed 0.025 reference.
  This represents contributing mode energies, not the summed audio waveform.

## Fidelity

The picture reads the same running state that produces the sound, at ten
snapshots per simulation second. Observation does not advance the world or
consume randomness. Snapshot storage is preallocated; the audio thread skips
an update if the UI is copying it, rather than waiting for rendering.

Water depth uses fixed thresholds of 0.0001, 0.001, 0.01, 0.05, 0.2, 0.5 and
1 metre. Flow arrows use signed mean virtual-pipe fluxes, not a calibrated
velocity in m/s. Reduced maps average covered cells. Spatial proportions
assume a terminal character is approximately twice as tall as it is wide.
The chime view spans -0.16 to +0.16 metres on both horizontal axes and shows
the low-frequency suspension, not aliased audio-frequency bending.

Rain and heard-call marks last 0.5 simulation seconds. A source syllable can
precede listener audio by its propagation delay. Pause freezes the observations
and mark age. New worlds get new generation tags so old activity cannot leak
into their pictures. While a world is prepared, the picture continues to follow
the world still playing. Device buffering adds playback latency.

Water, chimes and hearing populations keep separate local spaces. Fire and
wind are abstract activity indicators. Bubble locations, individual leaf
geometry, and reverberant pressure fields are not invented. Logs retain the
more detailed observations. Missing event records or a future world exceeding
the bounded display storage are reported in a short status line.
