# Fun Key Synth

Fun Key Synth is a Linux desktop groovebox written in Rust. It combines analog-style synthesizers, 808/909-inspired drums, a lo-fi sampler, a step sequencer, song arrangement, and live master effects.

## Features

- Moog-style bass/lead synth and a second independent polyphonic Moog layer for chords and pads
- Monologue-style and TB-303-style synth voices, plus 808/909 drums and an SK-1-inspired sampler
- 16, 32, 48, and 64-step patterns with per-step note, velocity, gate, chord, accent, and slide controls
- Song arranger with pattern sections, bass drops, fills, and an automatic party-song arrangement
- Global filter, tempo-synced delay, reverb, chop, repeat, glitch, circuit bend, and output waveform display
- JSON project save and load

## Run

Install Rust and the Linux audio development libraries required by CPAL (ALSA). Then run:

```sh
cargo run
```

The application currently requires a default audio output device with `f32` sample format.

## Build check

```sh
cargo check
```
