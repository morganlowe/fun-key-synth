use serde::{Deserialize, Serialize};

use super::{
    root_name, ChordType, DelayDivision, DrumModel, DrumTrackSeq, DrumVoiceId, GlobalFilterMode,
    MelodicTrackSeq, MonologueOscWave, MoogWave, Project, SamplerMode, Scale, SongSection,
    StepCount, Tb303Wave, TrackId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum Vibe {
    #[default]
    DeepHouse,
    ElectroFunk,
    LofiChill,
    ElectroSoul,
    Synthwave,
    AcidClub,
    AmbientDub,
    UkGarage,
    JungleDnb,
    DetroitTechno,
    DreamPop,
}

impl Vibe {
    pub const ALL: [Vibe; 11] = [
        Vibe::DeepHouse,
        Vibe::ElectroFunk,
        Vibe::LofiChill,
        Vibe::ElectroSoul,
        Vibe::Synthwave,
        Vibe::AcidClub,
        Vibe::AmbientDub,
        Vibe::UkGarage,
        Vibe::JungleDnb,
        Vibe::DetroitTechno,
        Vibe::DreamPop,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Vibe::DeepHouse => "Deep House",
            Vibe::ElectroFunk => "Electro Funk",
            Vibe::LofiChill => "Lo-Fi Chill",
            Vibe::ElectroSoul => "Electro-Soul",
            Vibe::Synthwave => "Night Drive",
            Vibe::AcidClub => "Acid Club",
            Vibe::AmbientDub => "Ambient Dub",
            Vibe::UkGarage => "UK Garage",
            Vibe::JungleDnb => "Jungle / DnB",
            Vibe::DetroitTechno => "Detroit Techno",
            Vibe::DreamPop => "Dream Pop",
        }
    }

    pub const fn tagline(self) -> &'static str {
        match self {
            Vibe::DeepHouse => "Warm 9th chords, punchy 4x4 & sidechain bounce",
            Vibe::ElectroFunk => "Slappy syncopated bass, 808 cowbell & funky stabs",
            Vibe::LofiChill => "Dusty Rhodes chords, lazy swing & warm tape grit",
            Vibe::ElectroSoul => "Glitchy crisp hats, shimmering arps & emotional chords",
            Vibe::Synthwave => "Driving 8th-note Moog bass, 80s pads & neon lead",
            Vibe::AcidClub => "Squelchy 303 slides, 909 hats & club energy",
            Vibe::AmbientDub => "Spacious ping-pong dub chords, deep sub & huge reverb",
            Vibe::UkGarage => "Heavy 2-step swing, shuffling hats & warm organ bass",
            Vibe::JungleDnb => "170 BPM breakbeat energy, rolling Reese sub & lush pads",
            Vibe::DetroitTechno => "Driving 909 pulse, hypnotic chord stabs & deep analog bass",
            Vibe::DreamPop => "Shimmering wide pads, dreamy dotted-8th arps & lush reverb",
        }
    }

    pub const fn default_bpm(self) -> f32 {
        match self {
            Vibe::DeepHouse => 122.0,
            Vibe::ElectroFunk => 112.0,
            Vibe::LofiChill => 86.0,
            Vibe::ElectroSoul => 124.0,
            Vibe::Synthwave => 118.0,
            Vibe::AcidClub => 128.0,
            Vibe::AmbientDub => 108.0,
            Vibe::UkGarage => 132.0,
            Vibe::JungleDnb => 170.0,
            Vibe::DetroitTechno => 130.0,
            Vibe::DreamPop => 104.0,
        }
    }

    /// Swing stored in 0.0..=0.5 (displayed 50%..=75%)
    pub const fn default_swing(self) -> f32 {
        match self {
            Vibe::DeepHouse => 0.12,
            Vibe::ElectroFunk => 0.18,
            Vibe::LofiChill => 0.22,
            Vibe::ElectroSoul => 0.08,
            Vibe::Synthwave => 0.02,
            Vibe::AcidClub => 0.08,
            Vibe::AmbientDub => 0.06,
            Vibe::UkGarage => 0.26,
            Vibe::JungleDnb => 0.04,
            Vibe::DetroitTechno => 0.08,
            Vibe::DreamPop => 0.06,
        }
    }

    pub const fn default_root(self) -> u8 {
        match self {
            Vibe::DeepHouse => 9,     // A
            Vibe::ElectroFunk => 4,   // E
            Vibe::LofiChill => 2,     // D
            Vibe::ElectroSoul => 5,   // F
            Vibe::Synthwave => 9,     // A
            Vibe::AcidClub => 0,      // C
            Vibe::AmbientDub => 7,    // G
            Vibe::UkGarage => 5,      // F
            Vibe::JungleDnb => 2,     // D
            Vibe::DetroitTechno => 0, // C
            Vibe::DreamPop => 7,      // G
        }
    }

    pub const fn default_scale(self) -> Scale {
        match self {
            Vibe::DeepHouse => Scale::Minor,
            Vibe::ElectroFunk => Scale::Dorian,
            Vibe::LofiChill => Scale::Dorian,
            Vibe::ElectroSoul => Scale::Minor,
            Vibe::Synthwave => Scale::Minor,
            Vibe::AcidClub => Scale::AcidMinor,
            Vibe::AmbientDub => Scale::Minor,
            Vibe::UkGarage => Scale::Dorian,
            Vibe::JungleDnb => Scale::Minor,
            Vibe::DetroitTechno => Scale::Minor,
            Vibe::DreamPop => Scale::Major,
        }
    }

    pub const fn default_progression(self) -> usize {
        match self {
            Vibe::DeepHouse => 0,
            Vibe::ElectroFunk => 3,
            Vibe::LofiChill => 4,
            Vibe::ElectroSoul => 1,
            Vibe::Synthwave => 2,
            Vibe::AcidClub => 5,
            Vibe::AmbientDub => 6,
            Vibe::UkGarage => 0,
            Vibe::JungleDnb => 6,
            Vibe::DetroitTechno => 5,
            Vibe::DreamPop => 7,
        }
    }

    pub const fn accent_rgb(self) -> [u8; 3] {
        match self {
            Vibe::DeepHouse => [64, 196, 255],
            Vibe::ElectroFunk => [255, 168, 38],
            Vibe::LofiChill => [212, 148, 255],
            Vibe::ElectroSoul => [82, 232, 180],
            Vibe::Synthwave => [255, 86, 168],
            Vibe::AcidClub => [186, 255, 56],
            Vibe::AmbientDub => [96, 165, 250],
            Vibe::UkGarage => [255, 198, 66],
            Vibe::JungleDnb => [255, 96, 96],
            Vibe::DetroitTechno => [130, 178, 255],
            Vibe::DreamPop => [246, 146, 218],
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ChordSlot {
    pub offset: u8, // semitones from project root
    pub chord: ChordType,
}

#[derive(Debug, Clone, Copy)]
pub struct ChordProgression {
    pub name: &'static str,
    pub slots: [ChordSlot; 4],
}

pub const PROGRESSIONS: [ChordProgression; 8] = [
    ChordProgression {
        name: "Deep Soul (i9 - VI9 - iv9 - v7)",
        slots: [
            ChordSlot { offset: 0, chord: ChordType::Minor9 },
            ChordSlot { offset: 8, chord: ChordType::Major9 },
            ChordSlot { offset: 5, chord: ChordType::Minor9 },
            ChordSlot { offset: 7, chord: ChordType::Minor7 },
        ],
    },
    ChordProgression {
        name: "Indie Euphoria (i9 - VI9 - IIIadd9 - VII)",
        slots: [
            ChordSlot { offset: 0, chord: ChordType::Minor9 },
            ChordSlot { offset: 8, chord: ChordType::Major9 },
            ChordSlot { offset: 3, chord: ChordType::Add9 },
            ChordSlot { offset: 10, chord: ChordType::Major },
        ],
    },
    ChordProgression {
        name: "Night Drive (i7 - VI7 - III - VII)",
        slots: [
            ChordSlot { offset: 0, chord: ChordType::Minor7 },
            ChordSlot { offset: 8, chord: ChordType::Major7 },
            ChordSlot { offset: 3, chord: ChordType::Add9 },
            ChordSlot { offset: 10, chord: ChordType::Sus2 },
        ],
    },
    ChordProgression {
        name: "Heavy Funk (i9 - IV9 - VI9 - V9)",
        slots: [
            ChordSlot { offset: 0, chord: ChordType::Minor9 },
            ChordSlot { offset: 5, chord: ChordType::Dom9 },
            ChordSlot { offset: 8, chord: ChordType::Major9 },
            ChordSlot { offset: 7, chord: ChordType::Dom9 },
        ],
    },
    ChordProgression {
        name: "Lo-Fi Jazz (ii9 - V9 - Imaj9 - vi9)",
        slots: [
            ChordSlot { offset: 0, chord: ChordType::Minor9 },
            ChordSlot { offset: 5, chord: ChordType::Dom9 },
            ChordSlot { offset: 10, chord: ChordType::Major9 },
            ChordSlot { offset: 7, chord: ChordType::Minor11 },
        ],
    },
    ChordProgression {
        name: "Acid Hypnosis (i7 - i7 - bII7 - VII)",
        slots: [
            ChordSlot { offset: 0, chord: ChordType::Minor7 },
            ChordSlot { offset: 0, chord: ChordType::Minor9 },
            ChordSlot { offset: 3, chord: ChordType::Major7 },
            ChordSlot { offset: 10, chord: ChordType::Sus2 },
        ],
    },
    ChordProgression {
        name: "Dub Space (i11 - iv9 - i11 - v7)",
        slots: [
            ChordSlot { offset: 0, chord: ChordType::Minor11 },
            ChordSlot { offset: 5, chord: ChordType::Minor9 },
            ChordSlot { offset: 0, chord: ChordType::Minor9 },
            ChordSlot { offset: 7, chord: ChordType::Minor7 },
        ],
    },
    ChordProgression {
        name: "Sunrise Lift (VI9 - VII - i9 - IIIadd9)",
        slots: [
            ChordSlot { offset: 8, chord: ChordType::Major9 },
            ChordSlot { offset: 10, chord: ChordType::Add9 },
            ChordSlot { offset: 0, chord: ChordType::Minor9 },
            ChordSlot { offset: 3, chord: ChordType::Major9 },
        ],
    },
];

pub fn progression_at(idx: usize) -> &'static ChordProgression {
    &PROGRESSIONS[idx % PROGRESSIONS.len()]
}

/// Returns the human-readable chord symbol (e.g. "Am9", "Fmaj9").
pub fn chord_symbol(root_pc: u8, slot: ChordSlot) -> String {
    let pc = (root_pc + slot.offset) % 12;
    let suffix = match slot.chord {
        ChordType::Single => "",
        ChordType::Major => "",
        ChordType::Minor => "m",
        ChordType::Sus2 => "sus2",
        ChordType::Sus4 => "sus4",
        ChordType::Major7 => "maj7",
        ChordType::Minor7 => "m7",
        ChordType::Minor9 => "m9",
        ChordType::Major9 => "maj9",
        ChordType::Dom9 => "9",
        ChordType::Minor11 => "m11",
        ChordType::Add9 => "add9",
    };
    format!("{}{}", root_name(pc), suffix)
}

/// Places a bass root cleanly in the musical sub/bass register (MIDI 33..=45, A1..A2).
pub fn bass_midi_for(root_pc: u8, offset: u8) -> u8 {
    let pc = (root_pc + offset) % 12;
    // Center around 33..44 (A1..G#2)
    let mut note = 36 + pc;
    if note > 45 {
        note -= 12;
    }
    note
}

/// Builds a lush, voice-led chord centered around `center_midi` (typically 60 = C4)
/// so consecutive chords move smoothly without awkward octave jumps.
pub fn voice_led_chord(root_pc: u8, slot: ChordSlot, center_midi: u8) -> Vec<u8> {
    let chord_root_pc = (root_pc + slot.offset) % 12;
    // Place root close to center_midi - 7
    let mut base = (center_midi as i32 / 12) * 12 + chord_root_pc as i32;
    while base > center_midi as i32 - 2 {
        base -= 12;
    }
    while base < center_midi as i32 - 13 {
        base += 12;
    }

    let mut notes = Vec::with_capacity(5);
    for (idx, &iv) in slot.chord.intervals().iter().enumerate() {
        let mut n = base + iv as i32;
        // Fold upper extensions gently into the warm pad zone [center-10 .. center+12]
        if idx > 0 {
            while n > center_midi as i32 + 12 {
                n -= 12;
            }
            while n < center_midi as i32 - 9 {
                n += 12;
            }
        }
        let m = n.clamp(36, 84) as u8;
        if !notes.contains(&m) {
            notes.push(m);
        }
    }
    notes.sort_unstable();
    notes
}

/// Returns chord tones suitable for arpeggiation around MIDI 60..79.
pub fn arp_tones_for(root_pc: u8, slot: ChordSlot) -> Vec<u8> {
    let base_notes = voice_led_chord(root_pc, slot, 66);
    let mut out = base_notes.clone();
    if let Some(&first) = base_notes.first() {
        out.push((first + 12).min(88));
    }
    out
}

/// Playable Auto-Chord Pad descriptor (mapped to A S D F G H J K).
#[derive(Debug, Clone)]
pub struct AutoChordPad {
    pub key_label: &'static str,
    pub chord_name: String,
    pub bass_midi: u8,
    pub chord_notes: Vec<u8>,
    pub slot: ChordSlot,
}

/// Builds 8 diatonic, always-in-key lush chords for the Auto-Chord Keyboard / Pads.
pub fn build_auto_chord_pads(root_note: u8, scale: Scale) -> Vec<AutoChordPad> {
    const KEYS: [&str; 8] = ["A", "S", "D", "F", "G", "H", "J", "K"];
    let slots: [ChordSlot; 8] = match scale {
        Scale::Major => [
            ChordSlot { offset: 0, chord: ChordType::Major9 },
            ChordSlot { offset: 2, chord: ChordType::Minor9 },
            ChordSlot { offset: 4, chord: ChordType::Minor7 },
            ChordSlot { offset: 5, chord: ChordType::Major9 },
            ChordSlot { offset: 7, chord: ChordType::Dom9 },
            ChordSlot { offset: 9, chord: ChordType::Minor9 },
            ChordSlot { offset: 11, chord: ChordType::Minor7 },
            ChordSlot { offset: 0, chord: ChordType::Add9 },
        ],
        Scale::Dorian => [
            ChordSlot { offset: 0, chord: ChordType::Minor9 },
            ChordSlot { offset: 2, chord: ChordType::Minor7 },
            ChordSlot { offset: 3, chord: ChordType::Major9 },
            ChordSlot { offset: 5, chord: ChordType::Dom9 },
            ChordSlot { offset: 7, chord: ChordType::Minor9 },
            ChordSlot { offset: 9, chord: ChordType::Minor7 },
            ChordSlot { offset: 10, chord: ChordType::Add9 },
            ChordSlot { offset: 0, chord: ChordType::Minor11 },
        ],
        _ => [
            // Minor / Pentatonic / Acid / Phrygian — lush minor-key palette
            ChordSlot { offset: 0, chord: ChordType::Minor9 },
            ChordSlot { offset: 3, chord: ChordType::Major9 },
            ChordSlot { offset: 5, chord: ChordType::Minor9 },
            ChordSlot { offset: 7, chord: ChordType::Minor7 },
            ChordSlot { offset: 8, chord: ChordType::Major9 },
            ChordSlot { offset: 10, chord: ChordType::Add9 },
            ChordSlot { offset: 7, chord: ChordType::Dom9 },
            ChordSlot { offset: 0, chord: ChordType::Minor11 },
        ],
    };

    slots
        .iter()
        .enumerate()
        .map(|(i, &slot)| {
            let center = if i == 7 { 66 } else { 60 };
            let mut label = chord_symbol(root_note, slot);
            if i == 7 {
                label.push('+');
            }
            AutoChordPad {
                key_label: KEYS[i],
                chord_name: label,
                bass_midi: bass_midi_for(root_note, slot.offset),
                chord_notes: voice_led_chord(root_note, slot, center),
                slot,
            }
        })
        .collect()
}

/// Deterministic fast PRNG for reproducible groove generation.
#[derive(Debug, Clone, Copy)]
pub struct Rng(u32);

impl Rng {
    pub fn new(seed: u32) -> Self {
        Self(if seed == 0 { 0x6D2B_79F5 } else { seed })
    }

    pub fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }

    pub fn f32(&mut self) -> f32 {
        (self.next_u32() as f32) / (u32::MAX as f32)
    }

    pub fn range(&mut self, max: usize) -> usize {
        if max == 0 {
            0
        } else {
            (self.next_u32() as usize) % max
        }
    }
}

