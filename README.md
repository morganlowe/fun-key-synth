# Fun Key Synth — Vibe Studio

Fun Key Synth is a beginner-friendly, native Linux groovebox and vibe studio written in Rust. Pick a vibe, hit **Space** to groove, jam in-key on your computer keyboard (`A`–`K`), or flip on **Party Mode** to generate and evolve full tracks locally on the fly.

## Features

- **7 Curated Musical Vibes**: Deep House, Heavy Electro Funk, Lo-Fi Chill, Electro-Soul, Night Drive (Synthwave), Acid Club (303), and Ambient Dub
- **Auto-Chord Mode**: Play lush, voice-led 7th/9th/11th chords with a single key (`A`–`K`) or on-screen pad — with optional **Band Follow** so Bass and Arp lock to your live chords
- **Auto-Beat Mode**: Idiomatic, swing-grooved drum patterns per vibe with an Energy slider, turnaround fills, and right-click `2x/3x/4x` step ratchets
- **Party Mode (Local Vibe Song Generator)**: Automatically arranges and evolves tracks through *Intro → Groove → Build → Drop → Break → Drop 2* with filter sweeps, snare builds, and key-locked bass drops
- **True Stereo Audio & Mastering**: Stereo ping-pong delay, lush 4-comb + allpass stereo reverb, Haas stereo widening on chords & arps, kick-triggered sidechain ducking, real swing, smooth legato/slides, and a warm soft-clipping master limiter
- **Role-Named Studio Tracks**: Drums (808/909), Bass (Moog), Chords (Poly Moog), Arp (Monologue), Lead / 303 (TB-303), and Texture (Pro Sampler) — each with instant 1-click Voice Presets
- **Pro Sampler Workbench**: Dedicated `SAMPLER` tab supporting **MP3, WAV, FLAC, and OGG** loading, interactive waveform display with draggable Start/End handles and live playhead, Auto-Trim, Normalize, Reverse, One-Shot / Loop / Ping-Pong, Transpose & Fine Tune, SK-1 8-Bit / 12-Bit / Hi-Fi grit, LP/BP/HP filter, ADSR envelope, and playable pitch pads
- **DJ Performance Tools & Export**: Live `Bass Drop`, `Drum Cut`, `Snare Build`, `Stutter`, `Fill`, JSON Save/Load, and **1-click Stereo WAV Export**

## Quick Controls

- **`Space`** — Play / Stop
- **`A S D F G H J K`** — Play Auto-Chord Pads (or Scale-Locked Lead keys)
- **`[` / `]`** — Shift octave in Lead mode
- **Right-click drum step** — Cycle `1x / 2x / 3x / 4x` glitch ratchet
- **Right-click melodic step** — Select step to edit without toggling it off

## Run

Install Rust and the Linux ALSA development libraries, then run:

```sh
cargo run --release
```

## Test

```sh
cargo test
```
