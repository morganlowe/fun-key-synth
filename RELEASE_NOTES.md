## What's New in v1.5.0

A major DSP, synthesis engine, sampler, and workflow release for fun-key-synth.

### New Synthesis & Sample Engines
- **FM Synthesis Engine (`src/dsp/fm.rs`):** Added multi-operator FM synthesis for metallic bells, electric pianos, basses, and harmonic textures.
- **Wavetable Synthesis Engine (`src/dsp/wavetable.rs`):** Added sweepable wavetable oscillators for evolving pads, leads, and modern bass design.
- **SoundFont (`.sf2`) Player (`src/dsp/soundfont.rs`):** Added native SoundFont playback support so classic GM and custom `.sf2` banks can be loaded directly into tracks.
- **Multisample & Orchestral Instrument Engine (`src/dsp/multisample.rs`):** Added velocity- and pitch-mapped multisample instruments with sweet-register orchestral melody generation.
- **Expanded Analog & Bass Modeling:** Upgraded the Monologue (`monologue.rs`), Moog (`moog.rs`), and TB-303 (`tb303.rs`) DSP engines, including crisp Eurobeat Octave bass voicing and improved musicality across lead and bass patches.

### Physical-Modeling Drum Engine Overhaul (`src/dsp/drums.rs`)
- **Hybrid PCM + Physical Modeling Bank:** Added a procedural and sample-layered drum transient bank featuring 909/studio FM-coupled bronze hi-hats, an 18-inch hammered bronze crash cymbal with nonlinear von Karman pitch-glide and 4-line dispersive plate stereo shimmer, acoustic felt/wood kick beater transients, birch/maple shell resonances, and classic 12-bit EPROM companded drum-machine crack.
- **Studio Console & Room Processing:** Added 3-mode circular membrane snare and tom physics, sympathetic snare-wire buzz excited by kick and tom hits, Mid/Side decorrelated stereo snare wires with phase-clean mono summing, a 2-stage diffused early-reflection studio drum room, and a drum bus console contour (95 Hz punch, 380 Hz box carve, 4.2 kHz snap, 28 Hz subsonic HPF, and 14.5 kHz silk LPF).
- **TR-808 Metallic Bank Upgrade:** Added Schmitt-trigger metallic oscillators with diode-matrix intermodulation warmth and 12 dB band-limited air noise.

### Volca-Style 8-Part Sampler & Groove Sequencer (`src/volca_ui.rs`, `src/dsp/sampler.rs`)
- **Built-In Groove & Pattern Presets:** Added one-click groove generators for House 4x4, Boom Bap, Breakbeat, Afro / Polyrhythm (16 vs. 12 Euclidean), and Auto-Chop Cycle patterns.
- **Polymeter & Euclidean Tools:** Added per-lane active step lengths, Euclidean hit/rotation controls, drag-to-paint step entry, and live motion recording onto active steps while playing.
- **Improved Pad Ergonomics:** Added keyboard triggers (`1-8` / `A-K`), live trigger LEDs, step hit counters, and dedicated Mute/Solo micro-rows under each pad without hitbox overlap.

### Sound Lab, Preset Browser & DJ Performance Workflow (`src/ui.rs`, `src/settings.rs`)
- **Preset Browser, Favorites & User Patches:** Added a dedicated Preset Browser bar in Sound Lab with persistent favorite presets and custom user patch saving/loading (`UserPatch`).
- **Live DJ Instrument Picker & Hot-Reload:** Added live instrument switching and hot-reloading during performance alongside an A/B offline render harness (`examples/ab_render.rs`).
- **Undo / Redo & Input Reliability:** Added full project Undo/Redo with sample-generation tracking, and fixed X11 synthetic `KeyRelease + KeyPress` auto-repeat glitches so held chords and arpeggiators never re-trigger unintentionally.
- **Layout & Vibe Macros Fix:** Fixed Vibe Macros overflow ("Warmth", "Glitch", "Master") and Arrange/Party DJ picker bounds across both 1440px and 920px viewport widths.

### Installation
- **Debian / Ubuntu:** `sudo apt install ./fun-key-synth_1.5.0-1_amd64.deb`
- **Fedora / RHEL / openSUSE:** `sudo dnf install ./fun-key-synth-1.5.0-1.x86_64.rpm`
- **Arch / CachyOS:** `sudo pacman -U fun-key-synth-1.5.0-1-x86_64.pkg.tar.zst`
- **Generic Linux:** Extract `fun-key-synth-v1.5.0-linux-x86_64.tar.gz` and run `./fun-key-synth`
- **Windows:** Extract `fun-key-synth-windows-x86_64.zip` and run `fun-key-synth.exe`
- **macOS:** Extract the `arm64` (Apple Silicon) or `x86_64` (Intel) `.tar.gz` archive and run `./fun-key-synth`