/// Instant Voice Preset names per studio track (6 tracks).
pub fn voice_preset_names(track: TrackId) -> &'static [&'static str] {
    match track {
        TrackId::Drums => &["808 Warm", "909 Club", "Funk Electro", "Lo-Fi Dusty"],
        TrackId::Moog => &["Warm Sub", "Funk Slap", "Synthwave 8th", "Deep Reese"],
        TrackId::MoogPad => &["Lush Pad", "Warm Strings", "Dub Chord", "Neon Brass"],
        TrackId::Monologue => &["Pluck Arp", "Sparkle Bell", "Funk Clav", "Glide Lead"],
        TrackId::Tb303 => &["Acid Squelch", "Liquid 303", "Funky Acid", "Deep Sub 303"],
        TrackId::Sampler => &[
            "Minor9 Rhodes",
            "Brass Stab",
            "Choir Ahh",
            "Retro FM Bell",
            "Toy Piano",
        ],
        TrackId::MidiOut => &["Off"],
    }
}

/// Applies one of the quick voice presets to the project's synth/drum/sampler params.
pub fn apply_voice_preset(p: &mut Project, track: TrackId, preset_idx: usize) {
    match track {
        TrackId::Drums => {
            p.voice_presets[0] = preset_idx % 4;
            match p.voice_presets[0] {
                0 => {
                    // 808 Warm
                    p.drums.set_all_model(DrumModel::Tr808);
                    p.drums.voices[0].tune = -2.0;
                    p.drums.voices[0].decay = 0.62;
                    p.drums.voices[0].tone = 0.42;
                    p.drums.voices[0].level = 0.96;
                    p.drums.voices[1].snappy = 0.68;
                    p.drums.voices[1].decay = 0.34;
                    p.drums.voices[4].decay = 0.14;
                    p.drums.voices[4].level = 0.64;
                    p.drums.voices[5].decay = 0.42;
                }
                1 => {
                    // 909 Club
                    p.drums.set_all_model(DrumModel::Tr909);
                    p.drums.voices[0].tune = 1.0;
                    p.drums.voices[0].decay = 0.48;
                    p.drums.voices[0].tone = 0.58;
                    p.drums.voices[0].level = 0.98;
                    p.drums.voices[1].snappy = 0.78;
                    p.drums.voices[1].decay = 0.30;
                    p.drums.voices[4].decay = 0.12;
                    p.drums.voices[4].level = 0.68;
                    p.drums.voices[5].decay = 0.38;
                }
                2 => {
                    // Funk Electro (tight snappy punch + bright cowbell)
                    p.drums.set_hybrid();
                    p.drums.voices[0].tune = 0.0;
                    p.drums.voices[0].decay = 0.46;
                    p.drums.voices[0].tone = 0.56;
                    p.drums.voices[0].level = 0.98;
                    p.drums.voices[1].snappy = 0.85;
                    p.drums.voices[1].decay = 0.28;
                    p.drums.voices[2].level = 0.80;
                    p.drums.voices[3].level = 0.72; // Cowbell/Rim pops!
                    p.drums.voices[4].decay = 0.11;
                    p.drums.voices[4].level = 0.66;
                }
                _ => {
                    // Lo-Fi Dusty
                    p.drums.set_all_model(DrumModel::Tr808);
                    p.drums.voices[0].tune = -3.0;
                    p.drums.voices[0].decay = 0.52;
                    p.drums.voices[0].tone = 0.28;
                    p.drums.voices[0].level = 0.90;
                    p.drums.voices[1].snappy = 0.45;
                    p.drums.voices[1].tone = 0.38;
                    p.drums.voices[1].decay = 0.26;
                    p.drums.voices[4].tone = 0.42;
                    p.drums.voices[4].decay = 0.12;
                    p.drums.voices[4].level = 0.54;
                }
            }
        }
        TrackId::Moog => {
            p.voice_presets[1] = preset_idx % 4;
            p.moog.poly_mode = false;
            match p.voice_presets[1] {
                0 => {
                    // Warm Sub
                    p.moog.osc1_wave = MoogWave::Triangle;
                    p.moog.osc2_wave = MoogWave::Saw;
                    p.moog.osc2_octave = 0;
                    p.moog.osc2_detune = 5.0;
                    p.moog.osc1_level = 0.95;
                    p.moog.osc2_level = 0.50;
                    p.moog.osc3_level = 0.32;
                    p.moog.cutoff = 420.0;
                    p.moog.resonance = 0.18;
                    p.moog.env_amount = 0.42;
                    p.moog.drive = 0.22;
                    p.moog.filt_decay = 0.24;
                    p.moog.filt_sustain = 0.20;
                    p.moog.amp_attack = 0.005;
                    p.moog.amp_decay = 0.30;
                    p.moog.amp_sustain = 0.65;
                    p.moog.amp_release = 0.12;
                    p.moog.glide = 0.04;
                }
                1 => {
                    // Funk Slap (rubbery resonant Moog funk bass!)
                    p.moog.osc1_wave = MoogWave::Saw;
                    p.moog.osc2_wave = MoogWave::Square;
                    p.moog.osc2_octave = -1;
                    p.moog.osc2_detune = 7.0;
                    p.moog.osc1_level = 0.88;
                    p.moog.osc2_level = 0.72;
                    p.moog.osc3_level = 0.20;
                    p.moog.cutoff = 360.0;
                    p.moog.resonance = 0.48;
                    p.moog.env_amount = 0.78;
                    p.moog.drive = 0.42;
                    p.moog.filt_attack = 0.003;
                    p.moog.filt_decay = 0.18;
                    p.moog.filt_sustain = 0.12;
                    p.moog.amp_attack = 0.003;
                    p.moog.amp_decay = 0.22;
                    p.moog.amp_sustain = 0.42;
                    p.moog.amp_release = 0.08;
                    p.moog.glide = 0.05;
                }
                2 => {
                    // Synthwave 8th
                    p.moog.osc1_wave = MoogWave::Saw;
                    p.moog.osc2_wave = MoogWave::Saw;
                    p.moog.osc2_octave = 0;
                    p.moog.osc2_detune = 12.0;
                    p.moog.osc1_level = 0.85;
                    p.moog.osc2_level = 0.80;
                    p.moog.osc3_level = 0.40;
                    p.moog.cutoff = 720.0;
                    p.moog.resonance = 0.28;
                    p.moog.env_amount = 0.56;
                    p.moog.drive = 0.34;
                    p.moog.filt_decay = 0.16;
                    p.moog.filt_sustain = 0.22;
                    p.moog.amp_attack = 0.003;
                    p.moog.amp_decay = 0.20;
                    p.moog.amp_sustain = 0.55;
                    p.moog.amp_release = 0.10;
                    p.moog.glide = 0.02;
                }
                _ => {
                    // Deep Reese
                    p.moog.osc1_wave = MoogWave::Saw;
                    p.moog.osc2_wave = MoogWave::Saw;
                    p.moog.osc2_octave = 0;
                    p.moog.osc2_detune = 22.0;
                    p.moog.osc1_level = 0.85;
                    p.moog.osc2_level = 0.85;
                    p.moog.osc3_level = 0.45;
                    p.moog.cutoff = 520.0;
                    p.moog.resonance = 0.24;
                    p.moog.env_amount = 0.30;
                    p.moog.drive = 0.36;
                    p.moog.amp_attack = 0.02;
                    p.moog.amp_sustain = 0.82;
                    p.moog.amp_release = 0.24;
                    p.moog.glide = 0.10;
                }
            }
        }
        TrackId::MoogPad => {
            p.voice_presets[2] = preset_idx % 4;
            p.moog_pad.poly_mode = true;
            match p.voice_presets[2] {
                0 => {
                    // Lush Pad
                    p.moog_pad.osc1_wave = MoogWave::TriSaw;
                    p.moog_pad.osc2_wave = MoogWave::Saw;
                    p.moog_pad.osc2_octave = 0;
                    p.moog_pad.osc2_detune = 9.0;
                    p.moog_pad.osc1_level = 0.78;
                    p.moog_pad.osc2_level = 0.62;
                    p.moog_pad.osc3_level = 0.24;
                    p.moog_pad.cutoff = 1350.0;
                    p.moog_pad.resonance = 0.16;
                    p.moog_pad.env_amount = 0.26;
                    p.moog_pad.drive = 0.14;
                    p.moog_pad.amp_attack = 0.06;
                    p.moog_pad.amp_decay = 0.55;
                    p.moog_pad.amp_sustain = 0.72;
                    p.moog_pad.amp_release = 0.75;
                    p.moog_pad.filt_attack = 0.08;
                    p.moog_pad.filt_decay = 0.70;
                    p.moog_pad.filt_sustain = 0.42;
                }
                1 => {
                    // Warm Strings
                    p.moog_pad.osc1_wave = MoogWave::Saw;
                    p.moog_pad.osc2_wave = MoogWave::Saw;
                    p.moog_pad.osc2_detune = 15.0;
                    p.moog_pad.osc1_level = 0.70;
                    p.moog_pad.osc2_level = 0.70;
                    p.moog_pad.osc3_level = 0.30;
                    p.moog_pad.cutoff = 1850.0;
                    p.moog_pad.resonance = 0.12;
                    p.moog_pad.env_amount = 0.18;
                    p.moog_pad.drive = 0.10;
                    p.moog_pad.amp_attack = 0.18;
                    p.moog_pad.amp_sustain = 0.80;
                    p.moog_pad.amp_release = 0.95;
                }
                2 => {
                    // Dub Chord Stab
                    p.moog_pad.osc1_wave = MoogWave::WidePulse;
                    p.moog_pad.osc2_wave = MoogWave::Saw;
                    p.moog_pad.osc2_detune = 8.0;
                    p.moog_pad.cutoff = 720.0;
                    p.moog_pad.resonance = 0.42;
                    p.moog_pad.env_amount = 0.65;
                    p.moog_pad.drive = 0.24;
                    p.moog_pad.filt_attack = 0.004;
                    p.moog_pad.filt_decay = 0.26;
                    p.moog_pad.filt_sustain = 0.12;
                    p.moog_pad.amp_attack = 0.005;
                    p.moog_pad.amp_decay = 0.32;
                    p.moog_pad.amp_sustain = 0.15;
                    p.moog_pad.amp_release = 0.28;
                }
                _ => {
                    // Neon Brass / Funk Stab
                    p.moog_pad.osc1_wave = MoogWave::Saw;
                    p.moog_pad.osc2_wave = MoogWave::WidePulse;
                    p.moog_pad.osc2_detune = 10.0;
                    p.moog_pad.cutoff = 1100.0;
                    p.moog_pad.resonance = 0.26;
                    p.moog_pad.env_amount = 0.58;
                    p.moog_pad.drive = 0.26;
                    p.moog_pad.filt_attack = 0.015;
                    p.moog_pad.filt_decay = 0.28;
                    p.moog_pad.filt_sustain = 0.35;
                    p.moog_pad.amp_attack = 0.012;
                    p.moog_pad.amp_decay = 0.30;
                    p.moog_pad.amp_sustain = 0.48;
                    p.moog_pad.amp_release = 0.24;
                }
            }
        }
        TrackId::Monologue => {
            p.voice_presets[3] = preset_idx % 4;
            match p.voice_presets[3] {
                0 => {
                    // Pluck Arp
                    p.monologue.vco1_wave = MonologueOscWave::Saw;
                    p.monologue.vco2_wave = MonologueOscWave::Triangle;
                    p.monologue.vco1_shape = 0.35;
                    p.monologue.cutoff = 1600.0;
                    p.monologue.resonance = 0.26;
                    p.monologue.eg_int = 0.55;
                    p.monologue.eg_attack = 0.004;
                    p.monologue.eg_decay = 0.22;
                    p.monologue.drive = 0.18;
                    p.monologue.glide = 0.02;
                }
                1 => {
                    // Sparkle Bell
                    p.monologue.vco1_wave = MonologueOscWave::Triangle;
                    p.monologue.vco2_wave = MonologueOscWave::Square;
                    p.monologue.vco1_shape = 0.65;
                    p.monologue.cutoff = 3200.0;
                    p.monologue.resonance = 0.18;
                    p.monologue.eg_int = 0.40;
                    p.monologue.eg_attack = 0.002;
                    p.monologue.eg_decay = 0.36;
                    p.monologue.drive = 0.12;
                    p.monologue.glide = 0.01;
                }
                2 => {
                    // Funk Clav
                    p.monologue.vco1_wave = MonologueOscWave::Square;
                    p.monologue.vco2_wave = MonologueOscWave::Saw;
                    p.monologue.vco1_shape = 0.72;
                    p.monologue.cutoff = 1100.0;
                    p.monologue.resonance = 0.46;
                    p.monologue.eg_int = 0.72;
                    p.monologue.eg_attack = 0.002;
                    p.monologue.eg_decay = 0.16;
                    p.monologue.drive = 0.32;
                    p.monologue.glide = 0.01;
                }
                _ => {
                    // Glide Lead (expressive portamento)
                    p.monologue.vco1_wave = MonologueOscWave::Saw;
                    p.monologue.vco2_wave = MonologueOscWave::Square;
                    p.monologue.vco1_shape = 0.45;
                    p.monologue.cutoff = 2400.0;
                    p.monologue.resonance = 0.28;
                    p.monologue.eg_int = 0.35;
                    p.monologue.eg_attack = 0.015;
                    p.monologue.eg_decay = 0.48;
                    p.monologue.drive = 0.22;
                    p.monologue.glide = 0.14;
                }
            }
        }
        TrackId::Tb303 => {
            p.voice_presets[4] = preset_idx % 4;
            match p.voice_presets[4] {
                0 => {
                    // Acid Squelch
                    p.tb303.wave = Tb303Wave::Saw;
                    p.tb303.cutoff = 620.0;
                    p.tb303.resonance = 0.72;
                    p.tb303.env_mod = 0.78;
                    p.tb303.decay = 0.36;
                    p.tb303.accent = 0.76;
                    p.tb303.overdrive = 0.38;
                }
                1 => {
                    // Liquid 303
                    p.tb303.wave = Tb303Wave::Square;
                    p.tb303.cutoff = 520.0;
                    p.tb303.resonance = 0.62;
                    p.tb303.env_mod = 0.64;
                    p.tb303.decay = 0.42;
                    p.tb303.accent = 0.68;
                    p.tb303.overdrive = 0.24;
                }
                2 => {
                    // Funky Acid
                    p.tb303.wave = Tb303Wave::Saw;
                    p.tb303.cutoff = 780.0;
                    p.tb303.resonance = 0.58;
                    p.tb303.env_mod = 0.70;
                    p.tb303.decay = 0.26;
                    p.tb303.accent = 0.74;
                    p.tb303.overdrive = 0.30;
                }
                _ => {
                    // Deep Sub 303
                    p.tb303.wave = Tb303Wave::Square;
                    p.tb303.cutoff = 380.0;
                    p.tb303.resonance = 0.32;
                    p.tb303.env_mod = 0.45;
                    p.tb303.decay = 0.40;
                    p.tb303.accent = 0.55;
                    p.tb303.overdrive = 0.18;
                }
            }
        }
        TrackId::Sampler => {
            // Map preset 0..4 to BUILTIN_SAMPLE_NAMES:
            // 0: Minor9 Rhodes (idx 3), 1: Brass Stab (idx 1), 2: Choir Ahh (idx 0),
            // 3: Retro FM Bell (idx 4), 4: Toy Piano (idx 2)
            let idx = preset_idx % 5;
            p.voice_presets[5] = idx;
            let builtin_idx = match idx {
                0 => 3, // Minor9 Rhodes
                1 => 1, // Brass Stab
                2 => 0, // Choir Ahh
                3 => 4, // Retro FM Bell
                _ => 2, // Toy Piano
            };
            p.sample_data.builtin_id = Some(builtin_idx as u8);
            p.sample_data.name = crate::dsp::sampler::BUILTIN_SAMPLE_NAMES[builtin_idx].to_string();
            p.sample_data.pcm_i16_b64.clear();
            p.sample_generation = p.sample_generation.wrapping_add(1);
            match idx {
                0 => {
                    p.sampler.mode = SamplerMode::Vintage12Bit;
                    p.sampler.cutoff = 3600.0;
                    p.sampler.attack = 0.008;
                    p.sampler.release = 0.55;
                }
                1 => {
                    p.sampler.mode = SamplerMode::Vintage12Bit;
                    p.sampler.cutoff = 4200.0;
                    p.sampler.attack = 0.005;
                    p.sampler.release = 0.22;
                }
                2 => {
                    p.sampler.mode = SamplerMode::Sk1LoFi;
                    p.sampler.cutoff = 2800.0;
                    p.sampler.attack = 0.05;
                    p.sampler.release = 0.70;
                }
                3 => {
                    p.sampler.mode = SamplerMode::HiFi;
                    p.sampler.cutoff = 6500.0;
                    p.sampler.attack = 0.002;
                    p.sampler.release = 0.48;
                }
                _ => {
                    p.sampler.mode = SamplerMode::Sk1LoFi;
                    p.sampler.cutoff = 4800.0;
                    p.sampler.attack = 0.002;
                    p.sampler.release = 0.30;
                }
            }
        }
        TrackId::MidiOut => {}
    }
}

