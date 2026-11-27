## What's New in v1.7.0

**The Mixing, Groove & Genre Authenticity Release** — a ground-up overhaul of mix balance, drum & 303 synthesis, melodic composition, Party DJ curation, and Studio usability.

### Mixing, Headroom & Drum / 303 DSP Overhaul (`src/dsp/drums.rs`, `src/dsp/tb303.rs`, `src/dsp/fx.rs`, `src/engine.rs`)
- **Balanced Mix Hierarchy & Console Smoothing:** Re-calibrated per-track bus gains and Studio channel EQ (`StudioChannelEq`) so punchy Drums and warm Bass anchor the mix while Chords sit lushly underneath and Leads float smoothly inside the pocket without harsh 3–8 kHz peaks.
- **Fat Kicks, Tight Snares & Hand-Slap Claps:** Increased clean sub weight on kicks while taming papery beater clicks, added wood-shell punch and tighter high-frequency snap to snares, and shaped clap bandpass resonances (`590 Hz` / `1.1 kHz`) to replace raw white-noise tail bursts.
- **Musical, Non-Screeching TB-303:** Tamed filter sweep range (`2.8` octaves), capped self-oscillating resonance and overdrive fizz with a `5.5 kHz` post-drive low-pass, and stopped Party Mode from pushing 303 cutoffs into piercing extremes.
- **Polyphonic Headroom & True-Stereo Mastering:** Continuous polyphonic voice scaling across `MoogSynth`, `FmSynth`, and `WavetableSynth`, true-stereo 120 Hz bus-glue detector, wet-send sidechain ducking (`set_wet_duck`), and softer `PartyFilterSweep` transitions with zero glitch/stutter bursts.

### Composition & Beats Overhaul (`src/model/vibe.rs`)
- **Genre Pockets Preserved Across Every Seed:** Fixed `generate_melodic_tracks` so every vibe keeps its hand-authored chord comping and bass groove across all variations instead of falling back to generic patterns.
- **Spacious, Singable Lead Phrases:** Replaced robotic long whistle drones on LA Hip Hop and frantic 9-note machine-gun arpeggios on Eurobeat/Trance/Imperial Bass with structured ABAC call-and-response hooks and breathing rests, alongside tight low-mid 303 counter-lines (`MIDI 36–55`).
- **Cleaner Drum Grooves:** Eliminated stray plastic tom solos (`hat_var == 4` and un-gated `fill_var` tom rolls) and kept half-time/4x4 club/hip-hop pockets tight and locked.

### 26 Vibes, Hip-Hop Crate & Genre-Authentic Party DJ (`src/model/vibe.rs`, `src/engine.rs`, `src/ui.rs`)
- **New Hip-Hop Vibes & Crate Mode:** Added **Atlanta Trap** (138 BPM, booming 808 glides + 3x/4x ratcheting hats) and **LA Hip Hop** (94 BPM West Coast G-Funk bounce + warm Minimoog whistle lead), plus the **Hip-Hop Crate** DJ mix mode.
- **Genre-Authentic DJ Curation & Flow:** Purged out-of-genre orchestral/woodwind presets from modern pools, enforced per-genre `ensemble_mask` instrument density with `Hip-Hop Pocket` & `Minimal Acid` arrangement templates, locked song identity across sections with smooth ramps, and added `[DJ TRIM]` live Studio fader trims.
- **Studio Usability:** Categorized Studio Vibe Picker (`Club`, `Hip-Hop`, `Chill`, `80s / Funk`, `Heavy Bass`, `All`) and `Fresh Groove` progression cycling.

### Installation
- **Debian / Ubuntu:** `sudo apt install ./fun-key-synth_1.7.0-1_amd64.deb`
- **Fedora / RHEL / openSUSE:** `sudo dnf install ./fun-key-synth-1.7.0-1.x86_64.rpm` (or `sudo zypper install ./fun-key-synth-1.7.0-1.x86_64.rpm`)
- **Arch / CachyOS:** `sudo pacman -U fun-key-synth-1.7.0-1-x86_64.pkg.tar.zst`
- **Generic Linux:** Extract `fun-key-synth-v1.7.0-linux-x86_64.tar.gz` and run `./fun-key-synth`
- **Windows:** Extract `fun-key-synth-windows-x86_64.zip` and run `fun-key-synth.exe`
- **macOS:** Extract the `arm64` (Apple Silicon) or `x86_64` (Intel) `.tar.gz` archive and run `./fun-key-synth`
