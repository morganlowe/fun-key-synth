use serde::{Deserialize, Serialize};

use super::{ClockDiv, Scale, StepCount};

pub const MAX_STEPS: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ChordType {
    #[default]
    Single,
    Power5,
    Major,
    Minor,
    Sus2,
    Sus4,
    Aug,
    Major6,
    Minor6,
    Major7,
    Minor7,
    Dom7,
    Dom7Sus4,
    Dim7,
    HalfDim7,
    Minor9,
    Major9,
    Dom9,
    Minor11,
    Add9,
}
impl ChordType {
    pub const ALL: [ChordType; 20] = [
        ChordType::Single,
        ChordType::Power5,
        ChordType::Major,
        ChordType::Minor,
        ChordType::Sus2,
        ChordType::Sus4,
        ChordType::Aug,
        ChordType::Major6,
        ChordType::Minor6,
        ChordType::Major7,
        ChordType::Minor7,
        ChordType::Dom7,
        ChordType::Dom7Sus4,
        ChordType::Dim7,
        ChordType::HalfDim7,
        ChordType::Minor9,
        ChordType::Major9,
        ChordType::Dom9,
        ChordType::Minor11,
        ChordType::Add9,
    ];
    pub const fn name(self) -> &'static str {
        match self {
            Self::Single => "Single",
            Self::Power5 => "5 (Power)",
            Self::Major => "Maj",
            Self::Minor => "Min",
            Self::Sus2 => "Sus2",
            Self::Sus4 => "Sus4",
            Self::Aug => "Aug",
            Self::Major6 => "Maj6",
            Self::Minor6 => "Min6",
            Self::Major7 => "Maj7",
            Self::Minor7 => "Min7",
            Self::Dom7 => "7",
            Self::Dom7Sus4 => "7sus4",
            Self::Dim7 => "dim7",
            Self::HalfDim7 => "m7b5",
            Self::Minor9 => "m9",
            Self::Major9 => "Maj9",
            Self::Dom9 => "9",
            Self::Minor11 => "m11",
            Self::Add9 => "add9",
        }
    }
    pub const fn intervals(self) -> &'static [u8] {
        match self {
            Self::Single => &[0],
            Self::Power5 => &[0, 7, 12],
            Self::Major => &[0, 4, 7],
            Self::Minor => &[0, 3, 7],
            Self::Sus2 => &[0, 2, 7],
            Self::Sus4 => &[0, 5, 7],
            Self::Aug => &[0, 4, 8],
            Self::Major6 => &[0, 4, 7, 9],
            Self::Minor6 => &[0, 3, 7, 9],
            Self::Major7 => &[0, 4, 7, 11],
            Self::Minor7 => &[0, 3, 7, 10],
            Self::Dom7 => &[0, 4, 7, 10],
            Self::Dom7Sus4 => &[0, 5, 7, 10],
            Self::Dim7 => &[0, 3, 6, 9],
            Self::HalfDim7 => &[0, 3, 6, 10],
            Self::Minor9 => &[0, 3, 7, 10, 14],
            Self::Major9 => &[0, 4, 7, 11, 14],
            Self::Dom9 => &[0, 4, 7, 10, 14],
            Self::Minor11 => &[0, 3, 7, 10, 17],
            Self::Add9 => &[0, 4, 7, 14],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct MotionLocks {
    pub cutoff: Option<f32>,
    pub resonance: Option<f32>,
    pub drive: Option<f32>,
    pub shape: Option<f32>,
}

impl MotionLocks {
    pub fn any_locked(&self) -> bool {
        self.cutoff.is_some()
            || self.resonance.is_some()
            || self.drive.is_some()
            || self.shape.is_some()
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn get(&self, lane: usize) -> Option<f32> {
        match lane {
            0 => self.cutoff,
            1 => self.resonance,
            2 => self.drive,
            3 => self.shape,
            _ => None,
        }
    }

    pub fn set(&mut self, lane: usize, val: Option<f32>) {
        let clamped = val.map(|v| v.clamp(0.0, 1.0));
        match lane {
            0 => self.cutoff = clamped,
            1 => self.resonance = clamped,
            2 => self.drive = clamped,
            3 => self.shape = clamped,
            _ => {}
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MelodicStep {
    pub active: bool,
    pub note: u8,
    pub velocity: f32,
    pub gate: f32,
    pub accent: bool,
    pub slide: bool,
    pub chord: ChordType,
    pub motion: MotionLocks,
}

impl Default for MelodicStep {
    fn default() -> Self {
        Self {
            active: false,
            note: 48, // C3
            velocity: 0.82,
            gate: 0.65,
            accent: false,
            slide: false,
            chord: ChordType::Single,
            motion: MotionLocks::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MelodicTrackSeq {
    pub step_count: StepCount,
    pub clock_div: ClockDiv,
    pub motion_smooth: bool,
    pub steps: Vec<MelodicStep>,
}

impl Default for MelodicTrackSeq {
    fn default() -> Self {
        Self {
            step_count: StepCount::Steps16,
            clock_div: ClockDiv::Sixteenth,
            motion_smooth: true,
            steps: vec![MelodicStep::default(); MAX_STEPS],
        }
    }
}

impl MelodicTrackSeq {
    pub fn new(step_count: StepCount) -> Self {
        Self {
            step_count,
            ..Self::default()
        }
    }

    pub fn clear(&mut self) {
        for st in &mut self.steps {
            st.active = false;
            st.accent = false;
            st.slide = false;
            st.motion.clear();
        }
    }

    pub fn with_default_note(note: u8, step_count: StepCount) -> Self {
        let mut s = Self {
            step_count,
            ..Self::default()
        };
        for st in &mut s.steps {
            st.note = note;
        }
        s
    }

    pub fn normalize(&mut self) {
        normalize_vec(&mut self.steps, MAX_STEPS);
        for st in &mut self.steps {
            st.note = st.note.clamp(12, 108);
            st.velocity = st.velocity.clamp(0.05, 1.0);
            st.gate = st.gate.clamp(0.05, 16.0);
        }
    }

    pub fn expand_to(&mut self, target: StepCount) {
        let from = self.step_count.as_usize();
        let to = target.as_usize();
        if to > from && from > 0 {
            for i in from..to {
                self.steps[i] = self.steps[i % from];
            }
        }
        self.step_count = target;
    }

    pub fn copy_page(&mut self, src_page: usize, dst_page: usize) {
        copy_page_slice(&mut self.steps, src_page, dst_page);
    }

    pub fn copy_page_to_rest(&mut self, src_page: usize) -> StepCount {
        let src = src_page.min(3);
        if self.step_count == StepCount::Steps16 {
            self.step_count = StepCount::Steps64;
        }
        let pages = self.step_count.pages();
        for dst in 0..pages {
            if dst != src {
                self.copy_page(src, dst);
            }
        }
        self.step_count
    }

    pub fn copy_page_to_next(&mut self, src_page: usize) -> usize {
        let src = src_page.min(3);
        if self.step_count == StepCount::Steps16 {
            self.step_count = StepCount::Steps32;
        }
        let pages = self.step_count.pages().max(1);
        let dst = (src + 1) % pages;
        if dst != src {
            self.copy_page(src, dst);
        }
        dst
    }

    pub fn shift(&mut self, delta: isize) {
        rotate_slice(&mut self.steps[..self.step_count.as_usize()], delta);
    }

    pub fn clear_active(&mut self) {
        for st in &mut self.steps {
            st.active = false;
            st.accent = false;
            st.slide = false;
            st.motion.clear();
        }
    }

    pub fn randomize(
        &mut self,
        root: u8,
        scale: Scale,
        base_octave: u8,
        density: f32,
        acid_style: bool,
        seed: u32,
    ) {
        let mut rng = SeqRng(seed.max(1));
        let low = (root % 12) + base_octave * 12;
        let high = (low + if acid_style { 19 } else { 24 }).min(96);
        let pool = scale.notes_in_range(root, low, high);
        let len = self.step_count.as_usize();
        for (i, st) in self.steps[..len].iter_mut().enumerate() {
            let beat_boost = if i % 4 == 0 { 0.25 } else { 0.0 };
            let active = rng.next_f32() < (density + beat_boost).clamp(0.05, 0.98);
            st.active = active;
            if active {
                let idx = if i % 4 == 0 && rng.next_f32() < 0.5 {
                    0
                } else {
                    (rng.next_u32() as usize) % pool.len()
                };
                st.note = pool[idx];
                st.velocity = 0.7 + rng.next_f32() * 0.3;
                st.gate = if acid_style {
                    0.45 + rng.next_f32() * 0.4
                } else {
                    0.35 + rng.next_f32() * 0.55
                };
                st.accent = rng.next_f32() < if acid_style { 0.28 } else { 0.15 };
                st.slide = rng.next_f32() < if acid_style { 0.26 } else { 0.10 };
            } else {
                st.accent = false;
                st.slide = false;
            }
        }
    }

    pub fn mutate(&mut self, root: u8, scale: Scale, seed: u32) {
        let mut rng = SeqRng(seed.max(1));
        let len = self.step_count.as_usize();
        let pool = scale.notes_in_range(root, 36, 84);
        for st in &mut self.steps[..len] {
            let r = rng.next_f32();
            if r < 0.14 {
                st.active = !st.active;
            } else if r < 0.32 && st.active {
                let cur_pos = pool.iter().position(|&n| n >= st.note).unwrap_or(0) as isize;
                let offset = (rng.next_u32() % 5) as isize - 2;
                let next_idx = (cur_pos + offset).clamp(0, pool.len() as isize - 1) as usize;
                st.note = pool[next_idx];
            } else if r < 0.40 && st.active {
                st.accent = !st.accent;
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DrumVoiceId {
    Kick,
    Snare,
    Clap,
    CowbellRim,
    ClosedHat,
    OpenHat,
    Tom,
    Cymbal,
}

impl DrumVoiceId {
    pub const ALL: [DrumVoiceId; 8] = [
        DrumVoiceId::Kick,
        DrumVoiceId::Snare,
        DrumVoiceId::Clap,
        DrumVoiceId::CowbellRim,
        DrumVoiceId::ClosedHat,
        DrumVoiceId::OpenHat,
        DrumVoiceId::Tom,
        DrumVoiceId::Cymbal,
    ];

    pub const fn index(self) -> usize {
        match self {
            DrumVoiceId::Kick => 0,
            DrumVoiceId::Snare => 1,
            DrumVoiceId::Clap => 2,
            DrumVoiceId::CowbellRim => 3,
            DrumVoiceId::ClosedHat => 4,
            DrumVoiceId::OpenHat => 5,
            DrumVoiceId::Tom => 6,
            DrumVoiceId::Cymbal => 7,
        }
    }

    pub const fn from_index(idx: usize) -> Option<Self> {
        match idx {
            0 => Some(DrumVoiceId::Kick),
            1 => Some(DrumVoiceId::Snare),
            2 => Some(DrumVoiceId::Clap),
            3 => Some(DrumVoiceId::CowbellRim),
            4 => Some(DrumVoiceId::ClosedHat),
            5 => Some(DrumVoiceId::OpenHat),
            6 => Some(DrumVoiceId::Tom),
            7 => Some(DrumVoiceId::Cymbal),
            _ => None,
        }
    }

    pub const fn short_name(self) -> &'static str {
        match self {
            DrumVoiceId::Kick => "KICK",
            DrumVoiceId::Snare => "SNARE",
            DrumVoiceId::Clap => "CLAP",
            DrumVoiceId::CowbellRim => "COWBELL",
            DrumVoiceId::ClosedHat => "C.HAT",
            DrumVoiceId::OpenHat => "O.HAT",
            DrumVoiceId::Tom => "TOM",
            DrumVoiceId::Cymbal => "CYM",
        }
    }

    pub const fn short_name_for_model(self, model: super::DrumModel) -> &'static str {
        match (self, model) {
            (DrumVoiceId::Kick, super::DrumModel::Tr808) => "808 KICK",
            (DrumVoiceId::Kick, super::DrumModel::Tr909) => "909 KICK",
            (DrumVoiceId::CowbellRim, super::DrumModel::Tr808) => "COWBELL",
            (DrumVoiceId::CowbellRim, super::DrumModel::Tr909) => "RIMSHOT",
            (DrumVoiceId::Cymbal, super::DrumModel::Tr808) => "808 CYM",
            (DrumVoiceId::Cymbal, super::DrumModel::Tr909) => "CRASH",
            _ => self.short_name(),
        }
    }

    pub const fn full_name(self) -> &'static str {
        match self {
            DrumVoiceId::Kick => "Bass Drum",
            DrumVoiceId::Snare => "Snare Drum",
            DrumVoiceId::Clap => "Hand Clap",
            DrumVoiceId::CowbellRim => "808 Cowbell / 909 Rimshot",
            DrumVoiceId::ClosedHat => "Closed Hi-Hat",
            DrumVoiceId::OpenHat => "Open Hi-Hat",
            DrumVoiceId::Tom => "Tom",
            DrumVoiceId::Cymbal => "Crash / Ride",
        }
    }

    pub const fn full_name_for_model(self, model: super::DrumModel) -> &'static str {
        match (self, model) {
            (DrumVoiceId::Kick, super::DrumModel::Tr808) => "TR-808 Bass Drum",
            (DrumVoiceId::Kick, super::DrumModel::Tr909) => "TR-909 Bass Drum",
            (DrumVoiceId::Snare, super::DrumModel::Tr808) => "TR-808 Snare Drum",
            (DrumVoiceId::Snare, super::DrumModel::Tr909) => "TR-909 Snare Drum",
            (DrumVoiceId::Clap, super::DrumModel::Tr808) => "TR-808 Hand Clap",
            (DrumVoiceId::Clap, super::DrumModel::Tr909) => "TR-909 Hand Clap",
            (DrumVoiceId::CowbellRim, super::DrumModel::Tr808) => "TR-808 Cowbell",
            (DrumVoiceId::CowbellRim, super::DrumModel::Tr909) => "TR-909 Rimshot",
            (DrumVoiceId::ClosedHat, super::DrumModel::Tr808) => "TR-808 Closed Hi-Hat",
            (DrumVoiceId::ClosedHat, super::DrumModel::Tr909) => "TR-909 Closed Hi-Hat",
            (DrumVoiceId::OpenHat, super::DrumModel::Tr808) => "TR-808 Open Hi-Hat",
            (DrumVoiceId::OpenHat, super::DrumModel::Tr909) => "TR-909 Open Hi-Hat",
            (DrumVoiceId::Tom, super::DrumModel::Tr808) => "TR-808 Conga / Tom",
            (DrumVoiceId::Tom, super::DrumModel::Tr909) => "TR-909 Low / Mid Tom",
            (DrumVoiceId::Cymbal, super::DrumModel::Tr808) => "TR-808 Cymbal",
            (DrumVoiceId::Cymbal, super::DrumModel::Tr909) => "TR-909 Crash / Ride",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DrumStep {
    pub active: bool,
    pub velocity: f32,
    pub accent: bool,
    pub flam: bool,
    pub probability: f32,
    pub ratchet: u8,
    /// Per-step FX (P-Locks): flip reverse for just this step!
    pub reverse: bool,
    /// Per-step pitch shift in semitones (-24..=+24)
    pub pitch_offset: i8,
    /// Per-step varispeed multiplier (0.25..=2.0, 1.0 = normal)
    pub speed_mult: f32,
    /// Optional per-step filter cutoff lock (0.0..=1.0)
    pub cutoff_lock: Option<f32>,
    /// Optional per-step stereo pan lock (-1.0..=1.0)
    pub pan_lock: Option<f32>,
}

impl Default for DrumStep {
    fn default() -> Self {
        Self {
            active: false,
            velocity: 0.85,
            accent: false,
            flam: false,
            probability: 1.0,
            ratchet: 1,
            reverse: false,
            pitch_offset: 0,
            speed_mult: 1.0,
            cutoff_lock: None,
            pan_lock: None,
        }
    }
}

impl DrumStep {
    pub fn has_step_fx(&self) -> bool {
        self.reverse
            || self.pitch_offset != 0
            || (self.speed_mult - 1.0).abs() > 0.01
            || self.cutoff_lock.is_some()
            || self.pan_lock.is_some()
    }

    pub fn clear_step_fx(&mut self) {
        self.reverse = false;
        self.pitch_offset = 0;
        self.speed_mult = 1.0;
        self.cutoff_lock = None;
        self.pan_lock = None;
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DrumLane {
    pub step_count: StepCount,
    pub steps: Vec<DrumStep>,
}

impl Default for DrumLane {
    fn default() -> Self {
        Self {
            step_count: StepCount::Steps16,
            steps: vec![DrumStep::default(); MAX_STEPS],
        }
    }
}

impl DrumLane {
    pub fn clear(&mut self) {
        for st in &mut self.steps {
            *st = DrumStep::default();
        }
    }

    pub fn normalize(&mut self) {
        normalize_vec(&mut self.steps, MAX_STEPS);
        for st in &mut self.steps {
            st.velocity = st.velocity.clamp(0.05, 1.0);
            st.probability = st.probability.clamp(0.0, 1.0);
        }
    }

    pub fn expand_to(&mut self, target: StepCount) {
        let from = self.step_count.as_usize();
        let to = target.as_usize();
        if to > from && from > 0 {
            for i in from..to {
                self.steps[i] = self.steps[i % from];
            }
        }
        self.step_count = target;
    }

    pub fn copy_page(&mut self, src_page: usize, dst_page: usize) {
        copy_page_slice(&mut self.steps, src_page, dst_page);
    }

    pub fn copy_page_to_rest(&mut self, src_page: usize) -> StepCount {
        let src = src_page.min(3);
        if self.step_count == StepCount::Steps16 {
            self.step_count = StepCount::Steps64;
        }
        let pages = self.step_count.pages();
        for dst in 0..pages {
            if dst != src {
                self.copy_page(src, dst);
            }
        }
        self.step_count
    }

    pub fn copy_page_to_next(&mut self, src_page: usize) -> usize {
        let src = src_page.min(3);
        if self.step_count == StepCount::Steps16 {
            self.step_count = StepCount::Steps32;
        }
        let pages = self.step_count.pages().max(1);
        let dst = (src + 1) % pages;
        if dst != src {
            self.copy_page(src, dst);
        }
        dst
    }

    pub fn fill_euclidean(&mut self, pulses: usize, rotation: usize) {
        let len = self.step_count.as_usize();
        let pattern = euclidean_rhythm(len, pulses, rotation);
        for (i, hit) in pattern.into_iter().enumerate() {
            self.steps[i].active = hit;
            self.steps[i].velocity = if i % 4 == 0 { 0.92 } else { 0.78 };
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DrumTrackSeq {
    pub step_count: StepCount,
    pub clock_div: ClockDiv,
    pub link_lengths: bool,
    pub lanes: [DrumLane; 8],
}

impl Default for DrumTrackSeq {
    fn default() -> Self {
        Self {
            step_count: StepCount::Steps16,
            clock_div: ClockDiv::Sixteenth,
            link_lengths: true,
            lanes: std::array::from_fn(|_| DrumLane::default()),
        }
    }
}

impl DrumTrackSeq {
    pub fn normalize(&mut self) {
        for lane in &mut self.lanes {
            lane.normalize();
            if self.link_lengths {
                lane.step_count = self.step_count;
            }
        }
    }

    pub fn set_all_step_counts(&mut self, target: StepCount, expand: bool) {
        self.step_count = target;
        for lane in &mut self.lanes {
            if expand {
                lane.expand_to(target);
            } else {
                lane.step_count = target;
            }
        }
    }

    pub fn copy_page(&mut self, src_page: usize, dst_page: usize) {
        for lane in &mut self.lanes {
            copy_page_slice(&mut lane.steps, src_page, dst_page);
        }
    }

    pub fn copy_page_to_rest(&mut self, src_page: usize) -> StepCount {
        let src = src_page.min(3);
        if self.step_count == StepCount::Steps16 {
            self.set_all_step_counts(StepCount::Steps64, false);
        }
        let pages = self.step_count.pages();
        for dst in 0..pages {
            if dst != src {
                self.copy_page(src, dst);
            }
        }
        self.step_count
    }

    pub fn copy_page_to_next(&mut self, src_page: usize) -> usize {
        let src = src_page.min(3);
        if self.step_count == StepCount::Steps16 {
            self.set_all_step_counts(StepCount::Steps32, false);
        }
        let pages = self.step_count.pages().max(1);
        let dst = (src + 1) % pages;
        if dst != src {
            self.copy_page(src, dst);
        }
        dst
    }

    pub fn shift(&mut self, delta: isize) {
        for lane in &mut self.lanes {
            let len = lane.step_count.as_usize();
            rotate_slice(&mut lane.steps[..len], delta);
        }
    }

    pub fn clear(&mut self) {
        for lane in &mut self.lanes {
            for st in &mut lane.steps {
                st.active = false;
                st.accent = false;
                st.flam = false;
            }
        }
    }

    pub fn randomize_groove(&mut self, seed: u32) {
        let mut rng = SeqRng(seed.max(1));
        self.clear();
        let len = self.step_count.as_usize();
        for i in 0..len {
            let step16 = i % 16;
            // Kick: four on the floor or syncopated
            self.lanes[0].steps[i].active =
                step16 % 4 == 0 || (step16 == 14 && rng.next_f32() < 0.35);
            self.lanes[0].steps[i].velocity = if step16 % 4 == 0 { 0.95 } else { 0.72 };
            // Snare on 4, 12
            self.lanes[1].steps[i].active =
                step16 == 4 || step16 == 12 || (step16 == 15 && rng.next_f32() < 0.25);
            self.lanes[1].steps[i].velocity = if step16 == 4 || step16 == 12 {
                0.9
            } else {
                0.6
            };
            // Clap layered occasionally
            self.lanes[2].steps[i].active = (step16 == 4 || step16 == 12) && rng.next_f32() < 0.75;
            // Cowbell/Rim syncopation
            self.lanes[3].steps[i].active =
                (step16 % 4 == 3 || step16 == 6) && rng.next_f32() < 0.45;
            // Closed hat
            self.lanes[4].steps[i].active = step16 % 2 == 0 || rng.next_f32() < 0.4;
            self.lanes[4].steps[i].velocity = if step16 % 4 == 2 { 0.85 } else { 0.6 };
            // Open hat on off-beats
            if step16 % 4 == 2 && rng.next_f32() < 0.55 {
                self.lanes[5].steps[i].active = true;
                self.lanes[4].steps[i].active = false;
            }
            // Tom fills at end of 16
            self.lanes[6].steps[i].active = (i >= len - 4) && rng.next_f32() < 0.45;
            // Crash on bar 1
            self.lanes[7].steps[i].active = i == 0 && rng.next_f32() < 0.5;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MidiStep {
    pub active: bool,
    pub note: u8,
    pub velocity: u8,
    pub gate: f32,
    pub cc1_val: Option<u8>,
    pub cc2_val: Option<u8>,
}

impl Default for MidiStep {
    fn default() -> Self {
        Self {
            active: false,
            note: 60,
            velocity: 100,
            gate: 0.65,
            cc1_val: None,
            cc2_val: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MidiTrackSeq {
    pub step_count: StepCount,
    pub clock_div: ClockDiv,
    pub channel: u8, // 1..=16
    pub cc1_num: u8,
    pub cc2_num: u8,
    pub steps: Vec<MidiStep>,
}

impl Default for MidiTrackSeq {
    fn default() -> Self {
        Self {
            step_count: StepCount::Steps16,
            clock_div: ClockDiv::Sixteenth,
            channel: 1,
            cc1_num: 74, // Filter Cutoff
            cc2_num: 1,  // Mod Wheel
            steps: vec![MidiStep::default(); MAX_STEPS],
        }
    }
}

impl MidiTrackSeq {
    pub fn normalize(&mut self) {
        normalize_vec(&mut self.steps, MAX_STEPS);
        self.channel = self.channel.clamp(1, 16);
        self.cc1_num = self.cc1_num.min(127);
        self.cc2_num = self.cc2_num.min(127);
        for st in &mut self.steps {
            st.note = st.note.clamp(12, 127);
            st.velocity = st.velocity.clamp(1, 127);
            st.gate = st.gate.clamp(0.05, 1.0);
        }
    }

    pub fn expand_to(&mut self, target: StepCount) {
        let from = self.step_count.as_usize();
        let to = target.as_usize();
        if to > from && from > 0 {
            for i in from..to {
                self.steps[i] = self.steps[i % from];
            }
        }
        self.step_count = target;
    }

    pub fn copy_page(&mut self, src_page: usize, dst_page: usize) {
        copy_page_slice(&mut self.steps, src_page, dst_page);
    }

    pub fn copy_page_to_rest(&mut self, src_page: usize) -> StepCount {
        let src = src_page.min(3);
        if self.step_count == StepCount::Steps16 {
            self.step_count = StepCount::Steps64;
        }
        let pages = self.step_count.pages();
        for dst in 0..pages {
            if dst != src {
                self.copy_page(src, dst);
            }
        }
        self.step_count
    }

    pub fn copy_page_to_next(&mut self, src_page: usize) -> usize {
        let src = src_page.min(3);
        if self.step_count == StepCount::Steps16 {
            self.step_count = StepCount::Steps32;
        }
        let pages = self.step_count.pages().max(1);
        let dst = (src + 1) % pages;
        if dst != src {
            self.copy_page(src, dst);
        }
        dst
    }

    pub fn shift(&mut self, delta: isize) {
        rotate_slice(&mut self.steps[..self.step_count.as_usize()], delta);
    }

    pub fn clear(&mut self) {
        for st in &mut self.steps {
            st.active = false;
            st.cc1_val = None;
            st.cc2_val = None;
        }
    }
}

/// 64-step per-pattern live macro & performance motion recording (Volca-style!).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MotionAutomation {
    pub filter: Vec<Option<f32>>,     // 0..1 normalized cutoff
    pub space: Vec<Option<f32>>,      // 0..1 reverb/delay macro
    pub sidechain: Vec<Option<f32>>,  // 0..1
    pub glitch: Vec<Option<f32>>,     // 0..1
    pub iso_bass: Vec<Option<f32>>,   // 0..2 Volca bass isolator
    pub iso_treble: Vec<Option<f32>>, // 0..2 Volca treble isolator
}

impl Default for MotionAutomation {
    fn default() -> Self {
        Self {
            filter: vec![None; MAX_STEPS],
            space: vec![None; MAX_STEPS],
            sidechain: vec![None; MAX_STEPS],
            glitch: vec![None; MAX_STEPS],
            iso_bass: vec![None; MAX_STEPS],
            iso_treble: vec![None; MAX_STEPS],
        }
    }
}

impl MotionAutomation {
    pub fn normalize(&mut self) {
        for lane in [
            &mut self.filter,
            &mut self.space,
            &mut self.sidechain,
            &mut self.glitch,
            &mut self.iso_bass,
            &mut self.iso_treble,
        ] {
            if lane.len() < MAX_STEPS {
                lane.resize(MAX_STEPS, None);
            } else if lane.len() > MAX_STEPS {
                lane.truncate(MAX_STEPS);
            }
        }
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn has_any(&self) -> bool {
        self.filter.iter().any(|v| v.is_some())
            || self.space.iter().any(|v| v.is_some())
            || self.sidechain.iter().any(|v| v.is_some())
            || self.glitch.iter().any(|v| v.is_some())
            || self.iso_bass.iter().any(|v| v.is_some())
            || self.iso_treble.iter().any(|v| v.is_some())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Pattern {
    pub name: String,
    pub moog: MelodicTrackSeq,
    pub moog_pad: MelodicTrackSeq,
    pub monologue: MelodicTrackSeq,
    pub tb303: MelodicTrackSeq,
    pub drums: DrumTrackSeq,
    pub sampler: MelodicTrackSeq,
    pub sample_lanes: Vec<DrumLane>,
    pub motion: MotionAutomation,
    pub midi_out: MidiTrackSeq,
    #[serde(default)]
    pub extra_seqs: Vec<MelodicTrackSeq>,
}

impl Default for Pattern {
    fn default() -> Self {
        let mut sample_lanes = vec![DrumLane::default(); 8];
        for l in &mut sample_lanes {
            l.step_count = StepCount::Steps64;
        }
        Self {
            name: "Pattern".to_string(),
            moog: MelodicTrackSeq::with_default_note(48, StepCount::Steps16),
            moog_pad: MelodicTrackSeq::with_default_note(60, StepCount::Steps16),
            monologue: MelodicTrackSeq::with_default_note(60, StepCount::Steps16),
            tb303: MelodicTrackSeq::with_default_note(36, StepCount::Steps16),
            drums: DrumTrackSeq::default(),
            sampler: MelodicTrackSeq::with_default_note(60, StepCount::Steps16),
            sample_lanes,
            motion: MotionAutomation::default(),
            midi_out: MidiTrackSeq::default(),
            extra_seqs: Vec::new(),
        }
    }
}

impl Pattern {
    pub fn named(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Self::default()
        }
    }

    pub fn melodic(&self, track: super::TrackId) -> Option<&MelodicTrackSeq> {
        match track {
            super::TrackId::Moog => Some(&self.moog),
            super::TrackId::MoogPad => Some(&self.moog_pad),
            super::TrackId::Monologue => Some(&self.monologue),
            super::TrackId::Tb303 => Some(&self.tb303),
            super::TrackId::Sampler => Some(&self.sampler),
            _ => None,
        }
    }

    pub fn melodic_mut(&mut self, track: super::TrackId) -> Option<&mut MelodicTrackSeq> {
        match track {
            super::TrackId::Moog => Some(&mut self.moog),
            super::TrackId::MoogPad => Some(&mut self.moog_pad),
            super::TrackId::Monologue => Some(&mut self.monologue),
            super::TrackId::Tb303 => Some(&mut self.tb303),
            super::TrackId::Sampler => Some(&mut self.sampler),
            _ => None,
        }
    }

    pub fn set_all_step_counts(&mut self, target: StepCount) {
        self.moog.step_count = target;
        self.moog_pad.step_count = target;
        self.monologue.step_count = target;
        self.tb303.step_count = target;
        self.drums.set_all_step_counts(target, false);
        self.set_sampler_step_counts(target);
        self.midi_out.step_count = target;
        for seq in &mut self.extra_seqs {
            seq.step_count = target;
        }
    }

    pub fn set_sampler_step_counts(&mut self, target: StepCount) {
        self.sampler.step_count = target;
        for l in &mut self.sample_lanes {
            l.step_count = target;
        }
    }

    pub fn copy_sampler_page_to_rest(&mut self, src_page: usize) -> StepCount {
        let src = src_page.min(3);
        let cur = self
            .sample_lanes
            .first()
            .map(|l| l.step_count)
            .unwrap_or(self.sampler.step_count);
        let target = if cur == StepCount::Steps16 {
            StepCount::Steps64
        } else {
            cur
        };
        self.set_sampler_step_counts(target);
        let pages = target.pages();
        for dst in 0..pages {
            if dst != src {
                self.sampler.copy_page(src, dst);
                for l in &mut self.sample_lanes {
                    l.copy_page(src, dst);
                }
            }
        }
        target
    }

    pub fn copy_sampler_page_to_next(&mut self, src_page: usize) -> usize {
        let src = src_page.min(3);
        let cur = self
            .sample_lanes
            .first()
            .map(|l| l.step_count)
            .unwrap_or(self.sampler.step_count);
        let target = if cur == StepCount::Steps16 {
            StepCount::Steps32
        } else {
            cur
        };
        self.set_sampler_step_counts(target);
        let pages = target.pages().max(1);
        let dst = (src + 1) % pages;
        if dst != src {
            self.sampler.copy_page(src, dst);
            for l in &mut self.sample_lanes {
                l.copy_page(src, dst);
            }
        }
        dst
    }

    pub fn copy_all_tracks_page_to_rest(&mut self, src_page: usize) {
        let src = src_page.min(3);
        self.moog.copy_page_to_rest(src);
        self.moog_pad.copy_page_to_rest(src);
        self.monologue.copy_page_to_rest(src);
        self.tb303.copy_page_to_rest(src);
        self.drums.copy_page_to_rest(src);
        self.copy_sampler_page_to_rest(src);
        for seq in &mut self.extra_seqs {
            seq.copy_page_to_rest(src);
        }
    }

    pub fn normalize(&mut self) {
        self.moog.normalize();
        self.moog_pad.normalize();
        self.monologue.normalize();
        self.tb303.normalize();
        self.drums.normalize();
        self.sampler.normalize();
        while self.sample_lanes.len() < 8 {
            let mut l = DrumLane::default();
            l.step_count = StepCount::Steps64;
            self.sample_lanes.push(l);
        }
        self.sample_lanes.truncate(8);
        for l in &mut self.sample_lanes {
            l.normalize();
        }
        self.motion.normalize();
        self.midi_out.normalize();
    }
}

pub fn detect_chord_from_notes(notes: &[u8]) -> (u8, ChordType) {
    let mut sorted: Vec<u8> = notes.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    if sorted.is_empty() {
        return (60, ChordType::Single);
    }
    if sorted.len() == 1 {
        return (sorted[0], ChordType::Single);
    }

    let root = sorted[0];
    let exact_intervals: Vec<u8> = sorted.iter().map(|&n| n - root).collect();
    for &ct in &ChordType::ALL {
        if ct == ChordType::Single {
            continue;
        }
        if ct.intervals() == exact_intervals.as_slice() {
            return (root, ct);
        }
    }

    let pcs_for_root = |r: u8| -> Vec<u8> {
        let rpc = r % 12;
        let mut pcs: Vec<u8> = sorted.iter().map(|&n| (n + 12 - rpc) % 12).collect();
        pcs.sort_unstable();
        pcs.dedup();
        pcs
    };
    let chord_pcs = |ct: ChordType| -> Vec<u8> {
        let mut pcs: Vec<u8> = ct.intervals().iter().map(|&i| i % 12).collect();
        pcs.sort_unstable();
        pcs.dedup();
        pcs
    };

    let root_pcs = pcs_for_root(root);
    for &ct in &ChordType::ALL {
        if ct == ChordType::Single {
            continue;
        }
        if root_pcs == chord_pcs(ct) {
            return (root, ct);
        }
    }

    // Check chord inversions
    for &cand_root in &sorted[1..] {
        let inv_pcs = pcs_for_root(cand_root);
        for &ct in &ChordType::ALL {
            if ct == ChordType::Single {
                continue;
            }
            if inv_pcs == chord_pcs(ct) {
                return (cand_root, ct);
            }
        }
    }

    (root, ChordType::Single)
}

pub fn euclidean_rhythm(steps: usize, pulses: usize, rotation: usize) -> Vec<bool> {
    if steps == 0 {
        return Vec::new();
    }
    let pulses = pulses.min(steps);
    let mut out = vec![false; steps];
    if pulses == 0 {
        return out;
    }
    // Bjorklund / Bresenham pulse spacing
    let mut bucket = 0usize;
    for i in 0..steps {
        bucket += pulses;
        if bucket >= steps {
            bucket -= steps;
            let idx = (i + rotation) % steps;
            out[idx] = true;
        }
    }
    out
}

fn normalize_vec<T: Default + Clone>(v: &mut Vec<T>, target: usize) {
    if v.len() < target {
        v.resize_with(target, T::default);
    } else if v.len() > target {
        v.truncate(target);
    }
}

fn copy_page_slice<T: Copy>(slice: &mut [T], src_page: usize, dst_page: usize) {
    if src_page >= 4 || dst_page >= 4 || src_page == dst_page || slice.len() < MAX_STEPS {
        return;
    }
    let src_start = src_page * 16;
    let dst_start = dst_page * 16;
    for i in 0..16 {
        slice[dst_start + i] = slice[src_start + i];
    }
}

fn rotate_slice<T>(slice: &mut [T], delta: isize) {
    let n = slice.len();
    if n <= 1 {
        return;
    }
    let shift = delta.rem_euclid(n as isize) as usize;
    if shift > 0 {
        slice.rotate_right(shift);
    }
}

pub(crate) struct SeqRng(pub u32);

impl SeqRng {
    pub fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }

    pub fn next_f32(&mut self) -> f32 {
        (self.next_u32() & 0x00FF_FFFF) as f32 / 16_777_216.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{generate_auto_beat, TrackId, Vibe};

    #[test]
    fn copy_page_to_rest_and_next_work_for_melodic_drums_and_pattern() {
        let mut seq = MelodicTrackSeq::new(StepCount::Steps16);
        seq.steps[0] = MelodicStep {
            active: true,
            note: 52,
            gate: 4.5,
            accent: true,
            ..Default::default()
        };
        seq.steps[7].active = true;
        seq.steps[7].note = 55;

        // Normalize preserves extended gate length up to 16.0
        seq.normalize();
        assert!((seq.steps[0].gate - 4.5).abs() < 1e-5);

        // Copy->Next on 16 steps expands to 32 and copies page 0 -> page 1
        let dst = seq.copy_page_to_next(0);
        assert_eq!(dst, 1);
        assert_eq!(seq.step_count, StepCount::Steps32);
        assert!(seq.steps[16].active);
        assert_eq!(seq.steps[16].note, 52);
        assert!(seq.steps[23].active);
        assert_eq!(seq.steps[23].note, 55);

        // Copy->Rest from 16 steps expands to 64 and fills all 4 bars
        let mut seq16 = MelodicTrackSeq::new(StepCount::Steps16);
        seq16.steps[3].active = true;
        seq16.steps[3].note = 64;
        assert_eq!(seq16.copy_page_to_rest(0), StepCount::Steps64);
        assert_eq!(seq16.step_count, StepCount::Steps64);
        for bar in 0..4 {
            assert!(seq16.steps[bar * 16 + 3].active);
            assert_eq!(seq16.steps[bar * 16 + 3].note, 64);
        }

        // Copy->Rest on 48 steps preserves 48 and copies src bar to bars 0..3
        let mut drums = DrumTrackSeq::default();
        drums.set_all_step_counts(StepCount::Steps48, false);
        drums.lanes[0].steps[16].active = true; // Bar 2 step 1
        assert_eq!(drums.copy_page_to_rest(1), StepCount::Steps48);
        assert!(drums.lanes[0].steps[0].active);
        assert!(drums.lanes[0].steps[16].active);
        assert!(drums.lanes[0].steps[32].active);
        assert!(!drums.lanes[0].steps[48].active);

        // Pattern::set_all_step_counts + copy_all_tracks_page_to_rest
        let mut pat = Pattern::default();
        pat.extra_seqs.push(MelodicTrackSeq::new(StepCount::Steps16));
        pat.set_all_step_counts(StepCount::Steps32);
        assert_eq!(pat.monologue.step_count, StepCount::Steps32);
        assert_eq!(pat.drums.step_count, StepCount::Steps32);
        assert_eq!(pat.sample_lanes[0].step_count, StepCount::Steps32);
        assert_eq!(pat.extra_seqs[0].step_count, StepCount::Steps32);

        pat.monologue.steps[2].active = true;
        pat.monologue.steps[2].note = 67;
        pat.drums.lanes[1].steps[4].active = true;
        pat.sample_lanes[2].steps[8].active = true;
        pat.extra_seqs[0].steps[12].active = true;
        pat.copy_all_tracks_page_to_rest(0);
        assert!(pat.melodic(TrackId::Monologue).unwrap().steps[18].active);
        assert_eq!(pat.melodic(TrackId::Monologue).unwrap().steps[18].note, 67);
        assert!(pat.drums.lanes[1].steps[20].active);
        assert!(pat.sample_lanes[2].steps[24].active);
        assert!(pat.extra_seqs[0].steps[28].active);

        // generate_auto_beat keeps DrumTrackSeq::step_count == Steps64
        let gen_drums = generate_auto_beat(Vibe::ElectroFunk, 0.7, 42);
        assert_eq!(gen_drums.step_count, StepCount::Steps64);
    }

    #[test]
    fn detect_chord_recognizes_triads_sevenths_and_inversions() {
        assert_eq!(detect_chord_from_notes(&[60]), (60, ChordType::Single));
        assert_eq!(detect_chord_from_notes(&[48, 55, 60]), (48, ChordType::Power5));
        assert_eq!(detect_chord_from_notes(&[60, 64, 67]), (60, ChordType::Major));
        assert_eq!(detect_chord_from_notes(&[60, 63, 67]), (60, ChordType::Minor));
        assert_eq!(detect_chord_from_notes(&[60, 62, 67]), (60, ChordType::Sus2));
        assert_eq!(detect_chord_from_notes(&[60, 65, 67]), (60, ChordType::Sus4));
        assert_eq!(detect_chord_from_notes(&[60, 64, 67, 71]), (60, ChordType::Major7));
        assert_eq!(detect_chord_from_notes(&[60, 63, 67, 70]), (60, ChordType::Minor7));
        assert_eq!(detect_chord_from_notes(&[60, 64, 67, 70]), (60, ChordType::Dom7));
        // Inversion: E4 G4 C5 -> C Major with root C5
        let (inv_root, inv_ct) = detect_chord_from_notes(&[64, 67, 72]);
        assert_eq!(inv_root % 12, 0);
        assert_eq!(inv_ct, ChordType::Major);
    }
}