/// Generates an idiomatic, groovy, swing-friendly 64-step (4-bar) drum beat
/// for the given `vibe`, `energy` (0.0..1.0), and `seed`.
pub fn generate_auto_beat(vibe: Vibe, energy: f32, seed: u32) -> DrumTrackSeq {
    let mut seq = DrumTrackSeq::default();
    let mut rng = Rng::new(seed ^ 0xBEEF_1234);
    let e = energy.clamp(0.0, 1.0);

    for lane in &mut seq.lanes {
        lane.step_count = StepCount::Steps64;
        lane.clear();
    }

    let set = |seq: &mut DrumTrackSeq, v: DrumVoiceId, step: usize, vel: f32, accent: bool, ratchet: u8| {
        if step < 64 {
            let st = &mut seq.lanes[v.index()].steps[step];
            st.active = true;
            st.velocity = vel.clamp(0.15, 1.0);
            st.accent = accent;
            st.probability = 1.0;
            st.ratchet = ratchet.clamp(1, 4);
        }
    };

    for bar in 0..4 {
        let base = bar * 16;
        let is_fill_bar = bar == 3;

        match vibe {
            Vibe::DeepHouse => {
                // Punchy 4-on-the-floor kick
                for beat in [0, 4, 8, 12] {
                    set(&mut seq, DrumVoiceId::Kick, base + beat, 0.96, beat == 0, 1);
                }
                if e > 0.65 && bar % 2 == 1 {
                    set(&mut seq, DrumVoiceId::Kick, base + 14, 0.62, false, 1);
                }
                // Warm clap + subtle snare on 2 & 4
                for beat in [4, 12] {
                    set(&mut seq, DrumVoiceId::Clap, base + beat, 0.86, true, 1);
                    if e > 0.35 {
                        set(&mut seq, DrumVoiceId::Snare, base + beat, 0.52, false, 1);
                    }
                }
                // Offbeat open hat (classic house shimmer)
                for step in [2, 6, 10, 14] {
                    set(&mut seq, DrumVoiceId::OpenHat, base + step, 0.72, false, 1);
                }
                // Groovy 16th closed hats with velocity contour
                for s in 0..16 {
                    if s % 4 == 2 {
                        continue;
                    }
                    let vel = match s % 4 {
                        0 => 0.42,
                        1 => 0.28,
                        _ => 0.56,
                    };
                    if e > 0.25 || s % 2 == 0 {
                        set(&mut seq, DrumVoiceId::ClosedHat, base + s, vel, false, 1);
                    }
                }
                // Subtle rimshot syncopation
                if e > 0.45 && rng.f32() < 0.75 {
                    set(&mut seq, DrumVoiceId::CowbellRim, base + 7, 0.46, false, 1);
                    if bar % 2 == 1 {
                        set(&mut seq, DrumVoiceId::CowbellRim, base + 15, 0.40, false, 1);
                    }
                }
                if is_fill_bar && e > 0.35 {
                    set(&mut seq, DrumVoiceId::Tom, base + 13, 0.62, false, 1);
                    set(&mut seq, DrumVoiceId::Tom, base + 15, 0.72, false, 1);
                }
            }
            Vibe::ElectroFunk => {
                // Heavy syncopated funk kick pocket!
                for &k in &[0, 6, 10] {
                    set(&mut seq, DrumVoiceId::Kick, base + k, 0.98, k == 0, 1);
                }
                if bar % 2 == 1 || e > 0.55 {
                    set(&mut seq, DrumVoiceId::Kick, base + 14, 0.80, false, 1);
                }
                if e > 0.7 && bar == 2 {
                    set(&mut seq, DrumVoiceId::Kick, base + 3, 0.70, false, 1);
                }
                // Snappy snare + fat clap on 2 & 4 + funky ghost snares
                for beat in [4, 12] {
                    set(&mut seq, DrumVoiceId::Snare, base + beat, 0.94, true, 1);
                    set(&mut seq, DrumVoiceId::Clap, base + beat, 0.84, true, 1);
                }
                if e > 0.3 {
                    set(&mut seq, DrumVoiceId::Snare, base + 9, 0.44, false, 1);
                    if bar % 2 == 1 {
                        set(&mut seq, DrumVoiceId::Snare, base + 15, 0.58, false, 1);
                    }
                }
                // Signature 808 Funk Cowbell / Rim pocket!
                for &c in &[2, 7, 11, 14] {
                    if e > 0.25 || c == 2 || c == 11 {
                        set(&mut seq, DrumVoiceId::CowbellRim, base + c, if c == 11 { 0.76 } else { 0.56 }, c == 11, 1);
                    }
                }
                // Crisp 16th funk hats
                for s in 0..16 {
                    if s == 14 && e > 0.4 {
                        set(&mut seq, DrumVoiceId::OpenHat, base + s, 0.74, false, 1);
                    } else if s % 2 == 0 || e > 0.45 {
                        let vel = if s % 4 == 0 { 0.72 } else if s % 2 == 0 { 0.56 } else { 0.38 };
                        let ratch = if is_fill_bar && s == 15 && e > 0.5 { 2 } else { 1 };
                        set(&mut seq, DrumVoiceId::ClosedHat, base + s, vel, s % 4 == 0, ratch);
                    }
                }
                if is_fill_bar {
                    set(&mut seq, DrumVoiceId::Tom, base + 11, 0.68, false, 1);
                    set(&mut seq, DrumVoiceId::Snare, base + 14, 0.80, false, 2);
                }
            }
            Vibe::LofiChill => {
                // Lazy boom-bap chill pocket
                set(&mut seq, DrumVoiceId::Kick, base, 0.92, true, 1);
                set(&mut seq, DrumVoiceId::Kick, base + 7, 0.68, false, 1);
                set(&mut seq, DrumVoiceId::Kick, base + 10, 0.84, false, 1);
                if bar % 2 == 1 && e > 0.45 {
                    set(&mut seq, DrumVoiceId::Kick, base + 15, 0.54, false, 1);
                }
                // Soft dusty snare on 2 & 4 + ghost notes
                for beat in [4, 12] {
                    set(&mut seq, DrumVoiceId::Snare, base + beat, 0.82, false, 1);
                }
                if e > 0.35 {
                    set(&mut seq, DrumVoiceId::Snare, base + 9, 0.34, false, 1);
                    set(&mut seq, DrumVoiceId::CowbellRim, base + 15, 0.38, false, 1);
                }
                // Swinging 8th/16th dusty hats
                for s in (0..16).step_by(2) {
                    let vel = if s % 4 == 0 { 0.58 } else { 0.40 };
                    set(&mut seq, DrumVoiceId::ClosedHat, base + s, vel, false, 1);
                }
                if e > 0.55 {
                    set(&mut seq, DrumVoiceId::OpenHat, base + 14, 0.50, false, 1);
                }
            }
            Vibe::ElectroSoul => {
                // Postal Service / Indie Electronic crisp glitchy groove
                for &k in &[0, 6, 8, 14] {
                    if k != 14 || bar % 2 == 1 {
                        set(&mut seq, DrumVoiceId::Kick, base + k, 0.94, k == 0, 1);
                    }
                }
                for beat in [4, 12] {
                    set(&mut seq, DrumVoiceId::Snare, base + beat, 0.86, true, 1);
                    set(&mut seq, DrumVoiceId::Clap, base + beat, 0.72, false, 1);
                }
                // Glitchy ratcheting 16th hats!
                for s in 0..16 {
                    let vel = match s % 4 {
                        0 => 0.66,
                        2 => 0.58,
                        _ => 0.36,
                    };
                    let ratchet = if e > 0.45 && ((bar == 1 && s == 15) || (bar == 3 && s == 11)) {
                        3
                    } else if e > 0.35 && s == 7 && bar % 2 == 1 {
                        2
                    } else {
                        1
                    };
                    if s % 2 == 0 || e > 0.3 {
                        set(&mut seq, DrumVoiceId::ClosedHat, base + s, vel, s % 4 == 0, ratchet);
                    }
                }
                set(&mut seq, DrumVoiceId::OpenHat, base + 10, 0.62, false, 1);
                if is_fill_bar && e > 0.4 {
                    set(&mut seq, DrumVoiceId::Snare, base + 14, 0.76, false, 2);
                    set(&mut seq, DrumVoiceId::CowbellRim, base + 15, 0.62, false, 1);
                }
            }
            Vibe::Synthwave => {
                // Driving 80s kick + huge snare on 2 & 4
                for &k in &[0, 4, 8, 12] {
                    set(&mut seq, DrumVoiceId::Kick, base + k, 0.95, k == 0 || k == 8, 1);
                }
                for beat in [4, 12] {
                    set(&mut seq, DrumVoiceId::Snare, base + beat, 0.96, true, 1);
                    set(&mut seq, DrumVoiceId::Clap, base + beat, 0.68, false, 1);
                }
                for s in (0..16).step_by(2) {
                    set(&mut seq, DrumVoiceId::ClosedHat, base + s, if s % 4 == 0 { 0.68 } else { 0.48 }, false, 1);
                }
                if e > 0.45 {
                    set(&mut seq, DrumVoiceId::OpenHat, base + 14, 0.66, false, 1);
                }
                if bar == 0 {
                    set(&mut seq, DrumVoiceId::Cymbal, 0, 0.65, true, 1);
                }
                if is_fill_bar {
                    // Classic 80s tom fill!
                    for (i, &t) in [10, 12, 13, 14, 15].iter().enumerate() {
                        set(&mut seq, DrumVoiceId::Tom, base + t, 0.65 + i as f32 * 0.06, i == 4, 1);
                    }
                }
            }
            Vibe::AcidClub => {
                // Driving 909 four-on-the-floor club beat
                for beat in [0, 4, 8, 12] {
                    set(&mut seq, DrumVoiceId::Kick, base + beat, 1.0, beat == 0, 1);
                }
                for beat in [4, 12] {
                    set(&mut seq, DrumVoiceId::Clap, base + beat, 0.88, true, 1);
                }
                for step in [2, 6, 10, 14] {
                    set(&mut seq, DrumVoiceId::OpenHat, base + step, 0.78, false, 1);
                }
                for s in 0..16 {
                    if s % 4 != 2 && (e > 0.3 || s % 2 == 0) {
                        set(&mut seq, DrumVoiceId::ClosedHat, base + s, if s % 4 == 0 { 0.56 } else { 0.42 }, false, 1);
                    }
                }
                if e > 0.55 {
                    for s in [0, 4, 8, 12] {
                        set(&mut seq, DrumVoiceId::Cymbal, base + s, 0.32, false, 1);
                    }
                }
                if is_fill_bar && e > 0.45 {
                    set(&mut seq, DrumVoiceId::Snare, base + 14, 0.74, false, 2);
                    set(&mut seq, DrumVoiceId::Snare, base + 15, 0.86, true, 2);
                }
            }
            Vibe::AmbientDub => {
                // Minimal hypnotic dub pulse
                set(&mut seq, DrumVoiceId::Kick, base, 0.88, true, 1);
                set(&mut seq, DrumVoiceId::Kick, base + 8, 0.78, false, 1);
                if bar % 2 == 1 && e > 0.4 {
                    set(&mut seq, DrumVoiceId::Kick, base + 14, 0.60, false, 1);
                }
                // Dub rimshot on 4 & 12
                set(&mut seq, DrumVoiceId::CowbellRim, base + 4, 0.68, false, 1);
                set(&mut seq, DrumVoiceId::CowbellRim, base + 12, 0.72, true, 1);
                if e > 0.45 {
                    for s in [2, 6, 10, 14] {
                        set(&mut seq, DrumVoiceId::ClosedHat, base + s, 0.42, false, 1);
                    }
                }
            }
            Vibe::UkGarage => {
                // Classic 2-step skipped kick + snappy snare on 2 & 4 + shuffling ghost hats!
                set(&mut seq, DrumVoiceId::Kick, base, 0.96, true, 1);
                set(&mut seq, DrumVoiceId::Kick, base + 10, 0.88, false, 1);
                if bar % 2 == 1 {
                    set(&mut seq, DrumVoiceId::Kick, base + 6, 0.70, false, 1);
                }
                for beat in [4, 12] {
                    set(&mut seq, DrumVoiceId::Snare, base + beat, 0.92, true, 1);
                    set(&mut seq, DrumVoiceId::Clap, base + beat, 0.72, false, 1);
                }
                if e > 0.35 {
                    set(&mut seq, DrumVoiceId::CowbellRim, base + 7, 0.56, false, 1);
                    set(&mut seq, DrumVoiceId::CowbellRim, base + 15, 0.62, false, 1);
                }
                for s in [2, 3, 6, 9, 10, 14, 15] {
                    let vel = if s % 4 == 2 { 0.76 } else { 0.48 };
                    let ratch = if is_fill_bar && s == 15 && e > 0.5 { 2 } else { 1 };
                    set(&mut seq, DrumVoiceId::ClosedHat, base + s, vel, s == 2 || s == 10, ratch);
                }
                set(&mut seq, DrumVoiceId::OpenHat, base + 14, 0.68, false, 1);
            }
            Vibe::JungleDnb => {
                // 170 BPM Breakbeat: Kick on 1 & 10, Snare on 4 & 12 + ghost snares on 7 & 9!
                set(&mut seq, DrumVoiceId::Kick, base, 0.98, true, 1);
                set(&mut seq, DrumVoiceId::Kick, base + 10, 0.90, false, 1);
                if bar == 3 && e > 0.5 {
                    set(&mut seq, DrumVoiceId::Kick, base + 11, 0.74, false, 1);
                }
                set(&mut seq, DrumVoiceId::Snare, base + 4, 0.96, true, 1);
                set(&mut seq, DrumVoiceId::Snare, base + 12, 0.94, true, 1);
                if e > 0.3 {
                    set(&mut seq, DrumVoiceId::Snare, base + 7, 0.46, false, 1);
                    set(&mut seq, DrumVoiceId::Snare, base + 9, 0.44, false, 1);
                    if is_fill_bar {
                        set(&mut seq, DrumVoiceId::Snare, base + 14, 0.80, false, 2);
                        set(&mut seq, DrumVoiceId::Snare, base + 15, 0.88, true, 2);
                    }
                }
                for s in (0..16).step_by(2) {
                    set(&mut seq, DrumVoiceId::ClosedHat, base + s, if s % 4 == 0 { 0.72 } else { 0.52 }, false, 1);
                }
                if e > 0.55 {
                    set(&mut seq, DrumVoiceId::OpenHat, base + 2, 0.54, false, 1);
                    set(&mut seq, DrumVoiceId::OpenHat, base + 14, 0.58, false, 1);
                }
            }
            Vibe::DetroitTechno => {
                // Hypnotic 909 four-on-the-floor + offbeat open hats + crisp ride cymbal!
                for beat in [0, 4, 8, 12] {
                    set(&mut seq, DrumVoiceId::Kick, base + beat, 0.98, beat == 0, 1);
                }
                for beat in [4, 12] {
                    set(&mut seq, DrumVoiceId::Clap, base + beat, 0.76, false, 1);
                }
                for step in [2, 6, 10, 14] {
                    set(&mut seq, DrumVoiceId::OpenHat, base + step, 0.76, false, 1);
                    if e > 0.45 {
                        set(&mut seq, DrumVoiceId::Cymbal, base + step, 0.40, false, 1);
                    }
                }
                for s in 0..16 {
                    if s % 4 != 2 && e > 0.35 {
                        set(&mut seq, DrumVoiceId::ClosedHat, base + s, 0.44, false, 1);
                    }
                }
                if is_fill_bar && e > 0.5 {
                    set(&mut seq, DrumVoiceId::Tom, base + 11, 0.68, false, 1);
                    set(&mut seq, DrumVoiceId::Tom, base + 14, 0.74, false, 1);
                }
            }
            Vibe::DreamPop => {
                // Warm steady indie-pop groove
                set(&mut seq, DrumVoiceId::Kick, base, 0.90, true, 1);
                set(&mut seq, DrumVoiceId::Kick, base + 6, 0.72, false, 1);
                set(&mut seq, DrumVoiceId::Kick, base + 8, 0.84, false, 1);
                for beat in [4, 12] {
                    set(&mut seq, DrumVoiceId::Snare, base + beat, 0.84, true, 1);
                }
                for s in (0..16).step_by(2) {
                    set(&mut seq, DrumVoiceId::ClosedHat, base + s, if s % 4 == 0 { 0.54 } else { 0.38 }, false, 1);
                }
                if bar == 0 {
                    set(&mut seq, DrumVoiceId::Cymbal, 0, 0.48, false, 1);
                }
            }
        }
    }
    seq
}

