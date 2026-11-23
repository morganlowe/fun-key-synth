use serde::{Deserialize, Serialize};

use super::{
    root_name, ChordType, DelayDivision, DrumKitParams, DrumModel, DrumTrackSeq, DrumVoiceId,
    EgTarget, FactoryPreset, GlobalFilterMode, LfoMode, LfoTarget, LfoWave, MelodicTrackSeq,
    MonologueOscWave, MonologueParams, MoogParams, MoogWave, MotionLocks, Osc2ModMode,
    PartyMixMode, Project, SamplerMode, SamplerParams, Scale, SongSection, StepCount, Tb303Params,
    Tb303Wave, TrackId,
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
    Disco70s,
    Arena80s,
    IndieRock,
    NeoSoul,
    ItaloCosmic,
    BoomBap,
    Trap808,
    Dubstep,
    BigRoomEdm,
    FutureBass,
    EuphoricTrance,
}

impl Vibe {
    pub const ALL: [Vibe; 22] = [
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
        Vibe::Disco70s,
        Vibe::Arena80s,
        Vibe::IndieRock,
        Vibe::NeoSoul,
        Vibe::ItaloCosmic,
        Vibe::BoomBap,
        Vibe::Trap808,
        Vibe::Dubstep,
        Vibe::BigRoomEdm,
        Vibe::FutureBass,
        Vibe::EuphoricTrance,
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
            Vibe::Disco70s => "70s Disco",
            Vibe::Arena80s => "80s OB-X Power",
            Vibe::IndieRock => "Electric Rock",
            Vibe::NeoSoul => "Neo-Soul",
            Vibe::ItaloCosmic => "Italo Cosmic",
            Vibe::BoomBap => "Boom Bap",
            Vibe::Trap808 => "Trap / 808",
            Vibe::Dubstep => "Dubstep",
            Vibe::BigRoomEdm => "Big Room EDM",
            Vibe::FutureBass => "Future Bass",
            Vibe::EuphoricTrance => "Euphoric Trance",
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
            Vibe::Disco70s => "70s Solina string synth swells, octave bass & crisp 4x4 disco",
            Vibe::Arena80s => "Massive OB-X 80s power synth brass, gated drums & arena anthems",
            Vibe::IndieRock => "Overdriven electric guitar riffs, picked bass & punchy rock drums",
            Vibe::NeoSoul => "Velvet Wurlitzer & Rhodes 9ths, swung pocket & warm sub",
            Vibe::ItaloCosmic => "Hypnotic 80s cosmic arps, 70s strings & sync screamer leads",
            Vibe::BoomBap => "90s golden-era hip-hop: dusty swung drums, warm 808 sub & chopped jazz Rhodes",
            Vibe::Trap808 => "142 BPM half-time trap: booming sliding 808s, ratcheted hat rolls & dark bells",
            Vibe::Dubstep => "140 BPM half-time filth: resonant LFO wobble bass, ring-mod growls & huge snare",
            Vibe::BigRoomEdm => "128 BPM mainstage anthem: massive festival supersaws, pumping sidechain & 909 drop",
            Vibe::FutureBass => "150 BPM half-time EDM: lush pumping supersaw 9ths, gliding 808 sub & sparkling bells",
            Vibe::EuphoricTrance => "138 BPM uplifting trance: rolling 16th bass, shimmering plucks & soaring wide pads",
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
            Vibe::Disco70s => 120.0,
            Vibe::Arena80s => 126.0,
            Vibe::IndieRock => 124.0,
            Vibe::NeoSoul => 92.0,
            Vibe::ItaloCosmic => 116.0,
            Vibe::BoomBap => 92.0,
            Vibe::Trap808 => 142.0,
            Vibe::Dubstep => 140.0,
            Vibe::BigRoomEdm => 128.0,
            Vibe::FutureBass => 150.0,
            Vibe::EuphoricTrance => 138.0,
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
            Vibe::Disco70s => 0.10,
            Vibe::Arena80s => 0.02,
            Vibe::IndieRock => 0.06,
            Vibe::NeoSoul => 0.24,
            Vibe::ItaloCosmic => 0.04,
            Vibe::BoomBap => 0.24,
            Vibe::Trap808 => 0.06,
            Vibe::Dubstep => 0.04,
            Vibe::BigRoomEdm => 0.04,
            Vibe::FutureBass => 0.08,
            Vibe::EuphoricTrance => 0.02,
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
            Vibe::Disco70s => 9,      // A
            Vibe::Arena80s => 0,      // C
            Vibe::IndieRock => 4,     // E
            Vibe::NeoSoul => 5,       // F
            Vibe::ItaloCosmic => 2,   // D
            Vibe::BoomBap => 2,       // D
            Vibe::Trap808 => 5,       // F
            Vibe::Dubstep => 4,       // E
            Vibe::BigRoomEdm => 5,    // F
            Vibe::FutureBass => 0,    // C
            Vibe::EuphoricTrance => 9, // A
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
            Vibe::Disco70s => Scale::Dorian,
            Vibe::Arena80s => Scale::Major,
            Vibe::IndieRock => Scale::Mixolydian,
            Vibe::NeoSoul => Scale::Dorian,
            Vibe::ItaloCosmic => Scale::HarmonicMinor,
            Vibe::BoomBap => Scale::Dorian,
            Vibe::Trap808 => Scale::Minor,
            Vibe::Dubstep => Scale::Phrygian,
            Vibe::BigRoomEdm => Scale::Minor,
            Vibe::FutureBass => Scale::Minor,
            Vibe::EuphoricTrance => Scale::Minor,
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
            Vibe::Disco70s => 9,
            Vibe::Arena80s => 8,
            Vibe::IndieRock => 10,
            Vibe::NeoSoul => 11,
            Vibe::ItaloCosmic => 13,
            Vibe::BoomBap => 16,
            Vibe::Trap808 => 17,
            Vibe::Dubstep => 18,
            Vibe::BigRoomEdm => 1,
            Vibe::FutureBass => 19,
            Vibe::EuphoricTrance => 15,
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
            Vibe::Disco70s => [255, 210, 82],
            Vibe::Arena80s => [92, 168, 255],
            Vibe::IndieRock => [255, 112, 72],
            Vibe::NeoSoul => [188, 134, 255],
            Vibe::ItaloCosmic => [78, 240, 224],
            Vibe::BoomBap => [245, 158, 66],
            Vibe::Trap808 => [239, 68, 98],
            Vibe::Dubstep => [168, 85, 247],
            Vibe::BigRoomEdm => [34, 211, 238],
            Vibe::FutureBass => [244, 114, 182],
            Vibe::EuphoricTrance => [56, 189, 248],
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

pub const PROGRESSIONS: [ChordProgression; 20] = [
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
    ChordProgression {
        name: "80s Power Anthem (I - V7sus4 - vi7 - IVadd9)",
        slots: [
            ChordSlot { offset: 0, chord: ChordType::Add9 },
            ChordSlot { offset: 7, chord: ChordType::Dom7Sus4 },
            ChordSlot { offset: 9, chord: ChordType::Minor7 },
            ChordSlot { offset: 5, chord: ChordType::Add9 },
        ],
    },
    ChordProgression {
        name: "70s Disco Strings (i7 - iv7 - VII7 - III6)",
        slots: [
            ChordSlot { offset: 0, chord: ChordType::Minor7 },
            ChordSlot { offset: 5, chord: ChordType::Minor7 },
            ChordSlot { offset: 10, chord: ChordType::Dom7 },
            ChordSlot { offset: 3, chord: ChordType::Major6 },
        ],
    },
    ChordProgression {
        name: "Classic Rock Riffs (i5 - VI5 - VII5 - IV5)",
        slots: [
            ChordSlot { offset: 0, chord: ChordType::Power5 },
            ChordSlot { offset: 8, chord: ChordType::Power5 },
            ChordSlot { offset: 10, chord: ChordType::Power5 },
            ChordSlot { offset: 5, chord: ChordType::Power5 },
        ],
    },
    ChordProgression {
        name: "Neo-Soul Velvet (i9 - IV9 - bVII9 - III9)",
        slots: [
            ChordSlot { offset: 0, chord: ChordType::Minor9 },
            ChordSlot { offset: 5, chord: ChordType::Dom9 },
            ChordSlot { offset: 10, chord: ChordType::Major9 },
            ChordSlot { offset: 3, chord: ChordType::Major9 },
        ],
    },
    ChordProgression {
        name: "Cinematic Noir (i6 - bVI9 - iv7 - V7sus4)",
        slots: [
            ChordSlot { offset: 0, chord: ChordType::Minor6 },
            ChordSlot { offset: 8, chord: ChordType::Major9 },
            ChordSlot { offset: 5, chord: ChordType::Minor7 },
            ChordSlot { offset: 7, chord: ChordType::Dom7Sus4 },
        ],
    },
    ChordProgression {
        name: "Italo Cosmic (i7 - bVI7 - bIIIadd9 - V7)",
        slots: [
            ChordSlot { offset: 0, chord: ChordType::Minor7 },
            ChordSlot { offset: 8, chord: ChordType::Major7 },
            ChordSlot { offset: 3, chord: ChordType::Add9 },
            ChordSlot { offset: 7, chord: ChordType::Dom7 },
        ],
    },
    ChordProgression {
        name: "Blues Funk Turn (I7 - IV9 - I7 - V9)",
        slots: [
            ChordSlot { offset: 0, chord: ChordType::Dom7 },
            ChordSlot { offset: 5, chord: ChordType::Dom9 },
            ChordSlot { offset: 0, chord: ChordType::Dom7 },
            ChordSlot { offset: 7, chord: ChordType::Dom9 },
        ],
    },
    ChordProgression {
        name: "Euphoric Trance (vi7 - IV9 - Iadd9 - Vsus4)",
        slots: [
            ChordSlot { offset: 9, chord: ChordType::Minor7 },
            ChordSlot { offset: 5, chord: ChordType::Major9 },
            ChordSlot { offset: 0, chord: ChordType::Add9 },
            ChordSlot { offset: 7, chord: ChordType::Sus4 },
        ],
    },
    ChordProgression {
        name: "Boom Bap Jazz (i9 - iv9 - bVII9 - v7)",
        slots: [
            ChordSlot { offset: 0, chord: ChordType::Minor9 },
            ChordSlot { offset: 5, chord: ChordType::Minor9 },
            ChordSlot { offset: 10, chord: ChordType::Dom9 },
            ChordSlot { offset: 7, chord: ChordType::Minor7 },
        ],
    },
    ChordProgression {
        name: "Dark Trap (i7 - bVI7 - i7 - V7sus4)",
        slots: [
            ChordSlot { offset: 0, chord: ChordType::Minor7 },
            ChordSlot { offset: 8, chord: ChordType::Major7 },
            ChordSlot { offset: 0, chord: ChordType::Minor7 },
            ChordSlot { offset: 7, chord: ChordType::Dom7Sus4 },
        ],
    },
    ChordProgression {
        name: "Dubstep Menace (i5 - bII5 - bVI5 - V5)",
        slots: [
            ChordSlot { offset: 0, chord: ChordType::Power5 },
            ChordSlot { offset: 1, chord: ChordType::Power5 },
            ChordSlot { offset: 8, chord: ChordType::Power5 },
            ChordSlot { offset: 7, chord: ChordType::Power5 },
        ],
    },
    ChordProgression {
        name: "Future Bass Lift (bVI9 - bVIIadd9 - i9 - bIII9)",
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

/// Curated chord progression indices that fit each Vibe's genre so Party DJ Mode
/// mixes through authentic genre-matched songs instead of out-of-style progressions.
pub const fn vibe_progressions(vibe: Vibe) -> &'static [usize] {
    match vibe {
        Vibe::DeepHouse => &[0, 1, 7, 11, 12],
        Vibe::ElectroFunk => &[3, 14, 9, 11],
        Vibe::LofiChill => &[4, 11, 16, 0],
        Vibe::ElectroSoul => &[1, 7, 11, 0, 19],
        Vibe::Synthwave => &[2, 12, 8, 13, 1],
        Vibe::AcidClub => &[5, 6, 17, 2],
        Vibe::AmbientDub => &[6, 0, 12, 5],
        Vibe::UkGarage => &[0, 11, 3, 1, 4],
        Vibe::JungleDnb => &[6, 12, 2, 0],
        Vibe::DetroitTechno => &[5, 6, 0, 12],
        Vibe::DreamPop => &[7, 1, 8, 15, 19],
        Vibe::Disco70s => &[9, 3, 11, 13],
        Vibe::Arena80s => &[8, 2, 7, 10, 15],
        Vibe::IndieRock => &[10, 8, 2, 1],
        Vibe::NeoSoul => &[11, 4, 0, 16, 3],
        Vibe::ItaloCosmic => &[13, 2, 9, 12],
        Vibe::BoomBap => &[16, 4, 11, 0],
        Vibe::Trap808 => &[17, 12, 5, 18],
        Vibe::Dubstep => &[18, 17, 5, 6],
        Vibe::BigRoomEdm => &[1, 15, 7, 8, 19],
        Vibe::FutureBass => &[19, 7, 1, 11, 15],
        Vibe::EuphoricTrance => &[15, 7, 1, 19, 2],
    }
}

/// Returns 5 musically related neighbor vibes (starting with `vibe` itself)
/// so Party DJ Mode can morph smoothly across adjacent genres without whiplash.
pub const fn vibe_neighbors(vibe: Vibe) -> &'static [Vibe] {
    match vibe {
        Vibe::DeepHouse => &[
            Vibe::DeepHouse,
            Vibe::UkGarage,
            Vibe::DetroitTechno,
            Vibe::ElectroSoul,
            Vibe::Disco70s,
        ],
        Vibe::ElectroFunk => &[
            Vibe::ElectroFunk,
            Vibe::Disco70s,
            Vibe::ItaloCosmic,
            Vibe::NeoSoul,
            Vibe::BoomBap,
        ],
        Vibe::LofiChill => &[
            Vibe::LofiChill,
            Vibe::NeoSoul,
            Vibe::BoomBap,
            Vibe::DreamPop,
            Vibe::AmbientDub,
        ],
        Vibe::ElectroSoul => &[
            Vibe::ElectroSoul,
            Vibe::DeepHouse,
            Vibe::DreamPop,
            Vibe::FutureBass,
            Vibe::Synthwave,
        ],
        Vibe::Synthwave => &[
            Vibe::Synthwave,
            Vibe::Arena80s,
            Vibe::ItaloCosmic,
            Vibe::DreamPop,
            Vibe::ElectroSoul,
        ],
        Vibe::AcidClub => &[
            Vibe::AcidClub,
            Vibe::DetroitTechno,
            Vibe::BigRoomEdm,
            Vibe::EuphoricTrance,
            Vibe::ElectroFunk,
        ],
        Vibe::AmbientDub => &[
            Vibe::AmbientDub,
            Vibe::DeepHouse,
            Vibe::LofiChill,
            Vibe::JungleDnb,
            Vibe::DreamPop,
        ],
        Vibe::UkGarage => &[
            Vibe::UkGarage,
            Vibe::DeepHouse,
            Vibe::JungleDnb,
            Vibe::NeoSoul,
            Vibe::FutureBass,
        ],
        Vibe::JungleDnb => &[
            Vibe::JungleDnb,
            Vibe::UkGarage,
            Vibe::Dubstep,
            Vibe::DetroitTechno,
            Vibe::AmbientDub,
        ],
        Vibe::DetroitTechno => &[
            Vibe::DetroitTechno,
            Vibe::AcidClub,
            Vibe::DeepHouse,
            Vibe::EuphoricTrance,
            Vibe::BigRoomEdm,
        ],
        Vibe::DreamPop => &[
            Vibe::DreamPop,
            Vibe::Synthwave,
            Vibe::IndieRock,
            Vibe::ElectroSoul,
            Vibe::LofiChill,
        ],
        Vibe::Disco70s => &[
            Vibe::Disco70s,
            Vibe::ElectroFunk,
            Vibe::ItaloCosmic,
            Vibe::DeepHouse,
            Vibe::Arena80s,
        ],
        Vibe::Arena80s => &[
            Vibe::Arena80s,
            Vibe::Synthwave,
            Vibe::IndieRock,
            Vibe::Disco70s,
            Vibe::ItaloCosmic,
        ],
        Vibe::IndieRock => &[
            Vibe::IndieRock,
            Vibe::Arena80s,
            Vibe::DreamPop,
            Vibe::Synthwave,
            Vibe::ElectroSoul,
        ],
        Vibe::NeoSoul => &[
            Vibe::NeoSoul,
            Vibe::LofiChill,
            Vibe::BoomBap,
            Vibe::ElectroFunk,
            Vibe::UkGarage,
        ],
        Vibe::ItaloCosmic => &[
            Vibe::ItaloCosmic,
            Vibe::Synthwave,
            Vibe::Disco70s,
            Vibe::ElectroFunk,
            Vibe::EuphoricTrance,
        ],
        Vibe::BoomBap => &[
            Vibe::BoomBap,
            Vibe::LofiChill,
            Vibe::NeoSoul,
            Vibe::Trap808,
            Vibe::ElectroFunk,
        ],
        Vibe::Trap808 => &[
            Vibe::Trap808,
            Vibe::BoomBap,
            Vibe::Dubstep,
            Vibe::FutureBass,
            Vibe::UkGarage,
        ],
        Vibe::Dubstep => &[
            Vibe::Dubstep,
            Vibe::Trap808,
            Vibe::JungleDnb,
            Vibe::BigRoomEdm,
            Vibe::FutureBass,
        ],
        Vibe::BigRoomEdm => &[
            Vibe::BigRoomEdm,
            Vibe::EuphoricTrance,
            Vibe::FutureBass,
            Vibe::AcidClub,
            Vibe::Dubstep,
        ],
        Vibe::FutureBass => &[
            Vibe::FutureBass,
            Vibe::Trap808,
            Vibe::BigRoomEdm,
            Vibe::EuphoricTrance,
            Vibe::ElectroSoul,
        ],
        Vibe::EuphoricTrance => &[
            Vibe::EuphoricTrance,
            Vibe::BigRoomEdm,
            Vibe::FutureBass,
            Vibe::DetroitTechno,
            Vibe::ItaloCosmic,
        ],
    }
}

/// Picks the Vibe for a given Party DJ `song_cycle` without repeating two songs
/// back-to-back when rotating across related or all vibes.
pub fn pick_party_vibe(base_vibe: Vibe, mix_mode: PartyMixMode, song_cycle: u32, seed: u32) -> Vibe {
    if song_cycle == 0 || matches!(mix_mode, PartyMixMode::SingleVibe) {
        return base_vibe;
    }

    let pool: &[Vibe] = match mix_mode {
        PartyMixMode::SingleVibe => return base_vibe,
        PartyMixMode::RelatedVibes => vibe_neighbors(base_vibe),
        PartyMixMode::AllVibes => &Vibe::ALL,
    };

    let base_idx = pool.iter().position(|&v| v == base_vibe).unwrap_or(0);
    let start = song_cycle.saturating_sub(8).max(1);
    let mut last_idx = if start > 1 {
        let mut r = Rng::new(
            seed ^ (start - 1).wrapping_mul(0x9E37_79B9) ^ (base_vibe as u32).wrapping_mul(131),
        );
        r.range(pool.len())
    } else {
        base_idx
    };

    for c in start..=song_cycle {
        let mut r = Rng::new(
            seed ^ c.wrapping_mul(0x9E37_79B9) ^ (base_vibe as u32).wrapping_mul(131),
        );
        let mut idx = r.range(pool.len());
        if pool.len() > 1 && idx == last_idx {
            idx = (idx + 1 + ((c as usize) % (pool.len() - 1))) % pool.len();
        }
        last_idx = idx;
    }
    pool[last_idx]
}

/// Generates a stylish DJ track title for Party Mode based on the Vibe and song number.
pub fn party_song_title(vibe: Vibe, song_idx: u32, root_note: u8) -> String {
    const ADJ: [&str; 20] = [
        "Midnight", "Neon", "Velvet", "Electric", "Cosmic",
        "Golden", "Underground", "Solar", "Crystal", "Analogue",
        "Hyper", "Sunset", "Magnetic", "Prism", "Subterranean",
        "Crimson", "Emerald", "Starlight", "Mirrored", "Afterhours",
    ];
    const NOUN: [&str; 20] = [
        "Groove", "Odyssey", "Pulse", "Mirage", "Horizon",
        "Bounce", "Warehouse", "Dream", "Anthem", "Session",
        "Cascade", "Vortex", "Circuit", "Skyline", "Signal",
        "Drift", "Frequencies", "Voyage", "Echoes", "Rush",
    ];
    let h = song_idx
        .wrapping_mul(2654435761)
        ^ (vibe as u32).wrapping_mul(0x517C_C1B7)
        ^ (root_note as u32).wrapping_mul(313);
    let a = ADJ[(h as usize) % ADJ.len()];
    let n = NOUN[((h >> 5) as usize) % NOUN.len()];
    format!(
        "#{} \"{} {}\" ({} • {})",
        song_idx + 1,
        a,
        n,
        vibe.name(),
        root_name(root_note)
    )
}

/// Complete per-song synth, drum, sampler, tempo, and FX rig used by Party DJ Mode
/// so each generated song gets its own distinct voice palette and sonic character.
#[derive(Debug, Clone)]
pub struct PartySongRig {
    pub vibe: Vibe,
    pub bpm: f32,
    pub swing: f32,
    pub sidechain_amount: f32,
    pub moog: MoogParams,
    pub moog_pad: MoogParams,
    pub monologue: MonologueParams,
    pub tb303: Tb303Params,
    pub drums: DrumKitParams,
    pub sampler: SamplerParams,
    pub sampler_builtin_id: Option<u8>,
    pub delay_div: DelayDivision,
    pub reverb_mix: f32,
    pub delay_mix: f32,
    pub reverb_size: f32,
    pub analog_warmth: f32,
    pub enable_303: bool,
    pub enable_sampler: bool,
    pub drums_vol: f32,
    pub bass_vol: f32,
    pub pad_vol: f32,
    pub lead_vol: f32,
    pub tb303_vol: f32,
    pub sampler_vol: f32,
    pub palette_label: String,
}

/// Curated (Drums, Moog, MoogPad, Monologue, Tb303, Sampler) voice preset indices
/// matched to each genre so Party DJ Mode swaps authentic palettes between songs.
const fn vibe_voice_pools(
    vibe: Vibe,
) -> (
    &'static [usize],
    &'static [usize],
    &'static [usize],
    &'static [usize],
    &'static [usize],
    &'static [usize],
) {
    match vibe {
        Vibe::DeepHouse => (&[0, 1, 5, 2], &[0, 1, 6, 3], &[0, 7, 3, 2], &[0, 2, 1], &[2, 1, 5], &[0, 3, 5]),
        Vibe::ElectroFunk => (&[2, 5, 0, 7], &[1, 6, 7], &[3, 7, 2, 5], &[2, 7, 4, 0], &[2, 0, 4], &[1, 0, 6]),
        Vibe::LofiChill => (&[3, 7, 2, 0], &[0, 1, 5, 3], &[7, 0, 2, 1], &[1, 2, 5, 0], &[2, 3, 5, 1], &[0, 4, 5, 7]),
        Vibe::ElectroSoul => (&[1, 2, 0, 7], &[1, 0, 6, 4], &[7, 3, 1, 0], &[2, 1, 3, 9], &[2, 1, 5], &[3, 0, 1]),
        Vibe::Synthwave => (&[1, 4, 5, 2], &[2, 6, 4, 1], &[3, 1, 5, 4], &[3, 2, 7, 4], &[2, 3, 1, 0], &[2, 6, 1]),
        Vibe::AcidClub => (&[1, 6, 2, 0], &[7, 1, 0, 3], &[2, 3, 0, 8], &[2, 7, 0, 9], &[0, 2, 4, 1], &[2, 1, 6]),
        Vibe::AmbientDub => (&[0, 3, 7, 2], &[3, 1, 0, 8], &[2, 7, 0, 1], &[2, 1, 6, 0], &[2, 3, 1, 5], &[2, 3, 5]),
        Vibe::UkGarage => (&[2, 7, 1], &[1, 6, 0, 3], &[2, 7, 3, 0], &[2, 0, 1], &[2, 1, 5], &[0, 3, 1]),
        Vibe::JungleDnb => (&[7, 1, 6, 2], &[3, 1, 9, 8], &[1, 2, 0, 4], &[2, 7, 1, 8], &[2, 3, 4, 0], &[2, 5, 3]),
        Vibe::DetroitTechno => (&[1, 6, 4, 2], &[0, 7, 1, 3], &[2, 3, 0, 1], &[0, 2, 7, 9], &[1, 2, 0, 4], &[2, 6, 1]),
        Vibe::DreamPop => (&[4, 3, 1, 5], &[0, 6, 5, 2], &[4, 7, 1, 0], &[5, 2, 6, 1], &[5, 2, 1, 3], &[5, 7, 3, 4]),
        Vibe::Disco70s => (&[5, 2, 0], &[6, 1, 5], &[3, 7, 4, 1], &[2, 6, 1, 7], &[2, 5, 1], &[5, 1, 0]),
        Vibe::Arena80s => (&[4, 1, 5, 2], &[4, 6, 2, 1], &[5, 3, 8, 7], &[4, 7, 2, 3], &[2, 4, 0, 5], &[6, 1, 2]),
        Vibe::IndieRock => (&[5, 7, 4, 2], &[5, 1, 4, 6], &[6, 7, 5, 3], &[5, 2, 4, 7], &[2, 4, 5], &[7, 6, 0]),
        Vibe::NeoSoul => (&[3, 2, 0], &[1, 0, 5], &[7, 3, 0, 1], &[2, 1, 5], &[2, 3, 5, 1], &[0, 7, 1]),
        Vibe::ItaloCosmic => (&[1, 4, 5, 2], &[6, 1, 2, 7], &[4, 3, 5, 2], &[7, 2, 3, 9], &[0, 2, 4], &[5, 1, 3, 6]),
        Vibe::BoomBap => (&[3, 7, 2, 0], &[0, 1, 8, 5], &[7, 3, 0, 1], &[2, 1, 5], &[2, 3, 5], &[0, 1, 7, 4]),
        Vibe::Trap808 => (&[8, 6, 2, 0], &[8, 1, 3, 0], &[0, 7, 9, 2], &[1, 2, 3, 9], &[2, 3, 4, 0], &[3, 2, 1]),
        Vibe::Dubstep => (&[9, 6, 8], &[9, 7, 3], &[8, 3, 5, 2], &[8, 7, 2, 4], &[4, 2, 0, 3], &[1, 2, 6]),
        Vibe::BigRoomEdm => (&[1, 6, 9, 2], &[10, 6, 4, 9], &[8, 3, 5, 9], &[9, 7, 4, 2], &[0, 2, 4, 5], &[2, 6, 1]),
        Vibe::FutureBass => (&[8, 1, 7, 2], &[8, 1, 0, 3], &[9, 7, 8, 0], &[1, 2, 9, 3], &[2, 5, 1, 3], &[3, 0, 1]),
        Vibe::EuphoricTrance => (&[1, 4, 6, 5], &[10, 6, 2, 0], &[8, 3, 1, 4], &[9, 2, 3, 6], &[1, 2, 5, 0], &[2, 5, 3]),
    }
}

/// Builds a fresh, genre-authentic `PartySongRig` for `song_cycle` without mutating
/// the user's saved `Project` state.
pub fn build_party_song_rig(
    base: &Project,
    song_vibe: Vibe,
    song_cycle: u32,
    seed: u32,
    sound_morph: bool,
) -> PartySongRig {
    let cur_palette = format!(
        "{} • {} • {} • {}",
        voice_preset_names(TrackId::Moog)[base.voice_presets[1] % 11],
        voice_preset_names(TrackId::MoogPad)[base.voice_presets[2] % 10],
        voice_preset_names(TrackId::Monologue)[base.voice_presets[3] % 10],
        voice_preset_names(TrackId::Drums)[base.voice_presets[0] % 10],
    );

    if !sound_morph || (song_cycle == 0 && song_vibe == base.vibe) {
        return PartySongRig {
            vibe: song_vibe,
            bpm: base.bpm,
            swing: base.swing,
            sidechain_amount: base.sidechain_amount,
            moog: base.moog.clone(),
            moog_pad: base.moog_pad.clone(),
            monologue: base.monologue.clone(),
            tb303: base.tb303.clone(),
            drums: base.drums.clone(),
            sampler: base.sampler.clone(),
            sampler_builtin_id: base.sample_data.builtin_id,
            delay_div: base.fx.delay_div,
            reverb_mix: base.fx.reverb_mix,
            delay_mix: base.fx.delay_mix,
            reverb_size: base.fx.reverb_size,
            analog_warmth: base.fx.analog_warmth,
            enable_303: if sound_morph { true } else { !base.mixer.channels[2].mute },
            enable_sampler: if sound_morph { true } else { !base.mixer.channels[4].mute },
            drums_vol: if sound_morph {
                base.mixer.channels[3].volume.min(0.72)
            } else {
                base.mixer.channels[3].volume
            },
            bass_vol: if sound_morph {
                base.mixer.channels[0].volume.min(0.66)
            } else {
                base.mixer.channels[0].volume
            },
            pad_vol: if sound_morph {
                base.mixer.moog_pad_volume.max(0.84)
            } else {
                base.mixer.moog_pad_volume
            },
            lead_vol: if sound_morph {
                base.mixer.channels[1].volume.max(0.84)
            } else {
                base.mixer.channels[1].volume
            },
            tb303_vol: if sound_morph {
                base.mixer.channels[2].volume.max(0.68)
            } else {
                base.mixer.channels[2].volume
            },
            sampler_vol: if sound_morph {
                base.mixer.channels[4].volume.max(0.74)
            } else {
                base.mixer.channels[4].volume
            },
            palette_label: cur_palette,
        };
    }

    let mut rng = Rng::new(
        seed ^ song_cycle.wrapping_mul(0x9E37_79B9) ^ (song_vibe as u32).wrapping_mul(0x517C_C1B7),
    );
    let mut tmp = Project::factory(FactoryPreset::BlankInit);
    let (drums_p, moog_p, pad_p, mono_p, tb_p, samp_p) = vibe_voice_pools(song_vibe);

    let d_idx = drums_p[(song_cycle as usize + rng.range(drums_p.len())) % drums_p.len()];
    let b_idx = moog_p[(song_cycle as usize + rng.range(moog_p.len())) % moog_p.len()];
    let p_idx = pad_p[(song_cycle as usize + rng.range(pad_p.len())) % pad_p.len()];
    let m_idx = mono_p[(song_cycle as usize + rng.range(mono_p.len())) % mono_p.len()];
    let t_idx = tb_p[(song_cycle as usize + rng.range(tb_p.len())) % tb_p.len()];
    let s_idx = samp_p[(song_cycle as usize + rng.range(samp_p.len())) % samp_p.len()];

    apply_voice_preset(&mut tmp, TrackId::Drums, d_idx);
    apply_voice_preset(&mut tmp, TrackId::Moog, b_idx);
    apply_voice_preset(&mut tmp, TrackId::MoogPad, p_idx);
    apply_voice_preset(&mut tmp, TrackId::Monologue, m_idx);
    apply_voice_preset(&mut tmp, TrackId::Tb303, t_idx);
    apply_voice_preset(&mut tmp, TrackId::Sampler, s_idx);

    // Subtle tasteful per-song analog sculpting so every song feels freshly mixed
    tmp.drums.voices[0].tune = (tmp.drums.voices[0].tune + (rng.f32() - 0.5) * 3.0).clamp(-12.0, 12.0);
    tmp.drums.voices[1].snappy = (tmp.drums.voices[1].snappy + (rng.f32() - 0.5) * 0.20).clamp(0.1, 1.0);
    tmp.moog.cutoff = (tmp.moog.cutoff * (0.85 + rng.f32() * 0.35)).clamp(180.0, 14_000.0);
    tmp.moog_pad.cutoff = (tmp.moog_pad.cutoff * (0.85 + rng.f32() * 0.35)).clamp(260.0, 15_000.0);
    tmp.monologue.cutoff = (tmp.monologue.cutoff * (0.85 + rng.f32() * 0.35)).clamp(320.0, 16_000.0);
    tmp.tb303.cutoff = (tmp.tb303.cutoff * (0.82 + rng.f32() * 0.45)).clamp(140.0, 7_000.0);
    tmp.tb303.resonance = (tmp.tb303.resonance + (rng.f32() - 0.5) * 0.16).clamp(0.25, 0.90);

    const BPM_DELTAS: [f32; 5] = [-4.0, -2.0, 0.0, 2.0, 4.0];
    let target_bpm = if song_vibe == base.vibe {
        (base.bpm + BPM_DELTAS[rng.range(BPM_DELTAS.len())]).clamp(68.0, 182.0)
    } else {
        (song_vibe.default_bpm() + (rng.range(5) as f32 - 2.0)).clamp(68.0, 182.0)
    };
    let swing_floor = if matches!(
        song_vibe,
        Vibe::ElectroFunk
            | Vibe::DeepHouse
            | Vibe::LofiChill
            | Vibe::UkGarage
            | Vibe::Disco70s
            | Vibe::NeoSoul
            | Vibe::BoomBap
    ) {
        0.14
    } else {
        0.08
    };
    let swing = (song_vibe.default_swing().max(swing_floor) + (rng.f32() - 0.5) * 0.04).clamp(0.06, 0.34);

    let sidechain_amount = match song_vibe {
        Vibe::DeepHouse | Vibe::AcidClub | Vibe::BigRoomEdm | Vibe::FutureBass | Vibe::EuphoricTrance => {
            0.58 + rng.f32() * 0.18
        }
        Vibe::AmbientDub | Vibe::LofiChill | Vibe::NeoSoul | Vibe::IndieRock => 0.25 + rng.f32() * 0.15,
        _ => 0.40 + rng.f32() * 0.20,
    };

    const DELAY_DIVS: [DelayDivision; 3] = [
        DelayDivision::DottedEighth,
        DelayDivision::Eighth,
        DelayDivision::Quarter,
    ];
    let delay_div = DELAY_DIVS[(song_cycle as usize + rng.range(DELAY_DIVS.len())) % DELAY_DIVS.len()];
    let reverb_mix = if matches!(song_vibe, Vibe::AmbientDub) {
        0.46
    } else if matches!(
        song_vibe,
        Vibe::Arena80s | Vibe::Disco70s | Vibe::EuphoricTrance | Vibe::BigRoomEdm | Vibe::FutureBass | Vibe::DreamPop
    ) {
        0.26 + rng.f32() * 0.08
    } else {
        0.20 + rng.f32() * 0.08
    };
    let delay_mix = if matches!(song_vibe, Vibe::AmbientDub) {
        0.44
    } else if matches!(song_vibe, Vibe::EuphoricTrance | Vibe::DreamPop | Vibe::ItaloCosmic) {
        0.26 + rng.f32() * 0.06
    } else {
        0.18 + rng.f32() * 0.06
    };
    let reverb_size = if matches!(song_vibe, Vibe::AmbientDub) {
        0.84
    } else if matches!(
        song_vibe,
        Vibe::Synthwave | Vibe::ElectroSoul | Vibe::Arena80s | Vibe::Disco70s | Vibe::EuphoricTrance | Vibe::BigRoomEdm | Vibe::FutureBass
    ) {
        0.66 + rng.f32() * 0.12
    } else {
        0.50 + rng.f32() * 0.12
    };
    let analog_warmth = if matches!(
        song_vibe,
        Vibe::LofiChill | Vibe::ElectroFunk | Vibe::NeoSoul | Vibe::IndieRock | Vibe::BoomBap | Vibe::Dubstep
    ) {
        0.34 + rng.f32() * 0.12
    } else {
        0.18 + rng.f32() * 0.10
    };

    let drums_vol = 0.70;
    let bass_vol = if matches!(song_vibe, Vibe::Dubstep | Vibe::Trap808) {
        0.68
    } else {
        0.62
    };
    let pad_vol = if matches!(
        song_vibe,
        Vibe::Arena80s | Vibe::Disco70s | Vibe::BigRoomEdm | Vibe::FutureBass | Vibe::EuphoricTrance
    ) {
        0.90
    } else {
        0.86
    };
    let lead_vol = if matches!(
        song_vibe,
        Vibe::ElectroFunk | Vibe::Arena80s | Vibe::Synthwave | Vibe::ItaloCosmic | Vibe::BigRoomEdm | Vibe::EuphoricTrance
    ) {
        0.90
    } else {
        0.84
    };
    let tb303_vol = if matches!(
        song_vibe,
        Vibe::AcidClub | Vibe::ElectroFunk | Vibe::ItaloCosmic | Vibe::DetroitTechno
    ) {
        0.76
    } else {
        0.66
    };
    let sampler_vol = if matches!(
        song_vibe,
        Vibe::LofiChill | Vibe::NeoSoul | Vibe::BoomBap | Vibe::Disco70s | Vibe::Arena80s | Vibe::IndieRock | Vibe::ElectroFunk
    ) {
        0.84
    } else {
        0.74
    };

    let palette_label = format!(
        "{} • {} • {} • {}",
        voice_preset_names(TrackId::Moog)[b_idx % 11],
        voice_preset_names(TrackId::MoogPad)[p_idx % 10],
        voice_preset_names(TrackId::Monologue)[m_idx % 10],
        voice_preset_names(TrackId::Drums)[d_idx % 10],
    );

    PartySongRig {
        vibe: song_vibe,
        bpm: target_bpm,
        swing,
        sidechain_amount,
        moog: tmp.moog,
        moog_pad: tmp.moog_pad,
        monologue: tmp.monologue,
        tb303: tmp.tb303,
        drums: tmp.drums,
        sampler: tmp.sampler,
        sampler_builtin_id: if base.sample_data.builtin_id.is_some() {
            tmp.sample_data.builtin_id
        } else {
            None
        },
        delay_div,
        reverb_mix,
        delay_mix,
        reverb_size,
        analog_warmth,
        enable_303: true,
        enable_sampler: true,
        drums_vol,
        bass_vol,
        pad_vol,
        lead_vol,
        tb303_vol,
        sampler_vol,
        palette_label,
    }
}

/// Returns the human-readable chord symbol (e.g. "Am9", "Fmaj9").
pub fn chord_symbol(root_pc: u8, slot: ChordSlot) -> String {
    let pc = (root_pc + slot.offset) % 12;
    let suffix = match slot.chord {
        ChordType::Single => "",
        ChordType::Power5 => "5",
        ChordType::Major => "",
        ChordType::Minor => "m",
        ChordType::Sus2 => "sus2",
        ChordType::Sus4 => "sus4",
        ChordType::Aug => "aug",
        ChordType::Major6 => "6",
        ChordType::Minor6 => "m6",
        ChordType::Major7 => "maj7",
        ChordType::Minor7 => "m7",
        ChordType::Dom7 => "7",
        ChordType::Dom7Sus4 => "7sus4",
        ChordType::Dim7 => "dim7",
        ChordType::HalfDim7 => "m7b5",
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
            ChordSlot { offset: 9, chord: ChordType::HalfDim7 },
            ChordSlot { offset: 10, chord: ChordType::Add9 },
            ChordSlot { offset: 0, chord: ChordType::Minor11 },
        ],
        Scale::Mixolydian => [
            ChordSlot { offset: 0, chord: ChordType::Dom7 },
            ChordSlot { offset: 2, chord: ChordType::Minor7 },
            ChordSlot { offset: 4, chord: ChordType::HalfDim7 },
            ChordSlot { offset: 5, chord: ChordType::Major6 },
            ChordSlot { offset: 7, chord: ChordType::Minor7 },
            ChordSlot { offset: 9, chord: ChordType::Minor9 },
            ChordSlot { offset: 10, chord: ChordType::Add9 },
            ChordSlot { offset: 0, chord: ChordType::Power5 },
        ],
        Scale::Lydian => [
            ChordSlot { offset: 0, chord: ChordType::Major9 },
            ChordSlot { offset: 2, chord: ChordType::Add9 },
            ChordSlot { offset: 4, chord: ChordType::Minor7 },
            ChordSlot { offset: 6, chord: ChordType::HalfDim7 },
            ChordSlot { offset: 7, chord: ChordType::Major7 },
            ChordSlot { offset: 9, chord: ChordType::Minor9 },
            ChordSlot { offset: 11, chord: ChordType::Minor7 },
            ChordSlot { offset: 0, chord: ChordType::Major6 },
        ],
        Scale::HarmonicMinor => [
            ChordSlot { offset: 0, chord: ChordType::Minor6 },
            ChordSlot { offset: 2, chord: ChordType::HalfDim7 },
            ChordSlot { offset: 3, chord: ChordType::Aug },
            ChordSlot { offset: 5, chord: ChordType::Minor7 },
            ChordSlot { offset: 7, chord: ChordType::Dom7 },
            ChordSlot { offset: 8, chord: ChordType::Major7 },
            ChordSlot { offset: 11, chord: ChordType::Dim7 },
            ChordSlot { offset: 7, chord: ChordType::Dom7Sus4 },
        ],
        Scale::Blues => [
            ChordSlot { offset: 0, chord: ChordType::Dom7 },
            ChordSlot { offset: 3, chord: ChordType::Major6 },
            ChordSlot { offset: 5, chord: ChordType::Dom9 },
            ChordSlot { offset: 6, chord: ChordType::Dim7 },
            ChordSlot { offset: 7, chord: ChordType::Dom9 },
            ChordSlot { offset: 10, chord: ChordType::Dom7 },
            ChordSlot { offset: 0, chord: ChordType::Power5 },
            ChordSlot { offset: 5, chord: ChordType::Power5 },
        ],
        Scale::MajorPentatonic => [
            ChordSlot { offset: 0, chord: ChordType::Major9 },
            ChordSlot { offset: 2, chord: ChordType::Minor7 },
            ChordSlot { offset: 4, chord: ChordType::Minor7 },
            ChordSlot { offset: 5, chord: ChordType::Add9 },
            ChordSlot { offset: 7, chord: ChordType::Dom7Sus4 },
            ChordSlot { offset: 9, chord: ChordType::Minor9 },
            ChordSlot { offset: 0, chord: ChordType::Major6 },
            ChordSlot { offset: 0, chord: ChordType::Power5 },
        ],
        _ => [
            // Minor / Pentatonic / Acid / Phrygian — lush minor-key palette
            ChordSlot { offset: 0, chord: ChordType::Minor9 },
            ChordSlot { offset: 3, chord: ChordType::Major9 },
            ChordSlot { offset: 5, chord: ChordType::Minor9 },
            ChordSlot { offset: 7, chord: ChordType::Minor7 },
            ChordSlot { offset: 8, chord: ChordType::Major9 },
            ChordSlot { offset: 10, chord: ChordType::Add9 },
            ChordSlot { offset: 7, chord: ChordType::Dom7Sus4 },
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
        TrackId::Drums => &[
            "808 Warm",
            "909 Club",
            "Funk Electro",
            "Lo-Fi Dusty",
            "80s Linn Gate",
            "70s Disco Crisp",
            "Industrial Hard",
            "Breakbeat Crunch",
            "Trap 808 Boom",
            "Dubstep Half-Time",
        ],
        TrackId::Moog => &[
            "Warm Sub",
            "Funk Slap",
            "Synthwave 8th",
            "Deep Reese",
            "OB-X Power Bass",
            "Electric Bass Pick",
            "70s Disco Octave",
            "Acid Moog Growl",
            "Trap 808 Sub",
            "Dubstep Wobble",
            "Trance Rolling Bass",
        ],
        TrackId::MoogPad => &[
            "Lush Pad",
            "Warm Strings",
            "Dub Chord",
            "Neon Brass",
            "70s String Synth",
            "OB-X 80s Power",
            "Electric Guitar",
            "Wurlitzer Keys",
            "Festival Supersaw",
            "Future Bass Saw",
        ],
        TrackId::Monologue => &[
            "Pluck Arp",
            "Sparkle Bell",
            "Funk Clav",
            "Glide Lead",
            "OB-X Power Lead",
            "Electric Guitar Lead",
            "70s String Lead",
            "Sync Screamer",
            "Dubstep Growl",
            "Trance Pluck",
        ],
        TrackId::Tb303 => &[
            "Acid Squelch",
            "Liquid 303",
            "Funky Acid",
            "Deep Sub 303",
            "Distorted 303",
            "Melodic Pluck 303",
        ],
        TrackId::Sampler => &[
            "Minor9 Rhodes",
            "Brass Stab",
            "Choir Ahh",
            "Retro FM Bell",
            "Toy Piano",
            "70s Solina String",
            "OB-X Power Chord",
            "Electric Guitar",
        ],
        TrackId::MidiOut => &["Off"],
    }
}

/// Applies one of the quick voice presets to the project's synth/drum/sampler params.
pub fn apply_voice_preset(p: &mut Project, track: TrackId, preset_idx: usize) {
    match track {
        TrackId::Drums => {
            p.voice_presets[0] = preset_idx % 10;
            p.drums.voices[3].level = 0.34;
            p.drums.voices[3].decay = 0.24;
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
                    p.drums.voices[3].model = DrumModel::Tr909;
                    p.drums.voices[3].level = 0.32;
                    p.drums.voices[4].decay = 0.12;
                    p.drums.voices[4].level = 0.68;
                    p.drums.voices[5].decay = 0.38;
                }
                2 => {
                    // Funk Electro (tight snappy punch + tamed cowbell pocket)
                    p.drums.set_hybrid();
                    p.drums.voices[0].tune = 0.0;
                    p.drums.voices[0].decay = 0.46;
                    p.drums.voices[0].tone = 0.56;
                    p.drums.voices[0].level = 0.98;
                    p.drums.voices[1].snappy = 0.85;
                    p.drums.voices[1].decay = 0.28;
                    p.drums.voices[2].level = 0.80;
                    p.drums.voices[3].level = 0.38;
                    p.drums.voices[3].decay = 0.22;
                    p.drums.voices[4].decay = 0.11;
                    p.drums.voices[4].level = 0.66;
                }
                3 => {
                    // Lo-Fi Dusty
                    p.drums.set_all_model(DrumModel::Tr808);
                    p.drums.voices[0].tune = -3.0;
                    p.drums.voices[0].decay = 0.52;
                    p.drums.voices[0].tone = 0.28;
                    p.drums.voices[0].level = 0.90;
                    p.drums.voices[1].snappy = 0.45;
                    p.drums.voices[1].tone = 0.38;
                    p.drums.voices[1].decay = 0.26;
                    p.drums.voices[3].model = DrumModel::Tr909;
                    p.drums.voices[3].level = 0.32;
                    p.drums.voices[4].tone = 0.42;
                    p.drums.voices[4].decay = 0.12;
                    p.drums.voices[4].level = 0.54;
                }
                4 => {
                    // 80s Linn Gate (huge punchy 80s rock/synthpop drums)
                    p.drums.set_all_model(DrumModel::Tr909);
                    p.drums.voices[0].tune = -1.5;
                    p.drums.voices[0].decay = 0.56;
                    p.drums.voices[0].tone = 0.64;
                    p.drums.voices[0].level = 1.0;
                    p.drums.voices[1].tune = -2.0;
                    p.drums.voices[1].snappy = 0.92;
                    p.drums.voices[1].decay = 0.46;
                    p.drums.voices[1].tone = 0.72;
                    p.drums.voices[2].level = 0.86;
                    p.drums.voices[4].decay = 0.13;
                    p.drums.voices[5].decay = 0.44;
                    p.drums.voices[6].decay = 0.54;
                    p.drums.voices[6].tune = -3.0;
                }
                5 => {
                    // 70s Disco Crisp (tight 4-on-the-floor kick & sizzling open hats)
                    p.drums.set_hybrid();
                    p.drums.voices[0].tune = 0.5;
                    p.drums.voices[0].decay = 0.38;
                    p.drums.voices[0].tone = 0.52;
                    p.drums.voices[0].level = 0.96;
                    p.drums.voices[1].tune = 1.5;
                    p.drums.voices[1].snappy = 0.84;
                    p.drums.voices[1].decay = 0.24;
                    p.drums.voices[2].level = 0.84;
                    p.drums.voices[3].model = DrumModel::Tr909;
                    p.drums.voices[3].level = 0.32;
                    p.drums.voices[4].decay = 0.09;
                    p.drums.voices[4].tone = 0.82;
                    p.drums.voices[5].decay = 0.34;
                    p.drums.voices[5].level = 0.76;
                }
                6 => {
                    // Industrial Hard (aggressive tuned 909 punch)
                    p.drums.set_all_model(DrumModel::Tr909);
                    p.drums.voices[0].tune = -4.0;
                    p.drums.voices[0].decay = 0.68;
                    p.drums.voices[0].tone = 0.88;
                    p.drums.voices[0].level = 1.0;
                    p.drums.voices[1].tune = -1.0;
                    p.drums.voices[1].snappy = 0.96;
                    p.drums.voices[1].decay = 0.36;
                    p.drums.voices[4].tone = 0.92;
                    p.drums.voices[4].decay = 0.10;
                    p.drums.voices[5].decay = 0.50;
                }
                7 => {
                    // Breakbeat Crunch (tight live-kit feel)
                    p.drums.set_hybrid();
                    p.drums.voices[0].tune = 1.0;
                    p.drums.voices[0].decay = 0.34;
                    p.drums.voices[0].tone = 0.62;
                    p.drums.voices[0].level = 0.95;
                    p.drums.voices[1].tune = 2.0;
                    p.drums.voices[1].snappy = 0.88;
                    p.drums.voices[1].decay = 0.22;
                    p.drums.voices[3].model = DrumModel::Tr909;
                    p.drums.voices[3].level = 0.32;
                    p.drums.voices[4].decay = 0.08;
                    p.drums.voices[4].level = 0.66;
                    p.drums.voices[5].decay = 0.28;
                    p.drums.voices[7].level = 0.72;
                }
                8 => {
                    // Trap 808 Boom (massive subby 808 kick, crisp tight snare/clap & bright hats)
                    p.drums.set_all_model(DrumModel::Tr808);
                    p.drums.voices[0].tune = -3.0;
                    p.drums.voices[0].decay = 0.78;
                    p.drums.voices[0].tone = 0.56;
                    p.drums.voices[0].level = 1.0;
                    p.drums.voices[1].tune = 3.0;
                    p.drums.voices[1].snappy = 0.94;
                    p.drums.voices[1].decay = 0.18;
                    p.drums.voices[1].level = 0.92;
                    p.drums.voices[2].tone = 0.78;
                    p.drums.voices[2].decay = 0.20;
                    p.drums.voices[2].level = 0.88;
                    p.drums.voices[4].tone = 0.86;
                    p.drums.voices[4].decay = 0.06;
                    p.drums.voices[4].level = 0.74;
                    p.drums.voices[5].decay = 0.30;
                    p.drums.voices[5].level = 0.68;
                }
                _ => {
                    // Dubstep Half-Time (heavy 909/808 sub-punch kick, massive cracking snare & dark hats)
                    p.drums.set_hybrid();
                    p.drums.voices[0].model = DrumModel::Tr909;
                    p.drums.voices[0].tune = -2.5;
                    p.drums.voices[0].decay = 0.58;
                    p.drums.voices[0].tone = 0.74;
                    p.drums.voices[0].level = 1.0;
                    p.drums.voices[1].model = DrumModel::Tr909;
                    p.drums.voices[1].tune = -1.0;
                    p.drums.voices[1].snappy = 0.98;
                    p.drums.voices[1].decay = 0.38;
                    p.drums.voices[1].level = 0.98;
                    p.drums.voices[2].level = 0.90;
                    p.drums.voices[2].decay = 0.28;
                    p.drums.voices[4].tone = 0.72;
                    p.drums.voices[4].decay = 0.09;
                    p.drums.voices[5].decay = 0.44;
                    p.drums.voices[7].level = 0.76;
                }
            }
        }
        TrackId::Moog => {
            p.voice_presets[1] = preset_idx % 11;
            p.moog.poly_mode = false;
            p.moog.noise_level = 0.0;
            p.moog.lfo_to_pitch = 0.0;
            p.moog.lfo_to_filter = 0.0;
            p.moog.osc3_octave = -1;
            p.moog.osc3_detune = -5.0;
            p.moog.osc3_wave = MoogWave::Triangle;
            p.moog.filt_attack = 0.004;
            match p.voice_presets[1] {
                0 => {
                    // Warm Sub (rubbery finger-bass thump)
                    p.moog.osc1_wave = MoogWave::Triangle;
                    p.moog.osc2_wave = MoogWave::Saw;
                    p.moog.osc2_octave = 0;
                    p.moog.osc2_detune = 5.0;
                    p.moog.osc1_level = 0.95;
                    p.moog.osc2_level = 0.56;
                    p.moog.osc3_level = 0.34;
                    p.moog.cutoff = 540.0;
                    p.moog.resonance = 0.34;
                    p.moog.env_amount = 0.66;
                    p.moog.drive = 0.34;
                    p.moog.filt_decay = 0.20;
                    p.moog.filt_sustain = 0.18;
                    p.moog.amp_attack = 0.004;
                    p.moog.amp_decay = 0.28;
                    p.moog.amp_sustain = 0.62;
                    p.moog.amp_release = 0.12;
                    p.moog.glide = 0.04;
                }
                1 => {
                    // Funk Slap (snappy Mutron auto-wah thumb slap!)
                    p.moog.osc1_wave = MoogWave::Saw;
                    p.moog.osc2_wave = MoogWave::Square;
                    p.moog.osc2_octave = -1;
                    p.moog.osc2_detune = 7.0;
                    p.moog.osc1_level = 0.88;
                    p.moog.osc2_level = 0.74;
                    p.moog.osc3_level = 0.24;
                    p.moog.cutoff = 440.0;
                    p.moog.resonance = 0.64;
                    p.moog.env_amount = 0.88;
                    p.moog.drive = 0.56;
                    p.moog.filt_attack = 0.002;
                    p.moog.filt_decay = 0.16;
                    p.moog.filt_sustain = 0.10;
                    p.moog.amp_attack = 0.002;
                    p.moog.amp_decay = 0.20;
                    p.moog.amp_sustain = 0.40;
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
                3 => {
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
                4 => {
                    // OB-X Power Bass (massive 80s Oberheim arena unison bass)
                    p.moog.osc1_wave = MoogWave::Saw;
                    p.moog.osc2_wave = MoogWave::Saw;
                    p.moog.osc3_wave = MoogWave::WidePulse;
                    p.moog.osc2_octave = 0;
                    p.moog.osc3_octave = -1;
                    p.moog.osc2_detune = 15.0;
                    p.moog.osc3_detune = -11.0;
                    p.moog.osc1_level = 0.88;
                    p.moog.osc2_level = 0.84;
                    p.moog.osc3_level = 0.65;
                    p.moog.cutoff = 880.0;
                    p.moog.resonance = 0.26;
                    p.moog.env_amount = 0.66;
                    p.moog.drive = 0.48;
                    p.moog.filt_attack = 0.004;
                    p.moog.filt_decay = 0.28;
                    p.moog.filt_sustain = 0.34;
                    p.moog.amp_attack = 0.004;
                    p.moog.amp_decay = 0.32;
                    p.moog.amp_sustain = 0.72;
                    p.moog.amp_release = 0.16;
                    p.moog.glide = 0.03;
                }
                5 => {
                    // Electric Bass Pick (thick, warm picked Fender P-Bass!)
                    p.moog.osc1_wave = MoogWave::TriSaw;
                    p.moog.osc2_wave = MoogWave::Triangle;
                    p.moog.osc3_wave = MoogWave::Saw;
                    p.moog.osc2_octave = 0;
                    p.moog.osc3_octave = -1;
                    p.moog.osc2_detune = 3.0;
                    p.moog.osc3_detune = -3.0;
                    p.moog.osc1_level = 0.88;
                    p.moog.osc2_level = 0.80;
                    p.moog.osc3_level = 0.36;
                    p.moog.noise_level = 0.03;
                    p.moog.cutoff = 520.0;
                    p.moog.resonance = 0.22;
                    p.moog.env_amount = 0.52;
                    p.moog.drive = 0.44;
                    p.moog.filt_attack = 0.002;
                    p.moog.filt_decay = 0.16;
                    p.moog.filt_sustain = 0.26;
                    p.moog.amp_attack = 0.003;
                    p.moog.amp_decay = 0.38;
                    p.moog.amp_sustain = 0.52;
                    p.moog.amp_release = 0.14;
                    p.moog.glide = 0.01;
                }
                6 => {
                    // 70s Disco Octave (snappy Giorgio Moroder style Moog bass)
                    p.moog.osc1_wave = MoogWave::Saw;
                    p.moog.osc2_wave = MoogWave::Square;
                    p.moog.osc3_wave = MoogWave::Saw;
                    p.moog.osc2_octave = 0;
                    p.moog.osc3_octave = -1;
                    p.moog.osc2_detune = 6.0;
                    p.moog.osc1_level = 0.90;
                    p.moog.osc2_level = 0.65;
                    p.moog.osc3_level = 0.45;
                    p.moog.cutoff = 480.0;
                    p.moog.resonance = 0.40;
                    p.moog.env_amount = 0.74;
                    p.moog.drive = 0.30;
                    p.moog.filt_attack = 0.002;
                    p.moog.filt_decay = 0.15;
                    p.moog.filt_sustain = 0.10;
                    p.moog.amp_attack = 0.002;
                    p.moog.amp_decay = 0.18;
                    p.moog.amp_sustain = 0.36;
                    p.moog.amp_release = 0.08;
                    p.moog.glide = 0.02;
                }
                7 => {
                    // Acid Moog Growl (high-resonance saturated ladder bass)
                    p.moog.osc1_wave = MoogWave::Saw;
                    p.moog.osc2_wave = MoogWave::NarrowPulse;
                    p.moog.osc2_octave = 0;
                    p.moog.osc2_detune = 9.0;
                    p.moog.osc1_level = 0.92;
                    p.moog.osc2_level = 0.74;
                    p.moog.osc3_level = 0.28;
                    p.moog.cutoff = 320.0;
                    p.moog.resonance = 0.72;
                    p.moog.env_amount = 0.84;
                    p.moog.drive = 0.68;
                    p.moog.filt_attack = 0.002;
                    p.moog.filt_decay = 0.22;
                    p.moog.filt_sustain = 0.14;
                    p.moog.amp_attack = 0.002;
                    p.moog.amp_decay = 0.24;
                    p.moog.amp_sustain = 0.50;
                    p.moog.amp_release = 0.10;
                    p.moog.glide = 0.07;
                }
                8 => {
                    // Trap 808 Sub (booming saturated triangle sub with smooth glide for 808 slides)
                    p.moog.osc1_wave = MoogWave::Triangle;
                    p.moog.osc2_wave = MoogWave::Triangle;
                    p.moog.osc2_octave = 0;
                    p.moog.osc2_detune = 1.5;
                    p.moog.osc1_level = 1.0;
                    p.moog.osc2_level = 0.45;
                    p.moog.osc3_level = 0.35;
                    p.moog.cutoff = 260.0;
                    p.moog.resonance = 0.14;
                    p.moog.env_amount = 0.36;
                    p.moog.drive = 0.46;
                    p.moog.filt_attack = 0.002;
                    p.moog.filt_decay = 0.14;
                    p.moog.filt_sustain = 0.22;
                    p.moog.amp_attack = 0.002;
                    p.moog.amp_decay = 0.75;
                    p.moog.amp_sustain = 0.72;
                    p.moog.amp_release = 0.28;
                    p.moog.glide = 0.095;
                }
                9 => {
                    // Dubstep Wobble (heavy overdriven ladder bass with 140 BPM 1/8th-note LFO wobble!)
                    p.moog.osc1_wave = MoogWave::Saw;
                    p.moog.osc2_wave = MoogWave::Square;
                    p.moog.osc2_octave = 0;
                    p.moog.osc2_detune = 14.0;
                    p.moog.osc1_level = 0.96;
                    p.moog.osc2_level = 0.84;
                    p.moog.osc3_level = 0.62;
                    p.moog.cutoff = 520.0;
                    p.moog.resonance = 0.58;
                    p.moog.env_amount = 0.34;
                    p.moog.drive = 0.78;
                    p.moog.lfo_rate = 4.667; // Exact 1/8th-note wobble at 140 BPM!
                    p.moog.lfo_to_filter = 0.88;
                    p.moog.filt_attack = 0.002;
                    p.moog.filt_decay = 0.32;
                    p.moog.filt_sustain = 0.48;
                    p.moog.amp_attack = 0.003;
                    p.moog.amp_decay = 0.45;
                    p.moog.amp_sustain = 0.88;
                    p.moog.amp_release = 0.12;
                    p.moog.glide = 0.06;
                }
                _ => {
                    // Trance Rolling Bass (tight punchy 16th-note rolling saw/pulse bass)
                    p.moog.osc1_wave = MoogWave::Saw;
                    p.moog.osc2_wave = MoogWave::WidePulse;
                    p.moog.osc2_octave = 0;
                    p.moog.osc2_detune = 6.0;
                    p.moog.osc1_level = 0.94;
                    p.moog.osc2_level = 0.68;
                    p.moog.osc3_level = 0.46;
                    p.moog.cutoff = 380.0;
                    p.moog.resonance = 0.24;
                    p.moog.env_amount = 0.68;
                    p.moog.drive = 0.34;
                    p.moog.filt_attack = 0.001;
                    p.moog.filt_decay = 0.11;
                    p.moog.filt_sustain = 0.08;
                    p.moog.amp_attack = 0.001;
                    p.moog.amp_decay = 0.14;
                    p.moog.amp_sustain = 0.34;
                    p.moog.amp_release = 0.06;
                    p.moog.glide = 0.01;
                }
            }
        }
        TrackId::MoogPad => {
            p.voice_presets[2] = preset_idx % 10;
            p.moog_pad.poly_mode = true;
            p.moog_pad.noise_level = 0.0;
            p.moog_pad.lfo_to_pitch = 0.0;
            p.moog_pad.lfo_to_filter = 0.0;
            p.moog_pad.lfo_rate = 3.5;
            p.moog_pad.osc2_octave = 0;
            p.moog_pad.osc3_octave = -1;
            p.moog_pad.osc3_detune = -5.0;
            p.moog_pad.osc3_wave = MoogWave::Triangle;
            p.moog_pad.key_track = 0.45;
            p.moog_pad.glide = 0.0;
            match p.voice_presets[2] {
                0 => {
                    // Lush Pad (animated breathing chords)
                    p.moog_pad.osc1_wave = MoogWave::TriSaw;
                    p.moog_pad.osc2_wave = MoogWave::Saw;
                    p.moog_pad.osc2_detune = 9.0;
                    p.moog_pad.osc1_level = 0.78;
                    p.moog_pad.osc2_level = 0.64;
                    p.moog_pad.osc3_level = 0.26;
                    p.moog_pad.cutoff = 2100.0;
                    p.moog_pad.resonance = 0.26;
                    p.moog_pad.env_amount = 0.46;
                    p.moog_pad.drive = 0.22;
                    p.moog_pad.lfo_rate = 3.4;
                    p.moog_pad.lfo_to_filter = 0.16;
                    p.moog_pad.amp_attack = 0.03;
                    p.moog_pad.amp_decay = 0.50;
                    p.moog_pad.amp_sustain = 0.70;
                    p.moog_pad.amp_release = 0.70;
                    p.moog_pad.filt_attack = 0.02;
                    p.moog_pad.filt_decay = 0.55;
                    p.moog_pad.filt_sustain = 0.36;
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
                    p.moog_pad.osc1_level = 0.76;
                    p.moog_pad.osc2_level = 0.66;
                    p.moog_pad.osc3_level = 0.22;
                    p.moog_pad.cutoff = 1050.0;
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
                3 => {
                    // Neon Brass / Funk Stab
                    p.moog_pad.osc1_wave = MoogWave::Saw;
                    p.moog_pad.osc2_wave = MoogWave::WidePulse;
                    p.moog_pad.osc2_detune = 10.0;
                    p.moog_pad.osc1_level = 0.78;
                    p.moog_pad.osc2_level = 0.70;
                    p.moog_pad.osc3_level = 0.28;
                    p.moog_pad.cutoff = 2200.0;
                    p.moog_pad.resonance = 0.36;
                    p.moog_pad.env_amount = 0.74;
                    p.moog_pad.drive = 0.36;
                    p.moog_pad.filt_attack = 0.008;
                    p.moog_pad.filt_decay = 0.26;
                    p.moog_pad.filt_sustain = 0.30;
                    p.moog_pad.amp_attack = 0.010;
                    p.moog_pad.amp_decay = 0.28;
                    p.moog_pad.amp_sustain = 0.46;
                    p.moog_pad.amp_release = 0.22;
                }
                4 => {
                    // 70s String Synth (warm silky Solina / ARP string ensemble!)
                    p.moog_pad.osc1_wave = MoogWave::Saw;
                    p.moog_pad.osc2_wave = MoogWave::TriSaw;
                    p.moog_pad.osc3_wave = MoogWave::Saw;
                    p.moog_pad.osc2_octave = 0;
                    p.moog_pad.osc3_octave = -1; // Warm cello foundation (no shrill +1 octave!)
                    p.moog_pad.osc2_detune = 8.0;
                    p.moog_pad.osc3_detune = -7.0;
                    p.moog_pad.osc1_level = 0.72;
                    p.moog_pad.osc2_level = 0.68;
                    p.moog_pad.osc3_level = 0.44;
                    p.moog_pad.cutoff = 2150.0;
                    p.moog_pad.resonance = 0.14;
                    p.moog_pad.env_amount = 0.18;
                    p.moog_pad.key_track = 0.45;
                    p.moog_pad.drive = 0.12;
                    p.moog_pad.lfo_rate = 5.4;
                    p.moog_pad.lfo_to_pitch = 0.016; // Subtle ~2.4 cent silky vibrato (in tune!)
                    p.moog_pad.lfo_to_filter = 0.06;
                    p.moog_pad.filt_attack = 0.22;
                    p.moog_pad.filt_decay = 0.85;
                    p.moog_pad.filt_sustain = 0.68;
                    p.moog_pad.filt_release = 1.10;
                    p.moog_pad.amp_attack = 0.24;
                    p.moog_pad.amp_decay = 0.60;
                    p.moog_pad.amp_sustain = 0.84;
                    p.moog_pad.amp_release = 1.15;
                }
                5 => {
                    // OB-X 80s Power (fat in-tune Oberheim OB-X/OB-Xa arena brass!)
                    p.moog_pad.osc1_wave = MoogWave::Saw;
                    p.moog_pad.osc2_wave = MoogWave::Saw;
                    p.moog_pad.osc3_wave = MoogWave::TriSaw;
                    p.moog_pad.osc2_octave = 0;
                    p.moog_pad.osc3_octave = -1;
                    p.moog_pad.osc2_detune = 11.0;
                    p.moog_pad.osc3_detune = -9.0;
                    p.moog_pad.osc1_level = 0.84;
                    p.moog_pad.osc2_level = 0.82;
                    p.moog_pad.osc3_level = 0.58;
                    p.moog_pad.cutoff = 2200.0;
                    p.moog_pad.resonance = 0.22;
                    p.moog_pad.env_amount = 0.56;
                    p.moog_pad.key_track = 0.50;
                    p.moog_pad.drive = 0.34;
                    p.moog_pad.lfo_rate = 4.2;
                    p.moog_pad.lfo_to_pitch = 0.010;
                    p.moog_pad.filt_attack = 0.020;
                    p.moog_pad.filt_decay = 0.42;
                    p.moog_pad.filt_sustain = 0.50;
                    p.moog_pad.filt_release = 0.55;
                    p.moog_pad.amp_attack = 0.012;
                    p.moog_pad.amp_decay = 0.45;
                    p.moog_pad.amp_sustain = 0.80;
                    p.moog_pad.amp_release = 0.62;
                }
                6 => {
                    // Electric Guitar (warm, thick humbucker guitar through tube amp & 4x12 cab!)
                    p.moog_pad.osc1_wave = MoogWave::TriSaw;
                    p.moog_pad.osc2_wave = MoogWave::Saw;
                    p.moog_pad.osc3_wave = MoogWave::Triangle;
                    p.moog_pad.osc2_octave = 0;
                    p.moog_pad.osc3_octave = 0;
                    p.moog_pad.osc2_detune = 4.0;
                    p.moog_pad.osc3_detune = -3.0;
                    p.moog_pad.osc1_level = 0.86;
                    p.moog_pad.osc2_level = 0.64;
                    p.moog_pad.osc3_level = 0.52;
                    p.moog_pad.noise_level = 0.03; // Subtle woody pick transient
                    p.moog_pad.cutoff = 760.0;     // Warm 4x12 cabinet roll-off
                    p.moog_pad.resonance = 0.26;
                    p.moog_pad.env_amount = 0.48;
                    p.moog_pad.key_track = 0.48;
                    p.moog_pad.drive = 0.64;       // Warm tube overdrive
                    p.moog_pad.lfo_rate = 5.2;
                    p.moog_pad.lfo_to_pitch = 0.014;
                    p.moog_pad.filt_attack = 0.003;
                    p.moog_pad.filt_decay = 0.22;
                    p.moog_pad.filt_sustain = 0.32;
                    p.moog_pad.filt_release = 0.24;
                    p.moog_pad.amp_attack = 0.004;
                    p.moog_pad.amp_decay = 0.78;
                    p.moog_pad.amp_sustain = 0.44;
                    p.moog_pad.amp_release = 0.28;
                }
                7 => {
                    // Wurlitzer Keys (barky vintage Wurli wah!)
                    p.moog_pad.osc1_wave = MoogWave::Triangle;
                    p.moog_pad.osc2_wave = MoogWave::NarrowPulse;
                    p.moog_pad.osc3_wave = MoogWave::Triangle;
                    p.moog_pad.osc2_octave = 1;
                    p.moog_pad.osc3_octave = 0;
                    p.moog_pad.osc2_detune = 3.0;
                    p.moog_pad.osc1_level = 0.90;
                    p.moog_pad.osc2_level = 0.38;
                    p.moog_pad.osc3_level = 0.22;
                    p.moog_pad.cutoff = 1950.0;
                    p.moog_pad.resonance = 0.36;
                    p.moog_pad.env_amount = 0.68;
                    p.moog_pad.drive = 0.44;
                    p.moog_pad.lfo_rate = 4.8;
                    p.moog_pad.lfo_to_filter = 0.22; // Vintage Wurli tremolo-wah
                    p.moog_pad.filt_attack = 0.002;
                    p.moog_pad.filt_decay = 0.28;
                    p.moog_pad.filt_sustain = 0.16;
                    p.moog_pad.amp_attack = 0.003;
                    p.moog_pad.amp_decay = 0.68;
                    p.moog_pad.amp_sustain = 0.30;
                    p.moog_pad.amp_release = 0.30;
                }
                8 => {
                    // Festival Supersaw (wide detuned 3-osc saw stack for Big Room EDM & Trance)
                    p.moog_pad.osc1_wave = MoogWave::Saw;
                    p.moog_pad.osc2_wave = MoogWave::Saw;
                    p.moog_pad.osc3_wave = MoogWave::Saw;
                    p.moog_pad.osc2_octave = 0;
                    p.moog_pad.osc3_octave = 0;
                    p.moog_pad.osc2_detune = 16.0;
                    p.moog_pad.osc3_detune = -15.0;
                    p.moog_pad.osc1_level = 0.88;
                    p.moog_pad.osc2_level = 0.82;
                    p.moog_pad.osc3_level = 0.78;
                    p.moog_pad.cutoff = 3800.0;
                    p.moog_pad.resonance = 0.22;
                    p.moog_pad.env_amount = 0.44;
                    p.moog_pad.drive = 0.28;
                    p.moog_pad.filt_attack = 0.006;
                    p.moog_pad.filt_decay = 0.38;
                    p.moog_pad.filt_sustain = 0.64;
                    p.moog_pad.amp_attack = 0.008;
                    p.moog_pad.amp_decay = 0.40;
                    p.moog_pad.amp_sustain = 0.82;
                    p.moog_pad.amp_release = 0.45;
                }
                _ => {
                    // Future Bass Saw (lush detuned supersaw with rhythmic LFO filter flutter)
                    p.moog_pad.osc1_wave = MoogWave::Saw;
                    p.moog_pad.osc2_wave = MoogWave::WidePulse;
                    p.moog_pad.osc3_wave = MoogWave::Saw;
                    p.moog_pad.osc2_octave = 0;
                    p.moog_pad.osc3_octave = 1;
                    p.moog_pad.osc2_detune = 14.0;
                    p.moog_pad.osc3_detune = -12.0;
                    p.moog_pad.osc1_level = 0.88;
                    p.moog_pad.osc2_level = 0.76;
                    p.moog_pad.osc3_level = 0.42;
                    p.moog_pad.cutoff = 2600.0;
                    p.moog_pad.resonance = 0.28;
                    p.moog_pad.env_amount = 0.48;
                    p.moog_pad.drive = 0.22;
                    p.moog_pad.lfo_rate = 5.0; // Exact 1/8th flutter at 150 BPM
                    p.moog_pad.lfo_to_filter = 0.46;
                    p.moog_pad.filt_attack = 0.015;
                    p.moog_pad.filt_decay = 0.42;
                    p.moog_pad.filt_sustain = 0.56;
                    p.moog_pad.amp_attack = 0.014;
                    p.moog_pad.amp_decay = 0.48;
                    p.moog_pad.amp_sustain = 0.78;
                    p.moog_pad.amp_release = 0.42;
                }
            }
        }
        TrackId::Monologue => {
            p.voice_presets[3] = preset_idx % 10;
            p.monologue.mod_mode = Osc2ModMode::Off;
            p.monologue.vco2_octave = 0;
            p.monologue.vco2_pitch = 0.0;
            p.monologue.vco1_level = 0.85;
            p.monologue.vco2_level = 0.65;
            p.monologue.vco2_shape = 0.35;
            p.monologue.eg_target = EgTarget::Cutoff;
            p.monologue.amp_sustain = false;
            p.monologue.lfo_wave = LfoWave::Triangle;
            p.monologue.lfo_mode = LfoMode::Slow;
            p.monologue.lfo_target = LfoTarget::Cutoff;
            p.monologue.lfo_rate = 0.25;
            p.monologue.lfo_int = 0.0;
            match p.voice_presets[3] {
                0 => {
                    // Pluck Arp
                    p.monologue.vco1_wave = MonologueOscWave::Saw;
                    p.monologue.vco2_wave = MonologueOscWave::Triangle;
                    p.monologue.vco1_shape = 0.35;
                    p.monologue.cutoff = 2300.0;
                    p.monologue.resonance = 0.38;
                    p.monologue.eg_int = 0.72;
                    p.monologue.eg_attack = 0.003;
                    p.monologue.eg_decay = 0.20;
                    p.monologue.drive = 0.28;
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
                    // Funk Clav (Hohner D6 Clavinet through CryBaby wah)
                    p.monologue.vco1_wave = MonologueOscWave::Square;
                    p.monologue.vco2_wave = MonologueOscWave::Saw;
                    p.monologue.vco1_shape = 0.72;
                    p.monologue.cutoff = 1950.0;
                    p.monologue.resonance = 0.62;
                    p.monologue.eg_int = 0.86;
                    p.monologue.eg_attack = 0.002;
                    p.monologue.eg_decay = 0.15;
                    p.monologue.drive = 0.48;
                    p.monologue.glide = 0.01;
                }
                3 => {
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
                4 => {
                    // OB-X Power Lead (fat detuned 80s Oberheim arena synth lead)
                    p.monologue.vco1_wave = MonologueOscWave::Saw;
                    p.monologue.vco2_wave = MonologueOscWave::Saw;
                    p.monologue.vco1_shape = 0.22;
                    p.monologue.vco2_shape = 0.18;
                    p.monologue.vco2_pitch = 0.10; // Lush 10-cent 80s detune!
                    p.monologue.vco1_level = 0.88;
                    p.monologue.vco2_level = 0.84;
                    p.monologue.cutoff = 2450.0;
                    p.monologue.resonance = 0.26;
                    p.monologue.eg_int = 0.52;
                    p.monologue.eg_attack = 0.012;
                    p.monologue.eg_decay = 0.44;
                    p.monologue.amp_sustain = true;
                    p.monologue.drive = 0.38;
                    p.monologue.glide = 0.07;
                    p.monologue.lfo_target = LfoTarget::Pitch;
                    p.monologue.lfo_rate = 0.54;
                    p.monologue.lfo_int = 0.014;
                }
                5 => {
                    // Electric Guitar Lead (warm creamy humbucker rock/fusion lead!)
                    p.monologue.vco1_wave = MonologueOscWave::Saw;
                    p.monologue.vco2_wave = MonologueOscWave::Triangle;
                    p.monologue.vco1_shape = 0.36;
                    p.monologue.vco2_shape = 0.44;
                    p.monologue.vco2_pitch = 0.05;
                    p.monologue.vco1_level = 0.88;
                    p.monologue.vco2_level = 0.72;
                    p.monologue.cutoff = 920.0; // Warm amp cabinet roll-off
                    p.monologue.resonance = 0.28;
                    p.monologue.eg_int = 0.44;
                    p.monologue.eg_attack = 0.003;
                    p.monologue.eg_decay = 0.34;
                    p.monologue.amp_sustain = true;
                    p.monologue.drive = 0.72; // Warm asymmetric analog tube drive
                    p.monologue.glide = 0.06;
                    p.monologue.lfo_target = LfoTarget::Pitch;
                    p.monologue.lfo_rate = 0.54;
                    p.monologue.lfo_int = 0.018; // Subtle expressive finger vibrato
                }
                6 => {
                    // 70s String Lead (warm silky vintage string melody)
                    p.monologue.vco1_wave = MonologueOscWave::Saw;
                    p.monologue.vco2_wave = MonologueOscWave::Triangle;
                    p.monologue.vco2_octave = 0;
                    p.monologue.vco2_pitch = 0.09;
                    p.monologue.vco1_shape = 0.14;
                    p.monologue.vco1_level = 0.80;
                    p.monologue.vco2_level = 0.65;
                    p.monologue.cutoff = 1650.0;
                    p.monologue.resonance = 0.16;
                    p.monologue.eg_int = 0.20;
                    p.monologue.eg_attack = 0.16;
                    p.monologue.eg_decay = 0.68;
                    p.monologue.amp_sustain = true;
                    p.monologue.drive = 0.12;
                    p.monologue.glide = 0.08;
                    p.monologue.lfo_target = LfoTarget::Pitch;
                    p.monologue.lfo_rate = 0.56;
                    p.monologue.lfo_int = 0.020;
                }
                7 => {
                    // Sync Screamer (classic Korg VCO2 hard-sync ripping lead!)
                    p.monologue.vco1_wave = MonologueOscWave::Saw;
                    p.monologue.vco2_wave = MonologueOscWave::Saw;
                    p.monologue.mod_mode = Osc2ModMode::Sync;
                    p.monologue.vco2_octave = 0;
                    p.monologue.vco2_pitch = 7.0; // Fifth sync harmonic bite
                    p.monologue.vco1_shape = 0.30;
                    p.monologue.vco1_level = 0.42;
                    p.monologue.vco2_level = 0.94;
                    p.monologue.eg_target = EgTarget::Pitch2;
                    p.monologue.eg_int = 0.52;
                    p.monologue.eg_attack = 0.008;
                    p.monologue.eg_decay = 0.34;
                    p.monologue.cutoff = 3400.0;
                    p.monologue.resonance = 0.32;
                    p.monologue.drive = 0.64;
                    p.monologue.glide = 0.09;
                }
                8 => {
                    // Dubstep Growl (ring-mod filth lead with aggressive filter wobble & drive!)
                    p.monologue.vco1_wave = MonologueOscWave::Saw;
                    p.monologue.vco2_wave = MonologueOscWave::Square;
                    p.monologue.mod_mode = Osc2ModMode::Ring;
                    p.monologue.vco2_octave = -1;
                    p.monologue.vco2_pitch = 5.0; // Metallic formant grit
                    p.monologue.vco1_shape = 0.65;
                    p.monologue.vco2_shape = 0.55;
                    p.monologue.vco1_level = 0.72;
                    p.monologue.vco2_level = 0.96;
                    p.monologue.eg_target = EgTarget::Cutoff;
                    p.monologue.eg_int = 0.68;
                    p.monologue.eg_attack = 0.004;
                    p.monologue.eg_decay = 0.28;
                    p.monologue.cutoff = 1150.0;
                    p.monologue.resonance = 0.62;
                    p.monologue.drive = 0.84;
                    p.monologue.glide = 0.06;
                    p.monologue.lfo_wave = LfoWave::Saw;
                    p.monologue.lfo_mode = LfoMode::Fast;
                    p.monologue.lfo_target = LfoTarget::Cutoff;
                    p.monologue.lfo_rate = 0.42;
                    p.monologue.lfo_int = 0.55;
                }
                _ => {
                    // Trance Pluck (crisp bright EDM/trance arp pluck with snappy decay)
                    p.monologue.vco1_wave = MonologueOscWave::Saw;
                    p.monologue.vco2_wave = MonologueOscWave::Saw;
                    p.monologue.vco2_octave = 1;
                    p.monologue.vco2_pitch = 0.08; // Slight shimmer detune
                    p.monologue.vco1_shape = 0.45;
                    p.monologue.vco1_level = 0.88;
                    p.monologue.vco2_level = 0.62;
                    p.monologue.eg_target = EgTarget::Cutoff;
                    p.monologue.eg_int = 0.78;
                    p.monologue.eg_attack = 0.001;
                    p.monologue.eg_decay = 0.16;
                    p.monologue.cutoff = 2100.0;
                    p.monologue.resonance = 0.24;
                    p.monologue.drive = 0.22;
                    p.monologue.glide = 0.01;
                }
            }
        }
        TrackId::Tb303 => {
            p.voice_presets[4] = preset_idx % 6;
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
                3 => {
                    // Deep Sub 303
                    p.tb303.wave = Tb303Wave::Square;
                    p.tb303.cutoff = 380.0;
                    p.tb303.resonance = 0.32;
                    p.tb303.env_mod = 0.45;
                    p.tb303.decay = 0.40;
                    p.tb303.accent = 0.55;
                    p.tb303.overdrive = 0.18;
                }
                4 => {
                    // Distorted 303 (warehouse rave screamer)
                    p.tb303.wave = Tb303Wave::Saw;
                    p.tb303.cutoff = 840.0;
                    p.tb303.resonance = 0.86;
                    p.tb303.env_mod = 0.88;
                    p.tb303.decay = 0.30;
                    p.tb303.accent = 0.90;
                    p.tb303.overdrive = 0.82;
                }
                _ => {
                    // Melodic Pluck 303 (clean resonant bassline)
                    p.tb303.wave = Tb303Wave::Square;
                    p.tb303.cutoff = 920.0;
                    p.tb303.resonance = 0.44;
                    p.tb303.env_mod = 0.52;
                    p.tb303.decay = 0.22;
                    p.tb303.accent = 0.60;
                    p.tb303.overdrive = 0.16;
                }
            }
        }
        TrackId::Sampler => {
            // Map preset 0..7 to BUILTIN_SAMPLE_NAMES:
            // 0: Minor9 Rhodes (3), 1: Brass Stab (1), 2: Choir Ahh (0),
            // 3: Retro FM Bell (4), 4: Toy Piano (2), 5: 70s Solina String (5),
            // 6: OB-X Power Chord (6), 7: Electric Guitar (7)
            let idx = preset_idx % 8;
            p.voice_presets[5] = idx;
            let builtin_idx = match idx {
                0 => 3, // Minor9 Rhodes
                1 => 1, // Brass Stab
                2 => 0, // Choir Ahh
                3 => 4, // Retro FM Bell
                4 => 2, // Toy Piano
                5 => 5, // 70s Solina String
                6 => 6, // OB-X Power Chord
                _ => 7, // Electric Guitar
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
                4 => {
                    p.sampler.mode = SamplerMode::Sk1LoFi;
                    p.sampler.cutoff = 4800.0;
                    p.sampler.attack = 0.002;
                    p.sampler.release = 0.30;
                }
                5 => {
                    // 70s Solina String
                    p.sampler.mode = SamplerMode::Vintage12Bit;
                    p.sampler.cutoff = 4600.0;
                    p.sampler.attack = 0.06;
                    p.sampler.release = 0.85;
                }
                6 => {
                    // OB-X Power Chord
                    p.sampler.mode = SamplerMode::HiFi;
                    p.sampler.cutoff = 5600.0;
                    p.sampler.attack = 0.004;
                    p.sampler.release = 0.42;
                }
                _ => {
                    // Electric Guitar
                    p.sampler.mode = SamplerMode::HiFi;
                    p.sampler.cutoff = 4800.0;
                    p.sampler.attack = 0.002;
                    p.sampler.release = 0.28;
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
                // Subtle tasteful 808 Funk Cowbell / Rim pocket accent!
                if bar % 2 == 1 {
                    set(&mut seq, DrumVoiceId::CowbellRim, base + 11, 0.44, false, 1);
                    if e > 0.58 {
                        set(&mut seq, DrumVoiceId::CowbellRim, base + 2, 0.34, false, 1);
                    }
                } else if e > 0.65 {
                    set(&mut seq, DrumVoiceId::CowbellRim, base + 14, 0.36, false, 1);
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
            Vibe::Disco70s => {
                // Iconic 70s four-on-the-floor disco with sizzling offbeat open hats & crisp claps!
                for beat in [0, 4, 8, 12] {
                    set(&mut seq, DrumVoiceId::Kick, base + beat, 0.96, beat == 0, 1);
                }
                for beat in [4, 12] {
                    set(&mut seq, DrumVoiceId::Snare, base + beat, 0.88, true, 1);
                    set(&mut seq, DrumVoiceId::Clap, base + beat, 0.78, false, 1);
                }
                for s in 0..16 {
                    if s % 4 == 2 {
                        set(&mut seq, DrumVoiceId::OpenHat, base + s, 0.74, false, 1);
                    } else if e > 0.35 && s % 2 == 0 {
                        set(&mut seq, DrumVoiceId::ClosedHat, base + s, 0.52, false, 1);
                    } else if e > 0.65 {
                        set(&mut seq, DrumVoiceId::ClosedHat, base + s, 0.36, false, 1);
                    }
                }
                if is_fill_bar && e > 0.45 {
                    for &(st, vel) in &[(11, 0.70), (13, 0.76), (14, 0.82), (15, 0.88)] {
                        set(&mut seq, DrumVoiceId::Tom, base + st, vel, st == 15, 1);
                    }
                }
            }
            Vibe::Arena80s => {
                // Huge 80s gated-reverb arena rock/synthpop beat!
                for beat in [0, 8] {
                    set(&mut seq, DrumVoiceId::Kick, base + beat, 0.98, true, 1);
                }
                set(&mut seq, DrumVoiceId::Kick, base + 6, 0.74, false, 1);
                if bar % 2 == 1 {
                    set(&mut seq, DrumVoiceId::Kick, base + 14, 0.78, false, 1);
                }
                for beat in [4, 12] {
                    set(&mut seq, DrumVoiceId::Snare, base + beat, 0.98, true, 1);
                    set(&mut seq, DrumVoiceId::Clap, base + beat, 0.74, false, 1);
                }
                for s in (0..16).step_by(2) {
                    set(&mut seq, DrumVoiceId::ClosedHat, base + s, if s % 4 == 0 { 0.64 } else { 0.46 }, false, 1);
                }
                if bar == 0 {
                    set(&mut seq, DrumVoiceId::Cymbal, 0, 0.72, true, 1);
                }
                if is_fill_bar && e > 0.4 {
                    set(&mut seq, DrumVoiceId::Tom, base + 10, 0.78, false, 1);
                    set(&mut seq, DrumVoiceId::Tom, base + 12, 0.84, true, 1);
                    set(&mut seq, DrumVoiceId::Tom, base + 14, 0.90, true, 1);
                    set(&mut seq, DrumVoiceId::Snare, base + 15, 0.92, true, 1);
                }
            }
            Vibe::IndieRock => {
                // Punchy driving rock backbeat with eighth-note hi-hats & tom fills!
                set(&mut seq, DrumVoiceId::Kick, base, 0.96, true, 1);
                set(&mut seq, DrumVoiceId::Kick, base + 6, 0.76, false, 1);
                set(&mut seq, DrumVoiceId::Kick, base + 8, 0.92, true, 1);
                if e > 0.5 {
                    set(&mut seq, DrumVoiceId::Kick, base + 10, 0.72, false, 1);
                }
                for beat in [4, 12] {
                    set(&mut seq, DrumVoiceId::Snare, base + beat, 0.95, true, 1);
                }
                for s in (0..16).step_by(2) {
                    if s == 14 && e > 0.45 {
                        set(&mut seq, DrumVoiceId::OpenHat, base + s, 0.70, false, 1);
                    } else {
                        set(&mut seq, DrumVoiceId::ClosedHat, base + s, if s % 4 == 0 { 0.68 } else { 0.50 }, false, 1);
                    }
                }
                if bar == 0 {
                    set(&mut seq, DrumVoiceId::Cymbal, 0, 0.76, true, 1);
                }
                if is_fill_bar {
                    set(&mut seq, DrumVoiceId::Snare, base + 13, 0.76, false, 1);
                    set(&mut seq, DrumVoiceId::Tom, base + 14, 0.84, true, 1);
                    set(&mut seq, DrumVoiceId::Tom, base + 15, 0.90, true, 1);
                }
            }
            Vibe::NeoSoul => {
                // Deep swung Dilla-inspired pocket with rimshots & ghost snares
                set(&mut seq, DrumVoiceId::Kick, base, 0.92, true, 1);
                set(&mut seq, DrumVoiceId::Kick, base + 7, 0.74, false, 1);
                set(&mut seq, DrumVoiceId::Kick, base + 10, 0.84, false, 1);
                if bar % 2 == 1 {
                    set(&mut seq, DrumVoiceId::Kick, base + 15, 0.64, false, 1);
                }
                for beat in [4, 12] {
                    set(&mut seq, DrumVoiceId::Snare, base + beat, 0.86, true, 1);
                    set(&mut seq, DrumVoiceId::CowbellRim, base + beat, 0.54, false, 1);
                }
                if e > 0.4 {
                    set(&mut seq, DrumVoiceId::Snare, base + 9, 0.36, false, 1);
                }
                for s in (0..16).step_by(2) {
                    set(&mut seq, DrumVoiceId::ClosedHat, base + s, if s % 4 == 0 { 0.56 } else { 0.36 }, false, 1);
                }
                if e > 0.55 {
                    set(&mut seq, DrumVoiceId::OpenHat, base + 14, 0.48, false, 1);
                }
            }
            Vibe::ItaloCosmic => {
                // Classic 80s Italo-disco pulse: steady kick, clapping backbeat & crisp 16th hats
                for beat in [0, 4, 8, 12] {
                    set(&mut seq, DrumVoiceId::Kick, base + beat, 0.95, beat == 0, 1);
                }
                for beat in [4, 12] {
                    set(&mut seq, DrumVoiceId::Clap, base + beat, 0.88, true, 1);
                    set(&mut seq, DrumVoiceId::Snare, base + beat, 0.68, false, 1);
                }
                for s in 0..16 {
                    if s % 4 == 2 {
                        set(&mut seq, DrumVoiceId::OpenHat, base + s, 0.66, false, 1);
                    } else if e > 0.45 && s % 2 == 0 {
                        set(&mut seq, DrumVoiceId::ClosedHat, base + s, 0.46, false, 1);
                    }
                }
                if e > 0.5 {
                    set(&mut seq, DrumVoiceId::CowbellRim, base + 6, 0.54, false, 1);
                    set(&mut seq, DrumVoiceId::CowbellRim, base + 14, 0.60, false, 1);
                }
            }
            Vibe::BoomBap => {
                // 90s Golden-Era Hip-Hop: dusty swung boom-bap pocket!
                set(&mut seq, DrumVoiceId::Kick, base, 0.98, true, 1);
                if bar % 2 == 0 {
                    set(&mut seq, DrumVoiceId::Kick, base + 7, 0.78, false, 1);
                    set(&mut seq, DrumVoiceId::Kick, base + 10, 0.92, true, 1);
                } else {
                    set(&mut seq, DrumVoiceId::Kick, base + 6, 0.72, false, 1);
                    set(&mut seq, DrumVoiceId::Kick, base + 10, 0.90, true, 1);
                    if e > 0.45 {
                        set(&mut seq, DrumVoiceId::Kick, base + 15, 0.68, false, 1);
                    }
                }
                // Crisp dusty snare on 2 & 4 + subtle ghost snares
                for beat in [4, 12] {
                    set(&mut seq, DrumVoiceId::Snare, base + beat, 0.94, true, 1);
                    if e > 0.5 {
                        set(&mut seq, DrumVoiceId::Clap, base + beat, 0.44, false, 1);
                    }
                }
                if e > 0.35 {
                    set(&mut seq, DrumVoiceId::Snare, base + 9, 0.38, false, 1);
                    if bar % 2 == 1 {
                        set(&mut seq, DrumVoiceId::Snare, base + 15, 0.48, false, 1);
                    }
                }
                // Swung 8th/16th vinyl hats
                for s in 0..16 {
                    if s % 2 == 0 {
                        let vel = if s % 4 == 0 { 0.68 } else { 0.48 };
                        set(&mut seq, DrumVoiceId::ClosedHat, base + s, vel, false, 1);
                    } else if e > 0.6 && (s == 7 || s == 15) {
                        set(&mut seq, DrumVoiceId::ClosedHat, base + s, 0.34, false, 1);
                    }
                }
                if bar % 2 == 1 && e > 0.3 {
                    set(&mut seq, DrumVoiceId::OpenHat, base + 14, 0.56, false, 1);
                }
                if e > 0.55 && bar % 2 == 0 {
                    set(&mut seq, DrumVoiceId::CowbellRim, base + 2, 0.42, false, 1);
                }
                if is_fill_bar && e > 0.4 {
                    set(&mut seq, DrumVoiceId::Snare, base + 14, 0.72, false, 1);
                    set(&mut seq, DrumVoiceId::Tom, base + 15, 0.68, false, 1);
                }
            }
            Vibe::Trap808 => {
                // 142 BPM Half-Time Trap: booming kicks, beat-3 snare/clap & ratcheting hat rolls!
                set(&mut seq, DrumVoiceId::Kick, base, 1.0, true, 1);
                if bar % 2 == 0 {
                    set(&mut seq, DrumVoiceId::Kick, base + 6, 0.88, false, 1);
                    set(&mut seq, DrumVoiceId::Kick, base + 10, 0.94, true, 1);
                } else {
                    set(&mut seq, DrumVoiceId::Kick, base + 3, 0.82, false, 1);
                    set(&mut seq, DrumVoiceId::Kick, base + 10, 0.92, true, 1);
                    if e > 0.45 {
                        set(&mut seq, DrumVoiceId::Kick, base + 14, 0.86, false, 1);
                    }
                }
                // Half-time snapping snare + layered clap on beat 3 (step 8)
                set(&mut seq, DrumVoiceId::Snare, base + 8, 0.96, true, 1);
                set(&mut seq, DrumVoiceId::Clap, base + 8, 0.90, true, 1);
                if e > 0.5 && bar % 2 == 1 {
                    set(&mut seq, DrumVoiceId::Snare, base + 15, 0.54, false, 1);
                }
                // Signature ratcheting trap hi-hat rolls!
                for s in (0..16).step_by(2) {
                    let ratchet = if e > 0.35 && ((bar % 2 == 0 && s == 14) || (bar % 2 == 1 && s == 6)) {
                        3
                    } else if e > 0.55 && (s == 10 || s == 12) && rng.f32() < 0.55 {
                        2
                    } else if is_fill_bar && s == 14 && e > 0.4 {
                        4
                    } else {
                        1
                    };
                    let vel = if s % 4 == 0 { 0.72 } else { 0.54 };
                    set(&mut seq, DrumVoiceId::ClosedHat, base + s, vel, ratchet > 1, ratchet);
                }
                if e > 0.4 {
                    set(&mut seq, DrumVoiceId::OpenHat, base + 4, 0.58, false, 1);
                }
                if is_fill_bar && e > 0.45 {
                    set(&mut seq, DrumVoiceId::Snare, base + 13, 0.68, false, 1);
                    set(&mut seq, DrumVoiceId::Snare, base + 14, 0.78, false, 2);
                    set(&mut seq, DrumVoiceId::Tom, base + 15, 0.80, false, 1);
                }
            }
            Vibe::Dubstep => {
                // 140 BPM Half-Time Dubstep: colossal kick on 1, massive cracking snare on 3!
                set(&mut seq, DrumVoiceId::Kick, base, 1.0, true, 1);
                if bar % 2 == 1 {
                    set(&mut seq, DrumVoiceId::Kick, base + 10, 0.92, true, 1);
                } else if e > 0.5 {
                    set(&mut seq, DrumVoiceId::Kick, base + 14, 0.86, false, 1);
                }
                if e > 0.7 && bar == 2 {
                    set(&mut seq, DrumVoiceId::Kick, base + 6, 0.78, false, 1);
                }
                // Massive half-time snare + clap slam on beat 3 (step 8)
                set(&mut seq, DrumVoiceId::Snare, base + 8, 1.0, true, 1);
                set(&mut seq, DrumVoiceId::Clap, base + 8, 0.92, true, 1);
                // Dark driving 8th hats + metallic open hats
                for s in (0..16).step_by(2) {
                    if s == 4 || s == 12 {
                        set(&mut seq, DrumVoiceId::OpenHat, base + s, 0.70, false, 1);
                    } else if e > 0.3 || s % 4 == 0 {
                        set(&mut seq, DrumVoiceId::ClosedHat, base + s, if s % 4 == 0 { 0.68 } else { 0.48 }, false, 1);
                    }
                }
                if bar == 0 {
                    set(&mut seq, DrumVoiceId::Cymbal, base, 0.78, true, 1);
                } else if e > 0.65 && bar == 2 {
                    set(&mut seq, DrumVoiceId::Cymbal, base + 8, 0.56, false, 1);
                }
                if is_fill_bar && e > 0.35 {
                    set(&mut seq, DrumVoiceId::Snare, base + 12, 0.74, false, 1);
                    set(&mut seq, DrumVoiceId::Snare, base + 14, 0.84, true, 2);
                    set(&mut seq, DrumVoiceId::Tom, base + 15, 0.86, true, 1);
                }
            }
            Vibe::BigRoomEdm => {
                // 128 BPM Mainstage Festival EDM: huge 4x4 909 kick, big claps & driving offbeat hats!
                for beat in [0, 4, 8, 12] {
                    set(&mut seq, DrumVoiceId::Kick, base + beat, 1.0, beat == 0, 1);
                }
                for beat in [4, 12] {
                    set(&mut seq, DrumVoiceId::Clap, base + beat, 0.94, true, 1);
                    set(&mut seq, DrumVoiceId::Snare, base + beat, 0.72, false, 1);
                }
                for s in [2, 6, 10, 14] {
                    set(&mut seq, DrumVoiceId::OpenHat, base + s, 0.80, false, 1);
                }
                for s in 0..16 {
                    if s % 4 != 2 && (e > 0.35 || s % 2 == 0) {
                        set(&mut seq, DrumVoiceId::ClosedHat, base + s, if s % 2 == 0 { 0.58 } else { 0.38 }, false, 1);
                    }
                }
                if bar == 0 {
                    set(&mut seq, DrumVoiceId::Cymbal, base, 0.82, true, 1);
                } else if e > 0.65 {
                    for s in [2, 6, 10, 14] {
                        set(&mut seq, DrumVoiceId::Cymbal, base + s, 0.34, false, 1);
                    }
                }
                if is_fill_bar && e > 0.4 {
                    set(&mut seq, DrumVoiceId::Snare, base + 12, 0.78, false, 1);
                    set(&mut seq, DrumVoiceId::Snare, base + 13, 0.82, false, 1);
                    set(&mut seq, DrumVoiceId::Snare, base + 14, 0.88, true, 2);
                    set(&mut seq, DrumVoiceId::Clap, base + 15, 0.92, true, 1);
                }
            }
            Vibe::FutureBass => {
                // 150 BPM Half-Time Future Bass: bouncy syncopated kicks, huge snare on 3 & sparkling hats!
                set(&mut seq, DrumVoiceId::Kick, base, 0.98, true, 1);
                set(&mut seq, DrumVoiceId::Kick, base + 6, 0.84, false, 1);
                if bar % 2 == 0 {
                    set(&mut seq, DrumVoiceId::Kick, base + 10, 0.90, true, 1);
                } else {
                    set(&mut seq, DrumVoiceId::Kick, base + 11, 0.86, false, 1);
                    if e > 0.5 {
                        set(&mut seq, DrumVoiceId::Kick, base + 14, 0.80, false, 1);
                    }
                }
                // Huge Future Bass snare + clap on beat 3 (step 8)
                set(&mut seq, DrumVoiceId::Snare, base + 8, 0.98, true, 1);
                set(&mut seq, DrumVoiceId::Clap, base + 8, 0.92, true, 1);
                // Sparkling hats with tasteful ratchets
                for s in (0..16).step_by(2) {
                    let ratchet = if e > 0.45 && s == 14 && bar % 2 == 1 {
                        3
                    } else if e > 0.6 && s == 6 {
                        2
                    } else {
                        1
                    };
                    set(&mut seq, DrumVoiceId::ClosedHat, base + s, if s % 4 == 0 { 0.66 } else { 0.48 }, ratchet > 1, ratchet);
                }
                if e > 0.35 {
                    set(&mut seq, DrumVoiceId::OpenHat, base + 12, 0.62, false, 1);
                    set(&mut seq, DrumVoiceId::CowbellRim, base + 3, 0.46, false, 1);
                }
                if bar == 0 {
                    set(&mut seq, DrumVoiceId::Cymbal, base, 0.72, true, 1);
                }
                if is_fill_bar && e > 0.4 {
                    set(&mut seq, DrumVoiceId::Tom, base + 12, 0.72, false, 1);
                    set(&mut seq, DrumVoiceId::Tom, base + 14, 0.82, false, 1);
                    set(&mut seq, DrumVoiceId::Snare, base + 15, 0.86, true, 2);
                }
            }
            Vibe::EuphoricTrance => {
                // 138 BPM Uplifting Trance: relentless 909 4x4, offbeat open hats & rolling 16ths!
                for beat in [0, 4, 8, 12] {
                    set(&mut seq, DrumVoiceId::Kick, base + beat, 0.98, beat == 0, 1);
                }
                for beat in [4, 12] {
                    set(&mut seq, DrumVoiceId::Clap, base + beat, 0.90, true, 1);
                }
                for s in [2, 6, 10, 14] {
                    set(&mut seq, DrumVoiceId::OpenHat, base + s, 0.78, false, 1);
                }
                for s in 0..16 {
                    if s % 4 == 2 {
                        continue;
                    }
                    let vel = match s % 4 {
                        0 => 0.46,
                        1 => 0.34,
                        _ => 0.64,
                    };
                    if e > 0.25 || s % 2 == 0 {
                        set(&mut seq, DrumVoiceId::ClosedHat, base + s, vel, false, 1);
                    }
                }
                if bar == 0 {
                    set(&mut seq, DrumVoiceId::Cymbal, base, 0.74, true, 1);
                } else if e > 0.55 {
                    for s in [2, 6, 10, 14] {
                        set(&mut seq, DrumVoiceId::Cymbal, base + s, 0.40, false, 1);
                    }
                }
                if is_fill_bar && e > 0.4 {
                    set(&mut seq, DrumVoiceId::Clap, base + 14, 0.82, false, 1);
                    set(&mut seq, DrumVoiceId::Snare, base + 15, 0.86, true, 2);
                }
            }
        }
    }

    // Seed-driven groove variation pass so consecutive songs & patterns in the
    // same Vibe feature distinct kick pockets, hat patterns, stereo perc, and fills.
    let mut vrng = Rng::new(seed ^ 0xD1B5_4A32);
    let kick_var = vrng.range(6);
    let hat_var = vrng.range(5);
    let fill_var = vrng.range(6);

    for bar in 0..4 {
        let base = bar * 16;
        match kick_var {
            1 => {
                if bar % 2 == 1 && !seq.lanes[DrumVoiceId::Kick.index()].steps[base + 14].active {
                    set(&mut seq, DrumVoiceId::Kick, base + 14, 0.68, false, 1);
                }
                if bar == 2 && e > 0.45 && !seq.lanes[DrumVoiceId::Kick.index()].steps[base + 7].active {
                    set(&mut seq, DrumVoiceId::Kick, base + 7, 0.60, false, 1);
                }
            }
            2 => {
                if bar % 2 == 0 && !seq.lanes[DrumVoiceId::Kick.index()].steps[base + 14].active {
                    set(&mut seq, DrumVoiceId::Kick, base + 14, 0.72, false, 1);
                }
                if bar == 3 && e > 0.45 {
                    seq.lanes[DrumVoiceId::Kick.index()].steps[base].active = false;
                }
            }
            3 => {
                if bar % 2 == 1 {
                    let k_idx = DrumVoiceId::Kick.index();
                    if seq.lanes[k_idx].steps[base + 10].active && !seq.lanes[k_idx].steps[base + 11].active {
                        seq.lanes[k_idx].steps[base + 10].active = false;
                        set(&mut seq, DrumVoiceId::Kick, base + 11, 0.78, false, 1);
                    } else if !seq.lanes[k_idx].steps[base + 6].active {
                        set(&mut seq, DrumVoiceId::Kick, base + 6, 0.72, false, 1);
                    }
                }
            }
            4 => {
                if e > 0.55 && bar % 2 == 1 {
                    set(&mut seq, DrumVoiceId::Kick, base + 15, 0.72, false, 1);
                } else if e <= 0.55 && bar == 2 {
                    seq.lanes[DrumVoiceId::Kick.index()].steps[base + 8].active = false;
                }
            }
            5 => {
                if e > 0.50 && bar % 2 == 1 {
                    set(&mut seq, DrumVoiceId::Kick, base + 14, 0.70, false, 2);
                }
            }
            _ => {}
        }

        match hat_var {
            1 => {
                for s in 0..16 {
                    if s % 4 == 2 {
                        continue;
                    }
                    let vel = if s % 4 == 0 { 0.46 } else { 0.32 };
                    if e > 0.30 || s % 2 == 0 {
                        set(&mut seq, DrumVoiceId::ClosedHat, base + s, vel, false, 1);
                    }
                }
                set(&mut seq, DrumVoiceId::OpenHat, base + 14, 0.72, false, 1);
                if bar % 2 == 1 {
                    set(&mut seq, DrumVoiceId::OpenHat, base + 6, 0.62, false, 1);
                }
            }
            2 => {
                if bar < 3 {
                    let ch_idx = DrumVoiceId::ClosedHat.index();
                    for s in 0..16 {
                        if s % 2 == 0 {
                            seq.lanes[ch_idx].steps[base + s].active = false;
                        }
                    }
                }
            }
            3 => {
                if e > 0.40 && bar % 2 == 1 {
                    for &(s, r) in &[(7usize, 2u8), (11, 2), (15, 3)] {
                        set(&mut seq, DrumVoiceId::ClosedHat, base + s, 0.58, false, r);
                    }
                }
            }
            4 => {
                if bar % 2 == 1 {
                    for &(s, pv) in &[(3usize, 0.34_f32), (11, 0.40)] {
                        let st = &mut seq.lanes[DrumVoiceId::CowbellRim.index()].steps[base + s];
                        st.active = true;
                        st.velocity = pv;
                        st.probability = 1.0;
                        st.ratchet = 1;
                        st.pan_lock = Some(-0.42);
                    }
                }
                for &(s, po) in &[(7usize, 2i8), (14, -2)] {
                    let st = &mut seq.lanes[DrumVoiceId::Tom.index()].steps[base + s];
                    st.active = true;
                    st.velocity = 0.60;
                    st.probability = 1.0;
                    st.ratchet = 1;
                    st.pitch_offset = po;
                    st.pan_lock = Some(0.42);
                }
            }
            _ => {}
        }

        // Funk pocket embellishments: syncopated ghost snares, subtle cowbell/rim clave, and open-hat barks
        if e > 0.30 {
            if !seq.lanes[DrumVoiceId::Snare.index()].steps[base + 7].active && (bar % 2 == 0 || vrng.f32() < 0.65) {
                set(&mut seq, DrumVoiceId::Snare, base + 7, 0.32, false, 1);
            }
            if !seq.lanes[DrumVoiceId::Snare.index()].steps[base + 9].active && (bar % 2 == 1 || e > 0.55) {
                set(&mut seq, DrumVoiceId::Snare, base + 9, 0.38, false, 1);
            }
            if bar % 2 == 1 && !seq.lanes[DrumVoiceId::Snare.index()].steps[base + 15].active {
                set(&mut seq, DrumVoiceId::Snare, base + 15, 0.44, false, 1);
            }
        }
        if e > 0.38 && hat_var != 2 {
            if matches!(
                vibe,
                Vibe::ElectroFunk | Vibe::Disco70s | Vibe::UkGarage | Vibe::DeepHouse | Vibe::NeoSoul
            ) && bar % 2 == 1
            {
                let st = &mut seq.lanes[DrumVoiceId::CowbellRim.index()].steps[base + 11];
                if !st.active && vrng.f32() < 0.35 {
                    st.active = true;
                    st.velocity = 0.34;
                    st.probability = 1.0;
                    st.ratchet = 1;
                    st.pan_lock = Some(0.30);
                }
            }
            if bar % 2 == 1 && !seq.lanes[DrumVoiceId::OpenHat.index()].steps[base + 2].active && vrng.f32() < 0.55 {
                set(&mut seq, DrumVoiceId::OpenHat, base + 2, 0.62, false, 1);
            }
        }
    }

    match fill_var {
        1 => {
            let pitches = [4i8, 2, 0, -3];
            let pans = [-0.50_f32, -0.15, 0.20, 0.55];
            for i in 0..4 {
                let st = &mut seq.lanes[DrumVoiceId::Tom.index()].steps[60 + i];
                st.active = true;
                st.velocity = 0.70 + i as f32 * 0.06;
                st.accent = i == 3;
                st.probability = 1.0;
                st.ratchet = 1;
                st.pitch_offset = pitches[i];
                st.pan_lock = Some(pans[i]);
            }
        }
        2 => {
            let sn = &mut seq.lanes[DrumVoiceId::Snare.index()];
            sn.steps[58].active = true;
            sn.steps[58].velocity = 0.76;
            sn.steps[58].flam = true;
            sn.steps[58].probability = 1.0;
            sn.steps[60].active = true;
            sn.steps[60].velocity = 0.84;
            sn.steps[60].flam = true;
            sn.steps[60].probability = 1.0;
            set(&mut seq, DrumVoiceId::Snare, 62, 0.86, true, 2);
            set(&mut seq, DrumVoiceId::Snare, 63, 0.90, true, 2);
        }
        3 => {
            for s in 60..64 {
                seq.lanes[DrumVoiceId::Kick.index()].steps[s].active = false;
                seq.lanes[DrumVoiceId::ClosedHat.index()].steps[s].active = false;
            }
            set(&mut seq, DrumVoiceId::Clap, 60, 0.92, true, 1);
        }
        4 => {
            set(&mut seq, DrumVoiceId::CowbellRim, 58, 0.42, false, 1);
            set(&mut seq, DrumVoiceId::Snare, 60, 0.48, false, 1);
            set(&mut seq, DrumVoiceId::CowbellRim, 61, 0.46, false, 1);
            set(&mut seq, DrumVoiceId::Snare, 63, 0.82, true, 2);
        }
        5 => {
            set(&mut seq, DrumVoiceId::Kick, 61, 0.84, false, 1);
            set(&mut seq, DrumVoiceId::Kick, 63, 0.90, true, 1);
            set(&mut seq, DrumVoiceId::Cymbal, 0, 0.78, true, 1);
        }
        _ => {}
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

    let chord_var = rng.range(5);
    let bass_var = rng.range(5);
    let hook_var = rng.range(6);
    let acid_var = rng.range(4);
    let sampler_var = rng.range(4);
    let scale_notes = vibe.default_scale().notes_in_range(root_note, 60, 84);

    // Pre-generate a coherent 16-step melodic motif so each song develops a memorable hook.
    let mut motif_hit = [false; 16];
    let mut motif_deg = [0usize; 16];
    let mut motif_gate = [0.95_f32; 16];
    let mut motif_slide = [false; 16];
    let mut motif_use_scale = [false; 16];
    match hook_var {
        0 => {
            // Shimmering Funk Arp (offbeat 16th syncopations)
            let steps: &[usize] = match rng.range(4) {
                0 => &[0, 2, 3, 6, 8, 10, 11, 14],
                1 => &[0, 3, 5, 6, 9, 11, 14, 15],
                2 => &[0, 3, 6, 7, 10, 12, 15],
                _ => &[0, 2, 6, 9, 10, 14],
            };
            let dir = rng.range(3);
            for (i, &st) in steps.iter().enumerate() {
                motif_hit[st] = true;
                motif_deg[st] = match dir {
                    0 => i,
                    1 => steps.len().saturating_sub(1 + i),
                    _ => if i % 2 == 0 { i / 2 } else { i / 2 + 2 },
                };
                motif_gate[st] = if matches!(vibe, Vibe::Dubstep) { 1.6 } else { 0.85 };
                motif_slide[st] = st == 15;
            }
        }
        1 => {
            // Catchy Funky Clav / Syncopated Lead Hook (hero notes + 16th ghost chucks)
            let steps: &[usize] = if rng.range(2) == 0 {
                &[0, 3, 5, 6, 9, 11, 14, 15]
            } else {
                &[0, 2, 3, 6, 7, 10, 12, 15]
            };
            for (i, &st) in steps.iter().enumerate() {
                motif_hit[st] = true;
                motif_deg[st] = (i * 2 + rng.range(3)) % 8;
                motif_use_scale[st] = i % 2 == 1;
                motif_gate[st] = if st == 0 || st == 6 || st == 10 { 1.6 } else { 0.72 };
                motif_slide[st] = st == 14 || st == 15 || st == 5;
            }
        }
        2 => {
            // Call-and-Response Funk Clav Riff
            for &st in &[0usize, 3, 5, 6, 10, 11, 14, 15] {
                motif_hit[st] = true;
                motif_deg[st] = (st / 2 + rng.range(3)) % 7;
                motif_use_scale[st] = st >= 8 || st == 5;
                motif_gate[st] = if st == 6 || st == 14 { 1.4 } else { 0.68 };
                motif_slide[st] = st == 11 || st == 15;
            }
        }
        3 => {
            // Driving 16th-Note Clavinet Ostinato
            let steps: &[usize] = if rng.range(2) == 0 {
                &[0, 2, 3, 5, 6, 8, 10, 11, 14, 15]
            } else {
                &[0, 2, 4, 5, 6, 9, 10, 11, 13, 15]
            };
            for (i, &st) in steps.iter().enumerate() {
                motif_hit[st] = true;
                motif_deg[st] = (i + rng.range(2)) % 6;
                motif_gate[st] = if st % 2 == 1 { 0.58 } else { 0.78 };
                motif_slide[st] = st == 5 || st == 15;
            }
        }
        4 => {
            // Dotted-8th Funk Echo Motif
            let steps: &[usize] = if rng.range(2) == 0 {
                &[0, 3, 6, 9, 11, 14, 15]
            } else {
                &[0, 3, 6, 7, 10, 14]
            };
            for (i, &st) in steps.iter().enumerate() {
                motif_hit[st] = true;
                motif_deg[st] = (i * 2 + rng.range(2)) % 6;
                motif_gate[st] = if st == 7 || st == 15 { 0.65 } else { 1.15 };
                motif_slide[st] = st == 14;
            }
        }
        _ => {
            // Expressive Talkbox / Blue-Note Glide Solo
            for (i, &st) in [0usize, 3, 5, 7, 10, 11, 14, 15].iter().enumerate() {
                motif_hit[st] = true;
                motif_deg[st] = (i + 1 + rng.range(3)) % 7;
                motif_use_scale[st] = true;
                motif_gate[st] = if st == 0 || st == 7 { 2.2 } else { 1.1 };
                motif_slide[st] = st == 3 || st == 5 || st == 11 || st == 15;
            }
        }
    }

    for bar in 0..4 {
        let slot = prog.slots[bar];
        let base = bar * 16;
        let bass_root = bass_midi_for(root_note, slot.offset);
        let pad_notes = voice_led_chord(root_note, slot, 60);
        let pad_root = pad_notes.first().copied().unwrap_or(57);
        let arp_tones = arp_tones_for(root_note, slot);

        // 1. CHORDS / PAD (MoogPad)
        if chord_var == 0 {
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
            Vibe::Arena80s => {
                // Iconic 80s OB-X anthem chord punches (Van Halen / synthpop rhythm!)
                for &(st, gate, vel) in &[(0, 4.5, 0.92), (6, 2.2, 0.80), (10, 4.5, 0.86)] {
                    let s = &mut pad.steps[base + st];
                    s.active = true;
                    s.note = pad_root;
                    s.chord = slot.chord;
                    s.velocity = vel;
                    s.gate = gate;
                    s.accent = st == 0 || st == 10;
                }
            }
            Vibe::IndieRock => {
                // Crunchy rhythmic electric guitar power-chord strums!
                for &(st, gate, vel) in &[(0, 2.4, 0.92), (3, 1.4, 0.76), (6, 1.6, 0.84), (8, 2.4, 0.90), (12, 2.0, 0.86)] {
                    let s = &mut pad.steps[base + st];
                    s.active = true;
                    s.note = pad_root;
                    s.chord = slot.chord;
                    s.velocity = vel;
                    s.gate = gate;
                    s.accent = st == 0 || st == 8;
                }
            }
            Vibe::BoomBap => {
                // Chopped 90s jazz-hop Wurlitzer/Rhodes chords!
                for &(st, gate, vel) in &[(0, 4.2, 0.84), (10, 3.6, 0.76)] {
                    let s = &mut pad.steps[base + st];
                    s.active = true;
                    s.note = pad_root;
                    s.chord = slot.chord;
                    s.velocity = vel;
                    s.gate = gate;
                    s.accent = st == 0;
                }
                if bar % 2 == 1 {
                    let s = &mut pad.steps[base + 7];
                    s.active = true;
                    s.note = pad_root;
                    s.chord = slot.chord;
                    s.velocity = 0.64;
                    s.gate = 1.8;
                }
            }
            Vibe::Dubstep => {
                // Sparse brooding power-chord stab on bar downbeat (leaving space for wobble bass)
                let s = &mut pad.steps[base];
                s.active = true;
                s.note = pad_root;
                s.chord = slot.chord;
                s.velocity = 0.76;
                s.gate = 3.5;
                s.accent = true;
            }
            Vibe::BigRoomEdm => {
                // Massive mainstage festival supersaw chord rhythm!
                for &(st, gate, vel) in &[(0, 2.6, 0.94), (3, 2.2, 0.82), (6, 2.4, 0.88), (10, 2.6, 0.90), (14, 1.8, 0.82)] {
                    let s = &mut pad.steps[base + st];
                    s.active = true;
                    s.note = pad_root;
                    s.chord = slot.chord;
                    s.velocity = vel;
                    s.gate = gate;
                    s.accent = st == 0 || st == 10;
                }
            }
            Vibe::FutureBass => {
                // Iconic pumping Future Bass supersaw 9th chords!
                for &(st, gate, vel) in &[(0, 5.2, 0.92), (6, 3.4, 0.82), (10, 5.2, 0.88)] {
                    let s = &mut pad.steps[base + st];
                    s.active = true;
                    s.note = pad_root;
                    s.chord = slot.chord;
                    s.velocity = vel;
                    s.gate = gate;
                    s.accent = st == 0 || st == 10;
                }
            }
            _ => {
                // Syncopated funk/soul keyboard comping instead of a flat whole-bar drone
                let comp_hits: &[(usize, f32, f32)] = if bar % 2 == 0 {
                    &[(0, 4.8, 0.80), (6, 1.1, 0.70), (10, 3.4, 0.76)]
                } else {
                    &[(0, 4.6, 0.82), (7, 1.0, 0.72), (10, 2.8, 0.78), (15, 0.85, 0.74)]
                };
                for &(st, gate, vel) in comp_hits {
                    let s = &mut pad.steps[base + st];
                    s.active = true;
                    s.note = pad_root;
                    s.chord = slot.chord;
                    s.velocity = vel;
                    s.gate = gate;
                    s.accent = st == 0 || st == 15;
                }
            }
        }
        } else {
            let hits: &[(usize, f32, f32)] = match chord_var {
                1 => {
                    if bar % 2 == 1 {
                        &[(0, 4.8, 0.80), (6, 1.1, 0.70), (10, 3.4, 0.76), (15, 0.85, 0.74)]
                    } else {
                        &[(0, 5.2, 0.82), (7, 1.2, 0.72), (10, 3.8, 0.78)]
                    }
                }
                2 => &[(0, 1.2, 0.86), (3, 0.65, 0.66), (6, 1.1, 0.76), (7, 0.65, 0.70), (10, 2.0, 0.84), (12, 0.8, 0.72), (15, 0.7, 0.78)],
                3 => &[(0, 1.8, 0.90), (2, 0.8, 0.72), (6, 1.5, 0.82), (9, 0.9, 0.74), (11, 1.6, 0.86), (14, 1.0, 0.78)],
                _ => {
                    if bar % 2 == 0 {
                        &[(2, 1.3, 0.84), (7, 0.8, 0.72), (10, 1.6, 0.78)]
                    } else {
                        &[(0, 1.8, 0.82), (6, 0.9, 0.74), (10, 1.4, 0.78), (15, 0.75, 0.76)]
                    }
                }
            };
            for &(st, gate, vel) in hits {
                let s = &mut pad.steps[base + st];
                s.active = true;
                s.note = pad_root;
                s.chord = slot.chord;
                s.velocity = vel;
                s.gate = gate;
                s.accent = st == 0 || st == 15;
            }
        }
        // Stamp expressive bright wah MotionLocks on short offbeat funk stabs; keep sustained chords open!
        for st in 0..16 {
            let s = &mut pad.steps[base + st];
            if s.active && (st % 2 == 1 || st == 6 || st == 15) && s.gate <= 2.0 {
                s.motion = MotionLocks {
                    cutoff: Some(if st == 15 { 0.86 } else { 0.80 }),
                    resonance: Some(0.42),
                    drive: Some(0.32),
                    shape: None,
                };
            }
        }

        // 2. BASS (Moog)
        if bass_var == 0 {
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
            Vibe::Disco70s | Vibe::ItaloCosmic => {
                // Classic 70s/Italo pumping octave bass!
                for s_idx in (0..16).step_by(2) {
                    let s = &mut bass.steps[base + s_idx];
                    s.active = true;
                    let high_oct = s_idx % 4 == 2;
                    s.note = if high_oct { (bass_root + 12).min(58) } else { bass_root };
                    s.velocity = if high_oct { 0.84 } else { 0.94 };
                    s.gate = 1.35;
                    s.accent = s_idx % 4 == 0;
                }
            }
            Vibe::Arena80s => {
                // Driving 80s OB-X arena 8th-note bass with punchy fifth lift
                for s_idx in (0..16).step_by(2) {
                    let s = &mut bass.steps[base + s_idx];
                    s.active = true;
                    let iv = if s_idx == 14 { 7 } else { 0 };
                    s.note = (bass_root + iv).min(54);
                    s.velocity = if s_idx % 4 == 0 { 0.94 } else { 0.78 };
                    s.gate = 1.65;
                    s.accent = s_idx == 0 || s_idx == 8;
                }
            }
            Vibe::IndieRock => {
                // Driving picked electric rock bass with passing fifth/octave
                for &(st, iv, gate, vel) in &[
                    (0, 0, 1.8, 0.94),
                    (2, 0, 1.6, 0.80),
                    (4, 0, 1.8, 0.86),
                    (6, 7, 1.4, 0.82),
                    (8, 0, 1.8, 0.92),
                    (10, 0, 1.5, 0.80),
                    (12, 7, 1.6, 0.86),
                    (14, 12, 1.4, 0.84),
                ] {
                    let s = &mut bass.steps[base + st];
                    s.active = true;
                    s.note = (bass_root + iv).min(56);
                    s.velocity = vel;
                    s.gate = gate;
                    s.accent = st == 0 || st == 8;
                }
            }
            Vibe::NeoSoul => {
                // Silky syncopated Neo-Soul sub-bass pocket
                for &(st, iv, gate, vel) in &[(0, 0, 4.5, 0.90), (7, 7, 1.6, 0.76), (10, 0, 3.2, 0.84), (14, 10, 1.6, 0.78)] {
                    let s = &mut bass.steps[base + st];
                    s.active = true;
                    s.note = (bass_root + iv).min(55);
                    s.velocity = vel;
                    s.gate = gate;
                    s.slide = st == 14;
                }
            }
            Vibe::BoomBap => {
                // Warm 90s Hip-Hop sub-bass locked to the boom-bap kick pocket
                for &(st, iv, gate, vel) in &[(0, 0, 4.5, 0.94), (7, 0, 2.2, 0.78), (10, 7, 3.4, 0.88)] {
                    let s = &mut bass.steps[base + st];
                    s.active = true;
                    s.note = (bass_root + iv).min(54);
                    s.velocity = vel;
                    s.gate = gate;
                    s.accent = st == 0 || st == 10;
                }
                if bar % 2 == 1 {
                    let s = &mut bass.steps[base + 14];
                    s.active = true;
                    s.note = (bass_root + 10).min(56);
                    s.velocity = 0.80;
                    s.gate = 1.8;
                    s.slide = true;
                }
            }
            Vibe::Trap808 => {
                // Booming 808 sub-bass with signature octave trap slides!
                let hits: &[(usize, u8, f32, f32, bool)] = if bar % 2 == 0 {
                    &[
                        (0, 0, 5.5, 0.98, false),
                        (6, 0, 3.2, 0.86, false),
                        (10, 0, 4.5, 0.94, false),
                    ]
                } else {
                    &[
                        (0, 0, 5.0, 0.98, false),
                        (6, 7, 2.5, 0.84, true),
                        (10, 0, 3.5, 0.94, false),
                        (14, 12, 1.8, 0.88, true),
                    ]
                };
                for &(st, iv, gate, vel, slide) in hits {
                    let s = &mut bass.steps[base + st];
                    s.active = true;
                    s.note = (bass_root + iv).min(56);
                    s.velocity = vel;
                    s.gate = gate;
                    s.accent = st == 0 || st == 10;
                    s.slide = slide;
                }
            }
            Vibe::Dubstep => {
                // Heavy 140 BPM LFO Wobble Bass phrases with Phrygian half-step slides!
                let hits: &[(usize, u8, f32, f32, bool)] = if bar % 2 == 0 {
                    &[
                        (0, 0, 7.2, 0.98, false),
                        (10, 1, 5.2, 0.92, true),
                    ]
                } else {
                    &[
                        (0, 0, 5.5, 0.98, false),
                        (6, 7, 1.8, 0.88, true),
                        (10, 0, 3.5, 0.94, false),
                        (14, 12, 1.6, 0.90, true),
                    ]
                };
                for &(st, iv, gate, vel, slide) in hits {
                    let s = &mut bass.steps[base + st];
                    s.active = true;
                    s.note = (bass_root + iv).min(56);
                    s.velocity = vel;
                    s.gate = gate;
                    s.accent = st == 0 || st == 10;
                    s.slide = slide;
                }
            }
            Vibe::BigRoomEdm => {
                // Pumping mainstage offbeat/syncopated EDM bass!
                for &(st, iv, gate, vel) in &[
                    (0, 0, 1.6, 0.92),
                    (2, 0, 1.8, 0.88),
                    (6, 0, 1.8, 0.90),
                    (10, 7, 1.8, 0.88),
                    (14, 12, 1.6, 0.86),
                ] {
                    let s = &mut bass.steps[base + st];
                    s.active = true;
                    s.note = (bass_root + iv).min(56);
                    s.velocity = vel;
                    s.gate = gate;
                    s.accent = st == 2 || st == 10;
                }
            }
            Vibe::FutureBass => {
                // Deep gliding Future Bass 808 sub locked to the chord pumps
                for &(st, iv, gate, vel, slide) in &[
                    (0, 0, 5.2, 0.96, false),
                    (6, 0, 3.2, 0.86, false),
                    (10, 0, 4.2, 0.92, false),
                    (14, 7, 1.8, 0.84, true),
                ] {
                    let s = &mut bass.steps[base + st];
                    s.active = true;
                    s.note = (bass_root + iv).min(55);
                    s.velocity = vel;
                    s.gate = gate;
                    s.accent = st == 0 || st == 10;
                    s.slide = slide;
                }
            }
            Vibe::EuphoricTrance => {
                // Classic rolling 16th-note trance bassline (offbeat 16th gallop!)
                for &st in &[2, 3, 6, 7, 10, 11, 14, 15] {
                    let s = &mut bass.steps[base + st];
                    s.active = true;
                    let oct = if st == 14 || st == 15 { 12 } else { 0 };
                    s.note = (bass_root + oct).min(58);
                    s.velocity = if st % 4 == 2 { 0.92 } else { 0.76 };
                    s.gate = 0.85;
                    s.accent = st % 4 == 2;
                }
            }
        }
        } else {
            let chord_ivs = slot.chord.intervals();
            match bass_var {
                1 => {
                    // Syncopated octave bounce with slides
                    let hits: &[(usize, u8, f32, f32, bool)] = if bar % 2 == 0 {
                        &[(0, 0, 2.2, 0.94, false), (3, 12, 1.1, 0.80, false), (6, 0, 1.8, 0.86, false), (10, 7, 1.8, 0.84, false), (14, 12, 1.4, 0.82, true)]
                    } else {
                        &[(0, 0, 2.0, 0.94, false), (6, 7, 1.5, 0.82, true), (8, 0, 1.6, 0.88, false), (11, 12, 1.2, 0.80, false), (14, 10, 1.4, 0.84, true)]
                    };
                    for &(st, iv, gate, vel, slide) in hits {
                        let s = &mut bass.steps[base + st];
                        s.active = true;
                        s.note = (bass_root + iv).min(58);
                        s.velocity = vel;
                        s.gate = gate;
                        s.accent = st == 0 || st == 8;
                        s.slide = slide;
                    }
                }
                2 => {
                    // Driving rhythmic 8th / offbeat bass pulse
                    let steps: &[usize] = if matches!(vibe, Vibe::DeepHouse | Vibe::DetroitTechno | Vibe::AcidClub) {
                        &[2, 6, 10, 14]
                    } else {
                        &[0, 2, 4, 6, 8, 10, 12, 14]
                    };
                    for &st in steps {
                        let s = &mut bass.steps[base + st];
                        s.active = true;
                        let iv = if st == 14 { 7 } else if st % 4 == 2 { 12 } else { 0 };
                        s.note = (bass_root + iv).min(58);
                        s.velocity = if st % 4 == 0 { 0.92 } else { 0.78 };
                        s.gate = 1.25;
                        s.accent = st % 4 == 0;
                    }
                }
                3 => {
                    // Deep gliding sub pocket with 5th / 7th turnaround
                    for &(st, iv, gate, slide) in &[(0usize, 0u8, 5.2_f32, false), (7, 7, 2.0, true), (10, 0, 3.2, false), (14, 10, 1.6, true)] {
                        let s = &mut bass.steps[base + st];
                        s.active = true;
                        s.note = (bass_root + iv).min(56);
                        s.velocity = if st == 0 || st == 10 { 0.92 } else { 0.78 };
                        s.gate = gate;
                        s.accent = st == 0;
                        s.slide = slide;
                    }
                }
                _ => {
                    // Melodic walking chord-tone bass
                    for (i, &st) in [0usize, 4, 6, 10, 12, 14].iter().enumerate() {
                        let iv = chord_ivs[(i + bar) % chord_ivs.len()] % 12;
                        let s = &mut bass.steps[base + st];
                        s.active = true;
                        s.note = (bass_root + iv).min(58);
                        s.velocity = if st == 0 || st == 10 { 0.90 } else { 0.76 };
                        s.gate = 1.5;
                        s.accent = st == 0;
                        s.slide = st == 14;
                    }
                }
            }
        }

        // Funk Slap & Chromatic Walkup + Mutron Auto-Wah MotionLocks pass across all 4 bars!
        {
            let next_slot = prog.slots[(bar + 1) % 4];
            let next_bass = bass_midi_for(root_note, next_slot.offset);
            if !matches!(vibe, Vibe::AmbientDub | Vibe::JungleDnb) {
                let slap_st = if bar % 2 == 0 { 3 } else { 11 };
                if !bass.steps[base + slap_st].active {
                    let s = &mut bass.steps[base + slap_st];
                    s.active = true;
                    s.note = (bass_root + if slap_st == 3 { 12 } else { 10 }).min(58);
                    s.velocity = 0.80;
                    s.gate = 0.45;
                    s.accent = true;
                }
                if !bass.steps[base + 15].active {
                    let s = &mut bass.steps[base + 15];
                    s.active = true;
                    s.note = if (bar + bass_var) % 2 == 0 {
                        next_bass.saturating_sub(1).max(33)
                    } else {
                        (next_bass + 1).min(58)
                    };
                    s.velocity = 0.76;
                    s.gate = 0.90;
                    s.slide = true;
                }
            }
            for st in 0..16 {
                let s = &mut bass.steps[base + st];
                if !s.active {
                    continue;
                }
                let is_pop = st % 2 == 1 || st == 2 || st == 6 || st == 14 || s.note > bass_root + 5;
                if st == 0 || (st % 4 == 0 && s.note <= bass_root) {
                    s.motion = MotionLocks {
                        cutoff: Some(0.44),
                        resonance: Some(0.36),
                        drive: Some(0.48),
                        shape: None,
                    };
                } else if is_pop {
                    let phase = ((st + bar) % 4) as f32 / 3.0;
                    s.motion = MotionLocks {
                        cutoff: Some(0.70 + phase * 0.16),
                        resonance: Some(0.58 + phase * 0.18),
                        drive: Some(0.56 + phase * 0.18),
                        shape: Some(0.65),
                    };
                } else {
                    s.motion = MotionLocks {
                        cutoff: Some(0.56),
                        resonance: Some(0.42),
                        drive: Some(0.46),
                        shape: Some(0.45),
                    };
                }
            }
        }

        // 3. ARP / MELODY (Monologue) — Coherent per-song motif locked to chord tones & Vibe scale!
        let chord_ivs = slot.chord.intervals();
        for st in 0..16 {
            if !motif_hit[st] {
                continue;
            }
            let s = &mut arp.steps[base + st];
            s.active = true;
            let bar_shift = if bar == 3 { 1 } else if bar == 1 { 0 } else { bar % 2 };
            let mut note = if matches!(vibe, Vibe::Dubstep) {
                (bass_root + 12 + chord_ivs[(motif_deg[st] + bar) % chord_ivs.len()]).clamp(40, 60)
            } else if motif_use_scale[st] && !scale_notes.is_empty() {
                let anchor = arp_tones.first().copied().unwrap_or(64);
                let anchor_pos = scale_notes.iter().position(|&n| n >= anchor).unwrap_or(0);
                let idx = (anchor_pos + motif_deg[st] + bar_shift) % scale_notes.len();
                scale_notes[idx]
            } else {
                arp_tones[(motif_deg[st] + bar_shift) % arp_tones.len().max(1)]
            };
            if bar == 3 && st >= 10 && note <= 74 && !matches!(vibe, Vibe::Dubstep) {
                note = (note + 12).min(86);
            }
            s.note = note;
            s.velocity = if st == 0 || st == 10 { 0.92 } else if st % 2 == 0 { 0.84 } else { 0.76 };
            s.gate = if matches!(vibe, Vibe::Dubstep) {
                1.8
            } else if matches!(vibe, Vibe::ElectroSoul | Vibe::Synthwave | Vibe::Arena80s | Vibe::IndieRock | Vibe::BigRoomEdm) {
                (motif_gate[st] * 1.15).clamp(0.55, 3.2)
            } else {
                motif_gate[st].max(0.55)
            };
            s.accent = st == 0 || st == 15;
            s.slide = motif_slide[st] || (bar == 3 && st == 14);
            if st % 2 == 1 || s.accent {
                let wah_phase = ((st + bar * 3) % 5) as f32 / 4.0;
                s.motion = MotionLocks {
                    cutoff: Some(0.78 + wah_phase * 0.14),
                    resonance: Some(0.44 + wah_phase * 0.14),
                    drive: Some(0.36 + wah_phase * 0.24),
                    shape: Some(0.38 + wah_phase * 0.42),
                };
            }
        }

        // 4. TB-303 ACID / COUNTER-LINE — Generated for all 22 vibes so Party DJ can spotlight it anywhere!
        {
            let dense_vibe = matches!(
                vibe,
                Vibe::AcidClub | Vibe::ElectroFunk | Vibe::ItaloCosmic | Vibe::DetroitTechno | Vibe::BigRoomEdm | Vibe::EuphoricTrance
            );
            let hits: &[usize] = match acid_var {
                0 => {
                    if dense_vibe {
                        &[0, 2, 3, 6, 8, 10, 11, 14, 15]
                    } else {
                        &[0, 3, 6, 10, 12, 15]
                    }
                }
                1 => &[2, 5, 6, 10, 13, 14],
                2 => &[0, 6, 10, 14],
                _ => {
                    if bar % 2 == 1 {
                        &[0, 2, 6, 8, 10, 11, 14, 15]
                    } else {
                        &[0, 10]
                    }
                }
            };
            for (i, &st) in hits.iter().enumerate() {
                let iv = chord_ivs[(i + bar + (acid_var % 3)) % chord_ivs.len()];
                let oct_bump = if (i + acid_var) % 3 == 2 { 12 } else { 0 };
                let s = &mut acid.steps[base + st];
                s.active = true;
                s.note = (bass_root + iv + oct_bump).clamp(33, 66);
                s.velocity = if i % 3 == 0 { 0.94 } else { 0.82 };
                s.gate = if acid_var == 2 { 1.35 } else { 0.90 };
                s.accent = i % 3 == 0;
                s.slide = st == 3 || st == 11 || st == 14 || st == 15;
                let sq_phase = ((i + bar) % 4) as f32 / 3.0;
                s.motion = MotionLocks {
                    cutoff: Some(0.58 + sq_phase * 0.32),
                    resonance: Some(0.58 + sq_phase * 0.26),
                    drive: Some(0.34 + sq_phase * 0.34),
                    shape: Some(0.60 + sq_phase * 0.30),
                };
            }
        }

        // 5. SAMPLER TEXTURE (Rhodes / Brass / Choir / Bell / Strings / Guitar — 100% chord-root locked!)
        match sampler_var {
            0 => match vibe {
                Vibe::LofiChill | Vibe::NeoSoul => {
                    let s = &mut sampler.steps[base];
                    s.active = true;
                    s.note = (48 + (root_note + slot.offset) % 12).clamp(48, 63);
                    s.velocity = 0.86;
                    s.gate = 12.0;
                }
                Vibe::BoomBap => {
                    for &(st, gate, vel) in &[(0, 4.0, 0.92), (10, 3.5, 0.80)] {
                        let s = &mut sampler.steps[base + st];
                        s.active = true;
                        s.note = (48 + (root_note + slot.offset) % 12).clamp(48, 63);
                        s.velocity = vel;
                        s.gate = gate;
                    }
                }
                Vibe::Trap808 | Vibe::FutureBass => {
                    for &(st, vel) in &[(0, 0.86), (8, 0.70)] {
                        let s = &mut sampler.steps[base + st];
                        s.active = true;
                        s.note = arp_tones.last().copied().unwrap_or(69);
                        s.velocity = vel;
                        s.gate = 4.0;
                    }
                }
                Vibe::Dubstep => {
                    if bar % 2 == 0 {
                        let s = &mut sampler.steps[base];
                        s.active = true;
                        s.note = (48 + (root_note + slot.offset) % 12).clamp(48, 60);
                        s.velocity = 0.92;
                        s.gate = 4.0;
                    }
                }
                Vibe::BigRoomEdm | Vibe::EuphoricTrance | Vibe::Disco70s => {
                    let s = &mut sampler.steps[base];
                    s.active = true;
                    s.note = (60 + (root_note + slot.offset) % 12).clamp(55, 72);
                    s.velocity = 0.80;
                    s.gate = 14.0;
                }
                Vibe::ElectroFunk | Vibe::Arena80s => {
                    for &st in &[0, 10] {
                        let s = &mut sampler.steps[base + st];
                        s.active = true;
                        s.note = (60 + (root_note + slot.offset) % 12).clamp(55, 72);
                        s.velocity = if st == 0 { 0.90 } else { 0.76 };
                        s.gate = 2.0;
                    }
                }
                Vibe::IndieRock => {
                    for &st in &[0, 6, 10, 14] {
                        let s = &mut sampler.steps[base + st];
                        s.active = true;
                        s.note = (52 + (root_note + slot.offset) % 12).clamp(48, 68);
                        s.velocity = if st == 0 { 0.94 } else { 0.80 };
                        s.gate = 2.2;
                    }
                }
                _ => {
                    if bar % 2 == 0 {
                        let s = &mut sampler.steps[base + 8];
                        s.active = true;
                        s.note = arp_tones.last().copied().unwrap_or(69);
                        s.velocity = 0.72;
                        s.gate = 4.0;
                    }
                }
            },
            1 => {
                let s = &mut sampler.steps[base];
                s.active = true;
                s.note = (54 + (root_note + slot.offset) % 12).clamp(48, 70);
                s.velocity = 0.82;
                s.gate = 8.0;
            }
            2 => {
                let hits: &[usize] = if bar % 2 == 0 { &[0, 10] } else { &[6, 14] };
                for &st in hits {
                    let s = &mut sampler.steps[base + st];
                    s.active = true;
                    s.note = (52 + (root_note + slot.offset) % 12).clamp(48, 68);
                    s.velocity = if st == 0 { 0.88 } else { 0.74 };
                    s.gate = 2.5;
                }
            }
            _ => {
                let hits: &[usize] = if bar % 2 == 1 { &[4, 12] } else { &[0, 10] };
                for &st in hits {
                    let s = &mut sampler.steps[base + st];
                    s.active = true;
                    s.note = arp_tones[(st / 4 + bar) % arp_tones.len().max(1)];
                    s.velocity = 0.78;
                    s.gate = 3.0;
                }
            }
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
    p.active_tracks = [true; 6];

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
        Vibe::Disco70s => {
            apply_voice_preset(p, TrackId::Drums, 5);     // 70s Disco Crisp
            apply_voice_preset(p, TrackId::Moog, 6);      // 70s Disco Octave
            apply_voice_preset(p, TrackId::MoogPad, 4);   // 70s String Synth
            apply_voice_preset(p, TrackId::Monologue, 6); // 70s String Lead
            apply_voice_preset(p, TrackId::Tb303, 5);     // Melodic Pluck 303
            apply_voice_preset(p, TrackId::Sampler, 5);   // 70s Solina String
            p.sidechain_amount = 0.42;
        }
        Vibe::Arena80s => {
            apply_voice_preset(p, TrackId::Drums, 4);     // 80s Linn Gate
            apply_voice_preset(p, TrackId::Moog, 4);      // OB-X Power Bass
            apply_voice_preset(p, TrackId::MoogPad, 5);   // OB-X 80s Power
            apply_voice_preset(p, TrackId::Monologue, 4); // OB-X Power Lead
            apply_voice_preset(p, TrackId::Tb303, 5);     // Melodic Pluck 303
            apply_voice_preset(p, TrackId::Sampler, 6);   // OB-X Power Chord
            p.sidechain_amount = 0.46;
        }
        Vibe::IndieRock => {
            apply_voice_preset(p, TrackId::Drums, 7);     // Breakbeat Crunch
            apply_voice_preset(p, TrackId::Moog, 5);      // Electric Bass Pick
            apply_voice_preset(p, TrackId::MoogPad, 6);   // Electric Guitar
            apply_voice_preset(p, TrackId::Monologue, 5); // Electric Guitar Lead
            apply_voice_preset(p, TrackId::Tb303, 4);     // Distorted 303
            apply_voice_preset(p, TrackId::Sampler, 7);   // Electric Guitar (Karplus-Strong)
            p.sidechain_amount = 0.22;
        }
        Vibe::NeoSoul => {
            apply_voice_preset(p, TrackId::Drums, 3);     // Lo-Fi Dusty
            apply_voice_preset(p, TrackId::Moog, 0);      // Warm Sub
            apply_voice_preset(p, TrackId::MoogPad, 7);   // Wurlitzer Keys
            apply_voice_preset(p, TrackId::Monologue, 1); // Sparkle Bell
            apply_voice_preset(p, TrackId::Tb303, 3);     // Deep Sub 303
            apply_voice_preset(p, TrackId::Sampler, 0);   // Minor9 Rhodes
            p.sidechain_amount = 0.28;
        }
        Vibe::ItaloCosmic => {
            apply_voice_preset(p, TrackId::Drums, 4);     // 80s Linn Gate
            apply_voice_preset(p, TrackId::Moog, 6);      // 70s Disco Octave
            apply_voice_preset(p, TrackId::MoogPad, 4);   // 70s String Synth
            apply_voice_preset(p, TrackId::Monologue, 7); // Sync Screamer
            apply_voice_preset(p, TrackId::Tb303, 2);     // Funky Acid
            apply_voice_preset(p, TrackId::Sampler, 3);   // Retro FM Bell
            p.sidechain_amount = 0.52;
        }
        Vibe::BoomBap => {
            apply_voice_preset(p, TrackId::Drums, 3);     // Lo-Fi Dusty
            apply_voice_preset(p, TrackId::Moog, 8);      // Trap 808 Sub (warm hip-hop sub)
            apply_voice_preset(p, TrackId::MoogPad, 7);   // Wurlitzer Keys
            apply_voice_preset(p, TrackId::Monologue, 1); // Sparkle Bell
            apply_voice_preset(p, TrackId::Tb303, 3);     // Deep Sub 303
            apply_voice_preset(p, TrackId::Sampler, 0);   // Minor9 Rhodes
            p.sidechain_amount = 0.34;
        }
        Vibe::Trap808 => {
            apply_voice_preset(p, TrackId::Drums, 8);     // Trap 808 Boom
            apply_voice_preset(p, TrackId::Moog, 8);      // Trap 808 Sub
            apply_voice_preset(p, TrackId::MoogPad, 0);   // Lush Pad
            apply_voice_preset(p, TrackId::Monologue, 1); // Sparkle Bell
            apply_voice_preset(p, TrackId::Tb303, 3);     // Deep Sub 303
            apply_voice_preset(p, TrackId::Sampler, 3);   // Retro FM Bell
            p.sidechain_amount = 0.44;
        }
        Vibe::Dubstep => {
            apply_voice_preset(p, TrackId::Drums, 9);     // Dubstep Half-Time
            apply_voice_preset(p, TrackId::Moog, 9);      // Dubstep Wobble
            apply_voice_preset(p, TrackId::MoogPad, 2);   // Dub Chord
            apply_voice_preset(p, TrackId::Monologue, 8); // Dubstep Growl
            apply_voice_preset(p, TrackId::Tb303, 4);     // Distorted 303
            apply_voice_preset(p, TrackId::Sampler, 1);   // Brass Stab
            p.sidechain_amount = 0.66;
        }
        Vibe::BigRoomEdm => {
            apply_voice_preset(p, TrackId::Drums, 1);     // 909 Club
            apply_voice_preset(p, TrackId::Moog, 0);      // Warm Sub
            apply_voice_preset(p, TrackId::MoogPad, 8);   // Festival Supersaw
            apply_voice_preset(p, TrackId::Monologue, 4); // OB-X Power Lead
            apply_voice_preset(p, TrackId::Tb303, 1);     // Liquid 303
            apply_voice_preset(p, TrackId::Sampler, 2);   // Choir Ahh
            p.sidechain_amount = 0.72;
        }
        Vibe::FutureBass => {
            apply_voice_preset(p, TrackId::Drums, 8);     // Trap 808 Boom
            apply_voice_preset(p, TrackId::Moog, 8);      // Trap 808 Sub
            apply_voice_preset(p, TrackId::MoogPad, 9);   // Future Bass Saw
            apply_voice_preset(p, TrackId::Monologue, 1); // Sparkle Bell
            apply_voice_preset(p, TrackId::Tb303, 5);     // Melodic Pluck 303
            apply_voice_preset(p, TrackId::Sampler, 3);   // Retro FM Bell
            p.sidechain_amount = 0.68;
        }
        Vibe::EuphoricTrance => {
            apply_voice_preset(p, TrackId::Drums, 1);     // 909 Club
            apply_voice_preset(p, TrackId::Moog, 10);     // Trance Rolling Bass
            apply_voice_preset(p, TrackId::MoogPad, 8);   // Festival Supersaw
            apply_voice_preset(p, TrackId::Monologue, 9); // Trance Pluck
            apply_voice_preset(p, TrackId::Tb303, 1);     // Liquid 303
            apply_voice_preset(p, TrackId::Sampler, 2);   // Choir Ahh
            p.sidechain_amount = 0.66;
        }
    }

    // Balanced, punchy, non-clipping stereo mix hierarchy!
    // Melodic layers (Chords, Arp/Lead, 303, Sampler) sit forward alongside tight Drums & Bass!
    let ch = &mut p.mixer.channels;
    // 0: Moog (Bass)
    ch[0].volume = if matches!(vibe, Vibe::Dubstep | Vibe::Trap808) { 0.68 } else { 0.62 };
    ch[0].pan = 0.0;
    ch[0].delay_send = 0.0;
    ch[0].reverb_send = if matches!(vibe, Vibe::Dubstep) { 0.08 } else { 0.04 };
    ch[0].mute = false;
    ch[0].solo = false;
    // 1: Monologue (Arp / Lead)
    ch[1].volume = if matches!(vibe, Vibe::Arena80s | Vibe::IndieRock | Vibe::Dubstep | Vibe::BigRoomEdm | Vibe::EuphoricTrance | Vibe::ElectroFunk) { 0.86 } else { 0.80 };
    ch[1].pan = 0.26;
    ch[1].delay_send = if matches!(vibe, Vibe::IndieRock) { 0.26 } else if matches!(vibe, Vibe::EuphoricTrance | Vibe::BigRoomEdm) { 0.38 } else { 0.32 };
    ch[1].reverb_send = if matches!(vibe, Vibe::Arena80s | Vibe::Disco70s | Vibe::EuphoricTrance | Vibe::BigRoomEdm | Vibe::FutureBass) { 0.28 } else { 0.18 };
    ch[1].mute = false;
    ch[1].solo = false;
    // 2: TB-303 (Acid / Funk Lead)
    ch[2].volume = if matches!(vibe, Vibe::AcidClub) { 0.76 } else if matches!(vibe, Vibe::ElectroFunk | Vibe::ItaloCosmic) { 0.68 } else { 0.60 };
    ch[2].pan = -0.14;
    ch[2].delay_send = 0.22;
    ch[2].reverb_send = 0.12;
    ch[2].mute = !matches!(vibe, Vibe::AcidClub | Vibe::ElectroFunk | Vibe::ItaloCosmic);
    ch[2].solo = false;
    // 3: Drums (TIGHT & PUNCHY)
    ch[3].volume = 0.72;
    ch[3].pan = 0.0;
    ch[3].delay_send = if matches!(vibe, Vibe::AmbientDub) { 0.16 } else { 0.04 };
    ch[3].reverb_send = if matches!(vibe, Vibe::Synthwave | Vibe::Arena80s | Vibe::Dubstep | Vibe::BigRoomEdm) { 0.18 } else { 0.08 };
    ch[3].mute = false;
    ch[3].solo = false;
    // 4: Sampler (Rhodes / Brass / Bell / Strings / Guitar Texture)
    ch[4].volume = if matches!(vibe, Vibe::LofiChill | Vibe::NeoSoul | Vibe::BoomBap) { 0.84 } else if matches!(vibe, Vibe::ElectroFunk | Vibe::Disco70s | Vibe::Arena80s | Vibe::IndieRock | Vibe::Dubstep) { 0.78 } else { 0.72 };
    ch[4].pan = -0.24;
    ch[4].delay_send = 0.22;
    ch[4].reverb_send = if matches!(vibe, Vibe::Disco70s | Vibe::Arena80s | Vibe::EuphoricTrance | Vibe::FutureBass) { 0.32 } else { 0.24 };
    ch[4].mute = matches!(vibe, Vibe::AcidClub | Vibe::AmbientDub);
    ch[4].solo = false;

    // MoogPad (Chords)
    p.mixer.moog_pad_volume = if matches!(vibe, Vibe::LofiChill | Vibe::NeoSoul | Vibe::BoomBap) { 0.80 } else if matches!(vibe, Vibe::Arena80s | Vibe::Disco70s | Vibe::IndieRock | Vibe::BigRoomEdm | Vibe::FutureBass | Vibe::EuphoricTrance) { 0.88 } else { 0.84 };
    p.mixer.moog_pad_delay_send = if matches!(vibe, Vibe::AmbientDub) { 0.56 } else if matches!(vibe, Vibe::IndieRock | Vibe::BoomBap) { 0.18 } else { 0.24 };
    p.mixer.moog_pad_reverb_send = if matches!(vibe, Vibe::AmbientDub) { 0.52 } else if matches!(vibe, Vibe::Disco70s | Vibe::Arena80s | Vibe::BigRoomEdm | Vibe::FutureBass | Vibe::EuphoricTrance) { 0.34 } else { 0.26 };
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
    p.fx.delay_feedback = if matches!(vibe, Vibe::AmbientDub) { 0.62 } else if matches!(vibe, Vibe::EuphoricTrance) { 0.46 } else { 0.36 };
    p.fx.delay_tone = if matches!(vibe, Vibe::LofiChill | Vibe::AmbientDub | Vibe::NeoSoul | Vibe::BoomBap) { 0.42 } else { 0.58 };
    p.fx.delay_mix = if matches!(vibe, Vibe::AmbientDub) { 0.48 } else if matches!(vibe, Vibe::EuphoricTrance) { 0.28 } else { 0.22 };
    p.fx.reverb_size = if matches!(vibe, Vibe::AmbientDub) { 0.84 } else if matches!(vibe, Vibe::Synthwave | Vibe::ElectroSoul | Vibe::Arena80s | Vibe::Disco70s | Vibe::EuphoricTrance | Vibe::BigRoomEdm | Vibe::FutureBass) { 0.70 } else { 0.54 };
    p.fx.reverb_damp = 0.52;
    p.fx.reverb_mix = if matches!(vibe, Vibe::AmbientDub) { 0.50 } else if matches!(vibe, Vibe::Arena80s | Vibe::Disco70s | Vibe::EuphoricTrance | Vibe::BigRoomEdm | Vibe::FutureBass) { 0.32 } else { 0.24 };
    p.fx.analog_warmth = if matches!(vibe, Vibe::LofiChill | Vibe::ElectroFunk | Vibe::NeoSoul | Vibe::IndieRock | Vibe::BoomBap | Vibe::Dubstep) { 0.38 } else { 0.20 };
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

/// Applies one of the 10 polyphonic synth presets (MoogPad palette) to a dynamic ExtraTrack.
pub fn apply_extra_track_preset(track: &mut super::ExtraTrack, preset_idx: usize) {
    let idx = preset_idx % voice_preset_names(TrackId::MoogPad).len();
    track.preset_idx = idx;
    let mut tmp = Project::default();
    apply_voice_preset(&mut tmp, TrackId::MoogPad, idx);
    track.synth = tmp.moog_pad;
}

/// Generates a chord-locked 64-step melodic/chord pattern tailored to a dynamic ExtraTrack's preset.
pub fn generate_extra_track_seq(
    root_note: u8,
    prog_idx: usize,
    preset_idx: usize,
    seed: u32,
) -> MelodicTrackSeq {
    let prog = progression_at(prog_idx);
    let mut rng = Rng::new(seed ^ 0x51A7_900D);
    let mut seq = MelodicTrackSeq::new(StepCount::Steps64);

    for bar in 0..4 {
        let slot = prog.slots[bar];
        let base = bar * 16;
        let pad_notes = voice_led_chord(root_note, slot, 60);
        let pad_root = pad_notes.first().copied().unwrap_or(57);

        match preset_idx % voice_preset_names(TrackId::MoogPad).len() {
            2 => {
                // Dub Chord: echo-ready offbeat stabs
                for &st in &[2, 10] {
                    let s = &mut seq.steps[base + st];
                    s.active = true;
                    s.note = pad_root;
                    s.chord = slot.chord;
                    s.velocity = if st == 2 { 0.84 } else { 0.68 };
                    s.gate = 0.9;
                    s.accent = st == 2;
                }
            }
            3 | 5 => {
                // Neon Brass / OB-X 80s Power: punchy anthem hits
                for &(st, gate, vel) in &[(0, 4.5, 0.88), (6, 2.2, 0.76), (10, 4.5, 0.82)] {
                    let s = &mut seq.steps[base + st];
                    s.active = true;
                    s.note = pad_root;
                    s.chord = slot.chord;
                    s.velocity = vel;
                    s.gate = gate;
                    s.accent = st == 0;
                }
            }
            6 => {
                // Electric Guitar: rhythmic strummed power/chord groove
                let hits: &[(usize, f32, f32)] = if rng.range(2) == 0 {
                    &[(0, 2.2, 0.88), (3, 1.4, 0.74), (6, 1.6, 0.80), (8, 2.2, 0.86), (12, 2.0, 0.82)]
                } else {
                    &[(0, 2.8, 0.88), (6, 1.8, 0.78), (10, 2.4, 0.84), (14, 1.4, 0.76)]
                };
                for &(st, gate, vel) in hits {
                    let s = &mut seq.steps[base + st];
                    s.active = true;
                    s.note = pad_root;
                    s.chord = slot.chord;
                    s.velocity = vel;
                    s.gate = gate;
                    s.accent = st == 0 || st == 8;
                }
            }
            7 => {
                // Wurlitzer Keys: warm soul syncopation
                for &(st, gate, vel) in &[(0, 5.5, 0.82), (10, 4.5, 0.74)] {
                    let s = &mut seq.steps[base + st];
                    s.active = true;
                    s.note = pad_root;
                    s.chord = slot.chord;
                    s.velocity = vel;
                    s.gate = gate;
                }
            }
            8 | 9 => {
                // Festival Supersaw / Future Bass Saw: pumping EDM chord hits
                for &(st, gate, vel) in &[(0, 5.0, 0.90), (6, 3.2, 0.80), (10, 5.0, 0.86)] {
                    let s = &mut seq.steps[base + st];
                    s.active = true;
                    s.note = pad_root;
                    s.chord = slot.chord;
                    s.velocity = vel;
                    s.gate = gate;
                    s.accent = st == 0 || st == 10;
                }
            }
            _ => {
                // Lush Pad / Warm Strings / 70s String Synth: smooth full-bar swells
                let s = &mut seq.steps[base];
                s.active = true;
                s.note = pad_root;
                s.chord = slot.chord;
                s.velocity = 0.76;
                s.gate = 14.5;
            }
        }
    }
    seq
}
