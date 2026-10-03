# Live events

Start any player (`ripple`, `ripple atlas`, or `ripple discover`) and press
**`l`**. The log shows observations from the simulation currently playing.
There is no separate visual script generating an approximation of the sound.

## Main-screen activity

Every player also shows a compact summary without opening the log. Earth
worlds and the alien library show:

- **Live / 10s:** recent discrete events per simulation second. Counts use
  rolling 100 ms bins over approximately ten seconds; during startup the rate
  uses elapsed time. Periodic state readings do not inflate the event rate.
- **Calls / callers:** syllable starts and distinct inhabitants that called
  in that window. **Heard** is the percentage of arrived calls that reached
  a receiver above its hearing/masking threshold; `--` means no arrivals yet.
- **Links:** distinct directed caller-to-receiver connections actually heard.
  On wider screens, **nudges** counts calls that changed a receiver's clock.
  These reveal local interaction; they do not measure global synchrony.
- **Contacts / bubbles / drops:** recent physical impacts, successful bubble
  voices or packets, and rain impacts. Ember pops or wave breaks appear when
  active. Unvoiced bubble attempts contribute to event rate but not bubbles.
- **Flow / soil / sediment:** the latest state reading. Soil is retained water
  in litres; sediment is suspended plus exported solid in cubic centimetres.
  Quiescent water shows weather instead. Continuous sound can persist even
  when no new discrete events occur.
- **Events/s, 20s:** one column per simulation second, oldest on the left.
  Height scales to the displayed peak count; the current column is incomplete.

The A/B experiment instead shows force starts, distinct excited nodes and
energy for the audible candidate, plus both candidates' recent force counts.
Switching A/B selects that candidate's statistics without resetting its history.
Small terminals show a shorter summary while retaining playback and log keys.

Summaries keep their own bounded bins, so rolling older log rows out of the
2,048-record history does not erase statistics. Filtering or holding the log
does not freeze the main summary. `Space` freezes simulation time, including
its activity windows; restarting or changing worlds clears them. In the
library they follow playback, even when another saved world is highlighted.
If capture or transport misses records, the summary says **partial** for that
playback session. Figures then describe the records received.

## Log controls

| Key | Action in the log |
| --- | --- |
| `l` | Return to the player; capture continues |
| `Tab` / `Shift-Tab` | Cycle All, Calls, Contacts, Water, Weather, Resonators |
| `h` | Hold or resume the view; audio and capture continue |
| `↑` / `↓`, `k` / `j`, `Page Up` / `Page Down` | Scroll a held view |
| `End` | Return to the live tail |
| `Space` | Pause or resume audio and simulation time |
| `+` / `-` | Adjust listening volume |
| `r` | Restart the same world and clear its timeline |
| `q` / `Esc` | Quit |

World selection, favourites, and search controls remain on the player screen.
In the original A/B experiment, `a` and `b` still change the audible candidate.
Both candidates continue evolving, so the log labels their events A or B.
The alien library observes only the world playing, not background candidates.

## What is an event?

Records describe the discrete interactions the engine models:

| Category | Observations |
| --- | --- |
| Calls | An actual audible syllable starts or ends; a call reaches another inhabitant and is heard or masked; the resulting behavioural clock adjustment |
| Contacts | A physical chime/body collision, or an ember pop striking wood |
| Water | Rain impacts, entrained bubble/packet attempts, and breaking waves; once-per-second water and sediment readings |
| Weather | Once-per-second air speed, gust, rainfall and daylight readings |
| Resonators | A/B force excitation starts and ends; once-per-second energy/weather readings |

These are all the discrete event types currently exposed by these mechanisms.
Continuous motion, each lattice vibration, and individual water-cell updates
are not separate records. Water/soil/sediment and weather entries are state
snapshots, not claims that erosion or wind happens only once per second.
Terrain preparation happens before playback and is not part of the live log.

Timestamps are **simulation seconds** since playback began. Calls are observed
at their sample; field, weather, rain, fire and contact changes have the engine's
32-sample control resolution. Hearing records describe another inhabitant's
ear, which can differ from what reaches the listener. Some recorded activity
can also be quiet or masked in the final mix. A bubble attempt marked “voice
pool busy” did not start an additional bubble voice.

The view keeps the latest 2,048 records. “Recent N of M” distinguishes retained
history from the total received; older history rolls off normally. “Missed”
counts records lost if bounded capture or transport fills. Holding a view
keeps its current history fixed while new records enter the live history.
Changing or restarting the world starts a new timeline, including from pause.

## Structured traces

```sh
cargo build --release
./target/release/ripple trace night-meadow 10 --seed 12345 > events.jsonl
./target/release/ripple trace discoveries/atlas/worlds/<id>.world 30 > alien-events.jsonl
```

`trace` uses the same observations at 48 kHz, renders without an audio device,
and writes newline-delimited JSON to stdout. It runs as fast as the simulation
allows, rather than waiting for wall-clock time. Use the live player to follow
events while listening. A saved world supplies its own seed; omit `--seed`.

Each record contains `sequence`, `sample`, `time_seconds`, `kind`, and fields
specific to its kind. Sequence numbers and population/caller/receiver/body
indices start at zero; the human-readable view displays indices from one.
Positions and pan use the engine's normalized coordinates; radii are metres,
water and sediment volumes are cubic metres, and frequencies are hertz.
Strength, level, and clock adjustments retain the model's internal units.

This JSON schema is experimental. A future visualizer can group by population
or body, place events along simulation time, and show source-to-receiver
relationships without trying to infer them from the audio waveform.

## Audio boundary

Observation consumes no random numbers and does not drive the simulation.
The engine writes fixed-size records into preallocated buffers. The audio
callback uses a bounded, nonblocking send; formatting, terminal output, and
history allocation happen on the UI thread. Offline traces drain each control
block and write outside a live audio callback. An `events_lost` record reports
any overflow there instead of silently pretending the trace is complete.

Tests compare observed and unobserved audio sample for sample, check delayed
hearing against source emissions, compare log counts with physics counters,
and exercise a full transport queue, pause, restart, filtering and held views.
Summary tests cover rolling windows, distinct connections, A/B separation,
busy periods, capture gaps, and independence from the visible log history.
