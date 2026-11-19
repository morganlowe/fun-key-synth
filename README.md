# Fun Key Synth v1.0 — Vibe Studio & 8-Part Performance Sampler

Fun Key Synth is a beginner-friendly, cross-platform groovebox, vibe studio, and Volca-style 8-part performance sampler written in Rust. Pick a vibe, hit **Space** to groove, jam in-key on your computer keyboard (`A`–`K`), auto-chop MP3/WAV samples across 8 pads, or flip on **Party Mode** to generate and evolve full tracks locally on the fly.

## Features

- **7 Curated Musical Vibes**: Deep House, Heavy Electro Funk, Lo-Fi Chill, Electro-Soul, Night Drive (Synthwave), Acid Club (303), and Ambient Dub
- **Auto-Chord Mode**: Play lush, voice-led 7th/9th/11th chords with a single key (`A`–`K`) or on-screen pad — with optional **Band Follow** so Bass and Arp lock to your live chords
- **Auto-Beat Mode**: Idiomatic, swing-grooved drum patterns per vibe with an Energy slider, turnaround fills, click-to-mute drum lanes, right-click drum tuning, and `2x/3x/4x` step ratchets
- **Party Mode (Local Vibe Song Generator)**: Automatically arranges and evolves tracks through *Intro → Groove → Build → Drop 1 → Break → Drop 2* with filter sweeps, snare builds, and key-locked bass drops
- **Volca-Style 8-Part Performance Sampler (`SAMPLER` tab)**:
  - Load **MP3, WAV, FLAC, or OGG** files onto any of the 8 pads — or **1-click Auto-Chop** a file evenly across all 8 pads with Choke Groups
  - Interactive visual waveform with draggable **Start / End** handles and live playhead
  - Live **Speed (`0.25x–2.0x`)**, Pitch (`±24 st`), Reverse, One-Shot / Loop / Ping-Pong, LP/BP/HP Filter, ADSR, SK-1 8-Bit / 12-Bit / Hi-Fi grit, Pan, Reverb Send, and **Volca Analog Isolator (Bass & Treble Kill/Boost EQ)**
  - Dedicated 8-lane step sequencer (`P1`–`P8`) with click-to-mute, right-click ratchets, and 1-click Auto-Chop Groove
- **Full Hardware Synth Lab (`SOUND LAB` tab)**: Deep controls for Moog (3-osc + 24dB ladder + dual ADSR + LFO), Monologue (dual VCO + Sync/Ring + SVF + EG + LFO), TB-303 (acid tuning/cutoff/res/accent), and 808/909 Drums — with a hands-free **`🔁 TEST NOTE`** (`1/4` / `1/8` / `Drone`) toggle
- **True Stereo Audio & Mastering**: Stereo ping-pong delay, lush 4-comb + allpass stereo reverb, Haas stereo widening, kick-triggered sidechain ducking, real swing, soft-clipping master limiter, JSON Save/Load, and **1-click Stereo WAV Export**

## Install (v1.0.0 Releases)

Download prebuilt packages from the [Releases](https://github.com/morganlowe/fun-key-synth/releases) page:

- **Debian / Ubuntu (`.deb`)**:
  ```sh
  sudo apt install ./fun-key-synth_1.0.0_amd64.deb
  ```
- **Arch Linux (`.pkg.tar.zst`)**:
  ```sh
  sudo pacman -U fun-key-synth-1.0.0-1-x86_64.pkg.tar.zst
  ```
- **Windows (`.zip`)**:
  Extract `fun-key-synth-v1.0.0-windows-x86_64.zip` and run `fun-key-synth.exe`.
- **Generic Linux (`.tar.gz`)**:
  Extract `fun-key-synth-v1.0.0-linux-x86_64.tar.gz` and run `./fun-key-synth`.

## Quick Controls

- **`Space`** — Play / Stop
- **`A S D F G H J K`** — Play Auto-Chord Pads (or Scale-Locked Lead keys)
- **`[` / `]`** — Shift octave in Lead mode
- **Click drum/pad label** — Mute / Unmute lane
- **Right-click drum/instrument label** — Full tuning & pattern options menu
- **Right-click drum/sampler step** — Cycle `1x / 2x / 3x / 4x` glitch ratchet
- **Double-click track name** — Open full synth in **SOUND LAB** (or **SAMPLER**)

## Build From Source

```sh
cargo run --release
```
