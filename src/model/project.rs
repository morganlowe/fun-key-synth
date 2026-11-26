use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::sequencer::{DrumStep, MelodicStep, MotionLocks, Pattern};
use super::{Scale, StepCount, TrackId};

pub const NUM_PATTERNS: usize = 8;
pub const MAX_EXTRA_TRACKS: usize = 10;

/// A user-added dynamic polyphonic/monophonic synth track in the mix (tracks 7..=16).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ExtraTrack {
    pub name: String,
    pub preset_idx: usize,
    pub synth: MoogParams,
    #[serde(default)]
    pub filter: TrackFilterParams,
    pub volume: f32,
    pub pan: f32,
    pub delay_send: f32,
    pub reverb_send: f32,
    pub mute: bool,
    pub solo: bool,
    pub color: [u8; 3],
}

impl Default for ExtraTrack {
    fn default() -> Self {
        let mut synth = MoogParams::default();
        synth.poly_mode = true;
        Self {
            name: "Synth Layer".to_string(),
            preset_idx: 4, // 70s String Synth
            synth,
            filter: TrackFilterParams::default(),
            volume: 0.52,
            pan: 0.0,
            delay_send: 0.24,
            reverb_send: 0.38,
            mute: false,
            solo: false,
            color: [120, 210, 255],
        }
    }
}

