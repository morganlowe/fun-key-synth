use serde::{Deserialize, Serialize};

use super::{ClockDiv, Scale, StepCount};

pub const MAX_STEPS: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ChordType {
    #[default]
    Single,
    Major,
    Minor,
    Sus2,
    Sus4,
    Major7,
    Minor7,
    Minor9,
    Major9,
    Dom9,
    Minor11,
    Add9,
}
impl ChordType {
    pub const ALL: [ChordType; 12] = [
        ChordType::Single,
        ChordType::Major,
        ChordType::Minor,
        ChordType::Sus2,
        ChordType::Sus4,
        ChordType::Major7,
        ChordType::Minor7,
        ChordType::Minor9,
        ChordType::Major9,
        ChordType::Dom9,
        ChordType::Minor11,
        ChordType::Add9,
    ];
    pub const fn name(self) -> &'static str {
        match self {
            Self::Single => "Single",
            Self::Major => "Maj",
            Self::Minor => "Min",
            Self::Sus2 => "Sus2",
            Self::Sus4 => "Sus4",
            Self::Major7 => "Maj7",
            Self::Minor7 => "Min7",
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
            Self::Major => &[0, 4, 7],
            Self::Minor => &[0, 3, 7],
            Self::Sus2 => &[0, 2, 7],
            Self::Sus4 => &[0, 5, 7],
            Self::Major7 => &[0, 4, 7, 11],
            Self::Minor7 => &[0, 3, 7, 10],
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
            DrumVoiceId::CowbellRim => "RIM",
            DrumVoiceId::ClosedHat => "C.HAT",
            DrumVoiceId::OpenHat => "O.HAT",
            DrumVoiceId::Tom => "TOM",
            DrumVoiceId::Cymbal => "CYM",
        }
    }

    pub const fn full_name(self) -> &'static str {
        match self {
            DrumVoiceId::Kick => "Bass Drum",
            DrumVoiceId::Snare => "Snare Drum",
            DrumVoiceId::Clap => "Hand Clap",
            DrumVoiceId::CowbellRim => "Cowbell / Rim",
            DrumVoiceId::ClosedHat => "Closed Hi-Hat",
            DrumVoiceId::OpenHat => "Open Hi-Hat",
            DrumVoiceId::Tom => "Tom",
            DrumVoiceId::Cymbal => "Crash / Ride",
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
        }
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
    pub midi_out: MidiTrackSeq,
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
            midi_out: MidiTrackSeq::default(),
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
        self.midi_out.normalize();
    }
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
