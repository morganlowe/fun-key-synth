pub mod project;
pub mod sequencer;

use serde::{Deserialize, Serialize};

pub use project::{
    load_project, save_project, DelayDivision, DrumKitParams, DrumModel, DrumVoiceParams, EgTarget,
    FactoryPreset, FxParams, GlobalFilterMode, LfoMode, LfoTarget, LfoWave, LoopMode, MidiSettings,
    MixerParams, MonologueOscWave, MonologueParams, MoogParams, MoogWave, Osc2ModMode, Project,
    SamplerMode, SamplerParams, SavedSample, SongSection, Tb303Params, Tb303Wave,
    TrackMixerChannel,
};
pub use sequencer::{
    euclidean_rhythm, ChordType, DrumLane, DrumStep, DrumTrackSeq, DrumVoiceId, MelodicStep,
    MelodicTrackSeq, MidiStep, MidiTrackSeq, MotionLocks, Pattern,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TrackId {
    Moog,
    MoogPad,
    Monologue,
    Tb303,
    Drums,
    Sampler,
    MidiOut,
}

impl TrackId {
    pub const ALL: [TrackId; 7] = [
        TrackId::Moog,
        TrackId::MoogPad,
        TrackId::Monologue,
        TrackId::Tb303,
        TrackId::Drums,
        TrackId::Sampler,
        TrackId::MidiOut,
    ];

    pub const fn index(self) -> usize {
        match self {
            TrackId::Moog => 0,
            TrackId::MoogPad => 6,
            TrackId::Monologue => 1,
            TrackId::Tb303 => 2,
            TrackId::Drums => 3,
            TrackId::Sampler => 4,
            TrackId::MidiOut => 5,
        }
    }

    pub const fn from_index(idx: usize) -> Option<TrackId> {
        match idx {
            0 => Some(TrackId::Moog),
            6 => Some(TrackId::MoogPad),
            1 => Some(TrackId::Monologue),
            2 => Some(TrackId::Tb303),
            3 => Some(TrackId::Drums),
            4 => Some(TrackId::Sampler),
            5 => Some(TrackId::MidiOut),
            _ => None,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            TrackId::Moog => "MOOG CLASSIC",
            TrackId::MoogPad => "MOOG LAYER",
            TrackId::Monologue => "MONOLOGUE",
            TrackId::Tb303 => "TB-303 ACID",
            TrackId::Drums => "808 / 909 DRUMS",
            TrackId::Sampler => "CASIO SK-1",
            TrackId::MidiOut => "MIDI OUT",
        }
    }

    pub const fn short_name(self) -> &'static str {
        match self {
            TrackId::Moog => "MOOG",
            TrackId::MoogPad => "MOOG PAD",
            TrackId::Monologue => "MONO",
            TrackId::Tb303 => "303",
            TrackId::Drums => "DRUM",
            TrackId::Sampler => "SK-1",
            TrackId::MidiOut => "MIDI",
        }
    }

    pub const fn is_melodic(self) -> bool {
        !matches!(self, TrackId::Drums)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum StepCount {
    #[default]
    Steps16,
    Steps32,
    Steps48,
    Steps64,
}

impl StepCount {
    pub const ALL: [StepCount; 4] = [
        StepCount::Steps16,
        StepCount::Steps32,
        StepCount::Steps48,
        StepCount::Steps64,
    ];

    pub const fn as_usize(self) -> usize {
        match self {
            StepCount::Steps16 => 16,
            StepCount::Steps32 => 32,
            StepCount::Steps48 => 48,
            StepCount::Steps64 => 64,
        }
    }

    pub const fn pages(self) -> usize {
        self.as_usize() / 16
    }

    pub const fn label(self) -> &'static str {
        match self {
            StepCount::Steps16 => "16",
            StepCount::Steps32 => "32",
            StepCount::Steps48 => "48",
            StepCount::Steps64 => "64",
        }
    }

    pub const fn from_usize(n: usize) -> Self {
        match n {
            0..=16 => StepCount::Steps16,
            17..=32 => StepCount::Steps32,
            33..=48 => StepCount::Steps48,
            _ => StepCount::Steps64,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum ClockDiv {
    Eighth,
    #[default]
    Sixteenth,
    ThirtySecond,
}

impl ClockDiv {
    pub const ALL: [ClockDiv; 3] = [
        ClockDiv::Eighth,
        ClockDiv::Sixteenth,
        ClockDiv::ThirtySecond,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            ClockDiv::Eighth => "1/8",
            ClockDiv::Sixteenth => "1/16",
            ClockDiv::ThirtySecond => "1/32",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum Scale {
    Chromatic,
    #[default]
    Minor,
    Major,
    Dorian,
    Phrygian,
    MinorPentatonic,
    AcidMinor,
}

impl Scale {
    pub const ALL: [Scale; 7] = [
        Scale::Chromatic,
        Scale::Minor,
        Scale::Major,
        Scale::Dorian,
        Scale::Phrygian,
        Scale::MinorPentatonic,
        Scale::AcidMinor,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Scale::Chromatic => "Chromatic",
            Scale::Minor => "Natural Minor",
            Scale::Major => "Major",
            Scale::Dorian => "Dorian",
            Scale::Phrygian => "Phrygian",
            Scale::MinorPentatonic => "Minor Pent",
            Scale::AcidMinor => "Acid Minor",
        }
    }

    pub const fn intervals(self) -> &'static [u8] {
        match self {
            Scale::Chromatic => &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
            Scale::Minor => &[0, 2, 3, 5, 7, 8, 10],
            Scale::Major => &[0, 2, 4, 5, 7, 9, 11],
            Scale::Dorian => &[0, 2, 3, 5, 7, 9, 10],
            Scale::Phrygian => &[0, 1, 3, 5, 7, 8, 10],
            Scale::MinorPentatonic => &[0, 3, 5, 7, 10],
            Scale::AcidMinor => &[0, 1, 3, 5, 7, 10],
        }
    }

    pub fn quantize(self, root: u8, midi: u8) -> u8 {
        if matches!(self, Scale::Chromatic) {
            return midi.clamp(12, 108);
        }
        let root_pc = root % 12;
        let intervals = self.intervals();
        let mut best_note = midi;
        let mut best_dist = i32::MAX;
        let target = midi as i32;
        for candidate in (target - 6).max(12)..=(target + 6).min(108) {
            let pc = ((candidate - root_pc as i32).rem_euclid(12)) as u8;
            if intervals.contains(&pc) {
                let d = (candidate - target).abs();
                if d < best_dist {
                    best_dist = d;
                    best_note = candidate as u8;
                }
            }
        }
        best_note
    }

    pub fn notes_in_range(self, root: u8, low: u8, high: u8) -> Vec<u8> {
        let root_pc = root % 12;
        let intervals = self.intervals();
        let mut out = Vec::new();
        for m in low..=high {
            let pc = (m + 12 - root_pc) % 12;
            if intervals.contains(&pc) {
                out.push(m);
            }
        }
        if out.is_empty() {
            out.push(low);
        }
        out
    }
}

pub const NOTE_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

pub fn note_name(midi: u8) -> String {
    let pc = (midi % 12) as usize;
    let octave = (midi as i32 / 12) - 1;
    format!("{}{}", NOTE_NAMES[pc], octave)
}

pub fn root_name(root: u8) -> &'static str {
    NOTE_NAMES[(root % 12) as usize]
}

pub fn midi_to_hz(midi: f32) -> f32 {
    440.0 * 2.0_f32.powf((midi - 69.0) / 12.0)
}