/// Generates chord-locked Bass, Chords, Arp, Lead/303, and Sampler patterns
/// for a 64-step (4-bar) pattern so every track harmonizes seamlessly.
pub fn generate_melodic_tracks(
    vibe: Vibe,
    root_note: u8,
    prog_idx: usize,
    seed: u32,
) -> (MelodicTrackSeq, MelodicTrackSeq, MelodicTrackSeq, MelodicTrackSeq, MelodicTrackSeq) {
    let prog = progression_at(prog_idx);
    let mut rng = Rng::new(seed ^ 0xCAFE_BABE);

    let mut bass = MelodicTrackSeq::new(StepCount::Steps64);
    let mut pad = MelodicTrackSeq::new(StepCount::Steps64);
    let mut arp = MelodicTrackSeq::new(StepCount::Steps64);
    let mut acid = MelodicTrackSeq::new(StepCount::Steps64);
    let mut sampler = MelodicTrackSeq::new(StepCount::Steps64);

    for bar in 0..4 {
        let slot = prog.slots[bar];
        let base = bar * 16;
        let bass_root = bass_midi_for(root_note, slot.offset);
        let pad_notes = voice_led_chord(root_note, slot, 60);
        let pad_root = pad_notes.first().copied().unwrap_or(57);
        let arp_tones = arp_tones_for(root_note, slot);

        // 1. CHORDS / PAD (MoogPad)
        match vibe {
            Vibe::ElectroFunk => {
                // Punchy funky offbeat chord stabs!
                for &(st, gate, vel) in &[(0, 0.70, 0.82), (6, 0.55, 0.74), (10, 0.85, 0.80)] {
                    let s = &mut pad.steps[base + st];
                    s.active = true;
                    s.note = pad_root;
                    s.chord = slot.chord;
                    s.velocity = vel;
                    s.gate = gate;
                    s.accent = st == 0;
                }
            }
            Vibe::AmbientDub => {
                // Classic dub offbeat stabs echoed by ping-pong delay
                for &st in &[2, 10] {
                    let s = &mut pad.steps[base + st];
                    s.active = true;
                    s.note = pad_root;
                    s.chord = slot.chord;
                    s.velocity = if st == 2 { 0.84 } else { 0.68 };
                    s.gate = 0.85;
                    s.accent = st == 2;
                }
            }
            Vibe::AcidClub => {
                // Sparse dark club pad stab on bar downbeat
                let s = &mut pad.steps[base];
                s.active = true;
                s.note = pad_root;
                s.chord = slot.chord;
                s.velocity = 0.72;
                s.gate = 1.8;
            }
            _ => {
                // Lush sustained chord (holds across the whole bar smoothly!)
                let s = &mut pad.steps[base];
                s.active = true;
                s.note = pad_root;
                s.chord = slot.chord;
                s.velocity = 0.78;
                s.gate = 14.5; // sustained 14.5 sixteenths (nearly full bar!)
                if matches!(vibe, Vibe::DeepHouse | Vibe::LofiChill) && bar % 2 == 1 {
                    // Subtle warm rhythmic re-trigger on beat 3.5
                    let s2 = &mut pad.steps[base + 10];
                    s2.active = true;
                    s2.note = pad_root;
                    s2.chord = slot.chord;
                    s2.velocity = 0.62;
                    s2.gate = 5.5;
                }
            }
        }

        // 2. BASS (Moog)
        match vibe {
            Vibe::DeepHouse => {
                // Warm syncopated house sub-bass
                let pattern = [(0, 0, 2.5, 0.90), (6, 0, 1.8, 0.76), (10, 7, 1.8, 0.78), (14, 12, 1.4, 0.72)];
                for &(st, iv, gate, vel) in &pattern {
                    let s = &mut bass.steps[base + st];
                    s.active = true;
                    s.note = (bass_root + iv).min(56);
                    s.velocity = vel;
                    s.gate = gate;
                    s.accent = st == 0;
                    s.slide = st == 14;
                }
            }
            Vibe::ElectroFunk => {
                // Slappy syncopated funk bass with octave pops & 5th/7th fills!
                let funk_hits: &[(usize, u8, f32, f32, bool)] = if bar % 2 == 0 {
                    &[
                        (0, 0, 1.4, 0.96, true),
                        (2, 12, 0.7, 0.86, true),
                        (6, 0, 1.2, 0.88, false),
                        (9, 7, 0.8, 0.78, false),
                        (10, 10, 1.2, 0.90, true),
                        (14, 12, 0.8, 0.84, false),
                    ]
                } else {
                    &[
                        (0, 0, 1.4, 0.96, true),
                        (3, 7, 0.8, 0.78, false),
                        (6, 0, 1.1, 0.88, false),
                        (8, 12, 0.8, 0.92, true),
                        (11, 10, 0.8, 0.80, false),
                        (14, 7, 0.8, 0.82, false),
                        (15, 10, 0.7, 0.86, true),
                    ]
                };
                for &(st, iv, gate, vel, acc) in funk_hits {
                    let s = &mut bass.steps[base + st];
                    s.active = true;
                    s.note = (bass_root + iv).min(58);
                    s.velocity = vel;
                    s.gate = gate;
                    s.accent = acc;
                    s.slide = st == 15;
                }
            }
            Vibe::LofiChill => {
                // Warm relaxed sub notes
                for &(st, iv, gate, vel) in &[(0, 0, 5.5, 0.86), (10, 7, 3.5, 0.72)] {
                    let s = &mut bass.steps[base + st];
                    s.active = true;
                    s.note = (bass_root + iv).min(54);
                    s.velocity = vel;
                    s.gate = gate;
                }
            }
            Vibe::Synthwave => {
                // Classic driving 8th-note Moog octave bass!
                for s_idx in (0..16).step_by(2) {
                    let s = &mut bass.steps[base + s_idx];
                    s.active = true;
                    s.note = if s_idx % 4 == 2 { bass_root + 12 } else { bass_root };
                    s.velocity = if s_idx % 4 == 0 { 0.92 } else { 0.78 };
                    s.gate = 1.4;
                    s.accent = s_idx % 4 == 0;
                }
            }
            Vibe::ElectroSoul => {
                // Melodic bouncing indie-electro bass
                for &(st, iv, gate, vel) in &[(0, 0, 2.5, 0.88), (6, 0, 1.5, 0.78), (8, 7, 2.2, 0.82), (14, 12, 1.2, 0.76)] {
                    let s = &mut bass.steps[base + st];
                    s.active = true;
                    s.note = (bass_root + iv).min(56);
                    s.velocity = vel;
                    s.gate = gate;
                    s.accent = st == 0;
                }
            }
            Vibe::AcidClub => {
                // Deep supportive sub under the 303
                for &(st, gate) in &[(0, 3.0), (8, 3.0)] {
                    let s = &mut bass.steps[base + st];
                    s.active = true;
                    s.note = bass_root;
                    s.velocity = 0.78;
                    s.gate = gate;
                }
            }
            Vibe::AmbientDub => {
                // Deep hypnotic dub sub
                for &(st, iv, gate) in &[(0, 0, 6.0), (10, 7, 4.0)] {
                    let s = &mut bass.steps[base + st];
                    s.active = true;
                    s.note = bass_root + iv;
                    s.velocity = 0.84;
                    s.gate = gate;
                }
            }
            Vibe::UkGarage => {
                // Bouncy 2-step organ/sub bass!
                for &(st, iv, gate, vel) in &[(0, 0, 2.2, 0.94), (3, 7, 1.4, 0.78), (10, 0, 2.2, 0.90), (14, 10, 1.4, 0.82)] {
                    let s = &mut bass.steps[base + st];
                    s.active = true;
                    s.note = (bass_root + iv).min(56);
                    s.velocity = vel;
                    s.gate = gate;
                    s.accent = st == 0 || st == 10;
                }
            }
            Vibe::JungleDnb => {
                // Deep rolling Reese sub-bass!
                let s = &mut bass.steps[base];
                s.active = true;
                s.note = bass_root;
                s.velocity = 0.94;
                s.gate = 14.0;
                if bar % 2 == 1 {
                    let s2 = &mut bass.steps[base + 10];
                    s2.active = true;
                    s2.note = (bass_root + 3).min(52);
                    s2.velocity = 0.84;
                    s2.gate = 5.0;
                    s2.slide = true;
                }
            }
            Vibe::DetroitTechno => {
                // Driving hypnotic offbeat techno bass
                for s_idx in [2, 6, 10, 14] {
                    let s = &mut bass.steps[base + s_idx];
                    s.active = true;
                    s.note = if s_idx == 14 { bass_root + 7 } else { bass_root };
                    s.velocity = 0.88;
                    s.gate = 1.6;
                    s.accent = s_idx == 2 || s_idx == 10;
                }
            }
            Vibe::DreamPop => {
                // Warm melodic 8th/dotted bass
                for &(st, iv, gate) in &[(0, 0, 3.5), (6, 0, 1.8), (8, 7, 3.5), (14, 12, 1.5)] {
                    let s = &mut bass.steps[base + st];
                    s.active = true;
                    s.note = (bass_root + iv).min(56);
                    s.velocity = 0.82;
                    s.gate = gate;
                }
            }
        }

        // 3. ARP / MELODY (Monologue) — 100% locked to active chord tones!
        let arp_steps: &[usize] = match vibe {
            Vibe::ElectroSoul | Vibe::Synthwave | Vibe::DreamPop => &[0, 2, 4, 6, 8, 10, 12, 14],
            Vibe::ElectroFunk | Vibe::UkGarage => &[2, 5, 7, 10, 14],
            Vibe::DeepHouse | Vibe::DetroitTechno => &[2, 6, 10, 14],
            Vibe::LofiChill => &[0, 6, 10],
            Vibe::AcidClub => &[2, 10, 14],
            Vibe::AmbientDub | Vibe::JungleDnb => &[4, 12],
        };
        for (i, &st) in arp_steps.iter().enumerate() {
            let tone_idx = (i + bar + rng.range(2)) % arp_tones.len().max(1);
            let s = &mut arp.steps[base + st];
            s.active = true;
            s.note = arp_tones[tone_idx];
            s.velocity = if i % 2 == 0 { 0.78 } else { 0.60 };
            s.gate = if matches!(vibe, Vibe::ElectroSoul | Vibe::Synthwave) {
                1.2
            } else {
                0.85
            };
            s.accent = i == 0;
        }

        // 4. TB-303 ACID / COUNTER-LINE (active on AcidClub & ElectroFunk, subtle accent elsewhere)
        if matches!(vibe, Vibe::AcidClub | Vibe::ElectroFunk) {
            let chord_ivs = slot.chord.intervals();
            let hits: &[usize] = if matches!(vibe, Vibe::AcidClub) {
                &[0, 2, 3, 6, 8, 10, 11, 14, 15]
            } else {
                &[0, 3, 6, 10, 12, 15]
            };
            for (i, &st) in hits.iter().enumerate() {
                let iv = chord_ivs[(i + bar) % chord_ivs.len()];
                let oct_bump = if i % 4 == 2 { 12 } else { 0 };
                let s = &mut acid.steps[base + st];
                s.active = true;
                s.note = (bass_root + iv + oct_bump).clamp(33, 64);
                s.velocity = if i % 3 == 0 { 0.94 } else { 0.74 };
                s.gate = 0.95;
                s.accent = i % 3 == 0;
                s.slide = st == 3 || st == 11 || st == 15;
            }
        }

        // 5. SAMPLER TEXTURE (Rhodes / Brass / Choir / Bell — 100% chord-root locked!)
        match vibe {
            Vibe::LofiChill => {
                // Warm Rhodes hits on bar downbeat
                let s = &mut sampler.steps[base];
                s.active = true;
                s.note = (48 + (root_note + slot.offset) % 12).clamp(48, 63);
                s.velocity = 0.76;
                s.gate = 12.0;
            }
            Vibe::ElectroFunk => {
                // Funky brass stabs on beat 1 & 3.5!
                for &st in &[0, 10] {
                    let s = &mut sampler.steps[base + st];
                    s.active = true;
                    s.note = (60 + (root_note + slot.offset) % 12).clamp(55, 72);
                    s.velocity = if st == 0 { 0.80 } else { 0.66 };
                    s.gate = 1.6;
                }
            }
            Vibe::ElectroSoul | Vibe::DeepHouse => {
                // Gentle shimmer bell / Rhodes accent on bar 1 & 3
                if bar % 2 == 0 {
                    let s = &mut sampler.steps[base + 8];
                    s.active = true;
                    s.note = arp_tones.last().copied().unwrap_or(69);
                    s.velocity = 0.62;
                    s.gate = 4.0;
                }
            }
            _ => {}
        }
    }

    (bass, pad, arp, acid, sampler)
}