fn default_extra_mask() -> u16 {
    0x03FF
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SongSection {
    pub name: String,
    pub pattern: usize,
    pub bars: u8,
    pub bass_drop: bool,
    pub fill: bool,
    /// Bitmask of active studio tracks:
    /// bit 0=Drums, 1=Bass(Moog), 2=Chords(MoogPad), 3=Arp(Monologue), 4=Lead/303, 5=Sampler
    pub track_mask: u8,
    /// Bitmask of active dynamic extra tracks (`0..MAX_EXTRA_TRACKS`).
    #[serde(default = "default_extra_mask")]
    pub extra_mask: u16,
}
impl Default for SongSection {
    fn default() -> Self {
        Self {
            name: "Section".into(),
            pattern: 0,
            bars: 4,
            bass_drop: false,
            fill: false,
            track_mask: 0b111111,
            extra_mask: 0x03FF,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum MoogWave {
    Triangle,
    TriSaw,
    #[default]
    Saw,
    WidePulse,
    Square,
    NarrowPulse,
}

impl MoogWave {
    pub const ALL: [MoogWave; 6] = [
        MoogWave::Triangle,
        MoogWave::TriSaw,
        MoogWave::Saw,
        MoogWave::WidePulse,
        MoogWave::Square,
        MoogWave::NarrowPulse,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            MoogWave::Triangle => "Tri",
            MoogWave::TriSaw => "TriSaw",
            MoogWave::Saw => "Saw",
            MoogWave::WidePulse => "Pulse 35%",
            MoogWave::Square => "Square",
            MoogWave::NarrowPulse => "Pulse 15%",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum SynthEngine {
    #[default]
    Analog,
    FmDx7,
    Wavetable,
    PhilharmoniaD50,
    SoundFont,
}

impl SynthEngine {
    pub const ALL: [SynthEngine; 5] = [
        SynthEngine::Analog,
        SynthEngine::FmDx7,
        SynthEngine::Wavetable,
        SynthEngine::PhilharmoniaD50,
        SynthEngine::SoundFont,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            SynthEngine::Analog => "Analog",
            SynthEngine::FmDx7 => "FM-DX7",
            SynthEngine::Wavetable => "Wavetable",
            SynthEngine::PhilharmoniaD50 => "Philharmonia",
            SynthEngine::SoundFont => "SoundFont",
        }
    }

    pub const fn bank_names(self) -> &'static [&'static str] {
        match self {
            SynthEngine::Analog => &[
                "Minimoog / Prophet",
                "OB-Xa Unison",
                "Juno Chorus",
                "Acid Ladder",
                "Warm Sub",
                "Vintage Brass",
            ],
            SynthEngine::FmDx7 => &[
                "DX7 Rhodes EP",
                "Glass Bell",
                "Woody Marimba",
                "FM Slap Bass",
                "Digitone Brass",
                "Crystal Pad",
            ],
            SynthEngine::Wavetable => &[
                "Virus 7x HyperSaw",
                "PPG Digital Glass",
                "Vocal Choir",
                "Waldorf Bell",
                "Juno PWM Strings",
                "Acid Morph",
            ],
            SynthEngine::PhilharmoniaD50 => &[
                "Strings Ensemble",
                "Cinematic Brass",
                "Real Guitar",
                "D-50 Fantasia",
                "Jazz Sax & Winds",
                "Plucked Chamber",
                "Solo Violin",
                "Solo Viola",
                "Solo Cello",
                "Double Bass",
                "Pizzicato Strings",
                "Banjo",
                "Mandolin",
                "Solo Trumpet",
                "French Horn",
                "Trombone",
                "Tuba",
                "Concert Flute",
                "Oboe",
                "Clarinet",
                "Bass Clarinet",
                "Cor Anglais",
                "Bassoon",
                "Contrabassoon",
                "Saxophone",
                "Philharmonia Percussion Kit",
            ],
            SynthEngine::SoundFont => &["SF2 Bank"],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MoogParams {
    pub poly_mode: bool,
    pub glide: f32, // 0..1
    pub osc1_wave: MoogWave,
    pub osc1_level: f32,
    pub osc2_wave: MoogWave,
    pub osc2_octave: i8,  // -2..=+2
    pub osc2_detune: f32, // -50..=+50 cents
    pub osc2_level: f32,
    pub osc3_wave: MoogWave,
    pub osc3_octave: i8,
    pub osc3_detune: f32,
    pub osc3_level: f32,
    pub noise_level: f32,
    pub drive: f32,       // 0..1 CP3 overdrive
    pub cutoff: f32,      // 20..18000 Hz
    pub resonance: f32,   // 0..0.96
    pub env_amount: f32,  // -1..=1
    pub key_track: f32,   // 0..1
    pub filt_attack: f32, // s
    pub filt_decay: f32,
    pub filt_sustain: f32,
    pub filt_release: f32,
    pub amp_attack: f32,
    pub amp_decay: f32,
    pub amp_sustain: f32,
    pub amp_release: f32,
    pub lfo_rate: f32,      // Hz
    pub lfo_to_pitch: f32,  // 0..1
    pub lfo_to_filter: f32, // 0..1
    #[serde(default)]
    pub engine: SynthEngine,
    #[serde(default)]
    pub engine_bank: u8,
    #[serde(default = "default_engine_morph")]
    pub engine_morph: f32,
    #[serde(default)]
    pub phaser_mix: f32,
    #[serde(default)]
    pub amp_cab: f32,
}

fn default_engine_morph() -> f32 {
    0.45
}

impl Default for MoogParams {
    fn default() -> Self {
        Self {
            poly_mode: true,
            glide: 0.08,
            osc1_wave: MoogWave::Saw,
            osc1_level: 0.85,
            osc2_wave: MoogWave::Saw,
            osc2_octave: 0,
            osc2_detune: 9.0,
            osc2_level: 0.72,
            osc3_wave: MoogWave::Square,
            osc3_octave: -1,
            osc3_detune: -6.0,
            osc3_level: 0.55,
            noise_level: 0.02,
            drive: 0.12,
            cutoff: 1350.0,
            resonance: 0.36,
            env_amount: 0.58,
            key_track: 0.45,
            filt_attack: 0.008,
            filt_decay: 0.38,
            filt_sustain: 0.25,
            filt_release: 0.28,
            amp_attack: 0.006,
            amp_decay: 0.28,
            amp_sustain: 0.65,
            amp_release: 0.24,
            lfo_rate: 2.8,
            lfo_to_pitch: 0.04,
            lfo_to_filter: 0.10,
            engine: SynthEngine::Analog,
            engine_bank: 0,
            engine_morph: default_engine_morph(),
            phaser_mix: 0.0,
            amp_cab: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum MonologueOscWave {
    #[default]
    Saw,
    Triangle,
    Square,
}

impl MonologueOscWave {
    pub const ALL: [MonologueOscWave; 3] = [
        MonologueOscWave::Saw,
        MonologueOscWave::Triangle,
        MonologueOscWave::Square,
    ];
    pub const fn name(self) -> &'static str {
        match self {
            MonologueOscWave::Saw => "Saw",
            MonologueOscWave::Triangle => "Tri",
            MonologueOscWave::Square => "Square",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum Osc2ModMode {
    #[default]
    Off,
    Sync,
    Ring,
}

impl Osc2ModMode {
    pub const ALL: [Osc2ModMode; 3] = [Osc2ModMode::Off, Osc2ModMode::Sync, Osc2ModMode::Ring];
    pub const fn name(self) -> &'static str {
        match self {
            Osc2ModMode::Off => "Off",
            Osc2ModMode::Sync => "Sync",
            Osc2ModMode::Ring => "Ring",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum EgTarget {
    #[default]
    Cutoff,
    Pitch,
    Pitch2,
    Shape,
}

impl EgTarget {
    pub const ALL: [EgTarget; 4] = [
        EgTarget::Cutoff,
        EgTarget::Pitch,
        EgTarget::Pitch2,
        EgTarget::Shape,
    ];
    pub const fn name(self) -> &'static str {
        match self {
            EgTarget::Cutoff => "Cutoff",
            EgTarget::Pitch => "Pitch",
            EgTarget::Pitch2 => "VCO2 Pitch",
            EgTarget::Shape => "Shape",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum LfoWave {
    #[default]
    Triangle,
    Saw,
    Square,
}

impl LfoWave {
    pub const ALL: [LfoWave; 3] = [LfoWave::Triangle, LfoWave::Saw, LfoWave::Square];
    pub const fn name(self) -> &'static str {
        match self {
            LfoWave::Triangle => "Tri",
            LfoWave::Saw => "Saw",
            LfoWave::Square => "Sq",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum LfoMode {
    #[default]
    Slow,
    Fast,
    OneShot,
}

impl LfoMode {
    pub const ALL: [LfoMode; 3] = [LfoMode::Slow, LfoMode::Fast, LfoMode::OneShot];
    pub const fn name(self) -> &'static str {
        match self {
            LfoMode::Slow => "Slow",
            LfoMode::Fast => "Fast",
            LfoMode::OneShot => "1-Shot",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum LfoTarget {
    #[default]
    Cutoff,
    Pitch,
    Shape,
}

impl LfoTarget {
    pub const ALL: [LfoTarget; 3] = [LfoTarget::Cutoff, LfoTarget::Pitch, LfoTarget::Shape];
    pub const fn name(self) -> &'static str {
        match self {
            LfoTarget::Cutoff => "Cutoff",
            LfoTarget::Pitch => "Pitch",
            LfoTarget::Shape => "Shape",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MonologueParams {
    pub vco1_wave: MonologueOscWave,
    pub vco1_shape: f32,
    pub vco1_level: f32,
    pub vco2_wave: MonologueOscWave,
    pub vco2_shape: f32,
    pub vco2_octave: i8, // -2..=2
    pub vco2_pitch: f32, // -12..=12 semitones
    pub vco2_level: f32,
    pub mod_mode: Osc2ModMode,
    pub glide: f32,
    pub cutoff: f32,    // 30..18000 Hz
    pub resonance: f32, // 0..0.95
    pub drive: f32,     // 0..1
    pub eg_attack: f32,
    pub eg_decay: f32,
    pub eg_int: f32, // -1..=1
    pub eg_target: EgTarget,
    pub amp_sustain: bool, // Monologue gate vs AD
    pub lfo_wave: LfoWave,
    pub lfo_mode: LfoMode,
    pub lfo_rate: f32, // 0..1
    pub lfo_int: f32,  // -1..=1
    pub lfo_target: LfoTarget,
    #[serde(default)]
    pub engine: SynthEngine,
    #[serde(default)]
    pub engine_bank: u8,
    #[serde(default = "default_engine_morph")]
    pub engine_morph: f32,
    #[serde(default)]
    pub phaser_mix: f32,
    #[serde(default)]
    pub amp_cab: f32,
}

impl Default for MonologueParams {
    fn default() -> Self {
        Self {
            vco1_wave: MonologueOscWave::Saw,
            vco1_shape: 0.35,
            vco1_level: 0.85,
            vco2_wave: MonologueOscWave::Square,
            vco2_shape: 0.5,
            vco2_octave: 0,
            vco2_pitch: 0.08,
            vco2_level: 0.7,
            mod_mode: Osc2ModMode::Off,
            glide: 0.12,
            cutoff: 1850.0,
            resonance: 0.42,
            drive: 0.16,
            eg_attack: 0.005,
            eg_decay: 0.28,
            eg_int: 0.55,
            eg_target: EgTarget::Cutoff,
            amp_sustain: true,
            lfo_wave: LfoWave::Triangle,
            lfo_mode: LfoMode::Slow,
            lfo_rate: 0.32,
            lfo_int: 0.15,
            lfo_target: LfoTarget::Shape,
            engine: SynthEngine::Analog,
            engine_bank: 0,
            engine_morph: default_engine_morph(),
            phaser_mix: 0.0,
            amp_cab: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum Tb303Wave {
    #[default]
    Saw,
    Square,
}

impl Tb303Wave {
    pub const fn name(self) -> &'static str {
        match self {
            Tb303Wave::Saw => "SAW",
            Tb303Wave::Square => "SQR",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Tb303Params {
    pub wave: Tb303Wave,
    pub tuning: f32,    // -12..=12 semitones
    pub cutoff: f32,    // 40..10000 Hz
    pub resonance: f32, // 0..0.98
    pub env_mod: f32,   // 0..1
    pub decay: f32,     // 0.06..1.8 s
    pub accent: f32,    // 0..1
    pub overdrive: f32, // 0..1
}

impl Default for Tb303Params {
    fn default() -> Self {
        Self {
            wave: Tb303Wave::Saw,
            tuning: 0.0,
            cutoff: 420.0,
            resonance: 0.72,
            env_mod: 0.68,
            decay: 0.34,
            accent: 0.78,
            overdrive: 0.12,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum DrumModel {
    #[default]
    Tr808,
    Tr909,
    Acoustic,
    Linn80s,
}

impl DrumModel {
    pub const ALL: [DrumModel; 4] = [
        DrumModel::Tr808,
        DrumModel::Tr909,
        DrumModel::Acoustic,
        DrumModel::Linn80s,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            DrumModel::Tr808 => "808",
            DrumModel::Tr909 => "909",
            DrumModel::Acoustic => "Acoustic",
            DrumModel::Linn80s => "Linn",
        }
    }

    pub const fn short_name(self) -> &'static str {
        match self {
            DrumModel::Tr808 => "808",
            DrumModel::Tr909 => "909",
            DrumModel::Acoustic => "Acou",
            DrumModel::Linn80s => "Linn",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DrumVoiceParams {
    pub model: DrumModel,
    pub tune: f32,   // -12..=12 semitones
    pub decay: f32,  // 0..1
    pub tone: f32,   // 0..1
    pub snappy: f32, // 0..1
    pub level: f32,  // 0..1.5
    pub pan: f32,    // -1..=1
    pub mute: bool,
}

impl Default for DrumVoiceParams {
    fn default() -> Self {
        Self {
            model: DrumModel::Tr808,
            tune: 0.0,
            decay: 0.55,
            tone: 0.5,
            snappy: 0.5,
            level: 0.9,
            pan: 0.0,
            mute: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DrumKitParams {
    pub voices: [DrumVoiceParams; 8],
    pub accent_level: f32,
}

impl Default for DrumKitParams {
    fn default() -> Self {
        let mut voices = [DrumVoiceParams::default(); 8];
        // Hybrid classic defaults: 808 punch + 909 crispness
        voices[0] = DrumVoiceParams {
            model: DrumModel::Tr808,
            tune: 0.0,
            decay: 0.65,
            tone: 0.45,
            snappy: 0.4,
            level: 1.05,
            pan: 0.0,
            mute: false,
        }; // Kick
        voices[1] = DrumVoiceParams {
            model: DrumModel::Tr909,
            tune: 0.5,
            decay: 0.22,
            tone: 0.55,
            snappy: 0.68,
            level: 0.78,
            pan: -0.05,
            mute: false,
        }; // Snare
        voices[2] = DrumVoiceParams {
            model: DrumModel::Tr808,
            tune: 0.0,
            decay: 0.18,
            tone: 0.6,
            snappy: 0.6,
            level: 0.62,
            pan: 0.08,
            mute: false,
        }; // Clap
        voices[3] = DrumVoiceParams {
            model: DrumModel::Tr808,
            tune: 0.0,
            decay: 0.24,
            tone: 0.55,
            snappy: 0.5,
            level: 0.36,
            pan: -0.25,
            mute: false,
        }; // Cowbell
        voices[4] = DrumVoiceParams {
            model: DrumModel::Tr909,
            tune: 0.0,
            decay: 0.25,
            tone: 0.65,
            snappy: 0.6,
            level: 0.75,
            pan: 0.18,
            mute: false,
        }; // CH
        voices[5] = DrumVoiceParams {
            model: DrumModel::Tr909,
            tune: 0.0,
            decay: 0.55,
            tone: 0.65,
            snappy: 0.6,
            level: 0.72,
            pan: 0.22,
            mute: false,
        }; // OH
        voices[6] = DrumVoiceParams {
            model: DrumModel::Tr808,
            tune: -1.0,
            decay: 0.55,
            tone: 0.5,
            snappy: 0.3,
            level: 0.85,
            pan: -0.18,
            mute: false,
        }; // Tom
        voices[7] = DrumVoiceParams {
            model: DrumModel::Tr909,
            tune: 0.0,
            decay: 0.65,
            tone: 0.6,
            snappy: 0.6,
            level: 0.65,
            pan: 0.30,
            mute: false,
        }; // Cymbal
        Self {
            voices,
            accent_level: 0.4,
        }
    }
}

impl DrumKitParams {
    pub fn set_all_model(&mut self, model: DrumModel) {
        for v in &mut self.voices {
            v.model = model;
        }
    }

    pub fn set_hybrid(&mut self) {
        self.voices[0].model = DrumModel::Tr808;
        self.voices[1].model = DrumModel::Tr909;
        self.voices[2].model = DrumModel::Tr808;
        self.voices[3].model = DrumModel::Tr808;
        self.voices[4].model = DrumModel::Tr909;
        self.voices[5].model = DrumModel::Tr909;
        self.voices[6].model = DrumModel::Tr808;
        self.voices[7].model = DrumModel::Tr909;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum SamplerMode {
    #[default]
    Sk1LoFi,
    Vintage12Bit,
    HiFi,
}

impl SamplerMode {
    pub const ALL: [SamplerMode; 3] = [
        SamplerMode::Sk1LoFi,
        SamplerMode::Vintage12Bit,
        SamplerMode::HiFi,
    ];
    pub const fn name(self) -> &'static str {
        match self {
            SamplerMode::Sk1LoFi => "SK-1 8-Bit (9.4kHz)",
            SamplerMode::Vintage12Bit => "12-Bit (26kHz)",
            SamplerMode::HiFi => "Hi-Fi",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum LoopMode {
    #[default]
    OneShot,
    Loop,
    PingPong,
}

impl LoopMode {
    pub const ALL: [LoopMode; 3] = [LoopMode::OneShot, LoopMode::Loop, LoopMode::PingPong];
    pub const fn name(self) -> &'static str {
        match self {
            LoopMode::OneShot => "1-Shot",
            LoopMode::Loop => "Loop",
            LoopMode::PingPong => "Ping-Pong",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SamplerParams {
    pub mode: SamplerMode,
    pub loop_mode: LoopMode,
    pub reverse: bool,
    pub key_pitch: bool,
    pub speed: f32,     // 0.25..=2.0 playback speed multiplier
    pub root_note: u8,
    pub transpose: i8,  // -24..=+24 semitones
    pub tune_cents: f32,
    pub start: f32,     // 0..1
    pub end: f32,       // 0..1
    pub bit_crush: f32, // 0..1 extra grit
    pub filter_enabled: bool,
    pub filter_mode: GlobalFilterMode,
    pub cutoff: f32,    // 150..18000 Hz
    pub resonance: f32, // 0..0.9
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
}

impl SamplerParams {
    /// Resets the sampler filter to pure transparent passthrough.
    pub fn set_filter_passthrough(&mut self) {
        self.filter_enabled = false;
        self.filter_mode = GlobalFilterMode::LowPass;
        self.cutoff = 18_000.0;
        self.resonance = 0.0;
    }
}

impl Default for SamplerParams {
    fn default() -> Self {
        Self {
            mode: SamplerMode::Sk1LoFi,
            loop_mode: LoopMode::OneShot,
            reverse: false,
            key_pitch: true,
            speed: 1.0,
            root_note: 60,
            transpose: 0,
            tune_cents: 0.0,
            start: 0.0,
            end: 1.0,
            bit_crush: 0.15,
            filter_enabled: true,
            filter_mode: GlobalFilterMode::LowPass,
            cutoff: 7200.0,
            resonance: 0.18,
            attack: 0.004,
            decay: 0.45,
            sustain: 0.85,
            release: 0.18,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SavedSample {
    pub name: String,
    pub sample_rate: u32,
    pub pcm_i16_b64: String,
    pub builtin_id: Option<u8>,
    /// `0` or `1` = mono, `2` = interleaved stereo `[L0, R0, L1, R1, ...]`.
    #[serde(default)]
    pub channels: u8,
}

impl Default for SavedSample {
    fn default() -> Self {
        Self {
            name: "SK-1 Choir Ahh".to_string(),
            sample_rate: 48_000,
            pcm_i16_b64: String::new(),
            builtin_id: Some(0),
            channels: 1,
        }
    }
}

impl SavedSample {
    pub fn from_f32_pcm(
        name: impl Into<String>,
        sample_rate: u32,
        pcm: &[f32],
        builtin_id: Option<u8>,
    ) -> Self {
        let mut bytes = Vec::with_capacity(pcm.len() * 2);
        for &s in pcm {
            let v = (s.clamp(-1.0, 1.0) * 32767.0).round() as i16;
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        Self {
            name: name.into(),
            sample_rate,
            pcm_i16_b64: b64_encode(&bytes),
            builtin_id,
            channels: 1,
        }
    }

    pub fn from_stereo_pcm(
        name: impl Into<String>,
        sample_rate: u32,
        left: &[f32],
        right: &[f32],
        builtin_id: Option<u8>,
    ) -> Self {
        let frames = left.len().min(right.len());
        let mut bytes = Vec::with_capacity(frames * 4);
        for i in 0..frames {
            let vl = (left[i].clamp(-1.0, 1.0) * 32767.0).round() as i16;
            let vr = (right[i].clamp(-1.0, 1.0) * 32767.0).round() as i16;
            bytes.extend_from_slice(&vl.to_le_bytes());
            bytes.extend_from_slice(&vr.to_le_bytes());
        }
        Self {
            name: name.into(),
            sample_rate,
            pcm_i16_b64: b64_encode(&bytes),
            builtin_id,
            channels: 2,
        }
    }

    pub fn decode_stereo_pcm(&self) -> Option<(Vec<f32>, Vec<f32>)> {
        if self.pcm_i16_b64.is_empty() {
            return None;
        }
        let bytes = b64_decode(&self.pcm_i16_b64).ok()?;
        if self.channels == 2 {
            let frames = bytes.len() / 4;
            let mut left = Vec::with_capacity(frames);
            let mut right = Vec::with_capacity(frames);
            for chunk in bytes.chunks_exact(4) {
                let vl = i16::from_le_bytes([chunk[0], chunk[1]]) as f32 / 32768.0;
                let vr = i16::from_le_bytes([chunk[2], chunk[3]]) as f32 / 32768.0;
                left.push(vl);
                right.push(vr);
            }
            Some((left, right))
        } else {
            let mut mono = Vec::with_capacity(bytes.len() / 2);
            for chunk in bytes.chunks_exact(2) {
                let v = i16::from_le_bytes([chunk[0], chunk[1]]);
                mono.push(v as f32 / 32768.0);
            }
            Some((mono.clone(), mono))
        }
    }

    pub fn decode_f32_pcm(&self) -> Option<Vec<f32>> {
        if self.pcm_i16_b64.is_empty() {
            return None;
        }
        let bytes = b64_decode(&self.pcm_i16_b64).ok()?;
        if self.channels == 2 {
            let mut out = Vec::with_capacity(bytes.len() / 4);
            for chunk in bytes.chunks_exact(4) {
                let vl = i16::from_le_bytes([chunk[0], chunk[1]]) as f32 / 32768.0;
                let vr = i16::from_le_bytes([chunk[2], chunk[3]]) as f32 / 32768.0;
                out.push((vl + vr) * 0.5);
            }
            Some(out)
        } else {
            let mut out = Vec::with_capacity(bytes.len() / 2);
            for chunk in bytes.chunks_exact(2) {
                let v = i16::from_le_bytes([chunk[0], chunk[1]]);
                out.push(v as f32 / 32768.0);
            }
            Some(out)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SamplePart {
    pub name: String,
    pub sample: SavedSample,
    pub generation: u64,
    pub start: f32,
    pub end: f32,
    pub speed: f32,
    pub transpose: i8,
    pub tune_cents: f32,
    pub reverse: bool,
    pub loop_mode: LoopMode,
    pub mode: SamplerMode,
    pub bit_crush: f32,
    pub filter_enabled: bool,
    pub filter_mode: GlobalFilterMode,
    pub cutoff: f32,
    pub resonance: f32,
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
    pub level: f32,
    pub pan: f32,
    pub reverb_send: f32,
    pub mute: bool,
    pub choke_group: u8, // 0 = off, 1..=2 = choke group
}

impl Default for SamplePart {
    fn default() -> Self {
        Self {
            name: "Pad".into(),
            sample: SavedSample::default(),
            generation: 1,
            start: 0.0,
            end: 1.0,
            speed: 1.0,
            transpose: 0,
            tune_cents: 0.0,
            reverse: false,
            loop_mode: LoopMode::OneShot,
            mode: SamplerMode::HiFi,
            bit_crush: 0.0,
            filter_enabled: true,
            filter_mode: GlobalFilterMode::LowPass,
            cutoff: 16_000.0,
            resonance: 0.12,
            attack: 0.002,
            decay: 0.55,
            sustain: 0.85,
            release: 0.18,
            level: 0.85,
            pan: 0.0,
            reverb_send: 0.22,
            mute: false,
            choke_group: 0,
        }
    }
}

impl SamplePart {
    /// Resets this pad's filter to pure transparent passthrough.
    pub fn set_filter_passthrough(&mut self) {
        self.filter_enabled = false;
        self.filter_mode = GlobalFilterMode::LowPass;
        self.cutoff = 18_000.0;
        self.resonance = 0.0;
    }

    pub fn to_sampler_params(&self) -> SamplerParams {
        SamplerParams {
            mode: self.mode,
            loop_mode: self.loop_mode,
            reverse: self.reverse,
            key_pitch: false,
            speed: self.speed,
            root_note: 60,
            transpose: self.transpose,
            tune_cents: self.tune_cents,
            start: self.start,
            end: self.end,
            bit_crush: self.bit_crush,
            filter_enabled: self.filter_enabled,
            filter_mode: self.filter_mode,
            cutoff: self.cutoff,
            resonance: self.resonance,
            attack: self.attack,
            decay: self.decay,
            sustain: self.sustain,
            release: self.release,
        }
    }

    pub fn default_bank() -> Vec<Self> {
        let defs: [(&str, u8, i8, bool, f32, SamplerMode); 8] = [
            ("Rhodes m9", 3, 0, false, -0.20, SamplerMode::Vintage12Bit),
            ("Brass Stab", 1, 0, false, 0.22, SamplerMode::Vintage12Bit),
            ("Choir Ahh", 0, 0, false, -0.35, SamplerMode::Sk1LoFi),
            ("FM Bell", 4, 0, false, 0.35, SamplerMode::HiFi),
            ("Toy Piano", 2, 0, false, -0.15, SamplerMode::Sk1LoFi),
            ("Rhodes +7", 3, 7, false, 0.18, SamplerMode::Vintage12Bit),
            ("Rev Brass", 1, 0, true, -0.28, SamplerMode::Vintage12Bit),
            ("Hi Bell", 4, 12, false, 0.40, SamplerMode::HiFi),
        ];
        defs.iter()
            .enumerate()
            .map(|(i, &(name, bid, trans, rev, pan, mode))| Self {
                name: name.into(),
                sample: SavedSample {
                    name: name.into(),
                    sample_rate: 48_000,
                    pcm_i16_b64: String::new(),
                    builtin_id: Some(bid),
                    channels: 1,
                },
                generation: (i as u64) + 1,
                transpose: trans,
                reverse: rev,
                pan,
                mode,
                ..Self::default()
            })
            .collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TrackMixerChannel {
    pub volume: f32,      // 0..1.5
    pub pan: f32,         // -1..=1
    pub delay_send: f32,  // 0..1
    pub reverb_send: f32, // 0..1
    pub delay_enabled: bool,
    pub reverb_enabled: bool,
    pub mute: bool,
    pub solo: bool,
    #[serde(default)]
    pub filter: TrackFilterParams,
}

impl Default for TrackMixerChannel {
    fn default() -> Self {
        Self {
            volume: 0.82,
            pan: 0.0,
            delay_send: 0.15,
            reverb_send: 0.15,
            delay_enabled: true,
            reverb_enabled: true,
            mute: false,
            solo: false,
            filter: TrackFilterParams::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MixerParams {
    pub channels: [TrackMixerChannel; 6],
    pub master_volume: f32,
    pub moog_pad_volume: f32,
    pub moog_pad_reverb_send: f32,
    pub moog_pad_delay_send: f32,
    pub moog_pad_delay_enabled: bool,
    pub moog_pad_reverb_enabled: bool,
    pub moog_pad_mute: bool,
    pub moog_pad_solo: bool,
    #[serde(default)]
    pub moog_pad_filter: TrackFilterParams,
}

impl Default for MixerParams {
    fn default() -> Self {
        let mut channels = [TrackMixerChannel::default(); 6];
        channels[0] = TrackMixerChannel {
            volume: 1.12,
            pan: -0.08,
            delay_send: 0.20,
            reverb_send: 0.06,
            delay_enabled: true,
            reverb_enabled: true,
            mute: false,
            solo: false,
            filter: TrackFilterParams::default(),
        }; // Moog
        channels[1] = TrackMixerChannel {
            volume: 0.78,
            pan: 0.12,
            delay_send: 0.42,
            reverb_send: 0.36,
            delay_enabled: true,
            reverb_enabled: true,
            mute: false,
            solo: false,
            filter: TrackFilterParams::default(),
        }; // Monologue
        channels[2] = TrackMixerChannel {
            volume: 0.82,
            pan: 0.0,
            delay_send: 0.34,
            reverb_send: 0.18,
            delay_enabled: true,
            reverb_enabled: true,
            mute: false,
            solo: false,
            filter: TrackFilterParams::default(),
        }; // 303
        channels[3] = TrackMixerChannel {
            volume: 0.58,
            pan: 0.0,
            delay_send: 0.06,
            reverb_send: 0.14,
            delay_enabled: true,
            reverb_enabled: true,
            mute: false,
            solo: false,
            filter: TrackFilterParams::default(),
        }; // Drums
        channels[4] = TrackMixerChannel {
            volume: 0.80,
            pan: -0.14,
            delay_send: 0.34,
            reverb_send: 0.36,
            delay_enabled: true,
            reverb_enabled: true,
            mute: false,
            solo: false,
            filter: TrackFilterParams::default(),
        }; // SK-1
        channels[5] = TrackMixerChannel {
            volume: 0.80,
            pan: 0.0,
            delay_send: 0.0,
            reverb_send: 0.0,
            delay_enabled: false,
            reverb_enabled: false,
            mute: false,
            solo: false,
            filter: TrackFilterParams::default(),
        }; // MIDI
        Self {
            channels,
            master_volume: 0.88,
            moog_pad_volume: 0.80,
            moog_pad_reverb_send: 0.56,
            moog_pad_delay_send: 0.24,
            moog_pad_delay_enabled: true,
            moog_pad_reverb_enabled: true,
            moog_pad_mute: false,
            moog_pad_solo: false,
            moog_pad_filter: TrackFilterParams::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum DelayDivision {
    Eighth,
    #[default]
    DottedEighth,
    Quarter,
    DottedQuarter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum GlobalFilterMode {
    #[default]
    LowPass,
    HighPass,
    BandPass,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TrackFilterParams {
    pub enabled: bool,
    pub mode: GlobalFilterMode,
    pub cutoff: f32,    // 40.0..=18_000.0 Hz
    pub resonance: f32, // 0.0..=0.88
}

impl Default for TrackFilterParams {
    fn default() -> Self {
        Self {
            enabled: true,
            mode: GlobalFilterMode::LowPass,
            cutoff: 18_000.0,
            resonance: 0.02,
        }
    }
}

impl TrackFilterParams {
    #[inline]
    pub fn is_active(&self) -> bool {
        self.enabled
            && (self.cutoff < 17_400.0 || !matches!(self.mode, GlobalFilterMode::LowPass))
    }

    pub fn normalize(&mut self) {
        self.cutoff = self.cutoff.clamp(40.0, 18_000.0);
        self.resonance = self.resonance.clamp(0.0, 0.88);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum FilterEnvTrigger {
    Off,
    #[default]
    Beat,
    HalfBar,
    Bar,
    Kick,
    Manual,
}

impl FilterEnvTrigger {
    pub const ALL: [FilterEnvTrigger; 6] = [
        FilterEnvTrigger::Off,
        FilterEnvTrigger::Beat,
        FilterEnvTrigger::HalfBar,
        FilterEnvTrigger::Bar,
        FilterEnvTrigger::Kick,
        FilterEnvTrigger::Manual,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            FilterEnvTrigger::Off => "Off",
            FilterEnvTrigger::Beat => "1/4 Beat",
            FilterEnvTrigger::HalfBar => "1/2 Bar",
            FilterEnvTrigger::Bar => "1 Bar",
            FilterEnvTrigger::Kick => "Kick",
            FilterEnvTrigger::Manual => "Manual",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DropMacroConfig {
    pub build_bars: u8,         // 1, 2, or 4 (default 2)
    pub cut_drums: bool,        // default true (replaces normal groove with accelerating roll)
    pub cut_bass: bool,         // default true
    pub cut_chords: bool,       // default false
    pub cut_lead: bool,         // default false (cuts Arp + 303)
    pub drum_accel: bool,       // default true (1/4 -> 1/8 -> 1/16 -> 1/32 pitch-climbing roll)
    pub riser_sweep: bool,      // default true (HP filter sweep + dual-osc siren + noise riser + reverb bloom)
    pub pre_drop_gap_steps: u8, // 0, 2, or 4 16th steps of dead-air cut before impact (default 2)
    pub impact_intensity: f32,  // 0.2..=1.0 (default 0.90)
    pub auto_drop_bars: u16,    // 0 = Manual, or 8 / 16 / 32 bars (default 0)
}

impl Default for DropMacroConfig {
    fn default() -> Self {
        Self {
            build_bars: 2,
            cut_drums: true,
            cut_bass: true,
            cut_chords: false,
            cut_lead: false,
            drum_accel: true,
            riser_sweep: true,
            pre_drop_gap_steps: 2,
            impact_intensity: 0.90,
            auto_drop_bars: 0,
        }
    }
}

impl DropMacroConfig {
    pub fn normalize(&mut self) {
        self.build_bars = match self.build_bars {
            0..=1 => 1,
            2..=3 => 2,
            _ => 4,
        };
        self.pre_drop_gap_steps = self.pre_drop_gap_steps.min(8);
        self.impact_intensity = self.impact_intensity.clamp(0.0, 1.0);
    }
}

impl DelayDivision {
    pub const ALL: [DelayDivision; 4] = [
        DelayDivision::Eighth,
        DelayDivision::DottedEighth,
        DelayDivision::Quarter,
        DelayDivision::DottedQuarter,
    ];
    pub const fn name(self) -> &'static str {
        match self {
            DelayDivision::Eighth => "1/8",
            DelayDivision::DottedEighth => "3/16.",
            DelayDivision::Quarter => "1/4",
            DelayDivision::DottedQuarter => "3/8.",
        }
    }
    pub const fn sixteenth_multiplier(self) -> f32 {
        match self {
            DelayDivision::Eighth => 2.0,
            DelayDivision::DottedEighth => 3.0,
            DelayDivision::Quarter => 4.0,
            DelayDivision::DottedQuarter => 6.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FxParams {
    pub delay_enabled: bool,
    pub reverb_enabled: bool,
    pub global_filter_enabled: bool,
    pub delay_div: DelayDivision,
    pub delay_feedback: f32,
    pub delay_tone: f32, // 0..1 LP
    pub delay_mix: f32,
    pub reverb_size: f32,
    pub reverb_damp: f32,
    pub reverb_mix: f32,
    #[serde(default = "default_reverb_shimmer")]
    pub reverb_shimmer: f32,
    pub analog_warmth: f32,
    #[serde(default = "default_master_crisp")]
    pub master_crisp: f32,
    #[serde(default = "default_stereo_chorus")]
    pub stereo_chorus: f32,
    #[serde(default = "default_club_punch")]
    pub club_punch: f32,
    pub global_filter_mode: GlobalFilterMode,
    pub global_filter_cutoff: f32,
    pub global_filter_resonance: f32,
    #[serde(default)]
    pub global_filter_env_amount: f32, // -1.0..=1.0
    #[serde(default)]
    pub global_filter_env_attack: f32, // 0.002..=1.5 s
    #[serde(default)]
    pub global_filter_env_decay: f32, // 0.04..=3.0 s
    #[serde(default)]
    pub global_filter_env_trigger: FilterEnvTrigger,
    pub chop: f32,
    pub repeat: f32,
    pub glitch: f32,
    pub circuit_bend: f32,
}

impl Default for FxParams {
    fn default() -> Self {
        Self {
            delay_enabled: true,
            reverb_enabled: true,
            global_filter_enabled: true,
            delay_div: DelayDivision::DottedEighth,
            delay_feedback: 0.36,
            delay_tone: 0.62,
            delay_mix: 0.36,
            reverb_size: 0.62,
            reverb_damp: 0.48,
            reverb_mix: 0.42,
            reverb_shimmer: default_reverb_shimmer(),
            analog_warmth: 0.14,
            master_crisp: default_master_crisp(),
            stereo_chorus: default_stereo_chorus(),
            club_punch: default_club_punch(),
            global_filter_mode: GlobalFilterMode::LowPass,
            global_filter_cutoff: 18000.0,
            global_filter_resonance: 0.08,
            global_filter_env_amount: 0.0,
            global_filter_env_attack: 0.015,
            global_filter_env_decay: 0.38,
            global_filter_env_trigger: FilterEnvTrigger::Beat,
            chop: 0.0,
            repeat: 0.0,
            glitch: 0.0,
            circuit_bend: 0.0,
        }
    }
}

fn default_reverb_shimmer() -> f32 {
    0.28
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MidiSettings {
    pub midi_in_target: TrackId,
    pub midi_clock_out: bool,
    pub quantize_record: bool,
}

impl Default for MidiSettings {
    fn default() -> Self {
        Self {
            midi_in_target: TrackId::Tb303,
            midi_clock_out: true,
            quantize_record: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum PartyMixMode {
    SingleVibe,
    #[default]
    RelatedVibes,
    ClubCrate,
    ChillCrate,
    RetroCrate,
    BassCrate,
    CustomCrate,
    AllVibes,
}

impl PartyMixMode {
    pub const ALL: [PartyMixMode; 8] = [
        PartyMixMode::SingleVibe,
        PartyMixMode::RelatedVibes,
        PartyMixMode::ClubCrate,
        PartyMixMode::ChillCrate,
        PartyMixMode::RetroCrate,
        PartyMixMode::BassCrate,
        PartyMixMode::CustomCrate,
        PartyMixMode::AllVibes,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            PartyMixMode::SingleVibe => "Same Vibe",
            PartyMixMode::RelatedVibes => "Related",
            PartyMixMode::ClubCrate => "Club 4x4",
            PartyMixMode::ChillCrate => "Chill / Lo-Fi",
            PartyMixMode::RetroCrate => "80s / Synth",
            PartyMixMode::BassCrate => "Heavy Bass",
            PartyMixMode::CustomCrate => "Custom Crate",
            PartyMixMode::AllVibes => "All 24",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum PartyPace {
    Fast,
    #[default]
    Normal,
    Extended,
}

impl PartyPace {
    pub const ALL: [PartyPace; 3] = [
        PartyPace::Fast,
        PartyPace::Normal,
        PartyPace::Extended,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            PartyPace::Fast => "Fast",
            PartyPace::Normal => "Normal",
            PartyPace::Extended => "Extended",
        }
    }
}

fn default_master_crisp() -> f32 {
    0.38
}

fn default_stereo_chorus() -> f32 {
    0.48
}

fn default_club_punch() -> f32 {
    0.65
}

fn default_true() -> bool {
    true
}

fn default_party_crate_mask() -> u32 {
    0x00FF_FFFF
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum ArpMode {
    #[default]
    Up,
    Down,
    UpDown,
    Random,
    AsPlayed,
}

impl ArpMode {
    pub const ALL: [ArpMode; 5] = [
        ArpMode::Up,
        ArpMode::Down,
        ArpMode::UpDown,
        ArpMode::Random,
        ArpMode::AsPlayed,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            ArpMode::Up => "Up",
            ArpMode::Down => "Down",
            ArpMode::UpDown => "Up/Dn",
            ArpMode::Random => "Rand",
            ArpMode::AsPlayed => "Played",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum ArpRate {
    Quarter,
    Eighth,
    EighthTriplet,
    #[default]
    Sixteenth,
    ThirtySecond,
}

impl ArpRate {
    pub const ALL: [ArpRate; 5] = [
        ArpRate::Quarter,
        ArpRate::Eighth,
        ArpRate::EighthTriplet,
        ArpRate::Sixteenth,
        ArpRate::ThirtySecond,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            ArpRate::Quarter => "1/4",
            ArpRate::Eighth => "1/8",
            ArpRate::EighthTriplet => "1/8T",
            ArpRate::Sixteenth => "1/16",
            ArpRate::ThirtySecond => "1/32",
        }
    }

    pub const fn step_beats(self) -> f32 {
        match self {
            ArpRate::Quarter => 1.0,
            ArpRate::Eighth => 0.5,
            ArpRate::EighthTriplet => 1.0 / 3.0,
            ArpRate::Sixteenth => 0.25,
            ArpRate::ThirtySecond => 0.125,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ArpParams {
    pub enabled: bool,
    pub latch: bool,
    pub mode: ArpMode,
    pub rate: ArpRate,
    pub octaves: u8,
    pub gate: f32,
    pub swing: bool,
}

impl Default for ArpParams {
    fn default() -> Self {
        Self {
            enabled: false,
            latch: false,
            mode: ArpMode::Up,
            rate: ArpRate::Sixteenth,
            octaves: 2,
            gate: 0.55,
            swing: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Project {
    pub version: u32,
    pub name: String,
    pub bpm: f32,
    pub swing: f32, // 0.0..=0.5 (displayed 50%..75%)
    pub root_note: u8,
    pub scale: Scale,
    pub vibe: super::Vibe,
    pub progression_idx: usize,
    pub bars_per_chord: u8,
    pub auto_chord_mode: bool,
    pub auto_chord_follow: bool,
    pub auto_beat_mode: bool,
    pub beat_energy: f32,
    pub sidechain_amount: f32,
    pub party_energy: f32,
    pub party_auto_evolve: bool,
    #[serde(default)]
    pub party_mix_mode: PartyMixMode,
    #[serde(default = "default_party_crate_mask")]
    pub party_crate_mask: u32,
    #[serde(default)]
    pub party_pace: PartyPace,
    #[serde(default = "default_true")]
    pub party_sound_morph: bool,
    pub voice_presets: [usize; 6],
    #[serde(default = "default_active_tracks")]
    pub active_tracks: [bool; 6],
    #[serde(default)]
    pub extra_tracks: Vec<ExtraTrack>,
    pub sample_generation: u64,
    pub moog: MoogParams,
    pub moog_pad: MoogParams,
    pub monologue: MonologueParams,
    pub tb303: Tb303Params,
    pub drums: DrumKitParams,
    pub sampler: SamplerParams,
    pub sample_data: SavedSample,
    pub sample_parts: Vec<SamplePart>,
    pub sampler_iso_bass: f32,
    pub sampler_iso_treble: f32,
    pub mixer: MixerParams,
    pub fx: FxParams,
    #[serde(default)]
    pub arp: ArpParams,
    #[serde(default)]
    pub drop_macro: DropMacroConfig,
    pub midi: MidiSettings,
    pub patterns: Vec<Pattern>,
    pub active_pattern: usize,
    pub song_mode: bool,
    pub song_sections: Vec<SongSection>,
    pub party_mode: bool,
    pub party_seed: u32,
    #[serde(default)]
    pub soundfont_path: Option<String>,
    #[serde(default)]
    pub soundfont_preset: usize,
}

fn default_active_tracks() -> [bool; 6] {
    [true; 6]
}

impl Default for Project {
    fn default() -> Self {
        let mut p = Self::factory(FactoryPreset::BlankInit);
        super::apply_vibe_to_project(&mut p, super::Vibe::ElectroFunk, None, 0xF00D_5EED);
        p
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FactoryPreset {
    AcidWarehouse,
    ElectroFunkSk1,
    Detroit909Deep,
    NightDrive,
    NeonSoul,
    WarmHouse,
    Disco70sStrings,
    Obx80sPower,
    ElectricRockJam,
    BoomBapHipHop,
    Trap808Banger,
    DubstepWobble,
    BigRoomFestival,
    FutureBassEdm,
    EuphoricTrance,
    EurobeatDrift,
    ImperialBass,
    BlankInit,
}

impl FactoryPreset {
    pub const ALL: [FactoryPreset; 18] = [
        FactoryPreset::AcidWarehouse,
        FactoryPreset::ElectroFunkSk1,
        FactoryPreset::Detroit909Deep,
        FactoryPreset::NightDrive,
        FactoryPreset::NeonSoul,
        FactoryPreset::WarmHouse,
        FactoryPreset::Disco70sStrings,
        FactoryPreset::Obx80sPower,
        FactoryPreset::ElectricRockJam,
        FactoryPreset::BoomBapHipHop,
        FactoryPreset::Trap808Banger,
        FactoryPreset::DubstepWobble,
        FactoryPreset::BigRoomFestival,
        FactoryPreset::FutureBassEdm,
        FactoryPreset::EuphoricTrance,
        FactoryPreset::EurobeatDrift,
        FactoryPreset::ImperialBass,
        FactoryPreset::BlankInit,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            FactoryPreset::AcidWarehouse => "Acid Warehouse (808/909 + 303)",
            FactoryPreset::ElectroFunkSk1 => "Electro Funk (SK-1 + Moog)",
            FactoryPreset::Detroit909Deep => "Detroit Deep 909 (64-Step)",
            FactoryPreset::NightDrive => "Night Drive (deep and spacious)",
            FactoryPreset::NeonSoul => "Neon Soul (warm electro funk)",
            FactoryPreset::WarmHouse => "Warm House (chords and bounce)",
            FactoryPreset::Disco70sStrings => "70s String Synth Disco",
            FactoryPreset::Obx80sPower => "80s OB-X Power Synth",
            FactoryPreset::ElectricRockJam => "Electric Guitar Rock Jam",
            FactoryPreset::BoomBapHipHop => "90s Boom Bap Hip-Hop",
            FactoryPreset::Trap808Banger => "Trap / 808 Half-Time",
            FactoryPreset::DubstepWobble => "140 BPM Dubstep Wobble",
            FactoryPreset::BigRoomFestival => "Big Room Mainstage EDM",
            FactoryPreset::FutureBassEdm => "Future Bass Supersaw EDM",
            FactoryPreset::EuphoricTrance => "Euphoric Uplifting Trance",
            FactoryPreset::EurobeatDrift => "Eurobeat Drift (158 BPM Hi-NRG)",
            FactoryPreset::ImperialBass => "Imperial Bass (126 BPM Cinematic)",
            FactoryPreset::BlankInit => "Blank Init",
        }
    }

    pub const fn cli_slug(self) -> &'static str {
        match self {
            FactoryPreset::AcidWarehouse => "acid",
            FactoryPreset::ElectroFunkSk1 => "electro",
            FactoryPreset::Detroit909Deep => "detroit",
            FactoryPreset::NightDrive => "night",
            FactoryPreset::NeonSoul => "soul",
            FactoryPreset::WarmHouse => "house",
            FactoryPreset::Disco70sStrings => "disco70s",
            FactoryPreset::Obx80sPower => "obx80s",
            FactoryPreset::ElectricRockJam => "rock",
            FactoryPreset::BoomBapHipHop => "boombap",
            FactoryPreset::Trap808Banger => "trap",
            FactoryPreset::DubstepWobble => "dubstep",
            FactoryPreset::BigRoomFestival => "bigroom",
            FactoryPreset::FutureBassEdm => "futurebass",
            FactoryPreset::EuphoricTrance => "trance",
            FactoryPreset::EurobeatDrift => "eurobeat",
            FactoryPreset::ImperialBass => "imperial",
            FactoryPreset::BlankInit => "blank",
        }
    }

    pub fn from_slug(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "acid" | "warehouse" => Some(Self::AcidWarehouse),
            "electro" | "funk" | "sk1" => Some(Self::ElectroFunkSk1),
            "detroit" | "909" | "deep" => Some(Self::Detroit909Deep),
            "night" | "drive" => Some(Self::NightDrive),
            "soul" | "neon" => Some(Self::NeonSoul),
            "house" | "warm" => Some(Self::WarmHouse),
            "disco" | "disco70s" | "strings" => Some(Self::Disco70sStrings),
            "obx" | "obx80s" | "arena" => Some(Self::Obx80sPower),
            "rock" | "guitar" => Some(Self::ElectricRockJam),
            "boombap" | "hiphop" | "hip-hop" => Some(Self::BoomBapHipHop),
            "trap" | "808" => Some(Self::Trap808Banger),
            "dubstep" | "wobble" => Some(Self::DubstepWobble),
            "bigroom" | "edm" | "festival" => Some(Self::BigRoomFestival),
            "futurebass" | "future" => Some(Self::FutureBassEdm),
            "trance" | "euphoric" => Some(Self::EuphoricTrance),
            "eurobeat" | "initiald" | "hinrg" | "drift" => Some(Self::EurobeatDrift),
            "imperial" | "cinematic" | "romanian" | "romaniandub" => Some(Self::ImperialBass),
            "blank" | "init" => Some(Self::BlankInit),
            _ => None,
        }
    }
}

impl Project {
    pub fn normalize(&mut self) {
        self.bpm = self.bpm.clamp(40.0, 240.0);
        self.swing = self.swing.clamp(0.0, 0.45);
        self.root_note %= 12;
        while self.sample_parts.len() < 8 {
            let defaults = SamplePart::default_bank();
            let idx = self.sample_parts.len();
            self.sample_parts.push(defaults[idx % defaults.len()].clone());
        }
        self.sample_parts.truncate(8);
        self.sampler_iso_bass = self.sampler_iso_bass.clamp(0.0, 2.0);
        self.sampler_iso_treble = self.sampler_iso_treble.clamp(0.0, 2.0);
        while self.patterns.len() < NUM_PATTERNS {
            let idx = self.patterns.len();
            let bank = (b'A' + idx as u8) as char;
            self.patterns.push(Pattern::named(format!("Bank {bank}")));
        }
        self.patterns.truncate(NUM_PATTERNS);
        if self.extra_tracks.len() > MAX_EXTRA_TRACKS {
            self.extra_tracks.truncate(MAX_EXTRA_TRACKS);
        }
        for p in &mut self.patterns {
            p.normalize();
            while p.extra_seqs.len() < self.extra_tracks.len() {
                p.extra_seqs
                    .push(super::MelodicTrackSeq::with_default_note(60, StepCount::Steps64));
            }
            p.extra_seqs.truncate(self.extra_tracks.len());
            for seq in &mut p.extra_seqs {
                seq.normalize();
            }
        }
        if self.active_pattern >= NUM_PATTERNS {
            self.active_pattern = 0;
        }
        for section in &mut self.song_sections {
            section.pattern = section.pattern.min(NUM_PATTERNS - 1);
            section.bars = section.bars.clamp(1, 16);
        }
        for ch in &mut self.mixer.channels {
            ch.filter.normalize();
        }
        self.mixer.moog_pad_filter.normalize();
        for tr in &mut self.extra_tracks {
            tr.filter.normalize();
        }
        self.fx.global_filter_env_amount = self.fx.global_filter_env_amount.clamp(-1.0, 1.0);
        self.fx.global_filter_env_attack = self.fx.global_filter_env_attack.clamp(0.002, 1.5);
        self.fx.global_filter_env_decay = self.fx.global_filter_env_decay.clamp(0.04, 3.0);
        self.fx.master_crisp = self.fx.master_crisp.clamp(0.0, 1.0);
        self.fx.stereo_chorus = self.fx.stereo_chorus.clamp(0.0, 1.0);
        self.fx.club_punch = self.fx.club_punch.clamp(0.0, 1.0);
        self.fx.reverb_shimmer = self.fx.reverb_shimmer.clamp(0.0, 1.0);
        self.moog.engine_morph = self.moog.engine_morph.clamp(0.0, 1.0);
        self.moog.phaser_mix = self.moog.phaser_mix.clamp(0.0, 1.0);
        self.moog.amp_cab = self.moog.amp_cab.clamp(0.0, 1.0);
        self.moog_pad.engine_morph = self.moog_pad.engine_morph.clamp(0.0, 1.0);
        self.moog_pad.phaser_mix = self.moog_pad.phaser_mix.clamp(0.0, 1.0);
        self.moog_pad.amp_cab = self.moog_pad.amp_cab.clamp(0.0, 1.0);
        self.monologue.engine_morph = self.monologue.engine_morph.clamp(0.0, 1.0);
        self.monologue.phaser_mix = self.monologue.phaser_mix.clamp(0.0, 1.0);
        self.monologue.amp_cab = self.monologue.amp_cab.clamp(0.0, 1.0);
        for v in &mut self.drums.voices {
            v.tune = v.tune.clamp(-12.0, 12.0);
        }
        self.arp.octaves = self.arp.octaves.clamp(1, 3);
        self.arp.gate = self.arp.gate.clamp(0.15, 0.95);
        self.drop_macro.normalize();
    }

    pub fn current_pattern(&self) -> &Pattern {
        &self.patterns[self.active_pattern]
    }

    pub fn current_pattern_mut(&mut self) -> &mut Pattern {
        &mut self.patterns[self.active_pattern]
    }

    pub fn factory(preset: FactoryPreset) -> Self {
        // Variations start with a proven musical arrangement, then set a distinct tempo,
        // tone, and mix rather than generating a new pile of unrelated notes.
        match preset {
            FactoryPreset::NightDrive => {
                let mut p = Self::factory(FactoryPreset::Detroit909Deep);
                p.name = preset.name().into();
                p.bpm = 116.0;
                p.root_note = 2;
                p.fx.delay_mix = 0.36;
                p.fx.reverb_mix = 0.44;
                p.fx.reverb_shimmer = 0.34;
                p.moog.cutoff = 980.0;
                p.moog_pad.cutoff = 3100.0;
                p.patterns[0].name = "Night Drive".into();
                return p;
            }
            FactoryPreset::NeonSoul => {
                let mut p = Self::factory(FactoryPreset::ElectroFunkSk1);
                p.name = preset.name().into();
                p.bpm = 106.0;
                p.root_note = 7;
                p.fx.delay_mix = 0.32;
                p.fx.reverb_mix = 0.38;
                p.fx.reverb_shimmer = 0.22;
                p.moog.cutoff = 1800.0;
                p.patterns[0].name = "Neon Soul".into();
                return p;
            }
            FactoryPreset::WarmHouse => {
                let mut p = Self::factory(FactoryPreset::AcidWarehouse);
                p.name = preset.name().into();
                p.bpm = 122.0;
                p.root_note = 9;
                p.tb303.cutoff = 720.0;
                p.tb303.resonance = 0.34;
                p.fx.delay_mix = 0.34;
                p.fx.reverb_mix = 0.40;
                p.fx.reverb_shimmer = 0.24;
                p.patterns[0].name = "Warm House".into();
                return p;
            }
            FactoryPreset::Disco70sStrings => {
                let mut p = Self::factory(FactoryPreset::BlankInit);
                super::apply_vibe_to_project(&mut p, super::Vibe::Disco70s, None, 0x7050_114A);
                p.name = preset.name().into();
                return p;
            }
            FactoryPreset::Obx80sPower => {
                let mut p = Self::factory(FactoryPreset::BlankInit);
                super::apply_vibe_to_project(&mut p, super::Vibe::Arena80s, None, 0x80B0_0B5A);
                p.name = preset.name().into();
                return p;
            }
            FactoryPreset::ElectricRockJam => {
                let mut p = Self::factory(FactoryPreset::BlankInit);
                super::apply_vibe_to_project(&mut p, super::Vibe::IndieRock, None, 0xE1EC_7A12);
                p.name = preset.name().into();
                return p;
            }
            FactoryPreset::BoomBapHipHop => {
                let mut p = Self::factory(FactoryPreset::BlankInit);
                super::apply_vibe_to_project(&mut p, super::Vibe::BoomBap, None, 0xB00B_9090);
                p.name = preset.name().into();
                return p;
            }
            FactoryPreset::Trap808Banger => {
                let mut p = Self::factory(FactoryPreset::BlankInit);
                super::apply_vibe_to_project(&mut p, super::Vibe::Trap808, None, 0x7A90_0808);
                p.name = preset.name().into();
                return p;
            }
            FactoryPreset::DubstepWobble => {
                let mut p = Self::factory(FactoryPreset::BlankInit);
                super::apply_vibe_to_project(&mut p, super::Vibe::Dubstep, None, 0xD0B5_7E90);
                p.name = preset.name().into();
                return p;
            }
            FactoryPreset::BigRoomFestival => {
                let mut p = Self::factory(FactoryPreset::BlankInit);
                super::apply_vibe_to_project(&mut p, super::Vibe::BigRoomEdm, None, 0xB16A_ED01);
                p.name = preset.name().into();
                return p;
            }
            FactoryPreset::FutureBassEdm => {
                let mut p = Self::factory(FactoryPreset::BlankInit);
                super::apply_vibe_to_project(&mut p, super::Vibe::FutureBass, None, 0xF070_BA55);
                p.name = preset.name().into();
                return p;
            }
            FactoryPreset::EuphoricTrance => {
                let mut p = Self::factory(FactoryPreset::BlankInit);
                super::apply_vibe_to_project(&mut p, super::Vibe::EuphoricTrance, None, 0xE0F0_1380);
                p.name = preset.name().into();
                return p;
            }
            FactoryPreset::EurobeatDrift => {
                let mut p = Self::factory(FactoryPreset::BlankInit);
                super::apply_vibe_to_project(&mut p, super::Vibe::Eurobeat, None, 0xE0A0_8EA7);
                p.name = preset.name().into();
                return p;
            }
            FactoryPreset::ImperialBass => {
                let mut p = Self::factory(FactoryPreset::BlankInit);
                super::apply_vibe_to_project(&mut p, super::Vibe::ImperialBass, None, 0xA9A5_4E12);
                p.name = preset.name().into();
                return p;
            }
            _ => {}
        }
        let mut patterns: Vec<Pattern> = (0..NUM_PATTERNS)
            .map(|i| Pattern::named(format!("Bank {}", (b'A' + i as u8) as char)))
            .collect();

        let mut moog_pad = MoogParams::default();
        moog_pad.poly_mode = true;
        moog_pad.osc1_wave = MoogWave::Triangle;
        moog_pad.cutoff = 4200.0;
        moog_pad.resonance = 0.17;
        moog_pad.filt_attack = 0.3;
        moog_pad.filt_decay = 1.2;
        moog_pad.filt_sustain = 0.62;
        moog_pad.filt_release = 2.6;
        moog_pad.amp_attack = 0.35;
        moog_pad.amp_decay = 0.8;
        moog_pad.amp_sustain = 0.82;
        moog_pad.amp_release = 2.4;

        let mut p = Self {
            version: 2,
            name: preset.name().to_string(),
            bpm: 122.0,
            swing: 0.12,
            root_note: 9, // A
            scale: Scale::Dorian,
            vibe: super::Vibe::ElectroFunk,
            progression_idx: 0,
            bars_per_chord: 1,
            auto_chord_mode: false,
            auto_chord_follow: true,
            auto_beat_mode: false,
            beat_energy: 0.65,
            sidechain_amount: 0.55,
            party_energy: 0.70,
            party_auto_evolve: true,
            party_mix_mode: PartyMixMode::RelatedVibes,
            party_crate_mask: 0x00FF_FFFF,
            party_pace: PartyPace::Normal,
            party_sound_morph: true,
            voice_presets: [0; 6],
            active_tracks: [true; 6],
            extra_tracks: Vec::new(),
            sample_generation: 1,
            moog: MoogParams::default(),
            moog_pad,
            monologue: MonologueParams::default(),
            tb303: Tb303Params::default(),
            drums: DrumKitParams::default(),
            sampler: SamplerParams::default(),
            sample_data: SavedSample::default(),
            sample_parts: SamplePart::default_bank(),
            sampler_iso_bass: 1.0,
            sampler_iso_treble: 1.0,
            mixer: MixerParams::default(),
            fx: FxParams::default(),
            arp: ArpParams::default(),
            drop_macro: DropMacroConfig::default(),
            midi: MidiSettings::default(),
            patterns: Vec::new(),
            active_pattern: 0,
            song_mode: false,
            song_sections: vec![SongSection {
                name: "Intro".into(),
                pattern: 0,
                bars: 4,
                bass_drop: false,
                fill: false,
                track_mask: 0b111111,
                extra_mask: 0x03FF,
            }],
            party_mode: false,
            party_seed: 0xF00D_5EED,
            soundfont_path: None,
            soundfont_preset: 0,
        };

        match preset {
            FactoryPreset::BlankInit => {
                p.bpm = 120.0;
                p.swing = 0.0;
            }
            FactoryPreset::AcidWarehouse => {
                p.bpm = 128.0;
                p.swing = 0.06;
                p.root_note = 0; // C
                p.scale = Scale::AcidMinor;
                p.tb303 = Tb303Params {
                    wave: Tb303Wave::Saw,
                    tuning: 0.0,
                    cutoff: 390.0,
                    resonance: 0.78,
                    env_mod: 0.74,
                    decay: 0.30,
                    accent: 0.84,
                    overdrive: 0.45,
                };
                p.monologue = MonologueParams {
                    vco1_wave: MonologueOscWave::Saw,
                    vco1_shape: 0.38,
                    vco1_level: 0.85,
                    vco2_wave: MonologueOscWave::Square,
                    vco2_shape: 0.55,
                    vco2_octave: 0,
                    vco2_pitch: 0.09,
                    vco2_level: 0.72,
                    mod_mode: Osc2ModMode::Sync,
                    glide: 0.14,
                    cutoff: 1650.0,
                    resonance: 0.45,
                    drive: 0.42,
                    eg_attack: 0.004,
                    eg_decay: 0.22,
                    eg_int: 0.58,
                    eg_target: EgTarget::Cutoff,
                    amp_sustain: false,
                    lfo_wave: LfoWave::Triangle,
                    lfo_mode: LfoMode::Slow,
                    lfo_rate: 0.35,
                    lfo_int: 0.20,
                    lfo_target: LfoTarget::Shape,
                    ..MonologueParams::default()
                };
                p.sample_data = SavedSample {
                    name: "SK-1 Minor9 Rhodes".to_string(),
                    sample_rate: 48_000,
                    pcm_i16_b64: String::new(),
                    builtin_id: Some(3),
                    channels: 1,
                };

                let pat = &mut patterns[0];
                pat.name = "Warehouse Main".to_string();
                // Drums: 32 steps (classic driving 808 kick + 909 hats/snare + bar-2 fill)
                pat.drums.set_all_step_counts(StepCount::Steps32, false);
                for i in 0..32 {
                    let s16 = i % 16;
                    pat.drums.lanes[0].steps[i] = DrumStep {
                        active: s16 % 4 == 0 || (i == 30),
                        velocity: if s16 % 4 == 0 { 0.96 } else { 0.74 },
                        accent: i == 0 || i == 16,
                        ..Default::default()
                    };
                    pat.drums.lanes[1].steps[i] = DrumStep {
                        active: s16 == 4 || s16 == 12 || i == 31,
                        velocity: if i == 31 { 0.65 } else { 0.9 },
                        flam: i == 28,
                        ..Default::default()
                    };
                    pat.drums.lanes[2].steps[i] = DrumStep {
                        active: s16 == 4 || s16 == 12,
                        velocity: 0.85,
                        ..Default::default()
                    };
                    pat.drums.lanes[3].steps[i] = DrumStep {
                        active: matches!(s16, 3 | 10 | 14),
                        velocity: 0.7,
                        ..Default::default()
                    };
                    pat.drums.lanes[4].steps[i] = DrumStep {
                        active: s16 % 2 == 0 || s16 == 15,
                        velocity: if s16 % 4 == 2 { 0.45 } else { 0.78 },
                        ..Default::default()
                    };
                    pat.drums.lanes[5].steps[i] = DrumStep {
                        active: s16 % 4 == 2,
                        velocity: 0.82,
                        ..Default::default()
                    };
                    pat.drums.lanes[6].steps[i] = DrumStep {
                        active: matches!(i, 27 | 29),
                        velocity: 0.82,
                        ..Default::default()
                    };
                    pat.drums.lanes[7].steps[i] = DrumStep {
                        active: i == 0,
                        velocity: 0.75,
                        ..Default::default()
                    };
                }

                // 303: 32 steps squelchy acid bassline
                pat.tb303.step_count = StepCount::Steps32;
                let acid_notes: [(usize, u8, bool, bool); 20] = [
                    (0, 36, true, false),
                    (2, 36, false, true),
                    (3, 48, false, false),
                    (5, 39, false, true),
                    (6, 36, true, false),
                    (8, 36, false, false),
                    (10, 43, true, true),
                    (11, 46, false, false),
                    (13, 36, false, false),
                    (14, 39, true, true),
                    (16, 36, true, false),
                    (18, 36, false, true),
                    (19, 48, true, false),
                    (21, 41, false, false),
                    (22, 39, false, true),
                    (24, 36, false, false),
                    (26, 46, true, true),
                    (27, 48, false, false),
                    (29, 43, false, true),
                    (30, 39, true, false),
                ];
                for (idx, note, accent, slide) in acid_notes {
                    pat.tb303.steps[idx] = MelodicStep {
                        active: true,
                        note,
                        velocity: if accent { 1.0 } else { 0.82 },
                        gate: if slide { 0.95 } else { 0.52 },
                        accent,
                        slide,
                        chord: super::ChordType::Single,
                        motion: MotionLocks::default(),
                    };
                }

                // Monologue: 48-step polymetric motion-locked synth arp/lead!
                pat.monologue.step_count = StepCount::Steps48;
                let mono_notes = [60u8, 63, 67, 70, 67, 63, 72, 67, 63, 60, 65, 67];
                for i in 0..48 {
                    let phase = i as f32 / 48.0;
                    let active = i % 3 == 0 || i % 8 == 5;
                    let note = mono_notes[i % mono_notes.len()];
                    let cutoff_lock =
                        0.25 + 0.65 * ((phase * std::f32::consts::TAU).sin() * 0.5 + 0.5);
                    let shape_lock = (i % 16) as f32 / 15.0;
                    pat.monologue.steps[i] = MelodicStep {
                        active,
                        note,
                        velocity: 0.84,
                        gate: 0.45,
                        accent: i % 12 == 0,
                        slide: i % 16 == 7,
                        chord: super::ChordType::Single,
                        motion: MotionLocks {
                            cutoff: Some(cutoff_lock),
                            resonance: Some(0.38 + 0.28 * (1.0 - phase)),
                            drive: None,
                            shape: Some(shape_lock),
                        },
                    };
                }

                // Moog: 16-step warm sub/chord stab
                pat.moog.step_count = StepCount::Steps16;
                for (idx, note, gate) in
                    [(0, 48, 0.85), (6, 51, 0.45), (10, 55, 0.55), (14, 58, 0.35)]
                {
                    pat.moog.steps[idx] = MelodicStep {
                        active: true,
                        note,
                        velocity: 0.85,
                        gate,
                        ..Default::default()
                    };
                }

                // Casio SK-1: 32-step lo-fi Rhodes chord chops
                pat.sampler.step_count = StepCount::Steps32;
                for (idx, note) in [(0, 60), (6, 63), (12, 65), (16, 60), (24, 67), (28, 58)] {
                    pat.sampler.steps[idx] = MelodicStep {
                        active: true,
                        note,
                        velocity: 0.88,
                        gate: 0.65,
                        ..Default::default()
                    };
                }

                // Bank B breakdown variation
                let mut pat_b = pat.clone();
                pat_b.name = "Breakdown Acid".to_string();
                for i in 0..32 {
                    pat_b.drums.lanes[0].steps[i].active = false; // drop kick
                    if i % 8 == 0 {
                        pat_b.tb303.steps[i].accent = true;
                    }
                }
                patterns[1] = pat_b;
            }
            FactoryPreset::ElectroFunkSk1 => {
                p.bpm = 114.0;
                p.swing = 0.16;
                p.root_note = 5; // F
                p.scale = Scale::MinorPentatonic;
                p.drums.set_all_model(DrumModel::Tr808);
                p.moog.poly_mode = false;
                p.moog.glide = 0.18;
                p.moog.cutoff = 1100.0;
                p.moog.resonance = 0.44;
                p.sample_data = SavedSample {
                    name: "SK-1 Choir Ahh".to_string(),
                    sample_rate: 48_000,
                    pcm_i16_b64: String::new(),
                    builtin_id: Some(0),
                    channels: 1,
                };

                let pat = &mut patterns[0];
                pat.name = "Planet Rock SK1".to_string();
                pat.drums.set_all_step_counts(StepCount::Steps16, false);
                for i in [0, 3, 6, 10, 12] {
                    pat.drums.lanes[0].steps[i].active = true;
                }
                for i in [4, 12] {
                    pat.drums.lanes[1].steps[i].active = true;
                    pat.drums.lanes[2].steps[i].active = true;
                }
                for i in [2, 6, 9, 11, 14] {
                    pat.drums.lanes[3].steps[i].active = true; // 808 cowbell!
                }
                for i in 0..16 {
                    pat.drums.lanes[4].steps[i].active = i % 2 == 0 || i == 7 || i == 15;
                }
                pat.drums.lanes[5].steps[10].active = true;

                // Funky Moog bass
                pat.moog.step_count = StepCount::Steps16;
                for (idx, note, slide) in [
                    (0, 41, false),
                    (3, 41, false),
                    (4, 53, true),
                    (6, 48, false),
                    (8, 41, false),
                    (10, 44, true),
                    (12, 46, false),
                    (14, 48, false),
                ] {
                    pat.moog.steps[idx] = MelodicStep {
                        active: true,
                        note,
                        velocity: 0.92,
                        gate: 0.65,
                        accent: idx == 0 || idx == 8,
                        slide,
                        chord: super::ChordType::Single,
                        motion: MotionLocks::default(),
                    };
                }

                // SK-1 Choir Chops
                pat.sampler.step_count = StepCount::Steps32;
                for (idx, note) in [
                    (0, 65),
                    (4, 68),
                    (8, 72),
                    (14, 70),
                    (16, 65),
                    (22, 68),
                    (26, 75),
                    (30, 72),
                ] {
                    pat.sampler.steps[idx] = MelodicStep {
                        active: true,
                        note,
                        velocity: 0.85,
                        gate: 0.72,
                        ..Default::default()
                    };
                }
            }
            FactoryPreset::Detroit909Deep => {
                p.bpm = 124.0;
                p.swing = 0.11;
                p.root_note = 9; // A
                p.scale = Scale::Dorian;
                p.drums.set_all_model(DrumModel::Tr909);
                p.tb303.wave = Tb303Wave::Square;
                p.tb303.cutoff = 310.0;
                p.tb303.resonance = 0.62;
                p.sample_data = SavedSample {
                    name: "SK-1 Toy Piano".to_string(),
                    sample_rate: 48_000,
                    pcm_i16_b64: String::new(),
                    builtin_id: Some(2),
                    channels: 1,
                };

                let pat = &mut patterns[0];
                pat.name = "Detroit 64".to_string();
                pat.drums.set_all_step_counts(StepCount::Steps16, false);
                for i in 0..16 {
                    pat.drums.lanes[0].steps[i].active = i % 4 == 0;
                    pat.drums.lanes[1].steps[i].active = i == 4 || i == 12;
                    pat.drums.lanes[4].steps[i].active = i % 2 == 1 || i == 0;
                    pat.drums.lanes[5].steps[i].active = i % 4 == 2;
                    pat.drums.lanes[7].steps[i].active = i % 4 == 0;
                    pat.drums.lanes[7].steps[i].velocity = 0.42;
                }
                pat.moog.step_count = StepCount::Steps64;
                let chords = [57u8, 60, 64, 67, 69, 64, 60, 57];
                for i in 0..64 {
                    if i % 3 == 0 || i % 16 == 14 {
                        let phase = i as f32 / 64.0;
                        pat.moog.steps[i] = MelodicStep {
                            active: true,
                            note: chords[i % chords.len()],
                            velocity: 0.80,
                            gate: 0.55,
                            motion: MotionLocks {
                                cutoff: Some(0.2 + 0.65 * phase),
                                ..Default::default()
                            },
                            ..Default::default()
                        };
                    }
                }
                pat.tb303.step_count = StepCount::Steps16;
                for (idx, note) in [(0, 33), (3, 33), (6, 40), (8, 33), (11, 36), (14, 38)] {
                    pat.tb303.steps[idx] = MelodicStep {
                        active: true,
                        note,
                        velocity: 0.85,
                        gate: 0.6,
                        accent: idx == 0,
                        slide: idx == 11,
                        ..Default::default()
                    };
                }
                pat.sampler.step_count = StepCount::Steps16;
                for (idx, note) in [(2, 69), (6, 72), (11, 76), (15, 74)] {
                    pat.sampler.steps[idx] = MelodicStep {
                        active: true,
                        note,
                        velocity: 0.78,
                        gate: 0.5,
                        ..Default::default()
                    };
                }
            }
            FactoryPreset::NightDrive
            | FactoryPreset::NeonSoul
            | FactoryPreset::WarmHouse
            | FactoryPreset::Disco70sStrings
            | FactoryPreset::Obx80sPower
            | FactoryPreset::ElectricRockJam
            | FactoryPreset::BoomBapHipHop
            | FactoryPreset::Trap808Banger
            | FactoryPreset::DubstepWobble
            | FactoryPreset::BigRoomFestival
            | FactoryPreset::FutureBassEdm
            | FactoryPreset::EuphoricTrance
            | FactoryPreset::EurobeatDrift
            | FactoryPreset::ImperialBass => unreachable!(),
        }

        p.patterns = patterns;
        p.normalize();
        p
    }
}

pub fn save_project(path: &Path, project: &Project) -> Result<(), String> {
    let mut normalized = project.clone();
    normalized.normalize();
    let json = serde_json::to_string_pretty(&normalized)
        .map_err(|e| format!("Failed to serialize project: {e}"))?;
    fs::write(path, json).map_err(|e| format!("Failed to write {}: {e}", path.display()))
}

pub fn load_project(path: &Path) -> Result<Project, String> {
    let raw =
        fs::read_to_string(path).map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
    let mut project: Project =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid project JSON: {e}"))?;
    project.normalize();
    Ok(project)
}

pub fn save_pattern(path: &Path, pattern: &Pattern) -> Result<(), String> {
    let mut normalized = pattern.clone();
    normalized.normalize();
    let json = serde_json::to_string_pretty(&normalized)
        .map_err(|e| format!("Failed to serialize pattern: {e}"))?;
    fs::write(path, json).map_err(|e| format!("Failed to write {}: {e}", path.display()))
}

pub fn load_pattern(path: &Path) -> Result<Pattern, String> {
    let raw =
        fs::read_to_string(path).map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
    let mut pat: Pattern =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid pattern JSON: {e}"))?;
    pat.normalize();
    Ok(pat)
}

const B64_TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn b64_encode(input: &[u8]) -> String {
    let mut out = String::with_capacity((input.len() + 2) / 3 * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(B64_TABLE[((triple >> 18) & 0x3F) as usize] as char);
        out.push(B64_TABLE[((triple >> 12) & 0x3F) as usize] as char);
        out.push(if chunk.len() > 1 {
            B64_TABLE[((triple >> 6) & 0x3F) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            B64_TABLE[(triple & 0x3F) as usize] as char
        } else {
            '='
        });
    }
    out
}

pub fn b64_decode(input: &str) -> Result<Vec<u8>, String> {
    let mut vals = [255u8; 256];
    for (i, &c) in B64_TABLE.iter().enumerate() {
        vals[c as usize] = i as u8;
    }
    let clean: Vec<u8> = input.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    if clean.len() % 4 != 0 {
        return Err("Invalid base64 length".to_string());
    }
    let mut out = Vec::with_capacity(clean.len() / 4 * 3);
    for chunk in clean.chunks_exact(4) {
        let mut q = [0u32; 4];
        let mut pad = 0usize;
        for i in 0..4 {
            if chunk[i] == b'=' {
                pad += 1;
                q[i] = 0;
            } else {
                let v = vals[chunk[i] as usize];
                if v == 255 {
                    return Err("Invalid base64 character".to_string());
                }
                q[i] = v as u32;
            }
        }
        let triple = (q[0] << 18) | (q[1] << 12) | (q[2] << 6) | q[3];
        out.push(((triple >> 16) & 0xFF) as u8);
        if pad < 2 {
            out.push(((triple >> 8) & 0xFF) as u8);
        }
        if pad < 1 {
            out.push((triple & 0xFF) as u8);
        }
    }
    Ok(out)
}
