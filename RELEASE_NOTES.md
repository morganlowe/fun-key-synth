## What's New in v1.6.0

**The Producer & Live Performance Release** — elevating **Fun Key Synth** with deeper sequencer expressiveness, Standard MIDI (`.mid`) export, hardware MIDI CC Learn & expression, and a full Stereo Console with live per-track metering.

### Deep Studio Sequencer (`src/model/sequencer.rs`, `src/engine.rs`, `src/ui.rs`)
- **Per-Track & Per-Drum-Lane Clock Division (`1/8`, `1/16`, `1/32`):** Run basslines in `1/8` half-time, arps at `1/16`, and hi-hat/snare rolls in `1/32` double-time independently across any pattern track or individual drum lane.
- **Step Probability & Full P-Locks:** Dial step probability (`10%–100%`) per melodic step or per drum hit (plus Drum Flam triggers), alongside **Filter, Resonance, Drive, and Shape** P-Locks in the Studio Step Inspector.
- **Interactive 16-Step Velocity Paint Strip:** Click or drag across the visual velocity strip directly under the melodic step pads, with micro-badges highlighting active P-Locks, Probability `< 100%`, Accents, and Slides.
- **Pattern & Drum Lane Generators:** Rotate steps (`◀`/`▶`), transpose (`-1`/`+1` scale-quantized, `-12`/`+12` octaves), `Humanize` velocities, and generate `Euclid` (`H` / `R`) grooves per melodic track or drum lane.

### Hardware MIDI & Standard MIDI (`.mid`) Export (`src/midi.rs`, `src/model/project.rs`)
- **Multi-Track Format-1 `.mid` Export:** Click **`MID`** (top bar) or **`Export .MID`** (Arrange tab) to export your active pattern or full Song Arrangement (`SongSection` structure) as a 480-PPQ Standard MIDI file with Tempo metadata, Ch 10 GM Drums, polyphonic `voice_led_chord` chord voicings, Bass, Lead, Acid, Sampler, and active Extra tracks.
- **MIDI Pitch Bend, Aftertouch & Click-to-Learn CC Mapping:** Supports 14-bit MIDI Pitch Bend (`0xE0`, `1–12 st` configurable range), Channel Aftertouch (`0xD0`) filter expression, and one-click **MIDI CC Learn** inside the `MIDI` device dropdown for 13 assignable parameters (including `Rec Motion` automation capture).

### Pro Stereo Console & Live Metering (`src/engine.rs`, `src/ui.rs`)
- **Stereo Console & Meters (`Sound Lab`):** Upgraded mixer panel featuring per-track **Mute/Solo**, **Volume (`0–140%`)**, **Pan**, **M/S Stereo Width (`0%–180%`)**, **Delay/Reverb Sends**, live **Track Peak Bars**, and a **Master Peak dBFS** readout across both core and Extra tracks.
- **Club-Ready `Mono Sub <120Hz` Crossover:** One-click master low-frequency mono summing keeps sub bass phase-tight on any sound system.

### Installation
- **Debian / Ubuntu:** `sudo apt install ./fun-key-synth_1.6.0-1_amd64.deb`
- **Fedora / RHEL / openSUSE:** `sudo dnf install ./fun-key-synth-1.6.0-1.x86_64.rpm`
- **Arch / CachyOS:** `sudo pacman -U fun-key-synth-1.6.0-1-x86_64.pkg.tar.zst`
- **Generic Linux:** Extract `fun-key-synth-v1.6.0-linux-x86_64.tar.gz` and run `./fun-key-synth`
- **Windows:** Extract `fun-key-synth-windows-x86_64.zip` and run `fun-key-synth.exe`
- **macOS:** Extract the `arm64` (Apple Silicon) or `x86_64` (Intel) `.tar.gz` archive and run `./fun-key-synth`
