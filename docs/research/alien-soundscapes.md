# Discovering alien soundscapes

Research note, 2026-10-03. Conceptual design; no implementation is proposed as proven.

## What the goal can mean

The useful target is **no authored species voices, scores, or named landscape recipes**. A program still needs a representation, dynamics, initial conditions, a way to turn state into audio, and some basis for selecting discoveries. Searching those choices merely moves this boundary outward. An entirely assumption-free generator is not a concrete software specification.

The current [README](../../README.md) already distinguishes simulated material resonance from authored creature songs, tuned chime dimensions, chosen turbulent scales, and preset world composition. Discovery would need to reach those choices, not just randomize their parameters.

## Evidence from primary sources

- **Unfamiliar timbres need not use familiar scales.** Sethares models local consonance from interactions between spectral partials and derives matching tunings for nonharmonic spectra, including beams and stretched or compressed spectra. This supports searching timbre and tuning together rather than imposing a fixed Western scale. It is a model of sensory consonance, not a universal measure of musical quality. [Sethares, 1993](https://sethares.engr.wisc.edu/papers/consance.html), [paper](https://sethares.engr.wisc.edu/paperspdf/consonance.pdf).
- **Roughness and pleasant harmony are distinct judgments.** Tsimane’ participants with little exposure to Western music showed aversion to acoustic roughness but did not prefer conventionally consonant chords or vocal harmonies over dissonant ones. Consonance preferences differed across the comparison populations. Human preference cannot be equated with one universal mathematical interval rule. [McDermott et al., 2016](https://www.nature.com/articles/nature18635).
- **Search can preserve many different good outcomes.** MAP-Elites stores high-performing candidates across a feature space chosen by the designer, instead of returning only one overall winner. Mutation and evaluation improve this archive. Its quality function and feature dimensions still encode choices; the original paper neither studies soundscapes nor establishes unlimited open-ended invention. [Mouret and Clune, 2015](https://arxiv.org/abs/1504.04909).

## Proposed implications for ripple

1. **Give discovery structural freedom.** Search over bodies or resonator networks, excitation mechanisms, controllers, coupling, and environment composition. A fixed cricket synthesizer with randomized frequencies only discovers variants within that authored voice.
2. **Let the world cause its sound.** Derive audio consistently from simulated motion and interactions. Shared forces and coupling can provide coherent relationships, but physical order alone does not guarantee human pleasure.
3. **Select for the listener's experience.** Use acoustic measures as inexpensive filters, then listener comparisons or a preference model for “I would spend time here.” Evaluate whole mixtures and longer behavior, not only isolated beautiful voices. No cited paper establishes a sufficient automatic score for this goal.
4. **Keep diversity explicitly.** A MAP-Elites-style archive could retain appealing worlds across dimensions such as event density, spectral texture, and temporal organization. Novelty alone does not ensure pleasantness; a single smoothness score risks selecting silence or a monotonous tone. These are design failure modes to test, not results from the cited experiments.
5. **Make discoveries reproducible and robust.** Save world descriptions and seeds. Compare candidates with current presets at matched loudness, across multiple runs and longer listening sessions. Test whether interaction actually matters by removing coupling. “Equally harmonious” and “alien” need separate listener judgments.

The plausible route is therefore a general sound-producing world plus a discovery process shaped by human listening. Specific organisms and landscapes can be found rather than prescribed; the laws, search space, and meaning of “good” remain explicit design commitments.

## Follow-up: laws for peaceful generation

The user chose to bias the generator itself toward peaceful listening, with
automatic selection instead of requiring a person to curate every candidate.
The first [alien discovery experiment](../alien-discovery.md) therefore bounds
mechanical energy and simultaneous excitation, softens excitation envelopes,
damps high-frequency modes, and varies the environment slowly. It searches
spring-network structures using a roughness proxy plus a spectral-diversity
constraint, with listening-level matching for comparisons. These are explicit
physical and aesthetic rules. They can enforce measurable bounds and search
preferences; they do not establish a guarantee of subjective peacefulness.

## Follow-up: shared-world diversity and a listening library

The user then chose richer worlds, explicit diversity preservation, and a
browsable library, with the full shared physics from main. The resulting
[alien world library](../alien-atlas.md) assembles the same `World` as Earth
presets. It discovers initial conditions for terrain, weather, materials,
fluids, population organs and behaviour, and listening space. Contacts, local
hearing, coupled bubbles, and water/sediment history remain the shared runtime
mechanisms. This is broader world composition within authored laws, not the
discovery of new physics or biological evolution.

A MAP-Elites-style archive uses rendered brightness, spectral entropy,
temporal variation and stereo width, with a calmness proxy selecting within
each occupied region. Complete recipes and separate favourite markers preserve
discoveries even after a region's champion changes. The richer engine uses a
bounded listening output; the original experiment's global mechanical-energy
ceiling is not claimed for the combined world. Perceptual alienness and
peacefulness still require listening evaluation.