/// Applies a complete, balanced, immediately musical Vibe to the Project.
pub fn apply_vibe_to_project(p: &mut Project, vibe: Vibe, prog_idx: Option<usize>, seed: u32) {
    let prog = prog_idx.unwrap_or_else(|| vibe.default_progression()) % PROGRESSIONS.len();
    p.vibe = vibe;
    p.progression_idx = prog;
    p.bpm = vibe.default_bpm();
    p.swing = vibe.default_swing();
    p.root_note = vibe.default_root();
    p.scale = vibe.default_scale();
    p.name = format!("{} Studio", vibe.name());
    p.party_seed = seed;

    // Apply curated voice presets for this vibe
    match vibe {
        Vibe::DeepHouse => {
            apply_voice_preset(p, TrackId::Drums, 0);     // 808 Warm
            apply_voice_preset(p, TrackId::Moog, 0);      // Warm Sub
            apply_voice_preset(p, TrackId::MoogPad, 0);   // Lush Pad
            apply_voice_preset(p, TrackId::Monologue, 0); // Pluck Arp
            apply_voice_preset(p, TrackId::Tb303, 1);     // Liquid 303
            apply_voice_preset(p, TrackId::Sampler, 0);   // Minor9 Rhodes
            p.sidechain_amount = 0.58;
        }
        Vibe::ElectroFunk => {
            apply_voice_preset(p, TrackId::Drums, 2);     // Funk Electro
            apply_voice_preset(p, TrackId::Moog, 1);      // Funk Slap
            apply_voice_preset(p, TrackId::MoogPad, 3);   // Neon Brass
            apply_voice_preset(p, TrackId::Monologue, 2); // Funk Clav
            apply_voice_preset(p, TrackId::Tb303, 2);     // Funky Acid
            apply_voice_preset(p, TrackId::Sampler, 1);   // Brass Stab
            p.sidechain_amount = 0.36;
        }
        Vibe::LofiChill => {
            apply_voice_preset(p, TrackId::Drums, 3);     // Lo-Fi Dusty
            apply_voice_preset(p, TrackId::Moog, 0);      // Warm Sub
            apply_voice_preset(p, TrackId::MoogPad, 0);   // Lush Pad
            apply_voice_preset(p, TrackId::Monologue, 1); // Sparkle Bell
            apply_voice_preset(p, TrackId::Tb303, 3);     // Deep Sub 303
            apply_voice_preset(p, TrackId::Sampler, 0);   // Minor9 Rhodes
            p.sidechain_amount = 0.30;
        }
        Vibe::ElectroSoul => {
            apply_voice_preset(p, TrackId::Drums, 1);     // 909 Club
            apply_voice_preset(p, TrackId::Moog, 0);      // Warm Sub
            apply_voice_preset(p, TrackId::MoogPad, 1);   // Warm Strings
            apply_voice_preset(p, TrackId::Monologue, 1); // Sparkle Bell
            apply_voice_preset(p, TrackId::Tb303, 1);     // Liquid 303
            apply_voice_preset(p, TrackId::Sampler, 3);   // Retro FM Bell
            p.sidechain_amount = 0.48;
        }
        Vibe::Synthwave => {
            apply_voice_preset(p, TrackId::Drums, 1);     // 909 Club
            apply_voice_preset(p, TrackId::Moog, 2);      // Synthwave 8th
            apply_voice_preset(p, TrackId::MoogPad, 1);   // Warm Strings
            apply_voice_preset(p, TrackId::Monologue, 3); // Glide Lead
            apply_voice_preset(p, TrackId::Tb303, 3);     // Deep Sub 303
            apply_voice_preset(p, TrackId::Sampler, 2);   // Choir Ahh
            p.sidechain_amount = 0.50;
        }
        Vibe::AcidClub => {
            apply_voice_preset(p, TrackId::Drums, 1);     // 909 Club
            apply_voice_preset(p, TrackId::Moog, 0);      // Warm Sub
            apply_voice_preset(p, TrackId::MoogPad, 2);   // Dub Chord
            apply_voice_preset(p, TrackId::Monologue, 0); // Pluck Arp
            apply_voice_preset(p, TrackId::Tb303, 0);     // Acid Squelch
            apply_voice_preset(p, TrackId::Sampler, 2);   // Choir Ahh
            p.sidechain_amount = 0.64;
        }
        Vibe::AmbientDub => {
            apply_voice_preset(p, TrackId::Drums, 0);     // 808 Warm
            apply_voice_preset(p, TrackId::Moog, 3);      // Deep Reese
            apply_voice_preset(p, TrackId::MoogPad, 2);   // Dub Chord
            apply_voice_preset(p, TrackId::Monologue, 1); // Sparkle Bell
            apply_voice_preset(p, TrackId::Tb303, 3);     // Deep Sub 303
            apply_voice_preset(p, TrackId::Sampler, 2);   // Choir Ahh
            p.sidechain_amount = 0.28;
        }
        Vibe::UkGarage => {
            apply_voice_preset(p, TrackId::Drums, 2);     // Funk Electro (crisp 2-step)
            apply_voice_preset(p, TrackId::Moog, 1);      // Funk Slap / Organ
            apply_voice_preset(p, TrackId::MoogPad, 2);   // Dub Chord
            apply_voice_preset(p, TrackId::Monologue, 0); // Pluck Arp
            apply_voice_preset(p, TrackId::Tb303, 1);     // Liquid 303
            apply_voice_preset(p, TrackId::Sampler, 0);   // Minor9 Rhodes
            p.sidechain_amount = 0.44;
        }
        Vibe::JungleDnb => {
            apply_voice_preset(p, TrackId::Drums, 1);     // 909 Club
            apply_voice_preset(p, TrackId::Moog, 3);      // Deep Reese
            apply_voice_preset(p, TrackId::MoogPad, 1);   // Warm Strings
            apply_voice_preset(p, TrackId::Monologue, 1); // Sparkle Bell
            apply_voice_preset(p, TrackId::Tb303, 3);     // Deep Sub 303
            apply_voice_preset(p, TrackId::Sampler, 2);   // Choir Ahh
            p.sidechain_amount = 0.42;
        }
        Vibe::DetroitTechno => {
            apply_voice_preset(p, TrackId::Drums, 1);     // 909 Club
            apply_voice_preset(p, TrackId::Moog, 0);      // Warm Sub
            apply_voice_preset(p, TrackId::MoogPad, 2);   // Dub Chord
            apply_voice_preset(p, TrackId::Monologue, 0); // Pluck Arp
            apply_voice_preset(p, TrackId::Tb303, 1);     // Liquid 303
            apply_voice_preset(p, TrackId::Sampler, 2);   // Choir Ahh
            p.sidechain_amount = 0.62;
        }
        Vibe::DreamPop => {
            apply_voice_preset(p, TrackId::Drums, 0);     // 808 Warm
            apply_voice_preset(p, TrackId::Moog, 0);      // Warm Sub
            apply_voice_preset(p, TrackId::MoogPad, 1);   // Warm Strings
            apply_voice_preset(p, TrackId::Monologue, 1); // Sparkle Bell
            apply_voice_preset(p, TrackId::Tb303, 3);     // Deep Sub 303
            apply_voice_preset(p, TrackId::Sampler, 3);   // Retro FM Bell
            p.sidechain_amount = 0.34;
        }
    }

    // Balanced, punchy, non-clipping stereo mix hierarchy!
    // Drums lead the mix; Bass sits tight in center; Chords & Arp sit wide in stereo!
    let ch = &mut p.mixer.channels;
    // 0: Moog (Bass)
    ch[0].volume = 0.66;
    ch[0].pan = 0.0;
    ch[0].delay_send = 0.0;
    ch[0].reverb_send = 0.04;
    ch[0].mute = false;
    ch[0].solo = false;
    // 1: Monologue (Arp / Lead)
    ch[1].volume = 0.42;
    ch[1].pan = 0.26;
    ch[1].delay_send = 0.48;
    ch[1].reverb_send = 0.30;
    ch[1].mute = false;
    ch[1].solo = false;
    // 2: TB-303 (Acid / Funk Lead)
    ch[2].volume = if matches!(vibe, Vibe::AcidClub) { 0.64 } else if matches!(vibe, Vibe::ElectroFunk) { 0.42 } else { 0.36 };
    ch[2].pan = -0.14;
    ch[2].delay_send = 0.28;
    ch[2].reverb_send = 0.14;
    ch[2].mute = !matches!(vibe, Vibe::AcidClub | Vibe::ElectroFunk);
    ch[2].solo = false;
    // 3: Drums (PUNCHY ON TOP!)
    ch[3].volume = 0.94;
    ch[3].pan = 0.0;
    ch[3].delay_send = if matches!(vibe, Vibe::AmbientDub) { 0.18 } else { 0.04 };
    ch[3].reverb_send = if matches!(vibe, Vibe::Synthwave) { 0.20 } else { 0.10 };
    ch[3].mute = false;
    ch[3].solo = false;
    // 4: Sampler (Rhodes / Brass / Bell Texture)
    ch[4].volume = if matches!(vibe, Vibe::LofiChill) { 0.62 } else if matches!(vibe, Vibe::ElectroFunk) { 0.48 } else { 0.44 };
    ch[4].pan = -0.24;
    ch[4].delay_send = 0.26;
    ch[4].reverb_send = 0.36;
    ch[4].mute = matches!(vibe, Vibe::AcidClub | Vibe::AmbientDub);
    ch[4].solo = false;

    // MoogPad (Chords)
    p.mixer.moog_pad_volume = if matches!(vibe, Vibe::LofiChill) { 0.40 } else { 0.52 };
    p.mixer.moog_pad_delay_send = if matches!(vibe, Vibe::AmbientDub) { 0.68 } else { 0.24 };
    p.mixer.moog_pad_reverb_send = if matches!(vibe, Vibe::AmbientDub) { 0.65 } else { 0.42 };
    p.mixer.moog_pad_mute = false;
    p.mixer.moog_pad_solo = false;
    p.mixer.master_volume = 0.86;

    // Lush FX sweet spots per vibe
    p.fx.delay_enabled = true;
    p.fx.reverb_enabled = true;
    p.fx.global_filter_enabled = true;
    p.fx.global_filter_mode = GlobalFilterMode::LowPass;
    p.fx.global_filter_cutoff = 18_000.0;
    p.fx.global_filter_resonance = 0.12;
    p.fx.delay_div = DelayDivision::DottedEighth;
    p.fx.delay_feedback = if matches!(vibe, Vibe::AmbientDub) { 0.62 } else { 0.36 };
    p.fx.delay_tone = if matches!(vibe, Vibe::LofiChill | Vibe::AmbientDub) { 0.42 } else { 0.58 };
    p.fx.delay_mix = if matches!(vibe, Vibe::AmbientDub) { 0.54 } else { 0.30 };
    p.fx.reverb_size = if matches!(vibe, Vibe::AmbientDub) { 0.84 } else if matches!(vibe, Vibe::Synthwave | Vibe::ElectroSoul) { 0.66 } else { 0.54 };
    p.fx.reverb_damp = 0.52;
    p.fx.reverb_mix = if matches!(vibe, Vibe::AmbientDub) { 0.56 } else { 0.32 };
    p.fx.analog_warmth = if matches!(vibe, Vibe::LofiChill | Vibe::ElectroFunk) { 0.38 } else { 0.20 };
    p.fx.chop = 0.0;
    p.fx.repeat = 0.0;
    p.fx.glitch = 0.0;
    p.fx.circuit_bend = 0.0;

    // Generate pattern 0 (active pattern)
    let active = p.active_pattern.min(p.patterns.len().saturating_sub(1));
    let drums = generate_auto_beat(vibe, p.beat_energy, seed);
    let (bass, pad, arp, acid, sampler) = generate_melodic_tracks(vibe, p.root_note, prog, seed);
    let pat = &mut p.patterns[active];
    pat.name = format!("{} Groove", vibe.name());
    pat.drums = drums;
    pat.moog = bass;
    pat.moog_pad = pad;
    pat.monologue = arp;
    pat.tb303 = acid;
    pat.sampler = sampler;

    // Build a clean default Song Arrangement (Intro -> Groove -> Build -> Drop -> Break -> Outro)
    p.song_sections = default_vibe_sections(active);
}

