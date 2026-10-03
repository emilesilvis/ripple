# Alien soundscape discovery

For richer worlds using the shared Earth-world physics, diversity search, and
a persistent browsing library, see [Alien world library](alien-atlas.md).
This page describes the original small resonator A/B experiment.

Run `cargo run --release -- discover --seed 12345` to listen to a generated
elastic network and an automatically selected descendant. The ordinary world
player remains available through `cargo run --release`.

The experiment asks whether an unfamiliar voice can be found inside general
constraints for calm listening. It has no cricket preset, note list, recorded
sample, or hand-written overtone series. It is a small material network with
stochastic excitation, not a simulated alien ecosystem.

## Listening and saving

The player starts with B, the automatically selected descendant.

| Key | Action |
| --- | --- |
| `a` / `b` | Compare the parent and selected descendant |
| `e` | Keep the audible candidate and search its descendants |
| `s` | Save this pair and its lineage in `discoveries/` |
| `r` | Replay the same pair from the beginning |
| `n` | Start an unrelated discovery from a new seed |
| `Space` | Pause both simulations |
| `+` / `-` | Change volume |
| `q` / `Esc` | Quit |

Reopen a saved pair with `cargo run --release -- discover discoveries/<file>.alien`.
The saved file contains the root seed, selection history, and model version.
The same model version and sample rate reproduce the pair. A/B smoothly changes the audible mixture; both candidates keep evolving.
Restart repeats the current lineage instead of discarding it.

## The authored laws

| Rule | What it constrains |
| --- | --- |
| Positive masses, anchors and bonds | A stable, passive spring network |
| Frequency-dependent damping | Higher modes lose energy faster |
| A shared excitation budget and mechanical energy ceiling | Activity cannot accumulate unlimited energy |
| At most two active drivers, with a minimum interval between starts | Excitation stays sparse |
| Smooth excitation envelopes and slowly changing weather | Excitation rises and falls gradually |
| Normalized velocity pickups and bounded listening gain | Source audio stays within a digital peak of 0.5 before the player's volume control |

Both candidates obey these rules. A is already constrained; B adds automatic
selection of a more promising structure. Masses, stiffnesses, connections and
excitation thresholds vary, including adding and removing masses and bonds.
The modes come from the resulting mass and stiffness matrices. The source is
a normalized measurement of their velocities, not calibrated airborne pressure.

Each search examines 108 mutations over 18 rounds. Its objective combines a
[Sethares-style spectral roughness proxy](https://sethares.engr.wisc.edu/papers/consance.html)
with a penalty for losing distinct audible resonances. Modes with almost the
same frequency count together, so repeated copies of one pitch do not count
as a diverse spectrum. The score uses estimated modal weights, not a full
psychoacoustic analysis of the rendered mixture.

The candidates are matched by RMS over their first twelve seconds, with gain
limited by the energy-derived peak bound. Gains then remain fixed. Later
passages can differ in loudness as the two structures respond to their shared
weather. RMS matching is an approximation to matching perceived loudness.

These are explicit physical and aesthetic priors. They constrain energy,
density, excitation and a roughness proxy; they cannot guarantee that every
listener will find every seed pleasant, peaceful or alien. Minimizing the
proxy is an experiment, not a demonstrated model of aesthetic preference.

## Verification

The implementation tests known two-mass resonances, structural mutation and
objective improvement, long-running energy/activity/audio bounds, RMS
matching, uninterrupted evolution while switching, and saved-lineage replay.
All discovery, calibration and allocation happen before a source reaches the
audio callback. Audio stepping performs only bounded work over at most twelve
masses and two active drivers.