pub fn default_vibe_sections(pat: usize) -> Vec<SongSection> {
    // track_mask bits: 0=Drums, 1=Bass(Moog), 2=Chords(MoogPad), 3=Arp(Mono), 4=303, 5=Sampler
    vec![
        SongSection {
            name: "Intro".into(),
            pattern: pat,
            bars: 4,
            bass_drop: false,
            fill: false,
            track_mask: 0b001101, // Drums + Chords + Arp
        },
        SongSection {
            name: "Groove".into(),
            pattern: pat,
            bars: 8,
            bass_drop: false,
            fill: true,
            track_mask: 0b101111, // Full groove
        },
        SongSection {
            name: "Build".into(),
            pattern: pat,
            bars: 4,
            bass_drop: false,
            fill: true,
            track_mask: 0b011101, // Pull bass for tension + snare build
        },
        SongSection {
            name: "Drop".into(),
            pattern: pat,
            bars: 8,
            bass_drop: true,
            fill: true,
            track_mask: 0b111111, // Everything slams in!
        },
        SongSection {
            name: "Break".into(),
            pattern: pat,
            bars: 4,
            bass_drop: false,
            fill: true,
            track_mask: 0b101100, // Lush chords + arp + sampler float
        },
        SongSection {
            name: "Outro".into(),
            pattern: pat,
            bars: 4,
            bass_drop: false,
            fill: false,
            track_mask: 0b000101, // Wind down
        },
    ]
}
