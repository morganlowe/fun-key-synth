use crate::{
    dsp::{
        one_pole_coeff,
        sampler::{generate_builtin_sample_stereo, VolcaSamplerBank},
        soundfont::discover_system_soundfonts,
        AnalogStereoChorus, Biquad, DrumMachine, MasterFx, MonologueSynth, MoogSynth, Sk1Sampler,
        SoundFontBank, Svf12Filter, Tb303Synth, Xorshift32,
    },
    model::{
        apply_voice_preset, arp_tones_for, bass_midi_for, build_party_song_rig_with_reroll,
        chord_symbol, generate_auto_beat, generate_melodic_tracks, party_palette_label,
        party_song_title, pick_party_vibe_crate, progression_at, vibe_progressions, voice_led_chord,
        voice_led_chord_near, ChordSlot, ChordType, ClockDiv, DrumVoiceId, FactoryPreset,
        FilterEnvTrigger, GlobalFilterMode, MelodicStep,
        MelodicTrackSeq, MotionLocks, PartyPace, PartySongRig, Project, SongSection,
        TrackFilterParams, TrackId, Vibe, ArpMode, ArpRate, MAX_EXTRA_TRACKS,
    },
};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::collections::VecDeque;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

pub struct OutputScope {
    samples: Box<[AtomicU32]>,
    cursor: AtomicUsize,
}

impl OutputScope {
    pub fn new(len: usize) -> Self {
        Self {
            samples: (0..len.max(64))
                .map(|_| AtomicU32::new(0.0_f32.to_bits()))
                .collect::<Vec<_>>()
                .into_boxed_slice(),
            cursor: AtomicUsize::new(0),
        }
    }

    pub fn push(&self, sample: f32) {
        let i = self.cursor.fetch_add(1, Ordering::Relaxed) % self.samples.len();
        self.samples[i].store(sample.clamp(-1.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    pub fn snapshot(&self, out: &mut Vec<f32>) {
        let len = self.samples.len();
        let end = self.cursor.load(Ordering::Relaxed) % len;
        out.clear();
        out.reserve(len);
        for offset in 0..len {
            let bits = self.samples[(end + offset) % len].load(Ordering::Relaxed);
            out.push(f32::from_bits(bits));
        }
    }
}

/// Live events sent from the UI (keyboard / pads / DJ buttons) to the audio engine.
#[derive(Debug, Clone)]
pub enum LiveEvent {
    PlayChord {
        notes: Vec<u8>,
        bass: Option<u8>,
        velocity: f32,
        slot: ChordSlot,
    },
    ReleaseChord,
    PlayNote {
        track: TrackId,
        note: u8,
        velocity: f32,
    },
    ReleaseNote {
        track: TrackId,
        note: u8,
    },
    TriggerBassDrop,
    TriggerGlobalFilterEnv,
    TriggerFill,
    NextPartySection,
    NextPartySong,
    ReloadPartyRig,
    SetPartyInstrument {
        track: TrackId,
        preset_idx: usize,
    },
    TriggerSamplePart {
        part: usize,
        velocity: f32,
    },
    TriggerSamplePartPitched {
        part: usize,
        velocity: f32,
        pitch_offset: i8,
    },
    ReleaseSamplePart {
        part: usize,
    },
    PlayExtraNote {
        idx: usize,
        note: u8,
        velocity: f32,
    },
    ReleaseExtraNote {
        idx: usize,
        note: u8,
    },
    SuppressTrackStep {
        track: Option<TrackId>,
        extra_idx: Option<usize>,
        lane: Option<usize>,
        global_step: usize,
    },
    LoadSoundFont {
        bank: Option<Arc<SoundFontBank>>,
        name: String,
    },
    JumpToSongSection {
        section_idx: usize,
    },
    QueueNextPartyVibe(Option<Vibe>),
    PrevPartySong,
    SetMidiExpression {
        bend_semi: f32,
        aftertouch: f32,
    },
}

#[derive(Debug, Clone, Default)]
pub struct EngineTelemetry {
    pub active_chord: String,
    pub party_section: String,
    pub party_bar: usize,
    pub party_section_bars: usize,
    pub party_song_idx: u32,
    pub party_song_title: String,
    pub party_palette: String,
    pub party_voice_presets: [usize; 6],
    pub party_bpm: f32,
    pub party_vibe_name: String,
    pub party_vibe: Vibe,
    pub party_rig_reroll: u32,
    pub party_template_idx: usize,
    pub party_dj_root: u8,
    pub party_prog_idx: usize,
    pub party_rig_custom: bool,
    pub party_next_title: String,
    pub party_next_vibe: String,
    pub party_next_bpm: f32,
    pub party_queued_vibe: Option<Vibe>,
    pub active_song_section: Option<usize>,
    pub peak_l: f32,
    pub peak_r: f32,
    pub track_peaks: [f32; 6],
    pub sidechain_duck: f32,
    pub sampler_playhead: Option<f32>,
    pub volca_active_mask: u8,
    pub global_filter_env_level: f32,
    pub drop_phase: String,
    pub drop_progress: f32,
}

#[derive(Debug, Clone, Copy)]
struct RatchetEvent {
    lane: usize,
    samples_left: f32,
    vel: f32,
    accent: bool,
    tune_offset: f32,
}

#[derive(Debug, Clone, Copy)]
enum PendingTrigger {
    Drum {
        lane: usize,
        vel: f32,
        accent: bool,
        flam: bool,
        pitch_offset: i8,
        pan_lock: Option<f32>,
    },
    PadNote {
        note: u8,
        vel: f32,
        accent: bool,
    },
    MonoNote {
        note: u8,
        prev_note: Option<u8>,
        vel: f32,
        accent: bool,
        slide: bool,
        motion: MotionLocks,
        party_rig_cut: bool,
        gate_samples: f32,
    },
    Melodic32 {
        target: ArpTarget,
        note: u8,
        vel: f32,
        accent: bool,
        slide: bool,
        chord: ChordType,
        motion: MotionLocks,
        gate_samples: f32,
        dj_root: u8,
    },
    Volca32 {
        part_idx: usize,
        step: crate::model::DrumStep,
    },
}

#[inline]
fn clock_div_indices(clock_div: ClockDiv, global: usize, len: usize) -> (Option<usize>, Option<usize>, f32) {
    let l = len.max(1);
    match clock_div {
        ClockDiv::Eighth => {
            if global % 2 == 0 {
                (Some((global / 2) % l), None, 2.0)
            } else {
                (None, None, 2.0)
            }
        }
        ClockDiv::Sixteenth => (Some(global % l), None, 1.0),
        ClockDiv::ThirtySecond => (Some((global * 2) % l), Some((global * 2 + 1) % l), 0.5),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArpTarget {
    Core(TrackId),
    Extra(usize),
    ChordPad,
}

#[derive(Debug, Clone)]
struct LiveArpState {
    target: ArpTarget,
    held: Vec<(u8, f32)>,
    latched: Vec<(u8, f32)>,
    bass_latched: Option<u8>,
    sounding: Option<(ArpTarget, u8)>,
    tick_left: f32,
    gate_left: f32,
    step_idx: usize,
    last_rand_idx: usize,
    was_enabled: bool,
    was_latch: bool,
    rng: Xorshift32,
}

impl LiveArpState {
    fn new() -> Self {
        Self {
            target: ArpTarget::Core(TrackId::Monologue),
            held: Vec::with_capacity(16),
            latched: Vec::with_capacity(16),
            bass_latched: None,
            sounding: None,
            tick_left: 0.0,
            gate_left: 0.0,
            step_idx: 0,
            last_rand_idx: usize::MAX,
            was_enabled: false,
            was_latch: false,
            rng: Xorshift32::new(0xA490_1984),
        }
    }
}

#[derive(Debug, Clone)]
struct BiquadPair {
    l: Biquad,
    r: Biquad,
}

impl BiquadPair {
    fn new() -> Self {
        Self {
            l: Biquad::new(),
            r: Biquad::new(),
        }
    }

    #[inline]
    fn process(&mut self, (l, r): (f32, f32)) -> (f32, f32) {
        (self.l.process(l), self.r.process(r))
    }

    fn set_highpass(&mut self, sr: f32, freq: f32, q: f32) {
        self.l.set_highpass(sr, freq, q);
        self.r.set_highpass(sr, freq, q);
    }

    fn set_peaking(&mut self, sr: f32, freq: f32, q: f32, gain_db: f32) {
        self.l.set_peaking(sr, freq, q, gain_db);
        self.r.set_peaking(sr, freq, q, gain_db);
    }

    fn set_high_shelf(&mut self, sr: f32, freq: f32, q: f32, gain_db: f32) {
        self.l.set_high_shelf(sr, freq, q, gain_db);
        self.r.set_high_shelf(sr, freq, q, gain_db);
    }
}

/// Per-channel studio mixing console EQ strips for clean frequency separation & mix pocket.
#[derive(Debug, Clone)]
struct StudioChannelEq {
    bass_sub: BiquadPair,
    bass_mud: BiquadPair,
    bass_pres: BiquadPair,
    lead_hp: BiquadPair,
    lead_body: BiquadPair,
    lead_pres_cut: BiquadPair,
    lead_shelf: BiquadPair,
    acid_hp: BiquadPair,
    acid_bite: BiquadPair,
    samp_hp: BiquadPair,
    samp_warmth: BiquadPair,
    samp_air: BiquadPair,
    pad_hp: BiquadPair,
    pad_carve: BiquadPair,
}

impl StudioChannelEq {
    fn new(sr: f32) -> Self {
        let mut eq = Self {
            bass_sub: BiquadPair::new(),
            bass_mud: BiquadPair::new(),
            bass_pres: BiquadPair::new(),
            lead_hp: BiquadPair::new(),
            lead_body: BiquadPair::new(),
            lead_pres_cut: BiquadPair::new(),
            lead_shelf: BiquadPair::new(),
            acid_hp: BiquadPair::new(),
            acid_bite: BiquadPair::new(),
            samp_hp: BiquadPair::new(),
            samp_warmth: BiquadPair::new(),
            samp_air: BiquadPair::new(),
            pad_hp: BiquadPair::new(),
            pad_carve: BiquadPair::new(),
        };
        eq.configure(sr);
        eq
    }

    fn configure(&mut self, sr: f32) {
        self.bass_sub.set_peaking(sr, 65.0, 0.9, 0.9);
        self.bass_mud.set_peaking(sr, 260.0, 1.3, -1.8);
        self.bass_pres.set_peaking(sr, 1200.0, 1.1, 1.1);

        self.lead_hp.set_highpass(sr, 130.0, 0.7071);
        self.lead_body.set_peaking(sr, 480.0, 1.0, 1.2);
        self.lead_pres_cut.set_peaking(sr, 3400.0, 1.6, -1.6);
        self.lead_shelf.set_high_shelf(sr, 8500.0, 0.7071, -1.6);

        self.acid_hp.set_highpass(sr, 110.0, 0.7071);
        self.acid_bite.set_peaking(sr, 1600.0, 1.2, 1.1);

        self.samp_hp.set_highpass(sr, 90.0, 0.7071);
        self.samp_warmth.set_peaking(sr, 240.0, 1.0, 0.8);
        self.samp_air.set_peaking(sr, 7500.0, 0.9, 0.8);

        self.pad_hp.set_highpass(sr, 120.0, 0.7071);
        self.pad_carve.set_peaking(sr, 1800.0, 1.1, -1.8);
    }

    #[inline]
    fn process_bass(&mut self, s: (f32, f32)) -> (f32, f32) {
        let s = self.bass_sub.process(s);
        let s = self.bass_mud.process(s);
        self.bass_pres.process(s)
    }

    #[inline]
    fn process_lead(&mut self, s: (f32, f32)) -> (f32, f32) {
        let s = self.lead_hp.process(s);
        let s = self.lead_body.process(s);
        let s = self.lead_pres_cut.process(s);
        self.lead_shelf.process(s)
    }

    #[inline]
    fn process_acid(&mut self, s: (f32, f32)) -> (f32, f32) {
        let s = self.acid_hp.process(s);
        self.acid_bite.process(s)
    }

    #[inline]
    fn process_sampler(&mut self, s: (f32, f32)) -> (f32, f32) {
        let s = self.samp_hp.process(s);
        let s = self.samp_warmth.process(s);
        self.samp_air.process(s)
    }

    #[inline]
    fn process_pad(&mut self, s: (f32, f32)) -> (f32, f32) {
        let s = self.pad_hp.process(s);
        let (l, r) = self.pad_carve.process(s);
        let mid = (l + r) * 0.5;
        let side = (l - r) * 0.5 * 1.10;
        (mid + side, mid - side)
    }
}

#[inline]
fn groove_pocket_scale(vibe: Vibe) -> f32 {
    match vibe {
        Vibe::DetroitTechno
        | Vibe::Eurobeat
        | Vibe::EuphoricTrance
        | Vibe::BigRoomEdm
        | Vibe::ImperialBass => 0.45,
        Vibe::Arena80s | Vibe::Synthwave | Vibe::ItaloCosmic | Vibe::Disco70s | Vibe::Dubstep => {
            0.75
        }
        _ => 1.0,
    }
}

#[inline]
fn phrase_vel_mul(global: usize, seed: u32, salt: u32) -> f32 {
    let beat_shape = if global % 4 == 0 {
        1.03
    } else if global % 2 == 0 {
        0.98
    } else {
        0.93
    };
    let jitter = (hash_f32(seed, global, salt) - 0.5) * 0.10;
    beat_shape * (1.0 + jitter)
}

#[inline]
fn trigger_drum_with_lock(
    drums: &mut DrumMachine,
    base_kit: &crate::model::DrumKitParams,
    lane: usize,
    voice: DrumVoiceId,
    vel: f32,
    accent: bool,
    flam: bool,
    pitch_offset: i8,
    pan_lock: Option<f32>,
) {
    if pitch_offset != 0 || pan_lock.is_some() {
        let mut dk = base_kit.clone();
        dk.voices[lane].tune = (dk.voices[lane].tune + pitch_offset as f32).clamp(-12.0, 12.0);
        if let Some(pan) = pan_lock {
            dk.voices[lane].pan = pan.clamp(-1.0, 1.0);
        }
        drums.trigger(voice, vel, accent, flam, &dk);
    } else {
        drums.trigger(voice, vel, accent, flam, base_kit);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DropMacroPhase {
    Idle,
    Building {
        step_idx: usize,
        total_steps: usize,
        gap_steps: usize,
    },
    Impact {
        steps_left: usize,
    },
}

#[inline]
fn apply_stereo_width(l: f32, r: f32, width: f32) -> (f32, f32) {
    if (width - 1.0).abs() < 0.002 {
        return (l, r);
    }
    let mid = 0.5 * (l + r);
    let side = 0.5 * (l - r) * width.clamp(0.0, 1.8);
    (mid + side, mid - side)
}

#[inline]
fn apply_track_filter(
    fl: &mut Svf12Filter,
    fr: &mut Svf12Filter,
    l: f32,
    r: f32,
    params: &TrackFilterParams,
    sr: f32,
) -> (f32, f32) {
    if !params.is_active() {
        return (l, r);
    }
    let cut = params.cutoff.clamp(35.0, sr * 0.42);
    let res = params.resonance.clamp(0.0, 0.88);
    let (lp_l, bp_l, hp_l) = fl.process_all(l, cut, res);
    let (lp_r, bp_r, hp_r) = fr.process_all(r, cut, res);
    match params.mode {
        GlobalFilterMode::LowPass => (lp_l, lp_r),
        GlobalFilterMode::HighPass => (hp_l, hp_r),
        GlobalFilterMode::BandPass => (bp_l, bp_r),
    }
}

#[inline]
fn pan_gains(pan: f32) -> (f32, f32) {
    let p = (pan.clamp(-1.0, 1.0) + 1.0) * 0.5;
    let angle = p * std::f32::consts::FRAC_PI_2;
    (angle.cos() * 1.4142, angle.sin() * 1.4142)
}

#[inline]
fn hash_f32(seed: u32, step: usize, salt: u32) -> f32 {
    let mut x = seed
        ^ (step as u32).wrapping_mul(0x9E37_79B9)
        ^ salt.wrapping_mul(0x85EB_CA6B);
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    (x as f32) / (u32::MAX as f32)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PartyFilterSweep {
    None,
    LowPassOpen,
    LowPassClose,
    HighPassRise,
    AtmosBreath,
}

struct PartySectionDef {
    name: &'static str,
    bars: usize,
    /// bit 0=Drums, 1=Bass, 2=Chords, 3=Arp, 4=303, 5=Sampler
    mask: u8,
    filter_sweep: PartyFilterSweep,
    snare_build_last_bar: bool,
    drop_on_start: bool,
    energy_mul: f32,
    evolve_progression: bool,
    space_throw: f32,
    lead_brightness: f32,
    acid_intensity: f32,
    turnaround_fx: bool,
}

// Template 0: Club Journey (20 bars at Normal pace — progressive space & contrast)
const PARTY_TMPL_CLUB: [PartySectionDef; 7] = [
    PartySectionDef {
        name: "INTRO",
        bars: 2,
        mask: 0b000101,
        filter_sweep: PartyFilterSweep::LowPassOpen,
        snare_build_last_bar: false,
        drop_on_start: false,
        energy_mul: 0.55,
        evolve_progression: true,
        space_throw: 0.14,
        lead_brightness: 0.45,
        acid_intensity: 0.25,
        turnaround_fx: false,
    },
    PartySectionDef {
        name: "GROOVE",
        bars: 4,
        mask: 0b001111,
        filter_sweep: PartyFilterSweep::None,
        snare_build_last_bar: false,
        drop_on_start: false,
        energy_mul: 0.82,
        evolve_progression: false,
        space_throw: 0.04,
        lead_brightness: 0.65,
        acid_intensity: 0.45,
        turnaround_fx: false,
    },
    PartySectionDef {
        name: "BUILD",
        bars: 2,
        mask: 0b011101,
        filter_sweep: PartyFilterSweep::HighPassRise,
        snare_build_last_bar: true,
        drop_on_start: false,
        energy_mul: 0.90,
        evolve_progression: true,
        space_throw: 0.36,
        lead_brightness: 0.82,
        acid_intensity: 0.72,
        turnaround_fx: true,
    },
    PartySectionDef {
        name: "DROP 1",
        bars: 4,
        mask: 0b011111,
        filter_sweep: PartyFilterSweep::None,
        snare_build_last_bar: false,
        drop_on_start: true,
        energy_mul: 1.02,
        evolve_progression: false,
        space_throw: 0.0,
        lead_brightness: 0.95,
        acid_intensity: 0.85,
        turnaround_fx: true,
    },
    PartySectionDef {
        name: "BREAK",
        bars: 2,
        mask: 0b101100,
        filter_sweep: PartyFilterSweep::AtmosBreath,
        snare_build_last_bar: true,
        drop_on_start: false,
        energy_mul: 0.44,
        evolve_progression: true,
        space_throw: 0.42,
        lead_brightness: 0.50,
        acid_intensity: 0.35,
        turnaround_fx: false,
    },
    PartySectionDef {
        name: "DROP 2",
        bars: 4,
        mask: 0b111111,
        filter_sweep: PartyFilterSweep::None,
        snare_build_last_bar: false,
        drop_on_start: true,
        energy_mul: 1.08,
        evolve_progression: false,
        space_throw: 0.0,
        lead_brightness: 1.0,
        acid_intensity: 0.95,
        turnaround_fx: true,
    },
    PartySectionDef {
        name: "OUTRO MIX",
        bars: 2,
        mask: 0b000111,
        filter_sweep: PartyFilterSweep::LowPassClose,
        snare_build_last_bar: false,
        drop_on_start: false,
        energy_mul: 0.60,
        evolve_progression: false,
        space_throw: 0.38,
        lead_brightness: 0.38,
        acid_intensity: 0.30,
        turnaround_fx: true,
    },
];

// Template 1: Instant Groove & Solo Spotlight (20 bars at Normal pace)
const PARTY_TMPL_GROOVE_SOLO: [PartySectionDef; 6] = [
    PartySectionDef {
        name: "LOCK IN",
        bars: 4,
        mask: 0b000111,
        filter_sweep: PartyFilterSweep::LowPassOpen,
        snare_build_last_bar: false,
        drop_on_start: false,
        energy_mul: 0.78,
        evolve_progression: true,
        space_throw: 0.08,
        lead_brightness: 0.55,
        acid_intensity: 0.35,
        turnaround_fx: false,
    },
    PartySectionDef {
        name: "CHORD BLOOM",
        bars: 4,
        mask: 0b101111,
        filter_sweep: PartyFilterSweep::None,
        snare_build_last_bar: false,
        drop_on_start: false,
        energy_mul: 0.88,
        evolve_progression: false,
        space_throw: 0.06,
        lead_brightness: 0.78,
        acid_intensity: 0.40,
        turnaround_fx: true,
    },
    PartySectionDef {
        name: "ACID SOLO",
        bars: 4,
        mask: 0b011111,
        filter_sweep: PartyFilterSweep::None,
        snare_build_last_bar: false,
        drop_on_start: false,
        energy_mul: 0.94,
        evolve_progression: true,
        space_throw: 0.10,
        lead_brightness: 0.85,
        acid_intensity: 1.0,
        turnaround_fx: true,
    },
    PartySectionDef {
        name: "STRIP BREAK",
        bars: 2,
        mask: 0b101100,
        filter_sweep: PartyFilterSweep::AtmosBreath,
        snare_build_last_bar: true,
        drop_on_start: false,
        energy_mul: 0.46,
        evolve_progression: true,
        space_throw: 0.44,
        lead_brightness: 0.58,
        acid_intensity: 0.50,
        turnaround_fx: false,
    },
    PartySectionDef {
        name: "PEAK DROP",
        bars: 4,
        mask: 0b011111,
        filter_sweep: PartyFilterSweep::None,
        snare_build_last_bar: false,
        drop_on_start: true,
        energy_mul: 1.08,
        evolve_progression: false,
        space_throw: 0.0,
        lead_brightness: 1.0,
        acid_intensity: 0.90,
        turnaround_fx: true,
    },
    PartySectionDef {
        name: "DJ MIX",
        bars: 2,
        mask: 0b000111,
        filter_sweep: PartyFilterSweep::LowPassClose,
        snare_build_last_bar: false,
        drop_on_start: false,
        energy_mul: 0.58,
        evolve_progression: false,
        space_throw: 0.38,
        lead_brightness: 0.40,
        acid_intensity: 0.25,
        turnaround_fx: true,
    },
];

// Template 2: Deep Hypnotic Rise (20 bars at Normal pace)
const PARTY_TMPL_HYPNOTIC: [PartySectionDef; 7] = [
    PartySectionDef {
        name: "ATMOS INTRO",
        bars: 2,
        mask: 0b100100,
        filter_sweep: PartyFilterSweep::AtmosBreath,
        snare_build_last_bar: false,
        drop_on_start: false,
        energy_mul: 0.48,
        evolve_progression: true,
        space_throw: 0.38,
        lead_brightness: 0.48,
        acid_intensity: 0.20,
        turnaround_fx: false,
    },
    PartySectionDef {
        name: "BASS ENTER",
        bars: 4,
        mask: 0b000111,
        filter_sweep: PartyFilterSweep::LowPassOpen,
        snare_build_last_bar: false,
        drop_on_start: false,
        energy_mul: 0.80,
        evolve_progression: false,
        space_throw: 0.08,
        lead_brightness: 0.65,
        acid_intensity: 0.45,
        turnaround_fx: false,
    },
    PartySectionDef {
        name: "HYPNO BUILD",
        bars: 2,
        mask: 0b011101,
        filter_sweep: PartyFilterSweep::HighPassRise,
        snare_build_last_bar: true,
        drop_on_start: false,
        energy_mul: 0.92,
        evolve_progression: true,
        space_throw: 0.34,
        lead_brightness: 0.84,
        acid_intensity: 0.78,
        turnaround_fx: true,
    },
    PartySectionDef {
        name: "MAIN DROP",
        bars: 4,
        mask: 0b011111,
        filter_sweep: PartyFilterSweep::None,
        snare_build_last_bar: false,
        drop_on_start: true,
        energy_mul: 1.04,
        evolve_progression: false,
        space_throw: 0.0,
        lead_brightness: 0.95,
        acid_intensity: 0.86,
        turnaround_fx: true,
    },
    PartySectionDef {
        name: "PERC BREAK",
        bars: 2,
        mask: 0b101100,
        filter_sweep: PartyFilterSweep::AtmosBreath,
        snare_build_last_bar: true,
        drop_on_start: false,
        energy_mul: 0.56,
        evolve_progression: true,
        space_throw: 0.36,
        lead_brightness: 0.52,
        acid_intensity: 0.55,
        turnaround_fx: true,
    },
    PartySectionDef {
        name: "CLIMAX",
        bars: 4,
        mask: 0b011111,
        filter_sweep: PartyFilterSweep::None,
        snare_build_last_bar: false,
        drop_on_start: true,
        energy_mul: 1.10,
        evolve_progression: false,
        space_throw: 0.0,
        lead_brightness: 1.0,
        acid_intensity: 0.96,
        turnaround_fx: true,
    },
    PartySectionDef {
        name: "OUTRO SWEEP",
        bars: 2,
        mask: 0b100101,
        filter_sweep: PartyFilterSweep::LowPassClose,
        snare_build_last_bar: false,
        drop_on_start: false,
        energy_mul: 0.54,
        evolve_progression: false,
        space_throw: 0.42,
        lead_brightness: 0.35,
        acid_intensity: 0.25,
        turnaround_fx: true,
    },
];

// Template 3: Verse-Hook Jam (20 bars at Normal pace)
const PARTY_TMPL_VERSE_HOOK: [PartySectionDef; 6] = [
    PartySectionDef {
        name: "HOOK 1",
        bars: 4,
        mask: 0b001111,
        filter_sweep: PartyFilterSweep::None,
        snare_build_last_bar: false,
        drop_on_start: false,
        energy_mul: 0.90,
        evolve_progression: true,
        space_throw: 0.04,
        lead_brightness: 0.82,
        acid_intensity: 0.55,
        turnaround_fx: false,
    },
    PartySectionDef {
        name: "VERSE 1",
        bars: 4,
        mask: 0b100111,
        filter_sweep: PartyFilterSweep::None,
        snare_build_last_bar: false,
        drop_on_start: false,
        energy_mul: 0.76,
        evolve_progression: false,
        space_throw: 0.10,
        lead_brightness: 0.60,
        acid_intensity: 0.40,
        turnaround_fx: true,
    },
    PartySectionDef {
        name: "PRE-DROP",
        bars: 2,
        mask: 0b011101,
        filter_sweep: PartyFilterSweep::LowPassOpen,
        snare_build_last_bar: true,
        drop_on_start: false,
        energy_mul: 0.92,
        evolve_progression: true,
        space_throw: 0.34,
        lead_brightness: 0.85,
        acid_intensity: 0.75,
        turnaround_fx: true,
    },
    PartySectionDef {
        name: "HOOK 2",
        bars: 4,
        mask: 0b011111,
        filter_sweep: PartyFilterSweep::None,
        snare_build_last_bar: false,
        drop_on_start: true,
        energy_mul: 1.04,
        evolve_progression: false,
        space_throw: 0.0,
        lead_brightness: 0.96,
        acid_intensity: 0.86,
        turnaround_fx: true,
    },
    PartySectionDef {
        name: "BRIDGE",
        bars: 2,
        mask: 0b101100,
        filter_sweep: PartyFilterSweep::AtmosBreath,
        snare_build_last_bar: true,
        drop_on_start: false,
        energy_mul: 0.48,
        evolve_progression: true,
        space_throw: 0.44,
        lead_brightness: 0.54,
        acid_intensity: 0.45,
        turnaround_fx: false,
    },
    PartySectionDef {
        name: "FINALE DROP",
        bars: 4,
        mask: 0b011111,
        filter_sweep: PartyFilterSweep::None,
        snare_build_last_bar: false,
        drop_on_start: true,
        energy_mul: 1.10,
        evolve_progression: false,
        space_throw: 0.0,
        lead_brightness: 1.0,
        acid_intensity: 0.98,
        turnaround_fx: true,
    },
];

const PARTY_TEMPLATES: [&[PartySectionDef]; 4] = [
    &PARTY_TMPL_CLUB,
    &PARTY_TMPL_GROOVE_SOLO,
    &PARTY_TMPL_HYPNOTIC,
    &PARTY_TMPL_VERSE_HOOK,
];

#[inline]
fn scaled_section_bars(base_bars: usize, pace: PartyPace) -> usize {
    match pace {
        PartyPace::Fast => (base_bars / 2).max(2),
        PartyPace::Normal => base_bars,
        PartyPace::Extended => (base_bars * 3 / 2).max(4),
    }
}

pub fn party_template_to_sections(template_idx: usize, pace: PartyPace) -> Vec<SongSection> {
    let tmpl = PARTY_TEMPLATES[template_idx % PARTY_TEMPLATES.len()];
    tmpl.iter()
        .map(|sec| {
            let pat = if sec.energy_mul < 0.65 {
                0
            } else if sec.snare_build_last_bar {
                2
            } else if sec.drop_on_start || sec.energy_mul >= 1.0 {
                3
            } else {
                1
            };
            SongSection {
                name: sec.name.to_string(),
                pattern: pat,
                bars: scaled_section_bars(sec.bars, pace).clamp(1, 16) as u8,
                bass_drop: sec.drop_on_start,
                fill: sec.snare_build_last_bar || sec.turnaround_fx,
                track_mask: sec.mask,
                extra_mask: if (sec.mask & 0b000100) != 0 && sec.energy_mul >= 0.65 {
                    0x03FF
                } else {
                    0
                },
            }
        })
        .collect()
}

struct Runtime {
    sr: f32,
    moog: MoogSynth,
    pad_moog: MoogSynth,
    mono: MonologueSynth,
    acid: Tb303Synth,
    drums: DrumMachine,
    sampler: Sk1Sampler,
    volca: VolcaSamplerBank,
    fx: MasterFx,
    pad_chorus: AnalogStereoChorus,
    arp_chorus: AnalogStereoChorus,
    sampler_chorus: AnalogStereoChorus,
    // Studio channel-strip console EQs & unmasking HPFs
    channel_eq: StudioChannelEq,
    // Per-instrument channel strip stereo filters
    track_filters: [(Svf12Filter, Svf12Filter); 6],
    pad_filter: (Svf12Filter, Svf12Filter),
    extra_filters: Vec<(Svf12Filter, Svf12Filter)>,
    // Global filter envelope
    gfilt_env: f32,
    gfilt_env_attacking: bool,
    // Timing & sequencer
    tick_left: f32,
    last_step_dur: f32,
    step: usize,
    was_playing: bool,
    suppressed_steps: Vec<(Option<TrackId>, Option<usize>, Option<usize>, usize)>,
    // Gate tracking (samples left before note_off; <=0 means off)
    gate_moog: f32,
    gate_pad: f32,
    gate_mono: f32,
    gate_acid: f32,
    gate_sampler: f32,
    extra_synths: Vec<MoogSynth>,
    extra_gates: [f32; MAX_EXTRA_TRACKS],
    last_moog_note: Option<u8>,
    last_mono_note: Option<u8>,
    last_acid_note: Option<u8>,
    last_pad_notes: Vec<u8>,
    // Sidechain ducking envelope
    sidechain_peak: f32,
    sidechain_env: f32,
    sidechain_decay: f32,
    // Sub bass dive & multi-stage Bass Drop Macro!
    drop_phase: f32,
    drop_env: f32,
    drop_pitch: f32,
    drop_target_hz: f32,
    drop_macro_phase: DropMacroPhase,
    drop_macro_progress: f32,
    drop_macro_target_prog: f32,
    drop_riser_phase: f32,
    drop_riser_phase2: f32,
    drop_riser_env: f32,
    drop_riser_target_env: f32,
    drop_riser_root_hz: f32,
    drop_riser_filter: Svf12Filter,
    drop_boom_env: f32,
    drop_boom_phase: f32,
    drop_boom_pitch: f32,
    drop_whoosh_env: f32,
    drop_whoosh_filter: Svf12Filter,
    drop_filter_slam: f32,
    drop_rng: Xorshift32,
    // Drum & Volca ratchets, groove micro-timing queue & fills
    ratchets: Vec<RatchetEvent>,
    volca_ratchets: Vec<RatchetEvent>,
    pending_triggers: Vec<(f32, PendingTrigger)>,
    force_fill_steps: usize,
    // Live chord override
    live_chord_held: bool,
    live_slot: Option<ChordSlot>,
    live_arp: LiveArpState,
    // Sampler sync
    loaded_sample_gen: u64,
    // Party mode state
    party_section_idx: usize,
    party_bar_in_sec: usize,
    party_cycle: u32,
    party_evolution: u32,
    party_template_idx: usize,
    party_rig: Option<PartySongRig>,
    party_rig_sig: (u8, u8, u32, u32, u32, bool, u32, Option<u8>),
    party_rig_reroll: u32,
    party_rig_custom: bool,
    party_queued_vibe: Option<Vibe>,
    party_forced_vibe: Option<(u32, Vibe)>,
    party_live_bpm: f32,
    last_base_vibe: Option<Vibe>,
    was_party_mode: bool,
    builtin_sample_cache: [Option<(Arc<Vec<f32>>, Arc<Vec<f32>>)>; 20],
    active_party_sample_id: Option<u8>,
    party_filter_hz: f32,
    party_filter_target_hz: f32,
    party_filter_mode: GlobalFilterMode,
    party_filter_res_boost: f32,
    party_space_boost: f32,
    party_space_target: f32,
    party_bass_cut_mul: f32,
    party_bass_cut_target: f32,
    party_pad_cut_mul: f32,
    party_pad_cut_target: f32,
    party_lead_cut_mul: f32,
    party_lead_cut_target: f32,
    party_acid_morph: f32,
    party_acid_target: f32,
    party_repeat_fx: f32,
    party_glitch_fx: f32,
    // Motion automation live playback overrides
    auto_filter: Option<f32>,
    auto_space: Option<f32>,
    auto_sidechain: Option<f32>,
    auto_glitch: Option<f32>,
    auto_iso_bass: Option<f32>,
    auto_iso_treble: Option<f32>,
    // Auto-beat & Party DJ song cache
    auto_beat_drums: crate::model::DrumTrackSeq,
    auto_beat_sig: (u8, u32, u32),
    party_melodic: (
        MelodicTrackSeq,
        MelodicTrackSeq,
        MelodicTrackSeq,
        MelodicTrackSeq,
        MelodicTrackSeq,
    ),
    party_melodic_sig: (u8, u8, usize, u32),
    // Peak metering
    peak_l: f32,
    peak_r: f32,
    track_peaks: [f32; 6],
    mono_sub_lpf: Svf12Filter,
    midi_bend_semi: f32,
    midi_aftertouch: f32,
}

impl Runtime {
    fn new(sr: f32) -> Self {
        let sr = sr.max(8_000.0);
        let sidechain_decay = (-1.0 / (0.145 * sr)).exp();
        Self {
            sr,
            moog: MoogSynth::new(sr),
            pad_moog: MoogSynth::new(sr),
            mono: MonologueSynth::new(sr),
            acid: Tb303Synth::new(sr),
            drums: DrumMachine::new(sr),
            sampler: Sk1Sampler::new(sr),
            volca: VolcaSamplerBank::new(sr),
            fx: MasterFx::new(sr),
            pad_chorus: AnalogStereoChorus::new(sr),
            arp_chorus: AnalogStereoChorus::new(sr),
            sampler_chorus: AnalogStereoChorus::new(sr),
            channel_eq: StudioChannelEq::new(sr),
            track_filters: std::array::from_fn(|_| (Svf12Filter::new(sr), Svf12Filter::new(sr))),
            pad_filter: (Svf12Filter::new(sr), Svf12Filter::new(sr)),
            extra_filters: (0..MAX_EXTRA_TRACKS)
                .map(|_| (Svf12Filter::new(sr), Svf12Filter::new(sr)))
                .collect(),
            gfilt_env: 0.0,
            gfilt_env_attacking: false,
            tick_left: 0.0,
            last_step_dur: (sr * 0.125).max(1.0),
            step: 0,
            was_playing: false,
            suppressed_steps: Vec::new(),
            gate_moog: 0.0,
            gate_pad: 0.0,
            gate_mono: 0.0,
            gate_acid: 0.0,
            gate_sampler: 0.0,
            extra_synths: (0..MAX_EXTRA_TRACKS).map(|_| MoogSynth::new(sr)).collect(),
            extra_gates: [0.0; MAX_EXTRA_TRACKS],
            last_moog_note: None,
            last_mono_note: None,
            last_acid_note: None,
            last_pad_notes: Vec::with_capacity(8),
            sidechain_peak: 0.0,
            sidechain_env: 0.0,
            sidechain_decay,
            drop_phase: 0.0,
            drop_env: 0.0,
            drop_pitch: 65.0,
            drop_target_hz: 32.7,
            drop_macro_phase: DropMacroPhase::Idle,
            drop_macro_progress: 0.0,
            drop_macro_target_prog: 0.0,
            drop_riser_phase: 0.0,
            drop_riser_phase2: 0.0,
            drop_riser_env: 0.0,
            drop_riser_target_env: 0.0,
            drop_riser_root_hz: 130.8,
            drop_riser_filter: Svf12Filter::new(sr),
            drop_boom_env: 0.0,
            drop_boom_phase: 0.0,
            drop_boom_pitch: 128.0,
            mono_sub_lpf: Svf12Filter::new(sr),
            midi_bend_semi: 0.0,
            midi_aftertouch: 0.0,
            drop_whoosh_env: 0.0,
            drop_whoosh_filter: Svf12Filter::new(sr),
            drop_filter_slam: 0.0,
            drop_rng: Xorshift32::new(0xD00D_F00D),
            ratchets: Vec::with_capacity(16),
            volca_ratchets: Vec::with_capacity(16),
            pending_triggers: Vec::with_capacity(64),
            force_fill_steps: 0,
            live_chord_held: false,
            live_slot: None,
            live_arp: LiveArpState::new(),
            loaded_sample_gen: 0,
            party_section_idx: 0,
            party_bar_in_sec: 0,
            party_cycle: 0,
            party_evolution: 0,
            party_template_idx: 0,
            party_rig: None,
            party_rig_sig: (255, 255, 0, u32::MAX, 0, false, 0, None),
            party_rig_reroll: 0,
            party_rig_custom: false,
            party_queued_vibe: None,
            party_forced_vibe: None,
            party_live_bpm: 0.0,
            last_base_vibe: None,
            was_party_mode: false,
            builtin_sample_cache: std::array::from_fn(|_| None),
            active_party_sample_id: None,
            party_filter_hz: 18_000.0,
            party_filter_target_hz: 18_000.0,
            party_filter_mode: GlobalFilterMode::LowPass,
            party_filter_res_boost: 0.0,
            party_space_boost: 0.0,
            party_space_target: 0.0,
            party_bass_cut_mul: 1.0,
            party_bass_cut_target: 1.0,
            party_pad_cut_mul: 1.0,
            party_pad_cut_target: 1.0,
            party_lead_cut_mul: 1.0,
            party_lead_cut_target: 1.0,
            party_acid_morph: 0.35,
            party_acid_target: 0.35,
            party_repeat_fx: 0.0,
            party_glitch_fx: 0.0,
            auto_filter: None,
            auto_space: None,
            auto_sidechain: None,
            auto_glitch: None,
            auto_iso_bass: None,
            auto_iso_treble: None,
            auto_beat_drums: crate::model::DrumTrackSeq::default(),
            auto_beat_sig: (255, 0, 0),
            party_melodic: (
                MelodicTrackSeq::default(),
                MelodicTrackSeq::default(),
                MelodicTrackSeq::default(),
                MelodicTrackSeq::default(),
                MelodicTrackSeq::default(),
            ),
            party_melodic_sig: (255, 255, usize::MAX, 0),
            peak_l: 0.0,
            peak_r: 0.0,
            track_peaks: [0.0; 6],
        }
    }

    fn sync_sampler_if_needed(&mut self, p: &Project) {
        if self.loaded_sample_gen == p.sample_generation {
            return;
        }
        self.loaded_sample_gen = p.sample_generation;
        if let Some((left, right)) = p.sample_data.decode_stereo_pcm() {
            if !left.is_empty() {
                let _ = self.sampler.swap_stereo_buffer(
                    Arc::new(left),
                    Arc::new(right),
                    p.sample_data.sample_rate.max(8_000),
                );
                return;
            }
        }
        let builtin = p.sample_data.builtin_id.unwrap_or(3);
        let (l, r) = generate_builtin_sample_stereo(builtin, self.sr as u32);
        let _ = self
            .sampler
            .swap_stereo_buffer(Arc::new(l), Arc::new(r), self.sr as u32);
    }

    fn party_dj_root(&self, p: &Project) -> u8 {
        if p.party_mode && self.party_cycle > 0 {
            const KEY_SHIFTS: [u8; 8] = [0, 7, 5, 10, 3, 8, 2, 9];
            let shift = KEY_SHIFTS[(self.party_cycle as usize) % KEY_SHIFTS.len()];
            (p.root_note + shift) % 12
        } else {
            p.root_note
        }
    }

    fn sync_party_sampler_buffer(&mut self, rig: &PartySongRig) {
        if let Some(id) = rig.sampler_builtin_id {
            if self.active_party_sample_id != Some(id) {
                let slot = (id as usize) % self.builtin_sample_cache.len();
                if self.builtin_sample_cache[slot].is_none() {
                    let (l, r) = generate_builtin_sample_stereo(id, self.sr as u32);
                    self.builtin_sample_cache[slot] = Some((Arc::new(l), Arc::new(r)));
                }
                if let Some((l, r)) = &self.builtin_sample_cache[slot] {
                    let _ = self.sampler.swap_stereo_buffer(
                        Arc::clone(l),
                        Arc::clone(r),
                        self.sr as u32,
                    );
                    self.active_party_sample_id = Some(id);
                }
            }
        }
    }

    fn ensure_party_rig(&mut self, p: &Project) {
        let forced = self
            .party_forced_vibe
            .and_then(|(c, v)| if c == self.party_cycle { Some(v) } else { None });
        let sig = (
            p.vibe as u8,
            p.party_mix_mode as u8,
            p.party_crate_mask,
            self.party_cycle,
            p.party_seed,
            p.party_sound_morph,
            self.party_rig_reroll,
            forced.map(|v| v as u8),
        );
        if self.party_rig_sig == sig && self.party_rig.is_some() {
            return;
        }
        let song_vibe = forced.unwrap_or_else(|| {
            pick_party_vibe_crate(
                p.vibe,
                p.party_mix_mode,
                p.party_crate_mask,
                self.party_cycle,
                p.party_seed,
            )
        });
        let rig = build_party_song_rig_with_reroll(
            p,
            song_vibe,
            self.party_cycle,
            p.party_seed,
            p.party_sound_morph || self.party_rig_custom,
            self.party_rig_reroll,
        );

        // Pick a fresh song arrangement template that differs from the previous song
        let prev_tmpl = self.party_template_idx;
        let h = p
            .party_seed
            .wrapping_add(self.party_cycle.wrapping_mul(2654435761))
            ^ ((song_vibe as u32).wrapping_mul(193));
        let mut tmpl_idx = if self.party_cycle == 0 {
            0
        } else {
            (h as usize) % PARTY_TEMPLATES.len()
        };
        if self.party_cycle > 0 && tmpl_idx == prev_tmpl {
            tmpl_idx = (tmpl_idx + 1) % PARTY_TEMPLATES.len();
        }
        self.party_template_idx = tmpl_idx;

        if p.party_sound_morph || self.party_rig_custom {
            self.sync_party_sampler_buffer(&rig);
        }

        if self.party_live_bpm < 40.0 {
            self.party_live_bpm = p.bpm;
        }
        self.party_rig = Some(rig);
        self.party_rig_sig = sig;
    }

    #[inline]
    fn trigger_global_filter_env(&mut self) {
        self.gfilt_env_attacking = true;
    }

    fn start_drop_macro(&mut self, p: &Project, max_steps_cap: Option<usize>) {
        let bars = match p.drop_macro.build_bars {
            1 => 1usize,
            4 => 4usize,
            _ => 2usize,
        };
        let total_steps = (bars * 16)
            .min(max_steps_cap.unwrap_or(usize::MAX).max(16));
        let gap_steps = (p.drop_macro.pre_drop_gap_steps as usize).min(total_steps / 2);
        let root = if p.party_mode {
            self.party_dj_root(p)
        } else {
            p.root_note
        };
        self.drop_macro_phase = DropMacroPhase::Building {
            step_idx: 0,
            total_steps,
            gap_steps,
        };
        self.drop_macro_progress = 0.0;
        self.drop_macro_target_prog = 0.0;
        self.drop_riser_root_hz = crate::model::midi_to_hz((48 + (root % 12)) as f32);
        self.drop_riser_phase = 0.0;
        self.drop_riser_phase2 = 0.0;
        if p.drop_macro.cut_bass {
            self.gate_moog = 0.0;
            if let Some(n) = self.last_moog_note.take() {
                self.moog.note_off(n);
            }
        }
    }

    fn trigger_drop_impact(
        &mut self,
        root_note: u8,
        intensity: f32,
        drums_p: &crate::model::DrumKitParams,
        trigger_drums: bool,
    ) {
        let inten = intensity.clamp(0.2, 1.0);
        let low_midi = (24 + (root_note % 12)) as f32;
        let high_hz = crate::model::midi_to_hz(low_midi + 24.0);
        let low_hz = crate::model::midi_to_hz(low_midi);
        self.drop_pitch = high_hz;
        self.drop_target_hz = low_hz;
        self.drop_env = inten;
        self.drop_phase = 0.0;
        self.drop_boom_env = inten;
        self.drop_boom_pitch = 128.0;
        self.drop_boom_phase = 0.0;
        self.drop_whoosh_env = inten;
        self.drop_filter_slam = 1.0;
        self.sidechain_peak = self.sidechain_peak.max(1.0);
        self.trigger_global_filter_env();
        if trigger_drums {
            self.drums
                .trigger(DrumVoiceId::Kick, 1.0, true, false, drums_p);
            self.drums.trigger(
                DrumVoiceId::Cymbal,
                (0.75 + 0.25 * inten).min(1.0),
                true,
                false,
                drums_p,
            );
        }
    }

    fn handle_live_event(&mut self, ev: LiveEvent, p: &Project) {
        match ev {
            LiveEvent::PlayChord {
                notes,
                bass,
                velocity,
                slot,
            } => {
                if p.arp.enabled {
                    let incoming_len = notes.len().min(16);
                    let same_chord = self.live_chord_held
                        && self.live_arp.target == ArpTarget::ChordPad
                        && self.live_arp.held.len() == incoming_len
                        && self
                            .live_arp
                            .held
                            .iter()
                            .zip(notes.iter().take(16))
                            .all(|(&(n, _), &m)| n == m);
                    if same_chord {
                        self.gate_moog = self.sr * 8.0;
                    } else {
                        self.stop_arp_sounding();
                        if let Some(prev_b) = self.live_arp.bass_latched.take() {
                            self.moog.note_off(prev_b);
                        }
                        self.pad_moog.all_notes_off();
                        self.live_arp.target = ArpTarget::ChordPad;
                        self.live_arp.held.clear();
                        self.live_arp.latched.clear();
                        for &n in notes.iter().take(16) {
                            self.live_arp.held.push((n, velocity));
                            self.live_arp.latched.push((n, velocity));
                        }
                        self.live_arp.tick_left = 0.0;
                        self.live_arp.step_idx = 0;
                        if let Some(b) = bass {
                            self.moog.note_on(b, velocity * 0.9, false, false, &p.moog);
                            self.last_moog_note = Some(b);
                            self.live_arp.bass_latched = Some(b);
                        }
                        self.live_chord_held = true;
                        self.live_slot = Some(slot);
                        self.gate_moog = self.sr * 8.0;
                    }
                } else if self.live_chord_held && self.live_slot == Some(slot) {
                    self.gate_pad = self.sr * 8.0;
                    self.gate_moog = self.sr * 8.0;
                } else {
                    self.pad_moog.all_notes_off();
                    for &n in &notes {
                        self.pad_moog
                            .note_on(n, velocity, false, false, &p.moog_pad);
                    }
                    if let Some(b) = bass {
                        self.moog.note_on(b, velocity * 0.9, false, false, &p.moog);
                        self.last_moog_note = Some(b);
                    }
                    self.live_chord_held = true;
                    self.live_slot = Some(slot);
                    // Keep live chord ringing while held
                    self.gate_pad = self.sr * 8.0;
                    self.gate_moog = self.sr * 8.0;
                }
            }
            LiveEvent::ReleaseChord => {
                self.live_arp.held.clear();
                self.live_chord_held = false;
                if p.arp.enabled && self.live_arp.target == ArpTarget::ChordPad {
                    if !p.arp.latch {
                        self.live_arp.latched.clear();
                        self.stop_arp_sounding();
                        if let Some(b) = self.live_arp.bass_latched.take() {
                            self.moog.note_off(b);
                        }
                        if let Some(b) = self.last_moog_note.take() {
                            self.moog.note_off(b);
                        }
                        self.gate_moog = 0.0;
                    }
                } else {
                    self.pad_moog.all_notes_off();
                    if let Some(b) = self.last_moog_note.take() {
                        self.moog.note_off(b);
                    }
                    self.gate_pad = 0.0;
                    self.gate_moog = 0.0;
                }
            }
            LiveEvent::PlayNote {
                track,
                note,
                velocity,
            } => {
                if p.arp.enabled && !matches!(track, TrackId::Drums | TrackId::MidiOut) {
                    let tgt = ArpTarget::Core(track);
                    let already_held = self.live_arp.target == tgt
                        && self.live_arp.held.iter().any(|&(n, _)| n == note);
                    if self.live_arp.target != tgt
                        || (p.arp.latch && self.live_arp.held.is_empty())
                    {
                        self.live_arp.latched.clear();
                        self.stop_arp_sounding();
                        if let Some(b) = self.live_arp.bass_latched.take() {
                            self.moog.note_off(b);
                        }
                    }
                    self.live_arp.target = tgt;
                    if let Some(entry) = self.live_arp.held.iter_mut().find(|(n, _)| *n == note) {
                        entry.1 = velocity;
                    } else if self.live_arp.held.len() < 16 {
                        self.live_arp.held.push((note, velocity));
                    }
                    if let Some(entry) = self.live_arp.latched.iter_mut().find(|(n, _)| *n == note)
                    {
                        entry.1 = velocity;
                    } else if self.live_arp.latched.len() < 16 {
                        self.live_arp.latched.push((note, velocity));
                    }
                    if !already_held && self.live_arp.latched.len() == 1 {
                        self.live_arp.tick_left = 0.0;
                        self.live_arp.step_idx = 0;
                    }
                } else {
                    match track {
                        TrackId::Moog => {
                            self.moog.note_on(note, velocity, false, false, &p.moog);
                            self.last_moog_note = Some(note);
                            self.gate_moog = self.sr * 4.0;
                        }
                        TrackId::MoogPad => {
                            self.pad_moog
                                .note_on(note, velocity, false, false, &p.moog_pad);
                            self.gate_pad = self.sr * 4.0;
                        }
                        TrackId::Monologue => {
                            self.mono
                                .note_on(note, velocity, false, false, &p.monologue);
                            self.last_mono_note = Some(note);
                            self.gate_mono = self.sr * 4.0;
                        }
                        TrackId::Tb303 => {
                            self.acid.note_on(note, velocity, false, false);
                            self.last_acid_note = Some(note);
                            self.gate_acid = self.sr * 4.0;
                        }
                        TrackId::Sampler => {
                            self.sampler.note_on(note, velocity, &p.sampler);
                            self.gate_sampler = self.sr * 4.0;
                        }
                        TrackId::Drums => {
                            let lane = (note as usize) % 8;
                            if let Some(v) = DrumVoiceId::from_index(lane) {
                                self.drums.trigger(v, velocity, false, false, &p.drums);
                                if v == DrumVoiceId::Kick {
                                    self.sidechain_peak = self.sidechain_peak.max(velocity);
                                    if p.fx.global_filter_env_trigger == FilterEnvTrigger::Kick {
                                        self.trigger_global_filter_env();
                                    }
                                }
                            }
                        }
                        TrackId::MidiOut => {}
                    }
                }
            }
            LiveEvent::ReleaseNote { track, note } => {
                self.live_arp.held.retain(|&(n, _)| n != note);
                if p.arp.enabled && !matches!(track, TrackId::Drums | TrackId::MidiOut) {
                    if !p.arp.latch {
                        self.live_arp.latched.retain(|&(n, _)| n != note);
                        if self.live_arp.latched.is_empty() {
                            self.stop_arp_sounding();
                        }
                    }
                } else {
                    match track {
                        TrackId::Moog => self.moog.note_off(note),
                        TrackId::MoogPad => self.pad_moog.note_off(note),
                        TrackId::Monologue => self.mono.note_off(note),
                        TrackId::Tb303 => self.acid.note_off(note),
                        TrackId::Sampler => self.sampler.note_off(note),
                        TrackId::Drums | TrackId::MidiOut => {}
                    }
                }
            }
            LiveEvent::TriggerBassDrop => {
                if self.was_playing {
                    if matches!(self.drop_macro_phase, DropMacroPhase::Building { .. }) {
                        // Clicking again while building cancels back to normal groove
                        self.drop_macro_phase = DropMacroPhase::Idle;
                        self.drop_riser_target_env = 0.0;
                        self.drop_macro_target_prog = 0.0;
                    } else {
                        self.start_drop_macro(p, None);
                    }
                } else {
                    let root = if p.party_mode {
                        self.party_dj_root(p)
                    } else {
                        p.root_note
                    };
                    let drums_p = if p.party_mode {
                        self.party_rig
                            .as_ref()
                            .map(|r| r.drums.clone())
                            .unwrap_or_else(|| p.drums.clone())
                    } else {
                        p.drums.clone()
                    };
                    self.trigger_drop_impact(root, p.drop_macro.impact_intensity, &drums_p, true);
                    self.drop_macro_phase = DropMacroPhase::Impact { steps_left: 16 };
                }
            }
            LiveEvent::TriggerGlobalFilterEnv => {
                self.trigger_global_filter_env();
            }
            LiveEvent::TriggerFill => self.force_fill_steps = 8,
            LiveEvent::NextPartySection => {
                let tmpl = PARTY_TEMPLATES[self.party_template_idx % PARTY_TEMPLATES.len()];
                self.party_section_idx = (self.party_section_idx + 1) % tmpl.len();
                self.party_bar_in_sec = 0;
                if self.party_section_idx == 0 {
                    self.party_cycle = self.party_cycle.wrapping_add(1);
                    if let Some(qv) = self.party_queued_vibe.take() {
                        self.party_forced_vibe = Some((self.party_cycle, qv));
                    }
                    self.party_space_boost = self.party_space_boost.max(0.42);
                    self.party_rig_reroll = 0;
                    self.party_rig_custom = false;
                    self.party_rig = None;
                    self.ensure_party_rig(p);
                }
                let tmpl = PARTY_TEMPLATES[self.party_template_idx % PARTY_TEMPLATES.len()];
                let sec = &tmpl[self.party_section_idx];
                if sec.evolve_progression {
                    self.party_evolution = self.party_evolution.wrapping_add(1);
                }
                if sec.drop_on_start {
                    let song_root = self.party_dj_root(p);
                    let drums_p = self
                        .party_rig
                        .as_ref()
                        .map(|r| r.drums.clone())
                        .unwrap_or_else(|| p.drums.clone());
                    self.trigger_drop_impact(
                        song_root,
                        p.drop_macro.impact_intensity.max(0.8),
                        &drums_p,
                        true,
                    );
                }
            }
            LiveEvent::NextPartySong => {
                // DJ mix straight into a brand-new song with fresh vibe, template, and voice palette!
                self.party_cycle = self.party_cycle.wrapping_add(1);
                if let Some(qv) = self.party_queued_vibe.take() {
                    self.party_forced_vibe = Some((self.party_cycle, qv));
                }
                self.party_space_boost = self.party_space_boost.max(0.42);
                self.party_evolution = self.party_evolution.wrapping_add(1);
                self.party_section_idx = 0;
                self.party_bar_in_sec = 0;
                self.force_fill_steps = 8;
                self.party_rig_reroll = 0;
                self.party_rig_custom = false;
                self.party_rig = None;
                self.ensure_party_rig(p);
                let song_root = self.party_dj_root(p);
                let drums_p = self
                    .party_rig
                    .as_ref()
                    .map(|r| r.drums.clone())
                    .unwrap_or_else(|| p.drums.clone());
                self.trigger_drop_impact(
                    song_root,
                    p.drop_macro.impact_intensity.max(0.8),
                    &drums_p,
                    true,
                );
            }
            LiveEvent::PrevPartySong => {
                self.party_cycle = self.party_cycle.saturating_sub(1);
                self.party_section_idx = 0;
                self.party_bar_in_sec = 0;
                self.party_space_boost = self.party_space_boost.max(0.36);
                self.party_rig_reroll = 0;
                self.party_rig_custom = false;
                self.party_rig = None;
                self.ensure_party_rig(p);
            }
            LiveEvent::QueueNextPartyVibe(vibe_opt) => {
                self.party_queued_vibe = vibe_opt;
            }
            LiveEvent::SetMidiExpression { bend_semi, aftertouch } => {
                self.midi_bend_semi = bend_semi.clamp(-12.0, 12.0);
                self.midi_aftertouch = aftertouch.clamp(0.0, 1.0);
            }
            LiveEvent::ReloadPartyRig => {
                let prev_presets = self.party_rig.as_ref().map(|r| r.voice_presets);
                let song_vibe = self
                    .party_rig
                    .as_ref()
                    .map(|r| r.vibe)
                    .unwrap_or_else(|| {
                        pick_party_vibe_crate(
                            p.vibe,
                            p.party_mix_mode,
                            p.party_crate_mask,
                            self.party_cycle,
                            p.party_seed,
                        )
                    });
                self.party_rig_reroll = self.party_rig_reroll.wrapping_add(1).max(1);
                self.party_rig_custom = true;
                let mut rig = build_party_song_rig_with_reroll(
                    p,
                    song_vibe,
                    self.party_cycle,
                    p.party_seed,
                    true,
                    self.party_rig_reroll,
                );
                if let Some(prev) = prev_presets {
                    for _ in 0..3 {
                        if rig.voice_presets[1] == prev[1] && rig.voice_presets[3] == prev[3] {
                            self.party_rig_reroll = self.party_rig_reroll.wrapping_add(1).max(1);
                            rig = build_party_song_rig_with_reroll(
                                p,
                                song_vibe,
                                self.party_cycle,
                                p.party_seed,
                                true,
                                self.party_rig_reroll,
                            );
                        } else {
                            break;
                        }
                    }
                }
                self.sync_party_sampler_buffer(&rig);
                self.moog.clear_motion_locks();
                self.pad_moog.clear_motion_locks();
                self.mono.clear_motion_locks();
                self.acid.clear_motion_locks();
                self.party_rig = Some(rig);
                let forced = self
                    .party_forced_vibe
                    .and_then(|(c, v)| if c == self.party_cycle { Some(v) } else { None });
                self.party_rig_sig = (
                    p.vibe as u8,
                    p.party_mix_mode as u8,
                    p.party_crate_mask,
                    self.party_cycle,
                    p.party_seed,
                    p.party_sound_morph,
                    self.party_rig_reroll,
                    forced.map(|v| v as u8),
                );
            }
            LiveEvent::SetPartyInstrument { track, preset_idx } => {
                self.ensure_party_rig(p);
                self.party_rig_custom = true;
                let mut tmp = Project::factory(FactoryPreset::BlankInit);
                apply_voice_preset(&mut tmp, track, preset_idx);
                if let Some(rig) = self.party_rig.as_mut() {
                    match track {
                        TrackId::Drums => {
                            rig.drums = tmp.drums;
                            rig.voice_presets[0] = tmp.voice_presets[0];
                        }
                        TrackId::Moog => {
                            rig.moog = tmp.moog;
                            rig.voice_presets[1] = tmp.voice_presets[1];
                            self.moog.clear_motion_locks();
                        }
                        TrackId::MoogPad => {
                            rig.moog_pad = tmp.moog_pad;
                            rig.voice_presets[2] = tmp.voice_presets[2];
                            self.pad_moog.clear_motion_locks();
                        }
                        TrackId::Monologue => {
                            rig.monologue = tmp.monologue;
                            rig.voice_presets[3] = tmp.voice_presets[3];
                            self.mono.clear_motion_locks();
                        }
                        TrackId::Tb303 => {
                            rig.tb303 = tmp.tb303;
                            rig.voice_presets[4] = tmp.voice_presets[4];
                            rig.enable_303 = true;
                            self.acid.clear_motion_locks();
                        }
                        TrackId::Sampler => {
                            rig.sampler = tmp.sampler;
                            rig.sampler_builtin_id = tmp.sample_data.builtin_id;
                            rig.voice_presets[5] = tmp.voice_presets[5];
                            rig.enable_sampler = true;
                            self.sampler.clear_motion_locks();
                        }
                        TrackId::MidiOut => {}
                    }
                    rig.palette_label = party_palette_label(rig.voice_presets);
                }
                if track == TrackId::Sampler {
                    if let Some(rig) = self.party_rig.clone() {
                        self.sync_party_sampler_buffer(&rig);
                    }
                }
            }
            LiveEvent::TriggerSamplePart { part, velocity } => {
                self.volca.sync_parts(&p.sample_parts);
                self.volca.trigger_part(part, velocity, &p.sample_parts);
            }
            LiveEvent::TriggerSamplePartPitched {
                part,
                velocity,
                pitch_offset,
            } => {
                self.volca.sync_parts(&p.sample_parts);
                let st = crate::model::DrumStep {
                    active: true,
                    velocity,
                    pitch_offset: pitch_offset.clamp(-24, 24),
                    ..Default::default()
                };
                self.volca
                    .trigger_part_with_step(part, velocity, &st, &p.sample_parts);
            }
            LiveEvent::ReleaseSamplePart { part } => {
                self.volca.release_part(part);
            }
            LiveEvent::PlayExtraNote { idx, note, velocity } => {
                if p.arp.enabled {
                    let tgt = ArpTarget::Extra(idx);
                    if self.live_arp.target != tgt
                        || (p.arp.latch && self.live_arp.held.is_empty())
                    {
                        self.live_arp.latched.clear();
                        self.stop_arp_sounding();
                    }
                    self.live_arp.target = tgt;
                    if let Some(entry) = self.live_arp.held.iter_mut().find(|(n, _)| *n == note) {
                        entry.1 = velocity;
                    } else if self.live_arp.held.len() < 16 {
                        self.live_arp.held.push((note, velocity));
                    }
                    if let Some(entry) = self.live_arp.latched.iter_mut().find(|(n, _)| *n == note)
                    {
                        entry.1 = velocity;
                    } else if self.live_arp.latched.len() < 16 {
                        self.live_arp.latched.push((note, velocity));
                    }
                    if self.live_arp.latched.len() == 1 {
                        self.live_arp.tick_left = 0.0;
                        self.live_arp.step_idx = 0;
                    }
                } else if let (Some(synth), Some(tr)) = (self.extra_synths.get_mut(idx), p.extra_tracks.get(idx)) {
                    synth.note_on(note, velocity, false, false, &tr.synth);
                    if idx < MAX_EXTRA_TRACKS {
                        self.extra_gates[idx] = self.sr * 4.0;
                    }
                }
            }
            LiveEvent::ReleaseExtraNote { idx, note } => {
                self.live_arp.held.retain(|&(n, _)| n != note);
                if p.arp.enabled {
                    if !p.arp.latch {
                        self.live_arp.latched.retain(|&(n, _)| n != note);
                        if self.live_arp.latched.is_empty() {
                            self.stop_arp_sounding();
                        }
                    }
                } else if let Some(synth) = self.extra_synths.get_mut(idx) {
                    synth.note_off(note);
                }
            }
            LiveEvent::SuppressTrackStep {
                track,
                extra_idx,
                lane,
                global_step,
            } => {
                if self.suppressed_steps.len() < 32 {
                    self.suppressed_steps
                        .push((track, extra_idx, lane, global_step));
                }
            }
            LiveEvent::LoadSoundFont { bank, .. } => {
                let _ = self.moog.set_soundfont_bank(bank.clone());
                let _ = self.pad_moog.set_soundfont_bank(bank.clone());
                let _ = self.mono.set_soundfont_bank(bank.clone());
                for s in &mut self.extra_synths {
                    let _ = s.set_soundfont_bank(bank.clone());
                }
            }
            LiveEvent::JumpToSongSection { section_idx } => {
                let start_step: usize = p
                    .song_sections
                    .iter()
                    .take(section_idx)
                    .map(|s| s.bars.max(1) as usize * 16)
                    .sum();
                self.step = start_step;
                self.tick_left = 0.0;
                self.all_notes_off();
            }
        }
    }

    fn is_step_suppressed(
        &self,
        track: Option<TrackId>,
        extra_idx: Option<usize>,
        lane: Option<usize>,
        global: usize,
    ) -> bool {
        self.suppressed_steps
            .iter()
            .any(|&(t, e, l, gs)| gs == global && t == track && e == extra_idx && l == lane)
    }

    fn all_notes_off(&mut self) {
        self.moog.all_notes_off();
        self.pad_moog.all_notes_off();
        self.mono.all_notes_off();
        self.acid.all_notes_off();
        self.sampler.all_notes_off();
        self.volca.all_notes_off();
        for (i, s) in self.extra_synths.iter_mut().enumerate() {
            s.all_notes_off();
            self.extra_gates[i] = 0.0;
        }
        self.gate_moog = 0.0;
        self.gate_pad = 0.0;
        self.gate_mono = 0.0;
        self.gate_acid = 0.0;
        self.gate_sampler = 0.0;
        self.ratchets.clear();
        self.volca_ratchets.clear();
        self.pending_triggers.clear();
        self.last_pad_notes.clear();
        self.stop_arp_sounding();
        if let Some(b) = self.live_arp.bass_latched.take() {
            self.moog.note_off(b);
        }
    }

    fn trigger_arp_voice(&mut self, target: ArpTarget, note: u8, velocity: f32, p: &Project) {
        let vel = velocity.clamp(0.05, 1.0);
        let use_rig = p.party_mode && (p.party_sound_morph || self.party_rig_custom);
        match target {
            ArpTarget::Core(TrackId::Moog) => {
                let mp = if use_rig {
                    self.party_rig.as_ref().map(|r| &r.moog).unwrap_or(&p.moog)
                } else {
                    &p.moog
                };
                self.moog.note_on(note, vel, false, false, mp);
            }
            ArpTarget::Core(TrackId::MoogPad) | ArpTarget::ChordPad => {
                let pp = if use_rig {
                    self.party_rig.as_ref().map(|r| &r.moog_pad).unwrap_or(&p.moog_pad)
                } else {
                    &p.moog_pad
                };
                self.pad_moog.note_on(note, vel, false, false, pp);
            }
            ArpTarget::Core(TrackId::Monologue) => {
                let mp = if use_rig {
                    self.party_rig.as_ref().map(|r| &r.monologue).unwrap_or(&p.monologue)
                } else {
                    &p.monologue
                };
                self.mono.clear_motion_locks();
                self.mono.note_on(note, vel, false, false, mp);
            }
            ArpTarget::Core(TrackId::Tb303) => {
                self.acid.clear_motion_locks();
                self.acid.note_on(note, vel, false, false);
            }
            ArpTarget::Core(TrackId::Sampler) => {
                let sp = if use_rig {
                    self.party_rig.as_ref().map(|r| &r.sampler).unwrap_or(&p.sampler)
                } else {
                    &p.sampler
                };
                self.sampler.clear_motion_locks();
                self.sampler.note_on(note, vel, sp);
            }
            ArpTarget::Extra(idx) => {
                if let (Some(synth), Some(tr)) = (self.extra_synths.get_mut(idx), p.extra_tracks.get(idx)) {
                    synth.note_on(note, vel, false, false, &tr.synth);
                }
            }
            ArpTarget::Core(TrackId::Drums | TrackId::MidiOut) => {}
        }
    }

    fn release_arp_voice(&mut self, target: ArpTarget, note: u8) {
        match target {
            ArpTarget::Core(TrackId::Moog) => self.moog.note_off(note),
            ArpTarget::Core(TrackId::MoogPad) | ArpTarget::ChordPad => self.pad_moog.note_off(note),
            ArpTarget::Core(TrackId::Monologue) => self.mono.note_off(note),
            ArpTarget::Core(TrackId::Tb303) => self.acid.note_off(note),
            ArpTarget::Core(TrackId::Sampler) => self.sampler.note_off(note),
            ArpTarget::Extra(idx) => {
                if let Some(synth) = self.extra_synths.get_mut(idx) {
                    synth.note_off(note);
                }
            }
            ArpTarget::Core(TrackId::Drums | TrackId::MidiOut) => {}
        }
    }

    fn stop_arp_sounding(&mut self) {
        if let Some((tgt, note)) = self.live_arp.sounding.take() {
            self.release_arp_voice(tgt, note);
        }
        self.live_arp.gate_left = 0.0;
    }

    fn process_live_arp(&mut self, p: &Project) {
        if self.live_arp.was_enabled && !p.arp.enabled {
            self.stop_arp_sounding();
            self.live_arp.held.clear();
            self.live_arp.latched.clear();
            if let Some(b) = self.live_arp.bass_latched.take() {
                self.moog.note_off(b);
            }
        }
        if self.live_arp.was_latch && !p.arp.latch {
            let held_copy = self.live_arp.held.clone();
            self.live_arp.latched = held_copy;
            if self.live_arp.latched.is_empty() {
                self.stop_arp_sounding();
                if let Some(b) = self.live_arp.bass_latched.take() {
                    self.moog.note_off(b);
                }
            }
        }
        self.live_arp.was_enabled = p.arp.enabled;
        self.live_arp.was_latch = p.arp.latch;

        if !p.arp.enabled {
            return;
        }

        if self.live_arp.gate_left > 0.0 {
            self.live_arp.gate_left -= 1.0;
            if self.live_arp.gate_left <= 0.0 && self.live_arp.sounding.is_some() {
                self.stop_arp_sounding();
            }
        }

        if self.live_arp.latched.is_empty() {
            return;
        }

        if self.live_arp.tick_left > 0.0 {
            self.live_arp.tick_left -= 1.0;
            return;
        }

        // Build expanded arpeggiator sequence across 1..=3 octaves (zero audio glitches, small stack Vec)
        let octaves = p.arp.octaves.clamp(1, 3);
        let mut pool = self.live_arp.latched.clone();
        match p.arp.mode {
            ArpMode::Up | ArpMode::UpDown => pool.sort_by_key(|&(n, _)| n),
            ArpMode::Down => pool.sort_by_key(|&(n, _)| std::cmp::Reverse(n)),
            ArpMode::AsPlayed | ArpMode::Random => {}
        }

        let mut seq: Vec<(u8, f32)> = Vec::with_capacity(pool.len() * octaves as usize * 2);
        if p.arp.mode == ArpMode::Down {
            for oct in (0..octaves).rev() {
                for &(n, v) in &pool {
                    seq.push(((n as u16 + oct as u16 * 12).min(108) as u8, v));
                }
            }
        } else {
            for oct in 0..octaves {
                for &(n, v) in &pool {
                    seq.push(((n as u16 + oct as u16 * 12).min(108) as u8, v));
                }
            }
        }

        if p.arp.mode == ArpMode::UpDown && seq.len() >= 3 {
            let n = seq.len();
            for i in (1..n - 1).rev() {
                seq.push(seq[i]);
            }
        }

        if seq.is_empty() {
            return;
        }

        let pick_idx = if p.arp.mode == ArpMode::Random {
            if seq.len() == 1 {
                0
            } else {
                let mut idx = (self.live_arp.rng.next_u32() as usize) % seq.len();
                if idx == self.live_arp.last_rand_idx {
                    idx = (idx + 1) % seq.len();
                }
                self.live_arp.last_rand_idx = idx;
                idx
            }
        } else {
            self.live_arp.step_idx % seq.len()
        };

        let (note, vel) = seq[pick_idx];
        self.stop_arp_sounding();
        let target = self.live_arp.target;
        self.trigger_arp_voice(target, note, vel, p);
        self.live_arp.sounding = Some((target, note));

        let bpm = if p.party_mode && self.party_live_bpm >= 40.0 {
            self.party_live_bpm
        } else {
            p.bpm
        }
        .clamp(40.0, 240.0);
        let base_dur = (60.0 / bpm) * p.arp.rate.step_beats() * self.sr;
        let step_dur = if p.arp.swing && p.arp.rate == ArpRate::Sixteenth {
            let sw = p.swing.clamp(0.0, 0.5) * 0.65;
            if self.live_arp.step_idx % 2 == 0 {
                base_dur * (1.0 + sw)
            } else {
                base_dur * (1.0 - sw)
            }
        } else {
            base_dur
        };

        self.live_arp.tick_left = step_dur.max(64.0);
        self.live_arp.gate_left = (step_dur * p.arp.gate.clamp(0.15, 0.95)).max(32.0);
        self.live_arp.step_idx = self.live_arp.step_idx.wrapping_add(1);
    }

    #[allow(clippy::too_many_arguments)]
    fn dispatch_mono_note(
        &mut self,
        p: &Project,
        use_rig: bool,
        note: u8,
        prev_note: Option<u8>,
        vel: f32,
        accent: bool,
        slide: bool,
        motion: MotionLocks,
        apply_party_cut: bool,
        gate_samples: f32,
    ) {
        if motion.any_locked() {
            let mut m = motion;
            if apply_party_cut {
                if let Some(c) = m.cutoff {
                    m.cutoff = Some((c + (self.party_lead_cut_mul - 1.0) * 0.14).clamp(0.72, 0.96));
                }
            }
            self.mono.set_motion_locks(m, true);
        } else {
            self.mono.clear_motion_locks();
        }
        if !slide {
            if let Some(prev) = prev_note {
                self.mono.note_off(prev);
            }
        }
        let mono_p = if use_rig {
            self.party_rig
                .as_ref()
                .map(|r| &r.monologue)
                .unwrap_or(&p.monologue)
        } else {
            &p.monologue
        };
        self.mono.note_on(note, vel, accent, slide, mono_p);
        self.gate_mono = gate_samples;
    }

    #[allow(clippy::too_many_arguments)]
    fn dispatch_melodic_32(
        &mut self,
        p: &Project,
        use_rig: bool,
        target: ArpTarget,
        note: u8,
        vel: f32,
        accent: bool,
        slide: bool,
        chord: ChordType,
        motion: MotionLocks,
        gate_samples: f32,
        dj_root: u8,
    ) {
        match target {
            ArpTarget::Core(TrackId::Moog) => {
                if motion.any_locked() {
                    self.moog.set_motion_locks(motion);
                } else {
                    self.moog.clear_motion_locks();
                }
                if !slide {
                    if let Some(prev) = self.last_moog_note.take() {
                        self.moog.note_off(prev);
                    }
                }
                let moog_p = if use_rig {
                    self.party_rig.as_ref().map(|r| &r.moog).unwrap_or(&p.moog)
                } else {
                    &p.moog
                };
                self.moog.note_on(note, vel, accent, slide, moog_p);
                self.last_moog_note = Some(note);
                self.gate_moog = gate_samples;
            }
            ArpTarget::ChordPad | ArpTarget::Core(TrackId::MoogPad) => {
                if motion.any_locked() {
                    self.pad_moog.set_motion_locks(motion);
                } else {
                    self.pad_moog.clear_motion_locks();
                }
                self.pad_moog.all_notes_off();
                let pad_p = if use_rig {
                    self.party_rig.as_ref().map(|r| &r.moog_pad).unwrap_or(&p.moog_pad)
                } else {
                    &p.moog_pad
                };
                if chord != ChordType::Single {
                    let slot = ChordSlot {
                        offset: (note + 12 - (dj_root % 12)) % 12,
                        chord,
                    };
                    let notes = voice_led_chord_near(dj_root, slot, &self.last_pad_notes, 60);
                    self.last_pad_notes = notes.clone();
                    for n in notes {
                        self.pad_moog.note_on(n, vel, accent, false, pad_p);
                    }
                } else {
                    self.pad_moog.note_on(note, vel, accent, false, pad_p);
                }
                self.gate_pad = gate_samples;
            }
            ArpTarget::Core(TrackId::Monologue) => {
                let prev_note = self.last_mono_note.take();
                self.last_mono_note = Some(note);
                self.dispatch_mono_note(
                    p,
                    use_rig,
                    note,
                    prev_note,
                    vel,
                    accent,
                    slide,
                    motion,
                    false,
                    gate_samples,
                );
            }
            ArpTarget::Core(TrackId::Tb303) => {
                if motion.any_locked() {
                    self.acid.set_motion_locks(motion);
                } else {
                    self.acid.clear_motion_locks();
                }
                if !slide {
                    if let Some(prev) = self.last_acid_note.take() {
                        self.acid.note_off(prev);
                    }
                }
                self.acid.note_on(note, vel, accent, slide);
                self.last_acid_note = Some(note);
                self.gate_acid = gate_samples;
            }
            ArpTarget::Core(TrackId::Sampler) => {
                if motion.any_locked() {
                    self.sampler.set_motion_locks(motion);
                } else {
                    self.sampler.clear_motion_locks();
                }
                let samp_p = if use_rig {
                    self.party_rig.as_ref().map(|r| &r.sampler).unwrap_or(&p.sampler)
                } else {
                    &p.sampler
                };
                self.sampler.note_on(note, vel, samp_p);
                self.gate_sampler = gate_samples;
            }
            ArpTarget::Extra(ei) if ei < p.extra_tracks.len().min(MAX_EXTRA_TRACKS) => {
                let synth = &mut self.extra_synths[ei];
                let tr = &p.extra_tracks[ei];
                if motion.any_locked() {
                    synth.set_motion_locks(motion);
                } else {
                    synth.clear_motion_locks();
                }
                synth.all_notes_off();
                if chord != ChordType::Single {
                    let slot = ChordSlot {
                        offset: (note + 12 - (dj_root % 12)) % 12,
                        chord,
                    };
                    for n in voice_led_chord(dj_root, slot, 60) {
                        synth.note_on(n, vel, accent, false, &tr.synth);
                    }
                } else {
                    synth.note_on(note, vel, accent, false, &tr.synth);
                }
                self.extra_gates[ei] = gate_samples;
            }
            _ => {}
        }
    }

    fn step_duration_samples(&self, p: &Project, step_index: usize) -> f32 {
        let (bpm, swing) = if p.party_mode {
            let b = if self.party_live_bpm >= 40.0 {
                self.party_live_bpm
            } else {
                p.bpm
            };
            let sw = self
                .party_rig
                .as_ref()
                .map(|r| r.swing)
                .unwrap_or(p.swing);
            (b, sw)
        } else {
            (p.bpm, p.swing)
        };
        let base_step = (60.0 / bpm.clamp(40.0, 240.0) / 4.0) * self.sr;
        let swing_ratio = (swing * 0.64).clamp(0.0, 0.30);
        if step_index % 2 == 0 {
            base_step * (1.0 + swing_ratio)
        } else {
            base_step * (1.0 - swing_ratio)
        }
    }

    fn trigger_step(
        &mut self,
        p: &Project,
        drum_cut: bool,
        snare_build: bool,
        telemetry: &mut EngineTelemetry,
    ) {
        let global = self.step;

        // Determine active section mask, pattern index, fill, and Party DJ state
        let mut pat_idx = p.active_pattern.min(p.patterns.len().saturating_sub(1));
        let mut section_mask: u8 = 0b111111;
        let mut extra_section_mask: u16 = 0x03FF;
        let mut section_fill = false;
        let mut party_snare_roll = false;
        let mut party_section_energy = p.party_energy;
        let mut song_vibe = p.vibe;
        let mut active_bpm = p.bpm;
        let mut sec_lead_bright = 0.75_f32;

        if p.party_mode {
            if self.last_base_vibe.is_some() && self.last_base_vibe != Some(p.vibe) {
                self.party_cycle = 0;
                self.party_section_idx = 0;
                self.party_bar_in_sec = 0;
                self.party_evolution = 0;
                self.party_live_bpm = p.bpm;
                self.party_rig_reroll = 0;
                self.party_rig_custom = false;
                self.party_rig = None;
                self.party_forced_vibe = None;
            }
            self.last_base_vibe = Some(p.vibe);
            self.ensure_party_rig(p);

            if global > 0 && global % 16 == 0 {
                let tmpl = PARTY_TEMPLATES[self.party_template_idx % PARTY_TEMPLATES.len()];
                let cur_sec = &tmpl[self.party_section_idx % tmpl.len()];
                let cur_bars = scaled_section_bars(cur_sec.bars, p.party_pace);
                self.party_bar_in_sec += 1;
                if self.party_bar_in_sec >= cur_bars {
                    self.party_bar_in_sec = 0;
                    self.party_section_idx = (self.party_section_idx + 1) % tmpl.len();
                    if self.party_section_idx == 0 {
                        self.party_cycle = self.party_cycle.wrapping_add(1);
                        if let Some(qv) = self.party_queued_vibe.take() {
                            self.party_forced_vibe = Some((self.party_cycle, qv));
                        }
                        self.party_space_boost = self.party_space_boost.max(0.42);
                        self.last_pad_notes.clear();
                        self.force_fill_steps = 8; // DJ song mix transition fill!
                        self.party_rig_reroll = 0;
                        self.party_rig_custom = false;
                        self.party_rig = None;
                        self.ensure_party_rig(p);
                        let new_root = self.party_dj_root(p);
                        let drums_p = self
                            .party_rig
                            .as_ref()
                            .map(|r| r.drums.clone())
                            .unwrap_or_else(|| p.drums.clone());
                        self.trigger_drop_impact(
                            new_root,
                            p.drop_macro.impact_intensity.max(0.8),
                            &drums_p,
                            true,
                        );
                    }
                    let next_tmpl = PARTY_TEMPLATES[self.party_template_idx % PARTY_TEMPLATES.len()];
                    let next_sec = &next_tmpl[self.party_section_idx % next_tmpl.len()];
                    if next_sec.evolve_progression {
                        self.party_evolution = self.party_evolution.wrapping_add(1);
                    }
                    if next_sec.drop_on_start {
                        let new_root = self.party_dj_root(p);
                        let drums_p = self
                            .party_rig
                            .as_ref()
                            .map(|r| r.drums.clone())
                            .unwrap_or_else(|| p.drums.clone());
                        self.trigger_drop_impact(
                            new_root,
                            p.drop_macro.impact_intensity.max(0.8),
                            &drums_p,
                            true,
                        );
                    }
                }
            }

            let (rig_bpm, rig_vibe, rig_palette) = if let Some(rig) = &self.party_rig {
                (rig.bpm, rig.vibe, rig.palette_label.clone())
            } else {
                (p.bpm, p.vibe, String::new())
            };
            song_vibe = rig_vibe;

            let next_cycle = self.party_cycle.wrapping_add(1);
            let next_vibe = self.party_queued_vibe.unwrap_or_else(|| {
                pick_party_vibe_crate(
                    p.vibe,
                    p.party_mix_mode,
                    p.party_crate_mask,
                    next_cycle,
                    p.party_seed,
                )
            });
            const KEY_SHIFTS_NEXT: [u8; 8] = [0, 7, 5, 10, 3, 8, 2, 9];
            let next_root = (p.root_note + KEY_SHIFTS_NEXT[(next_cycle as usize) % KEY_SHIFTS_NEXT.len()]) % 12;
            let next_bpm = next_vibe.default_bpm();
            telemetry.party_next_title = party_song_title(next_vibe, next_cycle, next_root);
            telemetry.party_next_vibe = next_vibe.name().to_string();
            telemetry.party_next_bpm = next_bpm;
            telemetry.party_queued_vibe = self.party_queued_vibe;

            let tmpl = PARTY_TEMPLATES[self.party_template_idx % PARTY_TEMPLATES.len()];
            let sec_idx = self.party_section_idx % tmpl.len();
            let sec = &tmpl[sec_idx];
            let sec_bars = scaled_section_bars(sec.bars, p.party_pace);
            let bar_progress =
                (self.party_bar_in_sec * 16 + (global % 16)) as f32 / (sec_bars * 16).max(1) as f32;

            let is_outro = sec_idx + 1 == tmpl.len();
            let target_bpm = if is_outro && p.party_auto_evolve {
                rig_bpm + (next_bpm - rig_bpm) * (bar_progress * 0.80)
            } else {
                rig_bpm
            };
            if self.party_live_bpm < 40.0 {
                self.party_live_bpm = rig_bpm;
            } else {
                let rate = if is_outro { 0.14 } else { 0.08 };
                self.party_live_bpm += (target_bpm - self.party_live_bpm) * rate;
            }
            active_bpm = self.party_live_bpm.clamp(40.0, 240.0);
            section_mask = sec.mask;
            party_section_energy = (p.party_energy * sec.energy_mul).clamp(0.18, 1.0);
            sec_lead_bright = sec.lead_brightness;
            if p.party_energy < 0.35 {
                section_mask &= !0b010000; // pull 303 on chill energy
            }
            match sec.filter_sweep {
                PartyFilterSweep::LowPassOpen => {
                    self.party_filter_target_hz = 750.0 * (18_000.0_f32 / 750.0).powf(bar_progress);
                    self.party_filter_mode = GlobalFilterMode::LowPass;
                    self.party_filter_res_boost = 0.24;
                }
                PartyFilterSweep::LowPassClose => {
                    self.party_filter_target_hz = 16_000.0 * (700.0_f32 / 16_000.0).powf(bar_progress);
                    self.party_filter_mode = GlobalFilterMode::LowPass;
                    self.party_filter_res_boost = 0.26;
                }
                PartyFilterSweep::HighPassRise => {
                    self.party_filter_target_hz = 55.0 * (950.0_f32 / 55.0).powf(bar_progress);
                    self.party_filter_mode = GlobalFilterMode::HighPass;
                    self.party_filter_res_boost = 0.30;
                }
                PartyFilterSweep::AtmosBreath => {
                    self.party_filter_target_hz = 1100.0 * (14_000.0_f32 / 1100.0).powf(bar_progress);
                    self.party_filter_mode = GlobalFilterMode::LowPass;
                    self.party_filter_res_boost = 0.18;
                }
                PartyFilterSweep::None => {
                    self.party_filter_target_hz = 18_000.0;
                    self.party_filter_mode = GlobalFilterMode::LowPass;
                    self.party_filter_res_boost = 0.0;
                }
            }

            // Smooth 2-bar analog timbre breathing across Bass, Chords, Lead, and 303
            let wave = ((global as f32 / 32.0) * std::f32::consts::TAU).sin();
            self.party_bass_cut_target = (0.75 + party_section_energy * 0.38 + wave * 0.18).clamp(0.60, 1.45);
            self.party_pad_cut_target = (0.95 + sec.lead_brightness * 0.55 + wave * 0.25).clamp(0.85, 2.05);
            self.party_lead_cut_target = (1.05 + sec.lead_brightness * 0.60 - wave * 0.22).clamp(0.92, 2.20);
            self.party_acid_target =
                (sec.acid_intensity * (0.65 + 0.35 * (0.5 + 0.5 * wave)) * party_section_energy.max(0.45))
                    .clamp(0.0, 1.0);

            self.party_space_target = if sec.space_throw > 0.20 {
                sec.space_throw
            } else if sec.space_throw > 0.0 && self.party_bar_in_sec + 1 == sec_bars {
                sec.space_throw * ((global % 16) as f32 / 15.0)
            } else {
                0.0
            };

            self.party_repeat_fx = 0.0;
            self.party_glitch_fx = 0.0;
            if p.party_sound_morph
                && sec.turnaround_fx
                && self.party_bar_in_sec + 1 == sec_bars
                && p.party_energy >= 0.60
                && (global % 16) >= 12
            {
                let fx_roll = hash_f32(
                    p.party_seed,
                    (self.party_cycle as usize) * 17 + sec_idx,
                    777,
                );
                if matches!(
                    song_vibe,
                    Vibe::Eurobeat | Vibe::EuphoricTrance | Vibe::ImperialBass
                ) {
                    if fx_roll < 0.65 {
                        self.party_repeat_fx = 0.72;
                    }
                } else if fx_roll < 0.45 {
                    self.party_repeat_fx = 0.72;
                } else if fx_roll < 0.72 && (global % 16) >= 14 {
                    self.party_glitch_fx = 0.38;
                }
            }

            if sec.snare_build_last_bar && self.party_bar_in_sec + 1 == sec_bars {
                party_snare_roll = true;
            }
            if self.party_bar_in_sec + 1 == sec_bars && global % 16 >= 12 {
                section_fill = true;
            }

            telemetry.party_section = sec.name.to_string();
            telemetry.party_bar = self.party_bar_in_sec + 1;
            telemetry.party_section_bars = sec_bars;
            telemetry.party_song_idx = self.party_cycle;
            telemetry.party_palette = rig_palette;
            telemetry.party_voice_presets = self
                .party_rig
                .as_ref()
                .map(|r| r.voice_presets)
                .unwrap_or(p.voice_presets);
            telemetry.party_bpm = active_bpm;
            telemetry.party_vibe_name = song_vibe.name().to_string();
            telemetry.party_vibe = song_vibe;
            telemetry.party_rig_reroll = self.party_rig_reroll;
            telemetry.party_template_idx = self.party_template_idx;
            telemetry.party_rig_custom = self.party_rig_custom;
            telemetry.active_song_section = None;
        } else if p.song_mode && !p.song_sections.is_empty() {
            self.party_filter_target_hz = 18_000.0;
            self.party_filter_mode = GlobalFilterMode::LowPass;
            self.party_filter_res_boost = 0.0;
            self.party_space_target = 0.0;
            self.party_repeat_fx = 0.0;
            self.party_glitch_fx = 0.0;
            telemetry.party_palette.clear();
            telemetry.party_voice_presets = p.voice_presets;
            telemetry.party_vibe_name.clear();
            telemetry.party_bpm = 0.0;
            telemetry.active_song_section = None;

            let total_steps: usize = p
                .song_sections
                .iter()
                .map(|s| s.bars.max(1) as usize * 16)
                .sum();
            let mut rem = if total_steps == 0 {
                0
            } else {
                global % total_steps
            };
            for (sec_i, sec) in p.song_sections.iter().enumerate() {
                let len = sec.bars.max(1) as usize * 16;
                if rem < len {
                    pat_idx = sec.pattern.min(p.patterns.len().saturating_sub(1));
                    section_mask = sec.track_mask;
                    extra_section_mask = sec.extra_mask;
                    if sec.bass_drop && rem == 0 {
                        self.trigger_drop_impact(
                            p.root_note,
                            p.drop_macro.impact_intensity.max(0.8),
                            &p.drums,
                            true,
                        );
                    }
                    section_fill = sec.fill && rem + 8 >= len;
                    telemetry.party_section = sec.name.clone();
                    telemetry.party_bar = rem / 16 + 1;
                    telemetry.party_section_bars = sec.bars.max(1) as usize;
                    telemetry.active_song_section = Some(sec_i);
                    break;
                }
                rem -= len;
            }
        } else {
            self.party_filter_target_hz = 18_000.0;
            self.party_filter_mode = GlobalFilterMode::LowPass;
            self.party_filter_res_boost = 0.0;
            self.party_space_target = 0.0;
            self.party_repeat_fx = 0.0;
            self.party_glitch_fx = 0.0;
            telemetry.party_palette.clear();
            telemetry.party_voice_presets = p.voice_presets;
            telemetry.party_vibe_name.clear();
            telemetry.party_bpm = 0.0;
            telemetry.party_section.clear();
            telemetry.party_bar = (global / 16) % 4 + 1;
            telemetry.party_section_bars = 4;
            telemetry.active_song_section = None;
        }

        let use_rig = p.party_mode && (p.party_sound_morph || self.party_rig_custom);
        let base_step_samples = (60.0 / active_bpm.clamp(40.0, 240.0) / 4.0) * self.sr;

        // Active chord slot & Party DJ Song root/progression (harmonic key shifts between DJ songs!)
        let dj_root = self.party_dj_root(p);
        let prog_offset = if p.party_mode && p.party_auto_evolve {
            let v_progs = vibe_progressions(song_vibe);
            let base_pos = v_progs
                .iter()
                .position(|&pi| pi == p.progression_idx)
                .unwrap_or(0);
            let step_advance = (self.party_cycle as usize) + (self.party_evolution as usize);
            v_progs[(base_pos + step_advance) % v_progs.len().max(1)]
        } else {
            p.progression_idx
        };
        if p.party_mode {
            telemetry.party_song_title = party_song_title(song_vibe, self.party_cycle, dj_root);
            telemetry.party_dj_root = dj_root;
            telemetry.party_prog_idx = prog_offset;
        } else {
            telemetry.party_song_title.clear();
        }
        let prog = progression_at(prog_offset);
        let bpc = p.bars_per_chord.clamp(1, 4) as usize;
        let chord_idx = (global / (16 * bpc)) % 4;
        let active_slot = if p.auto_chord_follow && self.live_chord_held {
            self.live_slot.unwrap_or(prog.slots[chord_idx])
        } else {
            prog.slots[chord_idx]
        };
        telemetry.active_chord = chord_symbol(dj_root, active_slot);

        let pat = &p.patterns[pat_idx];
        let party_phrase = (global / 64) as u32;
        let party_evolve_active = p.party_mode && (self.party_cycle > 0 || (p.party_auto_evolve && self.party_evolution > 0));
        let song_seed = p
            .party_seed
            .wrapping_add(self.party_cycle.wrapping_mul(7919))
            .wrapping_add(self.party_evolution.wrapping_mul(2053));
        let seed = if p.party_mode {
            song_seed
                .wrapping_add((self.party_section_idx as u32).wrapping_mul(1031))
                .wrapping_add(party_phrase.wrapping_mul(3571))
        } else {
            song_seed
        };

        // Refresh Party DJ Melodic Song Cache whenever a new song / progression / vibe starts!
        if p.party_mode {
            let msig = (song_vibe as u8, dj_root, prog_offset, song_seed);
            if self.party_melodic_sig != msig {
                self.party_melodic = generate_melodic_tracks(song_vibe, dj_root, prog_offset, song_seed);
                self.party_melodic_sig = msig;
            }
        }

        // --- 0. MOTION AUTOMATION PLAYBACK & GLOBAL FILTER ENVELOPE TRIGGERS ---
        let m_step = global % 64;
        self.auto_filter = pat.motion.filter.get(m_step).copied().flatten();
        self.auto_space = pat.motion.space.get(m_step).copied().flatten();
        self.auto_sidechain = pat.motion.sidechain.get(m_step).copied().flatten();
        self.auto_glitch = pat.motion.glitch.get(m_step).copied().flatten();
        self.auto_iso_bass = pat.motion.iso_bass.get(m_step).copied().flatten();
        self.auto_iso_treble = pat.motion.iso_treble.get(m_step).copied().flatten();

        match p.fx.global_filter_env_trigger {
            FilterEnvTrigger::Beat if global % 4 == 0 => self.trigger_global_filter_env(),
            FilterEnvTrigger::HalfBar if global % 8 == 0 => self.trigger_global_filter_env(),
            FilterEnvTrigger::Bar if global % 16 == 0 => self.trigger_global_filter_env(),
            _ => {}
        }

        // --- 0b. MULTI-STAGE CONFIGURABLE BASS DROP MACRO (AUTO + MANUAL) ---
        if matches!(self.drop_macro_phase, DropMacroPhase::Idle) && p.drop_macro.auto_drop_bars > 0 {
            let period = (p.drop_macro.auto_drop_bars as usize) * 16;
            let raw_build = (match p.drop_macro.build_bars {
                1 => 1usize,
                4 => 4usize,
                _ => 2usize,
            }) * 16;
            let eff_build = raw_build.min(period.saturating_sub(16).max(16));
            let trigger_mod = period.saturating_sub(eff_build);
            if global >= trigger_mod && global % period == trigger_mod {
                self.start_drop_macro(p, Some(eff_build));
            }
        }

        let mut drop_cut_drums_groove = false;
        let mut drop_in_gap = false;
        let drop_drums_base = if use_rig {
            self.party_rig
                .as_ref()
                .map(|r| r.drums.clone())
                .unwrap_or_else(|| p.drums.clone())
        } else {
            p.drums.clone()
        };

        match self.drop_macro_phase {
            DropMacroPhase::Building {
                step_idx,
                total_steps,
                gap_steps,
            } => {
                let active_build = total_steps.saturating_sub(gap_steps).max(1);
                if step_idx >= total_steps {
                    self.drop_macro_phase = DropMacroPhase::Impact { steps_left: 16 };
                    self.drop_riser_target_env = 0.0;
                    self.drop_macro_target_prog = 0.0;
                    self.trigger_drop_impact(
                        dj_root,
                        p.drop_macro.impact_intensity,
                        &drop_drums_base,
                        !drum_cut,
                    );
                } else if step_idx >= active_build {
                    drop_in_gap = true;
                    drop_cut_drums_groove = true;
                    section_mask = 0;
                    self.drop_riser_target_env = 0.0;
                    self.drop_macro_target_prog = 1.0;
                    if step_idx == active_build {
                        self.all_notes_off();
                    }
                    self.drop_macro_phase = DropMacroPhase::Building {
                        step_idx: step_idx + 1,
                        total_steps,
                        gap_steps,
                    };
                } else {
                    let prog = (step_idx as f32 / active_build as f32).clamp(0.0, 1.0);
                    self.drop_macro_target_prog = prog;
                    self.drop_riser_target_env = if p.drop_macro.riser_sweep {
                        (0.18 + prog * 0.82) * p.drop_macro.impact_intensity.clamp(0.3, 1.0)
                    } else {
                        0.0
                    };
                    if p.drop_macro.cut_drums {
                        drop_cut_drums_groove = true;
                    }
                    if p.drop_macro.cut_bass {
                        section_mask &= !0b000010;
                    }
                    if p.drop_macro.cut_chords {
                        section_mask &= !0b000100;
                    }
                    if p.drop_macro.cut_lead {
                        section_mask &= !(0b001000 | 0b010000);
                    }
                    if p.drop_macro.drum_accel && !drum_cut {
                        let tune_rise = prog * 10.5;
                        let mut dk = drop_drums_base.clone();
                        dk.voices[1].tune = (dk.voices[1].tune + tune_rise).clamp(-12.0, 12.0);
                        dk.voices[1].decay = (dk.voices[1].decay * (1.0 - prog * 0.42)).clamp(0.08, 0.8);
                        if prog < 0.35 {
                            if step_idx % 4 == 0 {
                                let vel = (0.58 + prog * 0.4).min(1.0);
                                self.drums.trigger(DrumVoiceId::Kick, vel * 0.85, false, false, &dk);
                                self.drums.trigger(DrumVoiceId::Snare, vel, false, false, &dk);
                            }
                        } else if prog < 0.62 {
                            if step_idx % 2 == 0 {
                                let vel = (0.68 + prog * 0.3).min(1.0);
                                if step_idx % 4 == 0 {
                                    self.drums.trigger(DrumVoiceId::Kick, vel * 0.80, false, false, &dk);
                                }
                                self.drums.trigger(DrumVoiceId::Snare, vel, false, false, &dk);
                            }
                        } else if prog < 0.84 {
                            let vel = (0.78 + prog * 0.2).min(1.0);
                            self.drums.trigger(DrumVoiceId::Snare, vel, prog > 0.75, false, &dk);
                        } else {
                            let vel = (0.88 + prog * 0.12).min(1.0);
                            self.drums.trigger(DrumVoiceId::Snare, vel, true, false, &dk);
                            let final_steps = active_build.saturating_sub(step_idx);
                            if final_steps <= 4 {
                                for r in [0.33_f32, 0.66_f32] {
                                    self.ratchets.push(RatchetEvent {
                                        lane: DrumVoiceId::Snare.index(),
                                        samples_left: base_step_samples * r,
                                        vel: 1.0,
                                        accent: true,
                                        tune_offset: tune_rise,
                                    });
                                }
                            } else {
                                self.ratchets.push(RatchetEvent {
                                    lane: DrumVoiceId::Snare.index(),
                                    samples_left: base_step_samples * 0.5,
                                    vel: 1.0,
                                    accent: true,
                                    tune_offset: tune_rise,
                                });
                            }
                        }
                    }
                    self.drop_macro_phase = DropMacroPhase::Building {
                        step_idx: step_idx + 1,
                        total_steps,
                        gap_steps,
                    };
                }
            }
            DropMacroPhase::Impact { steps_left } => {
                if steps_left <= 1 {
                    self.drop_macro_phase = DropMacroPhase::Idle;
                } else {
                    self.drop_macro_phase = DropMacroPhase::Impact {
                        steps_left: steps_left - 1,
                    };
                }
                self.drop_riser_target_env = 0.0;
                self.drop_macro_target_prog = 0.0;
            }
            DropMacroPhase::Idle => {
                self.drop_riser_target_env = 0.0;
                self.drop_macro_target_prog = 0.0;
            }
        }

        // --- 1. DRUMS (bit 0) ---
        if !drum_cut && !drop_cut_drums_groove && !drop_in_gap && (section_mask & 0b000001) != 0 {
            // Refresh Auto-Beat cache if needed
            let use_auto_beat = p.auto_beat_mode || p.party_mode;
            let beat_vibe = if p.party_mode { song_vibe } else { p.vibe };
            if use_auto_beat {
                let energy = if p.party_mode {
                    (party_section_energy * 0.85 + 0.15).clamp(0.15, 1.0)
                } else {
                    p.beat_energy
                };
                let sig = (beat_vibe as u8, (energy * 100.0) as u32, seed);
                if self.auto_beat_sig != sig {
                    self.auto_beat_drums = generate_auto_beat(beat_vibe, energy, seed);
                    self.auto_beat_sig = sig;
                }
            }
            let drum_seq = if use_auto_beat {
                &self.auto_beat_drums
            } else {
                &pat.drums
            };

            let pocket_scale = groove_pocket_scale(song_vibe);
            let phase = global % 16;
            for lane in 0..8 {
                let d_lane = &drum_seq.lanes[lane];
                let d_len = d_lane.step_count.as_usize();
                let d_div = if use_auto_beat || p.party_mode {
                    ClockDiv::Sixteenth
                } else {
                    d_lane.clock_div
                };
                let (prim_idx, sub_idx, _) = clock_div_indices(d_div, global, d_len);
                if let Some(p_idx) = prim_idx {
                    let st = d_lane.steps[p_idx];
                    if st.active
                        && !self.is_step_suppressed(Some(TrackId::Drums), None, Some(lane), global)
                        && (st.probability >= 0.999 || hash_f32(seed, global, lane as u32) <= st.probability)
                    {
                        if let Some(voice) = DrumVoiceId::from_index(lane) {
                            // Subtle velocity humanization + per-step Pitch/Pan P-Locks + studio groove pocket
                            let human = 0.94 + hash_f32(seed, global, lane as u32 + 99) * 0.12;
                            let vel = (st.velocity * human).clamp(0.1, 1.0);
                            let drums_p = if use_rig {
                                self.party_rig.as_ref().map(|r| &r.drums).unwrap_or(&p.drums)
                            } else {
                                &p.drums
                            };
                            let delay_ms = match voice {
                                DrumVoiceId::Kick => 0.0,
                                DrumVoiceId::Snare | DrumVoiceId::Clap => {
                                    (3.5 + hash_f32(seed, global, lane as u32 + 301) * 2.5) * pocket_scale
                                }
                                DrumVoiceId::ClosedHat | DrumVoiceId::OpenHat => {
                                    (0.6 + hash_f32(seed, global, lane as u32 + 301) * 3.0) * pocket_scale
                                }
                                DrumVoiceId::CowbellRim | DrumVoiceId::Tom | DrumVoiceId::Cymbal => {
                                    (1.2 + hash_f32(seed, global, lane as u32 + 301) * 2.5) * pocket_scale
                                }
                            };
                            let delay_samples = (delay_ms * 0.001 * self.sr).min(base_step_samples * 0.22);
                            if delay_samples > 1.0 {
                                self.pending_triggers.push((
                                    delay_samples,
                                    PendingTrigger::Drum {
                                        lane,
                                        vel,
                                        accent: st.accent,
                                        flam: st.flam,
                                        pitch_offset: st.pitch_offset,
                                        pan_lock: st.pan_lock,
                                    },
                                ));
                            } else {
                                trigger_drum_with_lock(
                                    &mut self.drums,
                                    drums_p,
                                    lane,
                                    voice,
                                    vel,
                                    st.accent,
                                    st.flam,
                                    st.pitch_offset,
                                    st.pan_lock,
                                );
                            }
                            if voice == DrumVoiceId::Kick {
                                self.sidechain_peak = self.sidechain_peak.max(vel);
                                if p.fx.global_filter_env_trigger == FilterEnvTrigger::Kick {
                                    self.gfilt_env_attacking = true;
                                }
                            }
                            if st.ratchet > 1 {
                                let n = st.ratchet.min(4) as usize;
                                let sub_len = base_step_samples / n as f32;
                                for r in 1..n {
                                    self.ratchets.push(RatchetEvent {
                                        lane,
                                        samples_left: delay_samples + sub_len * r as f32,
                                        vel: vel * (0.85 - r as f32 * 0.12).max(0.35),
                                        accent: false,
                                        tune_offset: 0.0,
                                    });
                                }
                            }
                        }
                    }
                }
                if let Some(s_idx) = sub_idx {
                    let st = d_lane.steps[s_idx];
                    if st.active
                        && !self.is_step_suppressed(Some(TrackId::Drums), None, Some(lane), global)
                        && (st.probability >= 0.999
                            || hash_f32(seed, global * 2 + 1, lane as u32) <= st.probability)
                    {
                        self.pending_triggers.push((
                            base_step_samples * 0.5,
                            PendingTrigger::Drum {
                                lane,
                                vel: st.velocity.clamp(0.1, 1.0),
                                accent: st.accent,
                                flam: st.flam,
                                pitch_offset: st.pitch_offset,
                                pan_lock: st.pan_lock,
                            },
                        ));
                    }
                }
            }

            // Live Snare Build or Party Build snare roll!
            if snare_build || party_snare_roll {
                let roll_rate = if phase >= 12 {
                    2 // 32nd-note rush!
                } else if phase >= 8 {
                    1 // 16ths
                } else if phase % 2 == 0 {
                    1 // 8ths
                } else {
                    0
                };
                if roll_rate > 0 {
                    let vel = 0.45 + (phase as f32 / 15.0) * 0.50;
                    let drums_p = if use_rig {
                        self.party_rig.as_ref().map(|r| &r.drums).unwrap_or(&p.drums)
                    } else {
                        &p.drums
                    };
                    self.drums
                        .trigger(DrumVoiceId::Snare, vel, phase >= 12, false, drums_p);
                    if roll_rate == 2 {
                        self.ratchets.push(RatchetEvent {
                            lane: DrumVoiceId::Snare.index(),
                            samples_left: base_step_samples * 0.5,
                            vel: (vel + 0.08).min(1.0),
                            accent: true,
                            tune_offset: 0.0,
                        });
                    }
                }
            }

            // Automatic or manual turnaround Fill!
            if section_fill || self.force_fill_steps > 0 {
                if self.force_fill_steps > 0 {
                    self.force_fill_steps -= 1;
                }
                if matches!(phase, 12 | 13 | 14 | 15) {
                    let voice = if phase >= 14 {
                        DrumVoiceId::Snare
                    } else {
                        DrumVoiceId::Tom
                    };
                    let vel = 0.58 + (phase - 12) as f32 * 0.11;
                    let drums_p = if use_rig {
                        self.party_rig.as_ref().map(|r| &r.drums).unwrap_or(&p.drums)
                    } else {
                        &p.drums
                    };
                    self.drums
                        .trigger(voice, vel, phase == 15, false, drums_p);
                }
            }
        }

        // Helper: trigger a melodic step with proper gate/slide/motion
        let bass_root = bass_midi_for(dj_root, active_slot.offset);
        let pad_notes = voice_led_chord(dj_root, active_slot, 60);
        let arp_tones = arp_tones_for(dj_root, active_slot);
        let follow_override = p.auto_chord_follow && self.live_chord_held;

        // --- 2. BASS / MOOG (bit 1) ---
        if (section_mask & 0b000010) != 0 && !follow_override {
            let seq = if p.party_mode { &self.party_melodic.0 } else { &pat.moog };
            let m_div = if p.party_mode { ClockDiv::Sixteenth } else { seq.clock_div };
            let (prim_idx, sub_idx, gate_scale) = clock_div_indices(m_div, global, seq.step_count.as_usize());
            let mut st = prim_idx.map(|i| seq.steps[i]).unwrap_or_default();
            if p.party_mode {
                let step16 = global % 16;
                let bass_style = (party_phrase + self.party_section_idx as u32) % 4;
                let straight_drive_bass = matches!(
                    song_vibe,
                    Vibe::Eurobeat
                        | Vibe::EuphoricTrance
                        | Vibe::BigRoomEdm
                        | Vibe::ImperialBass
                        | Vibe::Synthwave
                        | Vibe::DetroitTechno
                        | Vibe::ItaloCosmic
                        | Vibe::Arena80s
                        | Vibe::Disco70s
                );
                if straight_drive_bass {
                    if matches!(song_vibe, Vibe::Eurobeat)
                        && !st.active
                        && step16 % 2 == 0
                        && party_section_energy >= 0.75
                        && bass_style == 2
                    {
                        let oct = if step16 % 4 == 2 { 12 } else { 0 };
                        st = MelodicStep {
                            active: true,
                            note: (bass_root + oct).min(60),
                            velocity: if step16 % 4 == 0 { 0.88 } else { 0.76 },
                            gate: 0.90,
                            ..Default::default()
                        };
                    }
                } else {
                    // Clean, grounded musical groove support (no chromatic fart slides or resonant auto-wah locks)
                    if bass_style == 2 && party_section_energy >= 0.78 && step16 % 4 == 2 && !st.active {
                        st = MelodicStep {
                            active: true,
                            note: bass_root.clamp(33, 56),
                            velocity: 0.72,
                            gate: 0.75,
                            ..Default::default()
                        };
                    } else if (bass_style == 1 || bass_style == 3)
                        && !st.active
                        && step16 == 14
                        && chord_idx % 2 == 1
                        && hash_f32(seed, global, 11) < 0.40
                    {
                        let jump = if hash_f32(seed, global, 14) < 0.5 { 7 } else { 12 };
                        st = MelodicStep {
                            active: true,
                            note: (bass_root + jump).min(56),
                            velocity: 0.72,
                            gate: 1.0,
                            ..Default::default()
                        };
                    }
                }
            }
            let moog_prob = p.party_mode || st.probability >= 0.999 || hash_f32(seed, global, 412) <= st.probability;
            if st.active && moog_prob && !self.is_step_suppressed(Some(TrackId::Moog), None, None, global) {
                let note = st.note;
                if st.motion.any_locked() {
                    let mut m = st.motion;
                    if use_rig {
                        if let Some(c) = m.cutoff {
                            m.cutoff = Some((c + (self.party_bass_cut_mul - 1.0) * 0.16).clamp(0.28, 0.94));
                        }
                    }
                    self.moog.set_motion_locks(m);
                } else {
                    self.moog.clear_motion_locks();
                }
                if !st.slide {
                    if let Some(prev) = self.last_moog_note.take() {
                        self.moog.note_off(prev);
                    }
                }
                let moog_p = if use_rig {
                    self.party_rig.as_ref().map(|r| &r.moog).unwrap_or(&p.moog)
                } else {
                    &p.moog
                };
                let vel = (st.velocity * phrase_vel_mul(global, seed, 411)).clamp(0.15, 1.0);
                self.moog
                    .note_on(note, vel, st.accent, st.slide, moog_p);
                self.last_moog_note = Some(note);
                self.gate_moog = base_step_samples * gate_scale * st.gate.clamp(0.15, 16.0);
            }
            if let Some(s_idx) = sub_idx {
                let st2 = seq.steps[s_idx];
                if st2.active
                    && !self.is_step_suppressed(Some(TrackId::Moog), None, None, global)
                    && (st2.probability >= 0.999 || hash_f32(seed, global * 2 + 1, 412) <= st2.probability)
                {
                    self.pending_triggers.push((
                        base_step_samples * 0.5,
                        PendingTrigger::Melodic32 {
                            target: ArpTarget::Core(TrackId::Moog),
                            note: st2.note,
                            vel: st2.velocity.clamp(0.15, 1.0),
                            accent: st2.accent,
                            slide: st2.slide,
                            chord: st2.chord,
                            motion: st2.motion,
                            gate_samples: base_step_samples * 0.5 * st2.gate.clamp(0.15, 16.0),
                            dj_root,
                        },
                    ));
                }
            }
        }

        // --- 3. CHORDS / MOOG PAD (bit 2) ---
        if (section_mask & 0b000100) != 0 && !follow_override {
            let seq = if p.party_mode { &self.party_melodic.1 } else { &pat.moog_pad };
            let p_div = if p.party_mode { ClockDiv::Sixteenth } else { seq.clock_div };
            let (pad_prim, pad_sub, pad_gscale) = clock_div_indices(p_div, global, seq.step_count.as_usize());
            let mut st = pad_prim.map(|i| seq.steps[i]).unwrap_or_default();
            if p.party_mode {
                let step16 = global % 16;
                let chord_rhythm = (party_phrase + self.party_section_idx as u32) % 3;
                if !st.active && (step16 == 3 || step16 == 7 || step16 == 15) && party_section_energy >= 0.55 && hash_f32(seed, global, 21) < 0.48 {
                    // Tight 16th-note clav / brass funk chuck
                    st = MelodicStep {
                        active: true,
                        note: dj_root,
                        velocity: 0.84,
                        gate: 0.65,
                        accent: step16 == 15,
                        chord: active_slot.chord,
                        motion: MotionLocks {
                            cutoff: Some(0.84),
                            resonance: Some(0.46),
                            drive: Some(0.38),
                            shape: None,
                        },
                        ..Default::default()
                    };
                } else if chord_rhythm == 1 && !st.active && (step16 == 6 || step16 == 12) && party_section_energy >= 0.65 {
                    // Rhythmic stab hits on Groove/Drops
                    st = MelodicStep {
                        active: true,
                        note: dj_root,
                        velocity: 0.80,
                        gate: 1.5,
                        chord: active_slot.chord,
                        motion: MotionLocks {
                            cutoff: Some(0.80),
                            resonance: Some(0.44),
                            drive: Some(0.32),
                            shape: None,
                        },
                        ..Default::default()
                    };
                } else if chord_rhythm == 2 && !st.active && step16 == 10 && hash_f32(seed, global, 19) < 0.55 {
                    // Syncopated chord swell
                    st = MelodicStep {
                        active: true,
                        note: dj_root,
                        velocity: 0.78,
                        gate: 3.0,
                        chord: active_slot.chord,
                        ..Default::default()
                    };
                }
            }
            let pad_prob = p.party_mode || st.probability >= 0.999 || hash_f32(seed, global, 452) <= st.probability;
            if st.active && pad_prob && !self.is_step_suppressed(Some(TrackId::MoogPad), None, None, global) {
                if st.motion.any_locked() {
                    let mut m = st.motion;
                    if p.party_mode && p.party_sound_morph {
                        if let Some(c) = m.cutoff {
                            m.cutoff = Some((c + (self.party_pad_cut_mul - 1.0) * 0.14).clamp(0.72, 0.96));
                        }
                    }
                    self.pad_moog.set_motion_locks(m);
                } else {
                    self.pad_moog.clear_motion_locks();
                }
                self.pending_triggers
                    .retain(|(_, ev)| !matches!(ev, PendingTrigger::PadNote { .. }));
                self.pad_moog.all_notes_off();
                let pad_p = if use_rig {
                    self.party_rig.as_ref().map(|r| &r.moog_pad).unwrap_or(&p.moog_pad)
                } else {
                    &p.moog_pad
                };
                let pocket_scale = groove_pocket_scale(song_vibe);
                if st.chord != crate::model::ChordType::Single {
                    // Voice-led lush chord gliding smoothly from previous bar's voicing!
                    let slot = if party_evolve_active || p.party_mode {
                        active_slot
                    } else {
                        ChordSlot {
                            offset: (st.note + 12 - (dj_root % 12)) % 12,
                            chord: st.chord,
                        }
                    };
                    let mut notes_to_play =
                        voice_led_chord_near(dj_root, slot, &self.last_pad_notes, 60);
                    self.last_pad_notes = notes_to_play.clone();
                    // Subtle octave lift on Drop 2 / Climax / odd phrases for harmonic bloom (upper ceiling capped)
                    if p.party_mode && sec_lead_bright >= 0.96 && (party_phrase % 2 == 1) {
                        for n in &mut notes_to_play {
                            if *n <= 64 {
                                *n += 12;
                            }
                        }
                    }
                    for (k, n) in notes_to_play.into_iter().enumerate() {
                        let v_taper = (1.0 - k as f32 * 0.035).max(0.84);
                        let vel = (st.velocity * v_taper).clamp(0.15, 1.0);
                        let strum_samples = if k == 0 {
                            0.0
                        } else {
                            (k as f32 * 3.2 * pocket_scale * 0.001 * self.sr).min(0.014 * self.sr)
                        };
                        if strum_samples <= 1.0 {
                            self.pad_moog.note_on(n, vel, st.accent, false, pad_p);
                        } else {
                            self.pending_triggers.push((
                                strum_samples,
                                PendingTrigger::PadNote {
                                    note: n,
                                    vel,
                                    accent: st.accent,
                                },
                            ));
                        }
                    }
                } else {
                    self.pad_moog
                        .note_on(st.note, st.velocity, st.accent, false, pad_p);
                }
                self.gate_pad = base_step_samples * pad_gscale * st.gate.clamp(0.25, 16.0);
            }
            if let Some(s_idx) = pad_sub {
                let st2 = seq.steps[s_idx];
                if st2.active
                    && !self.is_step_suppressed(Some(TrackId::MoogPad), None, None, global)
                    && (st2.probability >= 0.999 || hash_f32(seed, global * 2 + 1, 452) <= st2.probability)
                {
                    self.pending_triggers.push((
                        base_step_samples * 0.5,
                        PendingTrigger::Melodic32 {
                            target: ArpTarget::ChordPad,
                            note: st2.note,
                            vel: st2.velocity.clamp(0.15, 1.0),
                            accent: st2.accent,
                            slide: st2.slide,
                            chord: st2.chord,
                            motion: st2.motion,
                            gate_samples: base_step_samples * 0.5 * st2.gate.clamp(0.25, 16.0),
                            dj_root,
                        },
                    ));
                }
            }
        }

        // --- 3b. EXTRA DYNAMIC SYNTH TRACKS (tracks 7..=16) ---
        let extra_build_cut = matches!(self.drop_macro_phase, DropMacroPhase::Building { .. })
            && p.drop_macro.cut_chords;
        if !drop_in_gap && !extra_build_cut && !follow_override {
            let extra_n = p.extra_tracks.len().min(MAX_EXTRA_TRACKS).min(pat.extra_seqs.len());
            for ei in 0..extra_n {
                let extra_on = if p.party_mode {
                    (section_mask & 0b000100) != 0
                } else {
                    (extra_section_mask & (1u16 << ei)) != 0
                };
                if !extra_on {
                    continue;
                }
                let seq = &pat.extra_seqs[ei];
                let e_div = if p.party_mode { ClockDiv::Sixteenth } else { seq.clock_div };
                let (e_prim, e_sub, e_gscale) = clock_div_indices(e_div, global, seq.step_count.as_usize());
                if let Some(e_idx) = e_prim {
                    let st = seq.steps[e_idx];
                    let e_prob = p.party_mode
                        || st.probability >= 0.999
                        || hash_f32(seed, global, 800 + ei as u32) <= st.probability;
                    if st.active && e_prob && !self.is_step_suppressed(None, Some(ei), None, global) {
                        let synth = &mut self.extra_synths[ei];
                        let tr = &p.extra_tracks[ei];
                        if st.motion.any_locked() {
                            synth.set_motion_locks(st.motion);
                        } else {
                            synth.clear_motion_locks();
                        }
                        synth.all_notes_off();
                        if st.chord != crate::model::ChordType::Single {
                            let notes_to_play = if party_evolve_active || p.party_mode {
                                pad_notes.clone()
                            } else {
                                let slot = ChordSlot {
                                    offset: (st.note + 12 - (dj_root % 12)) % 12,
                                    chord: st.chord,
                                };
                                voice_led_chord(dj_root, slot, 60)
                            };
                            for n in notes_to_play {
                                synth.note_on(n, st.velocity, st.accent, false, &tr.synth);
                            }
                        } else {
                            synth.note_on(st.note, st.velocity, st.accent, false, &tr.synth);
                        }
                        self.extra_gates[ei] = base_step_samples * e_gscale * st.gate.clamp(0.25, 16.0);
                    }
                }
                if let Some(s_idx) = e_sub {
                    let st2 = seq.steps[s_idx];
                    if st2.active
                        && !self.is_step_suppressed(None, Some(ei), None, global)
                        && (st2.probability >= 0.999
                            || hash_f32(seed, global * 2 + 1, 800 + ei as u32) <= st2.probability)
                    {
                        self.pending_triggers.push((
                            base_step_samples * 0.5,
                            PendingTrigger::Melodic32 {
                                target: ArpTarget::Extra(ei),
                                note: st2.note,
                                vel: st2.velocity.clamp(0.15, 1.0),
                                accent: st2.accent,
                                slide: st2.slide,
                                chord: st2.chord,
                                motion: st2.motion,
                                gate_samples: base_step_samples * 0.5 * st2.gate.clamp(0.25, 16.0),
                                dj_root,
                            },
                        ));
                    }
                }
            }
        }

        // --- 4. ARP / MONOLOGUE (bit 3) ---
        if (section_mask & 0b001000) != 0 {
            let seq = if p.party_mode { &self.party_melodic.2 } else { &pat.monologue };
            let m_div = if p.party_mode { ClockDiv::Sixteenth } else { seq.clock_div };
            let (mono_prim, mono_sub, mono_gscale) =
                clock_div_indices(m_div, global, seq.step_count.as_usize());
            let st = mono_prim.map(|i| seq.steps[i]).unwrap_or_default();
            let st2_opt = mono_sub.map(|i| seq.steps[i]);
            let arp_len = arp_tones.len().max(1);
            let arp_mode = (party_phrase + self.party_section_idx as u32) % 5;
            let pick_arp_note = |step_i: usize| -> u8 {
                let idx = match arp_mode {
                    0 => (step_i / 2) % arp_len,
                    1 => arp_len - 1 - ((step_i / 2) % arp_len),
                    2 => {
                        let span = (arp_len * 2).saturating_sub(2).max(1);
                        let pos = (step_i / 2) % span;
                        if pos < arp_len { pos } else { span - pos }
                    }
                    3 => ((step_i / 2) * 3) % arp_len,
                    _ => ((step_i / 2) + (step_i / 16)) % arp_len,
                };
                let mut n = arp_tones[idx];
                if p.party_mode && sec_lead_bright >= 0.90 && step_i % 8 == 6 && n <= 76 {
                    n += 12; // drop octave sparkle
                }
                n
            };
            let mono_prob = p.party_mode || st.probability >= 0.999 || hash_f32(seed, global, 512) <= st.probability;
            if st.active && mono_prob && !self.is_step_suppressed(Some(TrackId::Monologue), None, None, global) {
                let note = if follow_override {
                    pick_arp_note(global)
                } else {
                    st.note
                };
                let vel = (st.velocity * phrase_vel_mul(global, seed, 511)).clamp(0.15, 1.0);
                let gate_samples = base_step_samples * mono_gscale * st.gate.clamp(0.15, 8.0);
                let apply_party_cut = p.party_mode && p.party_sound_morph;
                let pocket_scale = groove_pocket_scale(song_vibe);
                let mono_delay = if global % 4 != 0 && !st.slide {
                    let delay_ms = (0.8 + hash_f32(seed, global, 521) * 2.4) * pocket_scale;
                    (delay_ms * 0.001 * self.sr).min(base_step_samples * 0.20)
                } else {
                    0.0
                };
                self.pending_triggers
                    .retain(|(_, ev)| !matches!(ev, PendingTrigger::MonoNote { .. }));
                let prev_note = self.last_mono_note.take();
                self.last_mono_note = Some(note);
                if mono_delay > 1.0 {
                    self.gate_mono = gate_samples + mono_delay;
                    self.pending_triggers.push((
                        mono_delay,
                        PendingTrigger::MonoNote {
                            note,
                            prev_note,
                            vel,
                            accent: st.accent,
                            slide: st.slide,
                            motion: st.motion,
                            party_rig_cut: apply_party_cut,
                            gate_samples,
                        },
                    ));
                } else {
                    self.dispatch_mono_note(
                        p,
                        use_rig,
                        note,
                        prev_note,
                        vel,
                        st.accent,
                        st.slide,
                        st.motion,
                        apply_party_cut,
                        gate_samples,
                    );
                }
            }
            if let Some(st2) = st2_opt {
                if st2.active
                    && !self.is_step_suppressed(Some(TrackId::Monologue), None, None, global)
                    && (st2.probability >= 0.999 || hash_f32(seed, global * 2 + 1, 512) <= st2.probability)
                {
                    let note = if follow_override {
                        pick_arp_note(global * 2 + 1)
                    } else {
                        st2.note
                    };
                    self.pending_triggers.push((
                        base_step_samples * 0.5,
                        PendingTrigger::Melodic32 {
                            target: ArpTarget::Core(TrackId::Monologue),
                            note,
                            vel: st2.velocity.clamp(0.15, 1.0),
                            accent: st2.accent,
                            slide: st2.slide,
                            chord: st2.chord,
                            motion: st2.motion,
                            gate_samples: base_step_samples * 0.5 * st2.gate.clamp(0.15, 8.0),
                            dj_root,
                        },
                    ));
                }
            }
        }

        // --- 5. LEAD / TB-303 (bit 4) ---
        if (section_mask & 0b010000) != 0 {
            let seq = if p.party_mode { &self.party_melodic.3 } else { &pat.tb303 };
            let a_div = if p.party_mode { ClockDiv::Sixteenth } else { seq.clock_div };
            let (a_prim, a_sub, a_gscale) = clock_div_indices(a_div, global, seq.step_count.as_usize());
            if let Some(a_idx) = a_prim {
                let st = seq.steps[a_idx];
                let a_prob = p.party_mode || st.probability >= 0.999 || hash_f32(seed, global, 612) <= st.probability;
                if st.active && a_prob && !self.is_step_suppressed(Some(TrackId::Tb303), None, None, global) {
                    let note = if follow_override {
                        let tone = arp_tones[global % arp_tones.len().max(1)];
                        (bass_root + 12 + (tone % 12)).clamp(36, 76)
                    } else {
                        st.note
                    };
                    if st.motion.any_locked() {
                        self.acid.set_motion_locks(st.motion);
                    } else {
                        self.acid.clear_motion_locks();
                    }
                    if !st.slide {
                        if let Some(prev) = self.last_acid_note.take() {
                            self.acid.note_off(prev);
                        }
                    }
                    let vel = (st.velocity * phrase_vel_mul(global, seed, 611)).clamp(0.15, 1.0);
                    self.acid.note_on(note, vel, st.accent, st.slide);
                    self.last_acid_note = Some(note);
                    self.gate_acid = base_step_samples * a_gscale * st.gate.clamp(0.15, 4.0);
                }
            }
            if let Some(s_idx) = a_sub {
                let st2 = seq.steps[s_idx];
                if st2.active
                    && !self.is_step_suppressed(Some(TrackId::Tb303), None, None, global)
                    && (st2.probability >= 0.999 || hash_f32(seed, global * 2 + 1, 612) <= st2.probability)
                {
                    let note = if follow_override {
                        let tone = arp_tones[(global * 2 + 1) % arp_tones.len().max(1)];
                        (bass_root + 12 + (tone % 12)).clamp(36, 76)
                    } else {
                        st2.note
                    };
                    self.pending_triggers.push((
                        base_step_samples * 0.5,
                        PendingTrigger::Melodic32 {
                            target: ArpTarget::Core(TrackId::Tb303),
                            note,
                            vel: st2.velocity.clamp(0.15, 1.0),
                            accent: st2.accent,
                            slide: st2.slide,
                            chord: st2.chord,
                            motion: st2.motion,
                            gate_samples: base_step_samples * 0.5 * st2.gate.clamp(0.15, 4.0),
                            dj_root,
                        },
                    ));
                }
            }
        }

        // --- 6. SAMPLER / 8-PART VOLCA BANK (bit 5) ---
        if (section_mask & 0b100000) != 0 {
            let seq = if p.party_mode { &self.party_melodic.4 } else { &pat.sampler };
            let s_div = if p.party_mode { ClockDiv::Sixteenth } else { seq.clock_div };
            let (s_prim, s_sub, s_gscale) = clock_div_indices(s_div, global, seq.step_count.as_usize());
            if let Some(s_idx) = s_prim {
                let st = seq.steps[s_idx];
                let s_prob = p.party_mode || st.probability >= 0.999 || hash_f32(seed, global, 712) <= st.probability;
                if st.active && s_prob && !self.is_step_suppressed(Some(TrackId::Sampler), None, None, global) {
                    let note = if follow_override {
                        (48 + (dj_root + active_slot.offset) % 12).clamp(48, 66)
                    } else {
                        st.note
                    };
                    if st.motion.any_locked() {
                        self.sampler.set_motion_locks(st.motion);
                    } else {
                        self.sampler.clear_motion_locks();
                    }
                    let samp_p = if use_rig {
                        self.party_rig.as_ref().map(|r| &r.sampler).unwrap_or(&p.sampler)
                    } else {
                        &p.sampler
                    };
                    let vel = (st.velocity * phrase_vel_mul(global, seed, 711)).clamp(0.15, 1.0);
                    self.sampler.note_on(note, vel, samp_p);
                    self.gate_sampler = base_step_samples * s_gscale * st.gate.clamp(0.25, 16.0);
                }
            }
            if let Some(sub_i) = s_sub {
                let st2 = seq.steps[sub_i];
                if st2.active
                    && !self.is_step_suppressed(Some(TrackId::Sampler), None, None, global)
                    && (st2.probability >= 0.999 || hash_f32(seed, global * 2 + 1, 712) <= st2.probability)
                {
                    let note = if follow_override {
                        (48 + (dj_root + active_slot.offset) % 12).clamp(48, 66)
                    } else {
                        st2.note
                    };
                    self.pending_triggers.push((
                        base_step_samples * 0.5,
                        PendingTrigger::Melodic32 {
                            target: ArpTarget::Core(TrackId::Sampler),
                            note,
                            vel: st2.velocity.clamp(0.15, 1.0),
                            accent: st2.accent,
                            slide: st2.slide,
                            chord: st2.chord,
                            motion: st2.motion,
                            gate_samples: base_step_samples * 0.5 * st2.gate.clamp(0.25, 16.0),
                            dj_root,
                        },
                    ));
                }
            }

            // 8-Part Volca Sample Lanes!
            for (part_idx, lane) in pat.sample_lanes.iter().take(8).enumerate() {
                let len = lane.effective_len();
                let v_div = if p.party_mode { ClockDiv::Sixteenth } else { lane.clock_div };
                let (v_prim, v_sub, _) = clock_div_indices(v_div, global, len);
                if let Some(v_idx) = v_prim {
                    let dst = lane.steps[v_idx];
                    if dst.active
                        && !self.is_step_suppressed(Some(TrackId::Sampler), None, Some(part_idx), global)
                        && (dst.probability >= 0.999
                            || hash_f32(seed, global, 200 + part_idx as u32) <= dst.probability)
                    {
                        let vel = (dst.velocity * if dst.accent { 1.15 } else { 1.0 }).clamp(0.1, 1.0);
                        self.volca.trigger_part_with_step(part_idx, vel, &dst, &p.sample_parts);
                        if dst.ratchet > 1 {
                            let n = dst.ratchet.min(4) as usize;
                            let sub_len = base_step_samples / n as f32;
                            for r in 1..n {
                                self.volca_ratchets.push(RatchetEvent {
                                    lane: part_idx,
                                    samples_left: sub_len * r as f32,
                                    vel: vel * (0.86 - r as f32 * 0.12).max(0.35),
                                    accent: false,
                                    tune_offset: 0.0,
                                });
                            }
                        }
                    }
                }
                if let Some(vs_idx) = v_sub {
                    let dst = lane.steps[vs_idx];
                    if dst.active
                        && !self.is_step_suppressed(Some(TrackId::Sampler), None, Some(part_idx), global)
                        && (dst.probability >= 0.999
                            || hash_f32(seed, global * 2 + 1, 200 + part_idx as u32) <= dst.probability)
                    {
                        self.pending_triggers.push((
                            base_step_samples * 0.5,
                            PendingTrigger::Volca32 {
                                part_idx,
                                step: dst,
                            },
                        ));
                    }
                }
            }
        }
        self.suppressed_steps.retain(|&(_, _, _, gs)| gs > global);
    }

    fn process_sample(
        &mut self,
        p: &Project,
        playing: bool,
        drum_cut: bool,
        snare_build: bool,
        selected_part: usize,
        telemetry: &mut EngineTelemetry,
    ) -> (f32, f32) {
        if self.was_party_mode && !p.party_mode {
            self.loaded_sample_gen = 0;
            self.active_party_sample_id = None;
            self.party_rig_reroll = 0;
            self.party_rig_custom = false;
            self.party_rig = None;
            self.party_live_bpm = 0.0;
            self.party_repeat_fx = 0.0;
            self.party_glitch_fx = 0.0;
            telemetry.party_palette.clear();
            telemetry.party_voice_presets = p.voice_presets;
        } else if !self.was_party_mode && p.party_mode {
            self.party_section_idx = 0;
            self.party_bar_in_sec = 0;
            self.party_cycle = 0;
            self.party_evolution = 0;
            self.party_rig_reroll = 0;
            self.party_rig_custom = false;
            self.party_rig = None;
            self.party_live_bpm = p.bpm;
        }
        self.was_party_mode = p.party_mode;

        self.sync_sampler_if_needed(p);
        self.volca.sync_parts(&p.sample_parts);

        if playing && !self.was_playing {
            self.step = 0;
            self.tick_left = 0.0;
            self.pending_triggers.clear();
            self.party_section_idx = 0;
            self.party_bar_in_sec = 0;
            self.party_cycle = 0;
            self.party_evolution = 0;
            self.party_template_idx = 0;
            self.party_rig_reroll = 0;
            self.party_rig_custom = false;
            self.party_rig = None;
            self.party_live_bpm = p.bpm;
            self.party_filter_hz = 18_000.0;
            self.party_filter_target_hz = 18_000.0;
            self.party_filter_mode = GlobalFilterMode::LowPass;
            self.party_filter_res_boost = 0.0;
            self.party_space_boost = 0.0;
            self.party_space_target = 0.0;
            self.party_bass_cut_mul = 1.0;
            self.party_bass_cut_target = 1.0;
            self.party_pad_cut_mul = 1.0;
            self.party_pad_cut_target = 1.0;
            self.party_lead_cut_mul = 1.0;
            self.party_lead_cut_target = 1.0;
            self.party_acid_morph = 0.35;
            self.party_acid_target = 0.35;
            self.party_repeat_fx = 0.0;
            self.party_glitch_fx = 0.0;
        }
        if !playing && self.was_playing {
            self.all_notes_off();
            self.drop_macro_phase = DropMacroPhase::Idle;
            self.drop_riser_target_env = 0.0;
            self.drop_riser_env = 0.0;
            self.drop_macro_progress = 0.0;
            self.drop_macro_target_prog = 0.0;
        }
        self.was_playing = playing;

        // Advance step sequencer with real swing
        if playing {
            if self.tick_left <= 0.0 {
                self.trigger_step(p, drum_cut, snare_build, telemetry);
                self.tick_left = self.step_duration_samples(p, self.step);
                self.last_step_dur = self.tick_left.max(1.0);
                self.step = self.step.wrapping_add(1);
            }
            self.tick_left -= 1.0;

            // Process drum ratchets
            let mut i = 0;
            while i < self.ratchets.len() {
                self.ratchets[i].samples_left -= 1.0;
                if self.ratchets[i].samples_left <= 0.0 {
                    let ev = self.ratchets.swap_remove(i);
                    if let Some(v) = DrumVoiceId::from_index(ev.lane) {
                        let drums_p = if p.party_mode && (p.party_sound_morph || self.party_rig_custom) {
                            self.party_rig.as_ref().map(|r| &r.drums).unwrap_or(&p.drums)
                        } else {
                            &p.drums
                        };
                        if ev.tune_offset.abs() > 0.01 {
                            let mut dk = drums_p.clone();
                            dk.voices[ev.lane].tune = (dk.voices[ev.lane].tune + ev.tune_offset).clamp(-12.0, 12.0);
                            self.drums.trigger(v, ev.vel, ev.accent, false, &dk);
                        } else {
                            self.drums.trigger(v, ev.vel, ev.accent, false, drums_p);
                        }
                    }
                } else {
                    i += 1;
                }
            }

            // Process sample-accurate groove micro-timing queue (drums, chord strum, melodic pocket)
            let mut pi = 0;
            let use_rig_pending = p.party_mode && (p.party_sound_morph || self.party_rig_custom);
            while pi < self.pending_triggers.len() {
                self.pending_triggers[pi].0 -= 1.0;
                if self.pending_triggers[pi].0 <= 0.0 {
                    let (_, ev) = self.pending_triggers.swap_remove(pi);
                    match ev {
                        PendingTrigger::Drum {
                            lane,
                            vel,
                            accent,
                            flam,
                            pitch_offset,
                            pan_lock,
                        } => {
                            if let Some(voice) = DrumVoiceId::from_index(lane) {
                                let drums_p = if use_rig_pending {
                                    self.party_rig.as_ref().map(|r| &r.drums).unwrap_or(&p.drums)
                                } else {
                                    &p.drums
                                };
                                trigger_drum_with_lock(
                                    &mut self.drums,
                                    drums_p,
                                    lane,
                                    voice,
                                    vel,
                                    accent,
                                    flam,
                                    pitch_offset,
                                    pan_lock,
                                );
                            }
                        }
                        PendingTrigger::PadNote { note, vel, accent } => {
                            let pad_p = if use_rig_pending {
                                self.party_rig
                                    .as_ref()
                                    .map(|r| &r.moog_pad)
                                    .unwrap_or(&p.moog_pad)
                            } else {
                                &p.moog_pad
                            };
                            self.pad_moog.note_on(note, vel, accent, false, pad_p);
                        }
                        PendingTrigger::MonoNote {
                            note,
                            prev_note,
                            vel,
                            accent,
                            slide,
                            motion,
                            party_rig_cut,
                            gate_samples,
                        } => {
                            self.dispatch_mono_note(
                                p,
                                use_rig_pending,
                                note,
                                prev_note,
                                vel,
                                accent,
                                slide,
                                motion,
                                party_rig_cut,
                                gate_samples,
                            );
                        }
                        PendingTrigger::Melodic32 {
                            target,
                            note,
                            vel,
                            accent,
                            slide,
                            chord,
                            motion,
                            gate_samples,
                            dj_root,
                        } => {
                            self.dispatch_melodic_32(
                                p,
                                use_rig_pending,
                                target,
                                note,
                                vel,
                                accent,
                                slide,
                                chord,
                                motion,
                                gate_samples,
                                dj_root,
                            );
                        }
                        PendingTrigger::Volca32 { part_idx, step } => {
                            let vel = (step.velocity * if step.accent { 1.15 } else { 1.0 }).clamp(0.1, 1.0);
                            self.volca.trigger_part_with_step(part_idx, vel, &step, &p.sample_parts);
                        }
                    }
                } else {
                    pi += 1;
                }
            }

            // Process Volca sample ratchets
            let mut vi = 0;
            while vi < self.volca_ratchets.len() {
                self.volca_ratchets[vi].samples_left -= 1.0;
                if self.volca_ratchets[vi].samples_left <= 0.0 {
                    let ev = self.volca_ratchets.swap_remove(vi);
                    self.volca.trigger_part(ev.lane, ev.vel, &p.sample_parts);
                } else {
                    vi += 1;
                }
            }
        }

        // Advance Vintage Hardware Arpeggiator (runs live whether playing or stopped!)
        self.process_live_arp(p);

        // Advance per-voice gate timers (proper sustain & release!)
        if self.gate_moog > 0.0 {
            self.gate_moog -= 1.0;
            if self.gate_moog <= 0.0 && !self.live_chord_held {
                if let Some(n) = self.last_moog_note.take() {
                    self.moog.note_off(n);
                }
            }
        }
        if self.gate_pad > 0.0 {
            self.gate_pad -= 1.0;
            if self.gate_pad <= 0.0 && !self.live_chord_held {
                self.pad_moog.all_notes_off();
            }
        }
        if self.gate_mono > 0.0 {
            self.gate_mono -= 1.0;
            if self.gate_mono <= 0.0 {
                if let Some(n) = self.last_mono_note.take() {
                    self.mono.note_off(n);
                }
            }
        }
        if self.gate_acid > 0.0 {
            self.gate_acid -= 1.0;
            if self.gate_acid <= 0.0 {
                if let Some(n) = self.last_acid_note.take() {
                    self.acid.note_off(n);
                }
            }
        }
        if self.gate_sampler > 0.0 {
            self.gate_sampler -= 1.0;
            if self.gate_sampler <= 0.0 {
                self.sampler.all_notes_off();
            }
        }
        for ei in 0..p.extra_tracks.len().min(MAX_EXTRA_TRACKS) {
            if self.extra_gates[ei] > 0.0 {
                self.extra_gates[ei] -= 1.0;
                if self.extra_gates[ei] <= 0.0 {
                    self.extra_synths[ei].all_notes_off();
                }
            }
        }

        // Smooth Party DJ timbre & FX modulation (~30 ms one-pole slew, click-free)
        let slew = 1.0 - (-1.0 / (0.030 * self.sr)).exp();
        self.party_filter_hz += (self.party_filter_target_hz - self.party_filter_hz) * slew;
        self.party_space_boost += (self.party_space_target - self.party_space_boost) * slew;
        self.party_bass_cut_mul += (self.party_bass_cut_target - self.party_bass_cut_mul) * slew;
        self.party_pad_cut_mul += (self.party_pad_cut_target - self.party_pad_cut_mul) * slew;
        self.party_lead_cut_mul += (self.party_lead_cut_target - self.party_lead_cut_mul) * slew;
        self.party_acid_morph += (self.party_acid_target - self.party_acid_morph) * slew;

        // Resolve active Party Rig & real-time modulated params (all on stack, zero heap alloc)
        let use_morph = p.party_mode && (p.party_sound_morph || self.party_rig_custom);
        let mut moog_p;
        let mut pad_p;
        let mut mono_p;
        let mut acid_p;
        let drums_p;
        let mut samp_p;
        let mut sc_base = p.sidechain_amount;
        let mut enable_303_override = false;
        let mut enable_samp_override = false;
        let mut vol_bass = p.mixer.channels[0].volume;
        let mut vol_lead = p.mixer.channels[1].volume;
        let mut vol_303 = p.mixer.channels[2].volume;
        let mut vol_drums = p.mixer.channels[3].volume;
        let mut vol_samp = p.mixer.channels[4].volume;
        let mut vol_pad = p.mixer.moog_pad_volume;

        if use_morph {
            if let Some(rig) = &self.party_rig {
                moog_p = rig.moog.clone();
                pad_p = rig.moog_pad.clone();
                mono_p = rig.monologue.clone();
                acid_p = rig.tb303.clone();
                drums_p = rig.drums.clone();
                samp_p = rig.sampler.clone();
                sc_base = rig.sidechain_amount;
                enable_303_override = rig.enable_303;
                enable_samp_override = rig.enable_sampler;
                vol_bass = rig.bass_vol;
                vol_lead = rig.lead_vol;
                vol_303 = rig.tb303_vol;
                vol_drums = rig.drums_vol;
                vol_samp = rig.sampler_vol;
                vol_pad = rig.pad_vol;
            } else {
                moog_p = p.moog.clone();
                pad_p = p.moog_pad.clone();
                mono_p = p.monologue.clone();
                acid_p = p.tb303.clone();
                drums_p = p.drums.clone();
                samp_p = p.sampler.clone();
            }
            moog_p.cutoff = (moog_p.cutoff * self.party_bass_cut_mul).clamp(80.0, 16_000.0);
            pad_p.cutoff = (pad_p.cutoff * self.party_pad_cut_mul).clamp(160.0, 16_500.0);
            mono_p.cutoff = (mono_p.cutoff * self.party_lead_cut_mul).clamp(220.0, 17_500.0);
            acid_p.cutoff = (acid_p.cutoff * (0.65 + self.party_acid_morph * 1.55)).clamp(90.0, 9_500.0);
            acid_p.resonance = (acid_p.resonance + self.party_acid_morph * 0.16).clamp(0.1, 0.92);
            acid_p.env_mod = (acid_p.env_mod + self.party_acid_morph * 0.18).clamp(0.1, 0.96);
        } else {
            moog_p = p.moog.clone();
            pad_p = p.moog_pad.clone();
            mono_p = p.monologue.clone();
            acid_p = p.tb303.clone();
            drums_p = p.drums.clone();
            samp_p = p.sampler.clone();
        }
        if self.midi_bend_semi.abs() > 0.005 {
            acid_p.tuning = (acid_p.tuning + self.midi_bend_semi).clamp(-12.0, 12.0);
            mono_p.vco2_pitch = (mono_p.vco2_pitch + self.midi_bend_semi).clamp(-12.0, 12.0);
            samp_p.tune_cents = (samp_p.tune_cents + self.midi_bend_semi * 100.0).clamp(-1200.0, 1200.0);
        }
        if self.midi_aftertouch > 0.005 {
            let cut_mul = 1.0 + self.midi_aftertouch * 1.45;
            moog_p.cutoff = (moog_p.cutoff * cut_mul).clamp(40.0, 16_000.0);
            pad_p.cutoff = (pad_p.cutoff * cut_mul).clamp(60.0, 16_500.0);
            mono_p.cutoff = (mono_p.cutoff * cut_mul).clamp(60.0, 17_500.0);
            acid_p.cutoff = (acid_p.cutoff * cut_mul).clamp(80.0, 10_000.0);
        }

        // Smooth click-free sidechain ducking envelope (~2.5ms attack, ~145ms release)
        self.sidechain_peak *= self.sidechain_decay;
        let sc_atk = one_pole_coeff(160.0, self.sr);
        self.sidechain_env += sc_atk * (self.sidechain_peak - self.sidechain_env);
        let sc_amt = self.auto_sidechain.unwrap_or(sc_base).clamp(0.0, 1.0);
        let duck = (1.0 - self.sidechain_env * sc_amt * 0.56).clamp(0.36, 1.0);
        let duck_bass = (1.0 - self.sidechain_env * sc_amt * 0.72).clamp(0.24, 1.0);
        telemetry.sidechain_duck = 1.0 - duck;

        // Global Filter Envelope advancement
        let atk_s = p.fx.global_filter_env_attack.clamp(0.002, 1.5);
        let dec_s = p.fx.global_filter_env_decay.clamp(0.04, 3.0);
        if self.gfilt_env_attacking {
            self.gfilt_env += 1.0 / (atk_s * self.sr).max(1.0);
            if self.gfilt_env >= 1.0 {
                self.gfilt_env = 1.0;
                self.gfilt_env_attacking = false;
            }
        } else if self.gfilt_env > 0.0001 {
            self.gfilt_env *= (-2.3 / (dec_s * self.sr).max(1.0)).exp();
        } else {
            self.gfilt_env = 0.0;
        }
        telemetry.global_filter_env_level = self.gfilt_env;

        // Smooth Bass Drop Macro progress + Riser Tension Synth + Layered Impact Drop
        let prog_slew = 1.0 - (-1.0 / (0.035 * self.sr)).exp();
        self.drop_macro_progress += (self.drop_macro_target_prog - self.drop_macro_progress) * prog_slew;
        let riser_slew = if self.drop_riser_target_env > self.drop_riser_env {
            1.0 - (-1.0 / (0.015 * self.sr)).exp()
        } else {
            1.0 - (-1.0 / (0.008 * self.sr)).exp()
        };
        self.drop_riser_env += (self.drop_riser_target_env - self.drop_riser_env) * riser_slew;
        self.drop_filter_slam *= (-1.0 / (0.14 * self.sr)).exp();

        let mut riser_sig = 0.0_f32;
        if self.drop_riser_env > 0.001 {
            let riser_hz = (self.drop_riser_root_hz * 2.0_f32.powf(self.drop_macro_progress * 2.0))
                .clamp(65.0, 2_800.0);
            self.drop_riser_phase = (self.drop_riser_phase + riser_hz / self.sr).fract();
            self.drop_riser_phase2 =
                (self.drop_riser_phase2 + (riser_hz * 1.498) / self.sr).fract();
            let osc = (self.drop_riser_phase * 2.0 - 1.0) * 0.55
                + (self.drop_riser_phase2 * 2.0 - 1.0) * 0.35;
            let noise = self.drop_rng.next_f32();
            let raw = osc * 0.45 + noise * 0.55;
            let r_cut = (320.0 * (9_000.0_f32 / 320.0).powf(self.drop_macro_progress))
                .clamp(200.0, self.sr * 0.40);
            let (_, bp, hp) = self.drop_riser_filter.process_all(
                raw,
                r_cut,
                0.48 + self.drop_macro_progress * 0.28,
            );
            riser_sig = (bp * 0.65 + hp * 0.35) * self.drop_riser_env * 0.34;
        }

        // Layered Drop Impact: Key-locked sub dive + chest-punch boom + filtered noise downlifter swoosh
        let mut sub_sig = 0.0_f32;
        if self.drop_env > 0.0008 {
            self.drop_pitch += (self.drop_target_hz - self.drop_pitch) * (5.2 / self.sr);
            self.drop_phase = (self.drop_phase + self.drop_pitch / self.sr).fract();
            let sub = (self.drop_phase * std::f32::consts::TAU).sin()
                + 0.28 * (self.drop_phase * 2.0 * std::f32::consts::TAU).sin();
            sub_sig = (sub * 1.5).tanh() * self.drop_env * 0.54;
            self.drop_env *= (-1.0 / (1.05 * self.sr)).exp();
        }
        let mut boom_sig = 0.0_f32;
        if self.drop_boom_env > 0.001 {
            self.drop_boom_pitch += (38.0 - self.drop_boom_pitch) * (28.0 / self.sr);
            self.drop_boom_phase = (self.drop_boom_phase + self.drop_boom_pitch / self.sr).fract();
            boom_sig = ((self.drop_boom_phase * std::f32::consts::TAU).sin() * 2.0).tanh()
                * self.drop_boom_env
                * 0.46;
            self.drop_boom_env *= (-1.0 / (0.095 * self.sr)).exp();
        }
        let mut whoosh_sig = 0.0_f32;
        if self.drop_whoosh_env > 0.001 {
            let wn = self.drop_rng.next_f32();
            let w_cut = (180.0 + self.drop_whoosh_env.powf(1.5) * 6_400.0)
                .clamp(120.0, self.sr * 0.40);
            let w_lp = self.drop_whoosh_filter.process_lp(wn, w_cut, 0.32);
            whoosh_sig = w_lp * self.drop_whoosh_env * 0.28;
            self.drop_whoosh_env *= (-1.0 / (0.72 * self.sr)).exp();
        }
        let drop_sig = riser_sig + sub_sig + boom_sig + whoosh_sig;

        let chorus_amt = if use_morph {
            self.party_rig
                .as_ref()
                .map(|r| r.stereo_chorus)
                .unwrap_or(p.fx.stereo_chorus)
        } else {
            p.fx.stereo_chorus
        };

        // Render all studio voices (6 core + up to 10 dynamic extra tracks!)
        let any_solo = p.mixer.channels.iter().any(|c| c.solo)
            || p.mixer.moog_pad_solo
            || p.extra_tracks.iter().any(|t| t.solo);
        let audible = |mute: bool, solo: bool| !mute && (!any_solo || solo);

        let ch = &p.mixer.channels;
        let mut mix_l = drop_sig;
        let mut mix_r = drop_sig;
        let mut del_l = 0.0_f32;
        let mut del_r = 0.0_f32;
        let mut rev_l = riser_sig * 0.45 + whoosh_sig * 0.35;
        let mut rev_r = riser_sig * 0.45 + whoosh_sig * 0.35;

        // 0: Bass (Moog) — center sub punch + true stereo upper harmonics + console strip EQ + deep sidechain pocket
        let (moog_l, moog_r) = self.moog.process_stereo(&moog_p);
        let moog_l = moog_l * duck_bass;
        let moog_r = moog_r * duck_bass;
        self.track_peaks[0] *= 0.9990;
        if audible(ch[0].mute, ch[0].solo) {
            let (gl, gr) = pan_gains(ch[0].pan);
            let sl = moog_l * vol_bass * gl * 0.56;
            let sr_v = moog_r * vol_bass * gr * 0.56;
            let (sl, sr_v) = self.channel_eq.process_bass((sl, sr_v));
            let (sl, sr_v) = apply_track_filter(
                &mut self.track_filters[0].0,
                &mut self.track_filters[0].1,
                sl,
                sr_v,
                &ch[0].filter,
                self.sr,
            );
            let (sl, sr_v) = apply_stereo_width(sl, sr_v, ch[0].width);
            self.track_peaks[0] = self.track_peaks[0].max(sl.abs().max(sr_v.abs()));
            mix_l += sl;
            mix_r += sr_v;
            if ch[0].delay_enabled {
                del_l += sl * ch[0].delay_send;
                del_r += sr_v * ch[0].delay_send;
            }
            if ch[0].reverb_enabled {
                rev_l += sl * ch[0].reverb_send;
                rev_r += sr_v * ch[0].reverb_send;
            }
        }

        // 1: Arp / Lead (Monologue) — true stereo unison + Juno BBD shimmer + studio console strip (130Hz HPF + body + silk)
        let (mono_l, mono_r) = self.mono.process_stereo(&mono_p);
        let mono_duck = 0.62 + 0.38 * duck;
        let (wl, wr) = self
            .arp_chorus
            .process_stereo(mono_l * mono_duck, mono_r * mono_duck, chorus_amt * 0.65);
        self.track_peaks[1] *= 0.9990;
        if audible(ch[1].mute, ch[1].solo) {
            let (gl, gr) = pan_gains(ch[1].pan);
            let sl_raw = wl * vol_lead * gl * 0.82;
            let sr_raw = wr * vol_lead * gr * 0.82;
            let (eq_l, eq_r) = self.channel_eq.process_lead((sl_raw, sr_raw));
            let (sl, sr_v) = apply_track_filter(
                &mut self.track_filters[1].0,
                &mut self.track_filters[1].1,
                eq_l,
                eq_r,
                &ch[1].filter,
                self.sr,
            );
            let (sl, sr_v) = apply_stereo_width(sl, sr_v, ch[1].width);
            self.track_peaks[1] = self.track_peaks[1].max(sl.abs().max(sr_v.abs()));
            mix_l += sl;
            mix_r += sr_v;
            if ch[1].delay_enabled {
                del_l += sl * ch[1].delay_send;
                del_r += sr_v * ch[1].delay_send;
            }
            if ch[1].reverb_enabled {
                rev_l += sl * ch[1].reverb_send;
                rev_r += sr_v * ch[1].reverb_send;
            }
        }

        // 2: Acid / 303 — present acid/funk counter-line + 110Hz HPF & 1.6kHz bite console strip
        let acid_s = self.acid.process_sample(&acid_p) * (0.55 + 0.45 * duck);
        let acid_mute = ch[2].mute && !enable_303_override;
        self.track_peaks[2] *= 0.9990;
        if audible(acid_mute, ch[2].solo) {
            let (gl, gr) = pan_gains(ch[2].pan);
            let sl = acid_s * vol_303 * gl * 0.80;
            let sr_v = acid_s * vol_303 * gr * 0.80;
            let (eq_l, eq_r) = self.channel_eq.process_acid((sl, sr_v));
            let (sl, sr_v) = apply_track_filter(
                &mut self.track_filters[2].0,
                &mut self.track_filters[2].1,
                eq_l,
                eq_r,
                &ch[2].filter,
                self.sr,
            );
            let (sl, sr_v) = apply_stereo_width(sl, sr_v, ch[2].width);
            self.track_peaks[2] = self.track_peaks[2].max(sl.abs().max(sr_v.abs()));
            mix_l += sl;
            mix_r += sr_v;
            if ch[2].delay_enabled {
                del_l += sl * ch[2].delay_send;
                del_r += sr_v * ch[2].delay_send;
            }
            if ch[2].reverb_enabled {
                rev_l += sl * ch[2].reverb_send;
                rev_r += sr_v * ch[2].reverb_send;
            }
        }

        // 3: Drums — TRUE STEREO preserved, punchy studio snap!
        let (dl, dr) = self.drums.process_sample(&drums_p);
        self.track_peaks[3] *= 0.9990;
        if audible(ch[3].mute, ch[3].solo) {
            let (gl, gr) = pan_gains(ch[3].pan);
            let sl = dl * vol_drums * gl * 0.62;
            let sr_v = dr * vol_drums * gr * 0.62;
            let (sl, sr_v) = apply_track_filter(
                &mut self.track_filters[3].0,
                &mut self.track_filters[3].1,
                sl,
                sr_v,
                &ch[3].filter,
                self.sr,
            );
            let (sl, sr_v) = apply_stereo_width(sl, sr_v, ch[3].width);
            self.track_peaks[3] = self.track_peaks[3].max(sl.abs().max(sr_v.abs()));
            mix_l += sl;
            mix_r += sr_v;
            if ch[3].delay_enabled {
                del_l += sl * ch[3].delay_send;
                del_r += sr_v * ch[3].delay_send;
            }
            if ch[3].reverb_enabled {
                rev_l += sl * ch[3].reverb_send;
                rev_r += sr_v * ch[3].reverb_send;
            }
        }

        // 4: Sampler + 8-Part Volca Bank — true stereo playback + BBD stereo chorus + Isolator EQ + console strip
        let (samp_raw_l, samp_raw_r) = self.sampler.process_stereo(&samp_p);
        let samp_duck = 0.55 + 0.45 * duck;
        let (swl, swr) = self.sampler_chorus.process_stereo(
            samp_raw_l * samp_duck,
            samp_raw_r * samp_duck,
            chorus_amt * 0.72,
        );
        let iso_b = self.auto_iso_bass.unwrap_or(p.sampler_iso_bass);
        let iso_t = self.auto_iso_treble.unwrap_or(p.sampler_iso_treble);
        let (vl, vr, vrev_l, vrev_r) =
            self.volca
                .process_stereo(&p.sample_parts, iso_b, iso_t);
        let v_duck = 0.55 + 0.45 * duck;
        let samp_mute = ch[4].mute && !enable_samp_override;
        self.track_peaks[4] *= 0.9990;
        if audible(samp_mute, ch[4].solo) {
            let (gl, gr) = pan_gains(ch[4].pan);
            let sl_raw = (swl * gl * 0.82 + vl * v_duck * 0.80) * vol_samp;
            let sr_raw = (swr * gr * 0.82 + vr * v_duck * 0.80) * vol_samp;
            let (eq_l, eq_r) = self.channel_eq.process_sampler((sl_raw, sr_raw));
            let (sl, sr_v) = apply_track_filter(
                &mut self.track_filters[4].0,
                &mut self.track_filters[4].1,
                eq_l,
                eq_r,
                &ch[4].filter,
                self.sr,
            );
            let (sl, sr_v) = apply_stereo_width(sl, sr_v, ch[4].width);
            self.track_peaks[4] = self.track_peaks[4].max(sl.abs().max(sr_v.abs()));
            mix_l += sl;
            mix_r += sr_v;
            if ch[4].delay_enabled {
                del_l += sl * ch[4].delay_send;
                del_r += sr_v * ch[4].delay_send;
            }
            if ch[4].reverb_enabled {
                rev_l += sl * ch[4].reverb_send + vrev_l * vol_samp * 0.55;
                rev_r += sr_v * ch[4].reverb_send + vrev_r * vol_samp * 0.55;
            }
        }

        // 5: Chords / MoogPad — true stereo unison + lush Juno BBD chorus + 120Hz HPF & 1.8kHz pocket console strip
        let (pad_l, pad_r) = self.pad_moog.process_stereo(&pad_p);
        let pad_duck = 0.48 + 0.52 * duck;
        let (pwl, pwr) = self
            .pad_chorus
            .process_stereo(pad_l * pad_duck, pad_r * pad_duck, chorus_amt.max(0.32));
        self.track_peaks[5] *= 0.9990;
        if audible(p.mixer.moog_pad_mute, p.mixer.moog_pad_solo) {
            let sl_raw = pwl * vol_pad * 0.88;
            let sr_raw = pwr * vol_pad * 0.88;
            let (eq_l, eq_r) = self.channel_eq.process_pad((sl_raw, sr_raw));
            let (sl, sr_v) = apply_track_filter(
                &mut self.pad_filter.0,
                &mut self.pad_filter.1,
                eq_l,
                eq_r,
                &p.mixer.moog_pad_filter,
                self.sr,
            );
            let (sl, sr_v) = apply_stereo_width(sl, sr_v, p.mixer.moog_pad_width);
            self.track_peaks[5] = self.track_peaks[5].max(sl.abs().max(sr_v.abs()));
            mix_l += sl;
            mix_r += sr_v;
            if p.mixer.moog_pad_delay_enabled {
                del_l += sl * p.mixer.moog_pad_delay_send;
                del_r += sr_v * p.mixer.moog_pad_delay_send;
            }
            if p.mixer.moog_pad_reverb_enabled {
                rev_l += sl * p.mixer.moog_pad_reverb_send;
                rev_r += sr_v * p.mixer.moog_pad_reverb_send;
            }
        }

        // 6+: Extra Dynamic Synth Tracks (tracks 7..=16) + true stereo + per-instrument filter + width
        let extra_duck = 0.52 + 0.48 * duck;
        for (ei, tr) in p.extra_tracks.iter().take(MAX_EXTRA_TRACKS).enumerate() {
            let (esl, esr) = self.extra_synths[ei].process_stereo(&tr.synth);
            let s_l = esl * extra_duck;
            let s_r = esr * extra_duck;
            if audible(tr.mute, tr.solo) {
                let (gl, gr) = pan_gains(tr.pan);
                let sl = s_l * tr.volume * gl * 0.78;
                let sr_v = s_r * tr.volume * gr * 0.78;
                let (efl, efr) = &mut self.extra_filters[ei];
                let (sl, sr_v) = apply_track_filter(
                    efl,
                    efr,
                    sl,
                    sr_v,
                    &tr.filter,
                    self.sr,
                );
                let (sl, sr_v) = apply_stereo_width(sl, sr_v, tr.width);
                mix_l += sl;
                mix_r += sr_v;
                del_l += sl * tr.delay_send;
                del_r += sr_v * tr.delay_send;
                rev_l += sl * tr.reverb_send;
                rev_r += sr_v * tr.reverb_send;
            }
        }

        if p.mixer.mono_sub {
            let mid = 0.5 * (mix_l + mix_r);
            let side = 0.5 * (mix_l - mix_r);
            let low_side = self.mono_sub_lpf.process_lp(side, 120.0, 0.05);
            let tight_side = side - low_side;
            mix_l = mid + tight_side;
            mix_r = mid - tight_side;
        }

        // Apply Motion Automation & Party DJ FX modulation cleanly on a stack copy of FxParams
        let mut fx_params = p.fx.clone();
        if use_morph {
            if let Some(rig) = &self.party_rig {
                fx_params.delay_div = rig.delay_div;
                fx_params.reverb_size = rig.reverb_size;
                fx_params.reverb_shimmer = rig.reverb_shimmer;
                fx_params.analog_warmth = rig.analog_warmth;
                fx_params.master_crisp = rig.master_crisp;
                fx_params.stereo_chorus = rig.stereo_chorus;
                fx_params.club_punch = rig.club_punch;
                if self.auto_space.is_none() {
                    fx_params.reverb_mix = rig.reverb_mix;
                    fx_params.delay_mix = rig.delay_mix;
                }
            }
        }
        if let Some(fnorm) = self.auto_filter {
            fx_params.global_filter_enabled = true;
            fx_params.global_filter_cutoff = 120.0 * (18_000.0_f32 / 120.0).powf(fnorm.clamp(0.0, 1.0));
        } else if p.party_mode
            && (self.party_filter_hz < 17_400.0 || !matches!(self.party_filter_mode, GlobalFilterMode::LowPass))
        {
            fx_params.global_filter_enabled = true;
            fx_params.global_filter_mode = self.party_filter_mode;
            fx_params.global_filter_cutoff = match self.party_filter_mode {
                GlobalFilterMode::HighPass => self.party_filter_hz.clamp(40.0, 1_400.0),
                _ => fx_params.global_filter_cutoff.min(self.party_filter_hz).clamp(240.0, 18_000.0),
            };
            fx_params.global_filter_resonance =
                (fx_params.global_filter_resonance + self.party_filter_res_boost).min(0.78);
        }

        // Drop Macro Riser Filter Sweep / Impact Filter Slam / Global Filter Envelope modulation
        let drop_building = matches!(
            self.drop_macro_phase,
            DropMacroPhase::Building {
                step_idx,
                total_steps,
                gap_steps,
            } if step_idx <= total_steps.saturating_sub(gap_steps)
        );
        if drop_building && p.drop_macro.riser_sweep {
            fx_params.global_filter_enabled = true;
            fx_params.global_filter_mode = GlobalFilterMode::HighPass;
            fx_params.global_filter_cutoff =
                (55.0 * (1_500.0_f32 / 55.0).powf(self.drop_macro_progress)).clamp(45.0, 1_800.0);
            fx_params.global_filter_resonance =
                (fx_params.global_filter_resonance + 0.16 + 0.32 * self.drop_macro_progress).min(0.76);
            fx_params.reverb_mix =
                (fx_params.reverb_mix + 0.30 * self.drop_macro_progress).min(0.85);
            fx_params.delay_mix =
                (fx_params.delay_mix + 0.22 * self.drop_macro_progress).min(0.80);
        } else if self.drop_filter_slam > 0.05 {
            fx_params.global_filter_mode = GlobalFilterMode::LowPass;
            fx_params.global_filter_cutoff = 18_000.0;
            fx_params.global_filter_resonance = 0.08;
        } else if fx_params.global_filter_enabled
            && fx_params.global_filter_env_amount.abs() > 0.005
            && self.gfilt_env > 0.0005
        {
            let oct = self.gfilt_env * fx_params.global_filter_env_amount.clamp(-1.0, 1.0) * 5.0;
            fx_params.global_filter_cutoff =
                (fx_params.global_filter_cutoff * 2.0_f32.powf(oct)).clamp(45.0, 18_000.0);
        }

        let target_drop_phase: &str = match self.drop_macro_phase {
            DropMacroPhase::Idle => "",
            DropMacroPhase::Building {
                step_idx,
                total_steps,
                gap_steps,
            } => {
                if step_idx > total_steps.saturating_sub(gap_steps) {
                    "HOLD..."
                } else {
                    "BUILD"
                }
            }
            DropMacroPhase::Impact { .. } => "DROP!",
        };
        if telemetry.drop_phase != target_drop_phase {
            telemetry.drop_phase.clear();
            telemetry.drop_phase.push_str(target_drop_phase);
        }
        telemetry.drop_progress = match self.drop_macro_phase {
            DropMacroPhase::Idle => 0.0,
            DropMacroPhase::Building { .. } => self.drop_macro_progress,
            DropMacroPhase::Impact { .. } => 1.0,
        };

        if let Some(sp) = self.auto_space {
            let s = sp.clamp(0.0, 1.0);
            fx_params.reverb_mix = s;
            fx_params.delay_mix = (s * 0.92).min(0.85);
        } else if p.party_mode && self.party_space_boost > 0.005 {
            fx_params.reverb_mix = (fx_params.reverb_mix + self.party_space_boost).min(0.82);
            fx_params.delay_mix = (fx_params.delay_mix + self.party_space_boost * 0.85).min(0.80);
        }

        if let Some(gl) = self.auto_glitch {
            fx_params.glitch = gl.clamp(0.0, 0.9);
        } else if p.party_mode {
            fx_params.glitch = fx_params.glitch.max(self.party_glitch_fx).clamp(0.0, 0.9);
        }
        if p.party_mode {
            fx_params.repeat = fx_params.repeat.max(self.party_repeat_fx).clamp(0.0, 0.9);
        }

        let active_bpm = if p.party_mode && self.party_live_bpm >= 40.0 {
            self.party_live_bpm
        } else {
            p.bpm
        };

        let (fx_l, fx_r) = self.fx.process_stereo(
            mix_l,
            mix_r,
            del_l,
            del_r,
            rev_l,
            rev_r,
            &fx_params,
            active_bpm,
        );

        let master = p.mixer.master_volume.clamp(0.0, 1.3);
        let out_l = (fx_l * master).clamp(-0.98, 0.98);
        let out_r = (fx_r * master).clamp(-0.98, 0.98);
        self.peak_l = (self.peak_l * 0.9992).max(out_l.abs());
        self.peak_r = (self.peak_r * 0.9992).max(out_r.abs());
        telemetry.peak_l = self.peak_l;
        telemetry.peak_r = self.peak_r;
        telemetry.track_peaks = self.track_peaks;
        telemetry.sampler_playhead = self
            .volca
            .playhead_norm(selected_part)
            .or_else(|| self.sampler.latest_playhead_norm());
        telemetry.volca_active_mask = self.volca.active_parts_mask();
        if p.party_mode {
            if let Some(rig) = &self.party_rig {
                telemetry.party_voice_presets = rig.voice_presets;
                if telemetry.party_palette != rig.palette_label {
                    telemetry.party_palette = rig.palette_label.clone();
                }
            } else {
                telemetry.party_voice_presets = p.voice_presets;
            }
        }
        (out_l, out_r)
    }
}

pub struct AudioEngine {
    _stream: Option<cpal::Stream>,
    pub project: Arc<Mutex<Project>>,
    pub playing: Arc<AtomicBool>,
    pub current_step: Arc<AtomicUsize>,
    pub step_progress: Arc<AtomicU32>,
    pub scope: Arc<OutputScope>,
    pub status: String,
    pub drum_cut: Arc<AtomicBool>,
    pub snare_build: Arc<AtomicBool>,
    pub selected_sample_part: Arc<AtomicUsize>,
    live_queue: Arc<Mutex<VecDeque<LiveEvent>>>,
    telemetry: Arc<Mutex<EngineTelemetry>>,
    sf2_meta: Arc<Mutex<(String, Vec<String>)>>,
}

impl AudioEngine {
    #[cfg(test)]
    pub fn start_silent(project: Project) -> Self {
        Self {
            _stream: None,
            project: Arc::new(Mutex::new(project)),
            playing: Arc::new(AtomicBool::new(false)),
            current_step: Arc::new(AtomicUsize::new(0)),
            step_progress: Arc::new(AtomicU32::new(0.0_f32.to_bits())),
            scope: Arc::new(OutputScope::new(256)),
            status: "Stereo Audio Live @ 44100 Hz".into(),
            drum_cut: Arc::new(AtomicBool::new(false)),
            snare_build: Arc::new(AtomicBool::new(false)),
            selected_sample_part: Arc::new(AtomicUsize::new(0)),
            live_queue: Arc::new(Mutex::new(VecDeque::new())),
            telemetry: Arc::new(Mutex::new(EngineTelemetry::default())),
            sf2_meta: Arc::new(Mutex::new((String::new(), Vec::new()))),
        }
    }

    #[cfg(test)]
    pub fn set_telemetry_for_test(&self, t: EngineTelemetry) {
        if let Ok(mut guard) = self.telemetry.lock() {
            *guard = t;
        }
    }

    pub fn start(project: Project) -> Self {
        let initial_sf2: Option<Arc<SoundFontBank>> = project
            .soundfont_path
            .as_deref()
            .and_then(|p| SoundFontBank::from_file(Path::new(p)).ok())
            .or_else(|| {
                discover_system_soundfonts()
                    .into_iter()
                    .find_map(|p| SoundFontBank::from_file(&p).ok())
            })
            .map(Arc::new);
        let init_meta = initial_sf2
            .as_ref()
            .map(|b| (b.name.clone(), b.preset_names()))
            .unwrap_or_default();
        let sf2_meta = Arc::new(Mutex::new(init_meta));

        let project = Arc::new(Mutex::new(project));
        let playing = Arc::new(AtomicBool::new(false));
        let current_step = Arc::new(AtomicUsize::new(0));
        let step_progress = Arc::new(AtomicU32::new(0.0_f32.to_bits()));
        let scope = Arc::new(OutputScope::new(256));
        let drum_cut = Arc::new(AtomicBool::new(false));
        let snare_build = Arc::new(AtomicBool::new(false));
        let selected_sample_part = Arc::new(AtomicUsize::new(0));
        let live_queue = Arc::new(Mutex::new(VecDeque::new()));
        let telemetry = Arc::new(Mutex::new(EngineTelemetry::default()));

        let host = cpal::default_host();
        let Some(device) = host.default_output_device() else {
            return Self {
                _stream: None,
                project,
                playing,
                current_step,
                step_progress,
                scope,
                status: "No audio output device".into(),
                drum_cut,
                snare_build,
                selected_sample_part,
                live_queue,
                telemetry,
                sf2_meta,
            };
        };
        let Ok(cfg) = device.default_output_config() else {
            return Self {
                _stream: None,
                project,
                playing,
                current_step,
                step_progress,
                scope,
                status: "No default output config".into(),
                drum_cut,
                snare_build,
                selected_sample_part,
                live_queue,
                telemetry,
                sf2_meta,
            };
        };

        let channels = cfg.channels() as usize;
        let sr = cfg.sample_rate().0 as f32;
        let p_ref = project.clone();
        let play_ref = playing.clone();
        let step_ref = current_step.clone();
        let prog_ref = step_progress.clone();
        let scope_ref = scope.clone();
        let cut_ref = drum_cut.clone();
        let build_ref = snare_build.clone();
        let part_ref = selected_sample_part.clone();
        let q_ref = live_queue.clone();
        let tel_ref = telemetry.clone();

        let mut rt = Runtime::new(sr);
        if let Some(bank) = initial_sf2 {
            let _ = rt.moog.set_soundfont_bank(Some(bank.clone()));
            let _ = rt.pad_moog.set_soundfont_bank(Some(bank.clone()));
            let _ = rt.mono.set_soundfont_bank(Some(bank.clone()));
            for s in &mut rt.extra_synths {
                let _ = s.set_soundfont_bank(Some(bank.clone()));
            }
        }
        let mut local_tel = EngineTelemetry::default();
        let mut scope_decim = 0usize;

        let stream = device.build_output_stream(
            &cfg.into(),
            move |data: &mut [f32], _| {
                let p = match p_ref.lock() {
                    Ok(g) => g.clone(),
                    Err(_) => {
                        data.fill(0.0);
                        return;
                    }
                };
                if let Ok(mut q) = q_ref.try_lock() {
                    while let Some(ev) = q.pop_front() {
                        rt.handle_live_event(ev, &p);
                    }
                }
                let is_playing = play_ref.load(Ordering::Relaxed);
                let is_cut = cut_ref.load(Ordering::Relaxed);
                let is_build = build_ref.load(Ordering::Relaxed);
                let sel_part = part_ref.load(Ordering::Relaxed);

                for frame in data.chunks_mut(channels.max(1)) {
                    let (out_l, out_r) =
                        rt.process_sample(&p, is_playing, is_cut, is_build, sel_part, &mut local_tel);
                    scope_decim = (scope_decim + 1) % 6;
                    if scope_decim == 0 {
                        scope_ref.push((out_l + out_r) * 0.5);
                    }
                    if channels >= 2 {
                        frame[0] = out_l;
                        frame[1] = out_r;
                        for ch in &mut frame[2..] {
                            *ch = 0.0;
                        }
                    } else if let Some(s) = frame.first_mut() {
                        *s = (out_l + out_r) * 0.5;
                    }
                }
                step_ref.store(rt.step.saturating_sub(1), Ordering::Relaxed);
                let prog = (1.0 - rt.tick_left / rt.last_step_dur.max(1.0)).clamp(0.0, 1.0);
                prog_ref.store(prog.to_bits(), Ordering::Relaxed);
                if let Ok(mut t) = tel_ref.try_lock() {
                    *t = local_tel.clone();
                }
            },
            |_| {},
            None,
        );

        match stream {
            Ok(s) => match s.play() {
                Ok(()) => Self {
                    _stream: Some(s),
                    project,
                    playing,
                    current_step,
                    step_progress,
                    scope,
                    status: format!("Stereo Audio Live @ {} Hz", sr as u32),
                    drum_cut,
                    snare_build,
                    selected_sample_part,
                    live_queue,
                    telemetry,
                    sf2_meta,
                },
                Err(e) => Self {
                    _stream: None,
                    project,
                    playing,
                    current_step,
                    step_progress,
                    scope,
                    status: format!("Audio play error: {e}"),
                    drum_cut,
                    snare_build,
                    selected_sample_part,
                    live_queue,
                    telemetry,
                    sf2_meta,
                },
            },
            Err(e) => Self {
                _stream: None,
                project,
                playing,
                current_step,
                step_progress,
                scope,
                status: format!("Audio stream error: {e}"),
                drum_cut,
                snare_build,
                selected_sample_part,
                live_queue,
                telemetry,
                sf2_meta,
            },
        }
    }

    pub fn load_soundfont_file(&self, path: &Path) -> Result<String, String> {
        let bank = Arc::new(SoundFontBank::from_file(path)?);
        let name = bank.name.clone();
        let presets = bank.preset_names();
        if let Ok(mut meta) = self.sf2_meta.lock() {
            *meta = (name.clone(), presets);
        }
        self.send_live(LiveEvent::LoadSoundFont {
            bank: Some(bank),
            name: name.clone(),
        });
        Ok(name)
    }

    pub fn soundfont_meta(&self) -> (String, Vec<String>) {
        self.sf2_meta
            .lock()
            .map(|m| m.clone())
            .unwrap_or_default()
    }

    pub fn step_phase(&self) -> f32 {
        f32::from_bits(self.step_progress.load(Ordering::Relaxed)).clamp(0.0, 1.0)
    }

    pub fn send_live(&self, ev: LiveEvent) {
        if let Ok(mut q) = self.live_queue.lock() {
            q.push_back(ev);
        }
    }

    pub fn telemetry(&self) -> EngineTelemetry {
        self.telemetry
            .lock()
            .map(|t| t.clone())
            .unwrap_or_default()
    }
}

/// Renders `bars` of stereo 44.1 kHz 16-bit WAV audio offline using the exact
/// same DSP, stereo mix, sidechain, and MasterFx pipeline.
pub fn export_wav(project: &Project, path: &Path, bars: usize) -> Result<(), String> {
    let sr = 44_100u32;
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: sr,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec).map_err(|e| e.to_string())?;
    let mut rt = Runtime::new(sr as f32);
    let mut tel = EngineTelemetry::default();
    let total_seconds = (bars.max(1) as f32) * 4.0 * (60.0 / project.bpm.clamp(40.0, 240.0));
    let total_samples = (total_seconds * sr as f32).round() as usize;

    let mut dither_rng = Xorshift32::new(0xD174_E201);
    for _ in 0..total_samples {
        let (l, r) = rt.process_sample(project, true, false, false, 0, &mut tel);
        let d_l = (dither_rng.next_f32() - dither_rng.next_f32()) / 32768.0;
        let d_r = (dither_rng.next_f32() - dither_rng.next_f32()) / 32768.0;
        let il = ((l + d_l).clamp(-1.0, 1.0) * 32767.0).round() as i16;
        let ir = ((r + d_r).clamp(-1.0, 1.0) * 32767.0).round() as i16;
        writer.write_sample(il).map_err(|e| e.to_string())?;
        writer.write_sample(ir).map_err(|e| e.to_string())?;
    }
    writer.finalize().map_err(|e| e.to_string())
}

/// Offline-renders `bars` of the current project into normalized mono f32 at 48 kHz
/// so you can resample your own synth chords, acid lines, or grooves straight onto a Volca Pad!
pub fn render_resample_mono(project: &Project, bars: usize) -> Vec<f32> {
    let sr = 48_000u32;
    let mut rt = Runtime::new(sr as f32);
    let mut tel = EngineTelemetry::default();
    let total_seconds = (bars.clamp(1, 8) as f32) * 4.0 * (60.0 / project.bpm.clamp(40.0, 240.0));
    let total_samples = (total_seconds * sr as f32).round() as usize;
    let mut out = Vec::with_capacity(total_samples);
    for _ in 0..total_samples {
        let (l, r) = rt.process_sample(project, true, false, false, 0, &mut tel);
        out.push(((l + r) * 0.5).clamp(-1.0, 1.0));
    }
    crate::dsp::sampler::normalize_peak(&out)
}

/// Exports the master mix + 6 individual stereo track stems into `dir`.
pub fn export_stems(project: &Project, dir: &Path, bars: usize) -> Result<usize, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let mut count = 0usize;
    export_wav(project, &dir.join("00_master_mix.wav"), bars)?;
    count += 1;

    let stems: [(&str, TrackId); 6] = [
        ("01_drums.wav", TrackId::Drums),
        ("02_bass.wav", TrackId::Moog),
        ("03_chords.wav", TrackId::MoogPad),
        ("04_arp.wav", TrackId::Monologue),
        ("05_lead_303.wav", TrackId::Tb303),
        ("06_sampler.wav", TrackId::Sampler),
    ];

    for (fname, track) in stems {
        let mut solo_proj = project.clone();
        for ch in &mut solo_proj.mixer.channels {
            ch.solo = false;
        }
        solo_proj.mixer.moog_pad_solo = false;
        for et in &mut solo_proj.extra_tracks {
            et.solo = false;
        }
        if track == TrackId::MoogPad {
            solo_proj.mixer.moog_pad_mute = false;
            solo_proj.mixer.moog_pad_solo = true;
        } else {
            let idx = track.index();
            solo_proj.mixer.channels[idx].mute = false;
            solo_proj.mixer.channels[idx].solo = true;
        }
        export_wav(&solo_proj, &dir.join(fname), bars)?;
        count += 1;
    }
    for (ei, tr) in project.extra_tracks.iter().take(MAX_EXTRA_TRACKS).enumerate() {
        if tr.mute {
            continue;
        }
        let mut solo_proj = project.clone();
        for ch in &mut solo_proj.mixer.channels {
            ch.solo = false;
        }
        solo_proj.mixer.moog_pad_solo = false;
        for (j, et) in solo_proj.extra_tracks.iter_mut().enumerate() {
            et.solo = j == ei;
            if j == ei {
                et.mute = false;
            }
        }
        let slug: String = tr
            .name
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
            .collect();
        let fname = format!("{:02}_extra_{}.wav", 7 + ei, slug.trim_matches('_'));
        export_wav(&solo_proj, &dir.join(fname), bars)?;
        count += 1;
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{apply_vibe_to_project, Vibe};

    #[test]
    fn all_vibes_render_clean_stereo_without_nans_or_clipping() {
        for &vibe in &Vibe::ALL {
            let mut p = Project::default();
            apply_vibe_to_project(&mut p, vibe, None, 0x1234_5678);
            let mut rt = Runtime::new(44_100.0);
            let mut tel = EngineTelemetry::default();
            let mut max_amp = 0.0_f32;
            let mut stereo_diff = 0.0_f32;
            for _ in 0..22_050 {
                let (l, r) = rt.process_sample(&p, true, false, false, 0, &mut tel);
                assert!(l.is_finite() && r.is_finite(), "NaN in vibe {:?}", vibe);
                assert!(l.abs() <= 1.0 && r.abs() <= 1.0, "Hard clip in vibe {:?}", vibe);
                max_amp = max_amp.max(l.abs()).max(r.abs());
                stereo_diff = stereo_diff.max((l - r).abs());
            }
            assert!(max_amp > 0.05, "Silent output for vibe {:?}", vibe);
            assert!(stereo_diff > 0.001, "Expected stereo width for vibe {:?}", vibe);
        }
    }

    #[test]
    fn party_mode_evolves_cleanly() {
        use std::collections::HashSet;

        let mut p = Project::default();
        apply_vibe_to_project(&mut p, Vibe::ElectroFunk, None, 0x9999);
        p.party_mode = true;
        p.party_auto_evolve = true;
        let mut rt = Runtime::new(44_100.0);
        let mut tel = EngineTelemetry::default();
        let mut max_amp = 0.0_f32;
        for _ in 0..44_100 {
            let (l, r) = rt.process_sample(&p, true, false, false, 0, &mut tel);
            assert!(l.is_finite() && r.is_finite());
            max_amp = max_amp.max(l.abs()).max(r.abs());
        }
        assert!(max_amp > 0.05);
        assert!(!tel.active_chord.is_empty());
        assert!(!tel.party_section.is_empty());

        // Step across 40 bars (640 steps) and verify sections, progressions, drum grooves, and full DJ songs actually evolve
        let mut seen_sections = HashSet::new();
        let mut seen_drum_sigs = HashSet::new();
        let mut seen_melodic_sigs = HashSet::new();
        let mut seen_song_titles = HashSet::new();
        for _ in 0..640 {
            rt.trigger_step(&p, false, false, &mut tel);
            rt.step = rt.step.wrapping_add(1);
            seen_sections.insert(tel.party_section.clone());
            seen_drum_sigs.insert(rt.auto_beat_sig);
            seen_melodic_sigs.insert(rt.party_melodic_sig);
            seen_song_titles.insert(tel.party_song_title.clone());
        }
        assert!(
            seen_sections.len() >= 6,
            "Expected all 6 party sections, got {:?}",
            seen_sections
        );
        assert!(
            rt.party_evolution >= 3,
            "Expected progression evolution >= 3 over 40 bars, got {}",
            rt.party_evolution
        );
        assert!(
            seen_drum_sigs.len() >= 6,
            "Expected fresh drum grooves across phrases/sections, got only {}",
            seen_drum_sigs.len()
        );
        assert!(
            seen_melodic_sigs.len() >= 3,
            "Expected Party DJ to generate fresh melodic arrangements as it transitions, got {}",
            seen_melodic_sigs.len()
        );
        assert!(
            seen_song_titles.len() >= 2,
            "Expected Party DJ to transition to a new song after a full 36-bar cycle, got {:?}",
            seen_song_titles
        );
    }

    #[test]
    fn all_voice_presets_samples_chords_scales_and_factory_presets_work() {
        use crate::dsp::sampler::{generate_builtin_sample, BUILTIN_SAMPLE_NAMES};
        use crate::model::{
            apply_voice_preset, build_auto_chord_pads, chord_symbol, voice_preset_names,
            ChordSlot, ChordType, FactoryPreset, Scale, PROGRESSIONS,
        };

        // 1. Every built-in sample (including 70s Solina String, OB-X Power Chord, Electric Guitar)
        for id in 0..BUILTIN_SAMPLE_NAMES.len() {
            let pcm = generate_builtin_sample(id as u8, 44_100);
            assert!(!pcm.is_empty());
            let peak = pcm.iter().fold(0.0_f32, |m, &x| {
                assert!(x.is_finite());
                m.max(x.abs())
            });
            assert!(peak > 0.1, "Sample {} was too quiet: {}", BUILTIN_SAMPLE_NAMES[id], peak);
        }

        // 2. Every voice preset on every studio track renders cleanly
        let mut p = Project::default();
        for &track in &TrackId::STUDIO_TRACKS {
            let names = voice_preset_names(track);
            assert!(!names.is_empty());
            for pi in 0..names.len() {
                apply_voice_preset(&mut p, track, pi);
            }
        }

        // 3. Every scale builds 8 valid Auto-Chord pads, and every ChordType has a valid symbol
        for &sc in &Scale::ALL {
            let pads = build_auto_chord_pads(0, sc);
            assert_eq!(pads.len(), 8);
            for pad in &pads {
                assert!(!pad.chord_notes.is_empty());
            }
        }
        for &ct in &ChordType::ALL {
            assert!(!ct.intervals().is_empty());
            let sym = chord_symbol(0, ChordSlot { offset: 0, chord: ct });
            assert!(!sym.is_empty());
        }
        assert_eq!(PROGRESSIONS.len(), 23);

        // 4. Every FactoryPreset builds cleanly, and drum voice names adapt to 808/909 models
        for &fp in &FactoryPreset::ALL {
            let proj = Project::factory(fp);
            assert!(!proj.name.is_empty());
        }
        assert_eq!(
            DrumVoiceId::CowbellRim.short_name_for_model( crate::model::DrumModel::Tr808),
            "COWBELL"
        );
        assert_eq!(
            DrumVoiceId::CowbellRim.short_name_for_model(crate::model::DrumModel::Tr909),
            "RIMSHOT"
        );
        assert_eq!(
            DrumVoiceId::CowbellRim.full_name_for_model(crate::model::DrumModel::Tr808),
            "TR-808 Cowbell"
        );
        assert_eq!(
            DrumVoiceId::CowbellRim.full_name_for_model(crate::model::DrumModel::Tr909),
            "TR-909 Rimshot"
        );
    }

    #[test]
    fn generators_vary_across_seeds() {
        let b1 = generate_auto_beat(Vibe::DeepHouse, 0.80, 0x1111_2222);
        let b2 = generate_auto_beat(Vibe::DeepHouse, 0.80, 0x9999_AAAA);
        assert_ne!(
            b1, b2,
            "Expected generate_auto_beat to produce distinct grooves for different seeds"
        );

        let m1 = generate_melodic_tracks(Vibe::Synthwave, 9, 2, 0x1111_2222);
        let m2 = generate_melodic_tracks(Vibe::Synthwave, 9, 2, 0x9999_AAAA);
        assert_ne!(
            m1, m2,
            "Expected generate_melodic_tracks to produce distinct hooks/chords/bass across seeds"
        );

        let (fbass, fpad, farp, facid, _) = generate_melodic_tracks(Vibe::ElectroFunk, 4, 3, 0xF00D_5EED);
        assert!(
            fbass
                .steps
                .iter()
                .all(|s| !s.active || s.motion.resonance.unwrap_or(0.0) <= 0.25),
            "Expected ElectroFunk bass to avoid harsh resonant auto-wah MotionLocks"
        );
        assert!(
            fpad.steps.iter().any(|s| s.active && s.motion.any_locked()),
            "Expected Chords wah/comping MotionLocks"
        );
        assert!(
            farp.steps.iter().any(|s| s.active)
                && farp
                    .steps
                    .iter()
                    .all(|s| !s.active || (s.motion.cutoff.is_none() && s.motion.resonance.is_none())),
            "Expected active ElectroFunk lead without harsh auto-wah MotionLocks"
        );
        assert!(
            facid.steps.iter().any(|s| s.active && s.motion.any_locked()),
            "Expected 303 MotionLocks"
        );
        assert!(
            fbass.steps.iter().enumerate().any(|(i, s)| i % 2 == 1 && s.active),
            "Expected syncopated odd-16th funk bass hits"
        );
    }

    #[test]
    fn party_mode_changes_songs_vibes_rigs_and_timbres() {
        use crate::model::{apply_vibe_to_project, PartyMixMode};
        use std::collections::HashSet;

        let mut p = Project::default();
        apply_vibe_to_project(&mut p, Vibe::DeepHouse, None, 0x4242_1337);
        p.party_mode = true;
        p.party_auto_evolve = true;
        p.party_mix_mode = PartyMixMode::RelatedVibes;
        p.party_pace = PartyPace::Normal;
        p.party_sound_morph = true;
        p.party_energy = 0.82;

        let mut rt = Runtime::new(44_100.0);
        let mut tel = EngineTelemetry::default();

        let mut seen_songs = HashSet::new();
        let mut seen_vibes = HashSet::new();
        let mut seen_palettes = HashSet::new();
        let mut seen_templates = HashSet::new();
        let mut min_filter_hz = f32::MAX;
        let mut max_filter_hz = 0.0_f32;
        let mut min_bass_mul = f32::MAX;
        let mut max_bass_mul = 0.0_f32;

        for step in 0..1024 {
            rt.trigger_step(&p, false, false, &mut tel);
            rt.step = rt.step.wrapping_add(1);

            seen_songs.insert(tel.party_song_title.clone());
            seen_vibes.insert(tel.party_vibe_name.clone());
            seen_palettes.insert(tel.party_palette.clone());
            seen_templates.insert(rt.party_template_idx);
            min_filter_hz = min_filter_hz.min(rt.party_filter_target_hz);
            max_filter_hz = max_filter_hz.max(rt.party_filter_target_hz);
            min_bass_mul = min_bass_mul.min(rt.party_bass_cut_target);
            max_bass_mul = max_bass_mul.max(rt.party_bass_cut_target);

            if step % 64 == 0 {
                for _ in 0..128 {
                    let (l, r) = rt.process_sample(&p, true, false, false, 0, &mut tel);
                    assert!(l.is_finite() && r.is_finite(), "Non-finite sample at step {step}");
                    assert!(l.abs() <= 1.0 && r.abs() <= 1.0, "Clipping at step {step}");
                }
            }
        }

        assert!(
            seen_songs.len() >= 3,
            "Expected >= 3 songs over 64 bars, got {:?}",
            seen_songs
        );
        assert!(
            seen_vibes.len() >= 2,
            "Expected >= 2 related vibes over 64 bars, got {:?}",
            seen_vibes
        );
        assert!(
            seen_palettes.len() >= 2,
            "Expected >= 2 voice palettes over 64 bars, got {:?}",
            seen_palettes
        );
        assert!(
            seen_templates.len() >= 2,
            "Expected >= 2 arrangement templates over 64 bars, got {:?}",
            seen_templates
        );
        assert!(
            max_filter_hz - min_filter_hz > 5000.0,
            "Expected wide dynamic filter sweeps, got min={} max={}",
            min_filter_hz,
            max_filter_hz
        );
        assert!(
            max_bass_mul - min_bass_mul > 0.20,
            "Expected dynamic bass timbre breathing, got min={} max={}",
            min_bass_mul,
            max_bass_mul
        );
    }

    #[test]
    fn party_mode_single_vibe_varies_patterns_and_palettes() {
        use crate::model::{apply_vibe_to_project, PartyMixMode};
        use std::collections::HashSet;

        let mut p = Project::default();
        apply_vibe_to_project(&mut p, Vibe::Synthwave, None, 0xCAFE_5678);
        p.party_mode = true;
        p.party_auto_evolve = true;
        p.party_mix_mode = PartyMixMode::SingleVibe;
        p.party_pace = PartyPace::Normal;
        p.party_sound_morph = true;

        let mut rt = Runtime::new(44_100.0);
        let mut tel = EngineTelemetry::default();
        let mut seen_songs = HashSet::new();
        let mut seen_palettes = HashSet::new();
        let mut seen_vibes = HashSet::new();
        let mut seen_melodic = HashSet::new();

        for _ in 0..768 {
            rt.trigger_step(&p, false, false, &mut tel);
            rt.step = rt.step.wrapping_add(1);
            seen_songs.insert(tel.party_song_title.clone());
            seen_palettes.insert(tel.party_palette.clone());
            seen_vibes.insert(tel.party_vibe_name.clone());
            seen_melodic.insert(rt.party_melodic_sig);
        }

        assert_eq!(seen_vibes.len(), 1);
        assert!(seen_vibes.contains(Vibe::Synthwave.name()));
        assert!(seen_songs.len() >= 2, "Expected >= 2 songs, got {:?}", seen_songs);
        assert!(
            seen_palettes.len() >= 2,
            "Expected >= 2 palettes even in SingleVibe, got {:?}",
            seen_palettes
        );
        assert!(seen_melodic.len() >= 4);
    }

    #[test]
    fn party_mode_fast_pace_transitions_rapidly() {
        use crate::model::{apply_vibe_to_project, PartyMixMode};

        let mut p = Project::default();
        apply_vibe_to_project(&mut p, Vibe::ElectroFunk, None, 0xABCD_1234);
        p.party_mode = true;
        p.party_auto_evolve = true;
        p.party_mix_mode = PartyMixMode::AllVibes;
        p.party_pace = PartyPace::Fast;
        p.party_sound_morph = true;

        let mut rt = Runtime::new(44_100.0);
        let mut tel = EngineTelemetry::default();

        for _ in 0..256 {
            rt.trigger_step(&p, false, false, &mut tel);
            rt.step = rt.step.wrapping_add(1);
        }

        assert!(
            rt.party_cycle >= 1,
            "Expected Fast pace to transition to song #2 within 16 bars (256 steps), got cycle={}",
            rt.party_cycle
        );
    }

    fn render_rms_and_peak(p: &Project, bars: usize) -> (f32, f32) {
        let sr = 44_100.0_f32;
        let mut rt = Runtime::new(sr);
        let mut tel = EngineTelemetry::default();
        let total_secs = (bars as f32) * 4.0 * (60.0 / p.bpm.clamp(40.0, 240.0));
        let n = (total_secs * sr).round() as usize;
        let mut sum_sq = 0.0_f64;
        let mut peak = 0.0_f32;
        for _ in 0..n {
            let (l, r) = rt.process_sample(p, true, false, false, 0, &mut tel);
            assert!(l.is_finite() && r.is_finite(), "Non-finite sample");
            assert!(l.abs() <= 1.0 && r.abs() <= 1.0, "Sample clipped: ({l}, {r})");
            peak = peak.max(l.abs()).max(r.abs());
            sum_sq += (l as f64) * (l as f64) + (r as f64) * (r as f64);
        }
        let rms = (sum_sq / (n.max(1) as f64 * 2.0)).sqrt() as f32;
        (rms, peak)
    }

    #[test]
    fn party_mix_balance_keeps_cowbell_tamed_and_synths_audible() {
        use crate::model::{apply_vibe_to_project, PartyMixMode};

        // 1. Cowbell pocket check in ElectroFunk (audible accent, never overpowering Kick+Snare)
        let mut base = Project::default();
        apply_vibe_to_project(&mut base, Vibe::ElectroFunk, None, 0xFACE_B00C);
        base.party_mode = false;
        base.auto_beat_mode = false;
        for ch in &mut base.mixer.channels {
            ch.solo = false;
        }
        base.mixer.moog_pad_solo = false;
        base.mixer.channels[3].solo = true;

        let mut cowbell_only = base.clone();
        for (i, vp) in cowbell_only.drums.voices.iter_mut().enumerate() {
            vp.mute = i != 3;
        }
        let (cowbell_rms, _) = render_rms_and_peak(&cowbell_only, 4);

        let mut kick_snare = base.clone();
        for (i, vp) in kick_snare.drums.voices.iter_mut().enumerate() {
            vp.mute = i != 0 && i != 1;
        }
        let (kick_snare_rms, _) = render_rms_and_peak(&kick_snare, 4);

        assert!(
            cowbell_rms > 0.001,
            "Expected cowbell pocket hits to remain audible, got cowbell_rms={cowbell_rms}"
        );
        assert!(
            cowbell_rms < kick_snare_rms * 0.45,
            "Cowbell overpowered kick+snare: cowbell_rms={cowbell_rms}, kick_snare_rms={kick_snare_rms}"
        );

        // 2. Party DJ Mode 8-bar mix hierarchy across ElectroFunk & DeepHouse
        for &vibe in &[Vibe::ElectroFunk, Vibe::DeepHouse] {
            let mut p = Project::default();
            apply_vibe_to_project(&mut p, vibe, None, 0x5555_7777);
            p.party_mode = true;
            p.party_auto_evolve = true;
            p.party_mix_mode = PartyMixMode::RelatedVibes;
            p.party_pace = PartyPace::Fast;
            p.party_sound_morph = true;
            p.party_energy = 0.85;

            let mut solo_drums = p.clone();
            solo_drums.mixer.channels[3].solo = true;
            let (drums_rms, _) = render_rms_and_peak(&solo_drums, 8);

            let mut solo_bass = p.clone();
            solo_bass.mixer.channels[0].solo = true;
            let (bass_rms, _) = render_rms_and_peak(&solo_bass, 8);

            let mut solo_melodic = p.clone();
            solo_melodic.mixer.moog_pad_solo = true;
            solo_melodic.mixer.channels[1].solo = true;
            solo_melodic.mixer.channels[2].solo = true;
            solo_melodic.mixer.channels[4].solo = true;
            let (melodic_rms, _) = render_rms_and_peak(&solo_melodic, 8);

            let mut solo_chords = p.clone();
            solo_chords.mixer.moog_pad_solo = true;
            let (chords_rms, _) = render_rms_and_peak(&solo_chords, 8);

            let mut solo_lead = p.clone();
            solo_lead.mixer.channels[1].solo = true;
            let (lead_rms, _) = render_rms_and_peak(&solo_lead, 8);

            let (_full_rms, full_peak) = render_rms_and_peak(&p, 8);

            assert!(
                melodic_rms >= drums_rms * 0.65,
                "[{:?}] Synths too quiet vs drums: melodic_rms={melodic_rms}, drums_rms={drums_rms}",
                vibe
            );
            assert!(
                melodic_rms >= bass_rms * 0.70,
                "[{:?}] Synths too quiet vs bass: melodic_rms={melodic_rms}, bass_rms={bass_rms}",
                vibe
            );
            assert!(
                chords_rms > 0.04,
                "[{:?}] Chords layer too faint: chords_rms={chords_rms}",
                vibe
            );
            assert!(
                lead_rms > 0.04,
                "[{:?}] Arp/Lead layer too faint: lead_rms={lead_rms}",
                vibe
            );
            assert!(
                full_peak <= 1.0,
                "[{:?}] Full mix clipped: peak={full_peak}",
                vibe
            );
        }
    }

    #[test]
    fn per_instrument_filter_shapes_individual_track_without_affecting_others() {
        use crate::model::{apply_vibe_to_project, TrackFilterParams};

        let mut base = Project::default();
        apply_vibe_to_project(&mut base, Vibe::ElectroFunk, None, 0x1111_2222);
        base.party_mode = false;
        base.auto_beat_mode = false;
        for ch in &mut base.mixer.channels {
            ch.solo = false;
        }
        base.mixer.moog_pad_solo = false;

        let mut drums_open = base.clone();
        drums_open.mixer.channels[3].solo = true;
        let (drums_open_rms, _) = render_rms_and_peak(&drums_open, 4);

        let mut drums_filtered = drums_open.clone();
        drums_filtered.mixer.channels[3].filter = TrackFilterParams {
            enabled: true,
            mode: GlobalFilterMode::HighPass,
            cutoff: 3_500.0,
            resonance: 0.02,
        };
        let (drums_hp_rms, _) = render_rms_and_peak(&drums_filtered, 4);

        assert!(
            drums_hp_rms < drums_open_rms * 0.65,
            "Expected per-instrument HP filter to trim drum body/sub significantly: open={drums_open_rms}, filtered={drums_hp_rms}"
        );

        let mut bass_open = base.clone();
        bass_open.mixer.channels[0].solo = true;
        let (bass_open_rms, _) = render_rms_and_peak(&bass_open, 4);

        let mut bass_with_drums_filtered = bass_open.clone();
        bass_with_drums_filtered.mixer.channels[3].filter = drums_filtered.mixer.channels[3].filter;
        let (bass_other_rms, _) = render_rms_and_peak(&bass_with_drums_filtered, 4);

        assert!(
            (bass_open_rms - bass_other_rms).abs() < 1e-4,
            "Drums filter leaked into Bass track: bass_open={bass_open_rms}, bass_after={bass_other_rms}"
        );
    }

    #[test]
    fn global_filter_envelope_modulates_cutoff_cleanly() {
        use crate::model::{apply_vibe_to_project, FilterEnvTrigger};

        let mut p = Project::default();
        apply_vibe_to_project(&mut p, Vibe::ElectroFunk, None, 0x2222_3333);
        p.party_mode = false;
        // Solo rich MoogPad chords so static 160Hz LP heavily attenuates harmonics while Beat Env opens it
        for ch in &mut p.mixer.channels {
            ch.solo = false;
        }
        p.mixer.moog_pad_solo = true;
        p.moog_pad.cutoff = 8_000.0;
        p.moog_pad.resonance = 0.1;
        p.fx.global_filter_enabled = true;
        p.fx.global_filter_mode = GlobalFilterMode::LowPass;
        p.fx.global_filter_cutoff = 160.0;
        p.fx.global_filter_resonance = 0.02;
        p.fx.global_filter_env_trigger = FilterEnvTrigger::Beat;
        p.fx.global_filter_env_attack = 0.005;
        p.fx.global_filter_env_decay = 0.45;
        p.fx.global_filter_env_amount = 0.0;

        let (static_rms, _) = render_rms_and_peak(&p, 2);

        p.fx.global_filter_env_amount = 0.95;
        let sr = 44_100.0_f32;
        let mut rt = Runtime::new(sr);
        let mut tel = EngineTelemetry::default();
        let total_secs = 2.0 * 4.0 * (60.0 / p.bpm.clamp(40.0, 240.0));
        let n = (total_secs * sr).round() as usize;
        let mut sum_sq = 0.0_f64;
        let mut max_env = 0.0_f32;
        for _ in 0..n {
            let (l, r) = rt.process_sample(&p, true, false, false, 0, &mut tel);
            assert!(l.is_finite() && r.is_finite(), "Non-finite sample under global filter env");
            assert!(l.abs() <= 1.0 && r.abs() <= 1.0, "Clipping under global filter env");
            max_env = max_env.max(tel.global_filter_env_level);
            sum_sq += (l as f64) * (l as f64) + (r as f64) * (r as f64);
        }
        let env_rms = (sum_sq / (n.max(1) as f64 * 2.0)).sqrt() as f32;

        assert!(max_env > 0.65, "Expected global filter env to fire above 0.65, got {max_env}");
        assert!(
            env_rms > static_rms * 1.35,
            "Expected positive filter envelope to sweep LP brighter than static 160Hz: static={static_rms}, env={env_rms}"
        );
    }

    #[test]
    fn bass_drop_macro_progresses_build_gap_impact_and_auto_drop() {
        use crate::model::apply_vibe_to_project;
        use std::collections::HashSet;

        let mut p = Project::default();
        apply_vibe_to_project(&mut p, Vibe::BigRoomEdm, None, 0x3333_4444);
        p.party_mode = false;
        p.drop_macro.build_bars = 1;
        p.drop_macro.pre_drop_gap_steps = 2;
        p.drop_macro.cut_drums = true;
        p.drop_macro.cut_bass = true;
        p.drop_macro.drum_accel = true;
        p.drop_macro.riser_sweep = true;
        p.drop_macro.impact_intensity = 0.92;
        p.drop_macro.auto_drop_bars = 0;

        let sr = 44_100.0_f32;
        let mut rt = Runtime::new(sr);
        let mut tel = EngineTelemetry::default();
        // Enter playing state before triggering live Drop Macro
        let _ = rt.process_sample(&p, true, false, false, 0, &mut tel);
        rt.handle_live_event(LiveEvent::TriggerBassDrop, &p);

        let two_bars_secs = 2.0 * 4.0 * (60.0 / p.bpm.clamp(40.0, 240.0));
        let n = (two_bars_secs * sr).round() as usize;
        let mut phases = HashSet::new();
        let mut impact_peak = 0.0_f32;
        for _ in 0..n {
            let (l, r) = rt.process_sample(&p, true, false, false, 0, &mut tel);
            assert!(l.is_finite() && r.is_finite(), "Non-finite sample during drop macro");
            assert!(l.abs() <= 1.0 && r.abs() <= 1.0, "Clipping during drop macro");
            if !tel.drop_phase.is_empty() {
                phases.insert(tel.drop_phase.clone());
            }
            if tel.drop_phase == "DROP!" {
                impact_peak = impact_peak.max(l.abs()).max(r.abs());
            }
        }

        assert!(phases.contains("BUILD"), "Expected BUILD phase, got {:?}", phases);
        assert!(phases.contains("HOLD..."), "Expected HOLD... pre-drop gap phase, got {:?}", phases);
        assert!(phases.contains("DROP!"), "Expected DROP! impact phase, got {:?}", phases);
        assert!(impact_peak > 0.25, "Expected strong impact transient, got {impact_peak}");

        // Part B: Verify Auto-Drop fires automatically across 9 bars when set to 8 bars
        let mut p_auto = p.clone();
        p_auto.drop_macro.auto_drop_bars = 8;
        p_auto.drop_macro.build_bars = 1;
        let mut rt_auto = Runtime::new(sr);
        let mut tel_auto = EngineTelemetry::default();
        let mut auto_phases = HashSet::new();
        for _ in 0..(9 * 16) {
            rt_auto.trigger_step(&p_auto, false, false, &mut tel_auto);
            rt_auto.step = rt_auto.step.wrapping_add(1);
            let (_, _) = rt_auto.process_sample(&p_auto, true, false, false, 0, &mut tel_auto);
            if !tel_auto.drop_phase.is_empty() {
                auto_phases.insert(tel_auto.drop_phase.clone());
            }
        }
        assert!(
            auto_phases.contains("BUILD") && auto_phases.contains("DROP!"),
            "Expected Auto-Drop 8B to cycle BUILD -> DROP!, got {:?}",
            auto_phases
        );
    }

    #[test]
    fn pitched_volca_sample_renders_clean_audio_across_transpositions() {
        let mut p = Project::default();
        p.party_mode = false;
        p.mixer.channels[TrackId::Sampler.index()].mute = false;
        p.sample_parts[0].mute = false;

        let sr = 44_100.0_f32;
        for &pitch_offset in &[-12_i8, 0, 12] {
            let mut rt = Runtime::new(sr);
            let mut tel = EngineTelemetry::default();
            rt.handle_live_event(
                LiveEvent::TriggerSamplePartPitched {
                    part: 0,
                    velocity: 0.92,
                    pitch_offset,
                },
                &p,
            );
            let mut sum_sq = 0.0_f64;
            let n = 4096;
            for _ in 0..n {
                let (l, r) = rt.process_sample(&p, false, false, false, 0, &mut tel);
                assert!(l.is_finite() && r.is_finite(), "Non-finite pitched sample output at {pitch_offset}");
                assert!(l.abs() <= 1.0 && r.abs() <= 1.0, "Clipping pitched sample output at {pitch_offset}");
                sum_sq += (l as f64) * (l as f64) + (r as f64) * (r as f64);
            }
            let rms = (sum_sq / (n as f64 * 2.0)).sqrt() as f32;
            assert!(rms > 0.001, "Expected audible pitched sample at {pitch_offset}st, got rms={rms}");
        }
    }

    #[test]
    fn suppress_track_step_prevents_double_trigger_on_target_only() {
        use crate::model::{FactoryPreset, StepCount};

        let mut p = Project::factory(FactoryPreset::BlankInit);
        p.party_mode = false;
        let pat = p.current_pattern_mut();
        pat.monologue.step_count = StepCount::Steps16;
        pat.monologue.steps[0].active = true;
        pat.monologue.steps[0].note = 60;
        pat.monologue.steps[1].active = true;
        pat.monologue.steps[1].note = 64;
        pat.moog.step_count = StepCount::Steps16;
        pat.moog.steps[1].active = true;
        pat.moog.steps[1].note = 43;

        let mut rt = Runtime::new(44_100.0);
        let mut tel = EngineTelemetry::default();

        // Step 0 triggers Monologue 60
        rt.step = 0;
        rt.trigger_step(&p, false, false, &mut tel);
        assert_eq!(rt.last_mono_note, Some(60));

        // Suppress Monologue step 1 only
        rt.handle_live_event(
            LiveEvent::SuppressTrackStep {
                track: Some(TrackId::Monologue),
                extra_idx: None,
                lane: None,
                global_step: 1,
            },
            &p,
        );
        rt.step = 1;
        rt.trigger_step(&p, false, false, &mut tel);
        assert_eq!(rt.last_mono_note, Some(60), "Monologue step 1 should be suppressed");
        assert_eq!(rt.last_moog_note, Some(43), "Moog step 1 should still trigger normally");

        // Next loop wrap (step 17 = step 1) should trigger Monologue 64 normally
        rt.step = 17;
        rt.trigger_step(&p, false, false, &mut tel);
        assert_eq!(rt.last_mono_note, Some(64));
    }

    #[test]
    fn pattern_length_16_and_32_loop_at_step_count() {
        use crate::model::{FactoryPreset, StepCount};

        let mut p = Project::factory(FactoryPreset::BlankInit);
        p.party_mode = false;
        {
            let pat = p.current_pattern_mut();
            pat.monologue.step_count = StepCount::Steps16;
            pat.monologue.steps[0].active = true;
            pat.monologue.steps[0].note = 60;
            pat.monologue.steps[16].active = true;
            pat.monologue.steps[16].note = 67;
        }

        let mut rt = Runtime::new(44_100.0);
        let mut tel = EngineTelemetry::default();
        rt.step = 16;
        rt.trigger_step(&p, false, false, &mut tel);
        assert_eq!(rt.last_mono_note, Some(60), "16-step pattern should wrap step 16 to step 0");

        p.current_pattern_mut().monologue.step_count = StepCount::Steps32;
        rt.step = 16;
        rt.trigger_step(&p, false, false, &mut tel);
        assert_eq!(rt.last_mono_note, Some(67), "32-step pattern should play step 16");

        rt.step = 32;
        rt.trigger_step(&p, false, false, &mut tel);
        assert_eq!(rt.last_mono_note, Some(60), "32-step pattern should wrap step 32 to step 0");
    }

    #[test]
    fn modern_crisp_enhances_highs_and_imperial_bass_renders_clean_punch() {
        use crate::dsp::MasterFx;
        use crate::model::{apply_vibe_to_project, FactoryPreset};

        assert_eq!(
            FactoryPreset::from_slug("imperial"),
            Some(FactoryPreset::ImperialBass)
        );
        let imp_proj = Project::factory(FactoryPreset::ImperialBass);
        assert!((imp_proj.bpm - 126.0).abs() < 0.01);
        assert_eq!(imp_proj.root_note, 4);

        let mut base = Project::default();
        apply_vibe_to_project(&mut base, Vibe::ImperialBass, None, 0xA9A5_4E12);
        base.party_mode = false;
        base.fx.analog_warmth = 0.0;

        let render_hf = |crisp: f32| -> (f32, f32, f32, f32) {
            let mut p = base.clone();
            p.fx.master_crisp = crisp;
            let sr = 44_100.0_f32;
            let mut rt = Runtime::new(sr);
            let mut tel = EngineTelemetry::default();
            let total_secs = 2.0 * 4.0 * (60.0 / p.bpm);
            let n = (total_secs * sr).round() as usize;
            let mut sum_sq = 0.0_f64;
            let mut hf_sq = 0.0_f64;
            let mut peak = 0.0_f32;
            let mut stereo_diff = 0.0_f32;
            let mut prev_m = 0.0_f32;
            for _ in 0..n {
                let (l, r) = rt.process_sample(&p, true, false, false, 0, &mut tel);
                assert!(l.is_finite() && r.is_finite(), "Non-finite sample in ImperialBass");
                assert!(l.abs() <= 1.0 && r.abs() <= 1.0, "Clipping in ImperialBass");
                peak = peak.max(l.abs()).max(r.abs());
                stereo_diff = stereo_diff.max((l - r).abs());
                let m = 0.5 * (l + r);
                let dm = m - prev_m;
                prev_m = m;
                sum_sq += (l as f64) * (l as f64) + (r as f64) * (r as f64);
                hf_sq += (dm as f64) * (dm as f64);
            }
            let rms = (sum_sq / (n.max(1) as f64 * 2.0)).sqrt() as f32;
            let hf_rms = (hf_sq / n.max(1) as f64).sqrt() as f32;
            (rms, peak, stereo_diff, hf_rms)
        };

        let (_, _, _, hf_flat) = render_hf(0.0);
        let (rms_crisp, peak_crisp, stereo_crisp, hf_crisp) = render_hf(0.90);
        assert!(rms_crisp > 0.08, "Expected strong audible RMS for ImperialBass, got {rms_crisp}");
        assert!(peak_crisp <= 1.0, "ImperialBass clipped at peak={peak_crisp}");
        assert!(stereo_crisp > 0.01, "Expected stereo width for ImperialBass, got {stereo_crisp}");
        assert!(
            hf_crisp > hf_flat * 1.15,
            "Expected master_crisp=0.90 to boost high-frequency crispness: flat={hf_flat}, crisp={hf_crisp}"
        );

        // Verify < 125 Hz sub-bass side energy is mono-tightened by master_crisp
        let measure_side_sub = |crisp: f32| -> f32 {
            let sr = 44_100.0_f32;
            let mut fx = MasterFx::new(sr);
            let mut params = base.fx.clone();
            params.delay_enabled = false;
            params.reverb_enabled = false;
            params.global_filter_enabled = false;
            params.analog_warmth = 0.0;
            params.master_crisp = crisp;
            let n = 22_050usize;
            let sub_a = (std::f32::consts::TAU * 125.0 / sr).clamp(0.001, 0.25);
            let mut side_lp = 0.0_f32;
            let mut sq_sum = 0.0_f64;
            let mut count = 0usize;
            for i in 0..n {
                let t = i as f32 / sr;
                let sub = (t * 60.0 * std::f32::consts::TAU).sin() * 0.45;
                let (ol, or) = fx.process_stereo(sub, 0.0, 0.0, 0.0, 0.0, 0.0, &params, 126.0);
                let side = 0.5 * (ol - or);
                side_lp += sub_a * (side - side_lp);
                if i >= n / 2 {
                    sq_sum += (side_lp as f64) * (side_lp as f64);
                    count += 1;
                }
            }
            (sq_sum / count.max(1) as f64).sqrt() as f32
        };
        let side_sub_flat = measure_side_sub(0.0);
        let side_sub_crisp = measure_side_sub(0.90);
        assert!(
            side_sub_crisp < side_sub_flat * 0.35,
            "Expected master_crisp=0.90 to mono-tighten <125 Hz sub: flat={side_sub_flat}, crisp={side_sub_crisp}"
        );

        // Soloed Imperial 909 drum beat renders cleanly and audibly
        let mut drums_only = base;
        for ch in &mut drums_only.mixer.channels {
            ch.solo = false;
        }
        drums_only.mixer.moog_pad_solo = false;
        drums_only.mixer.channels[3].solo = true;
        let (drums_rms, drums_peak) = render_rms_and_peak(&drums_only, 2);
        assert!(drums_rms > 0.02, "Expected audible Imperial 909 drums, got {drums_rms}");
        assert!(drums_peak <= 1.0, "Imperial 909 drums clipped at {drums_peak}");
    }

    #[test]
    fn eurobeat_bass_has_no_theremin_glide_or_high_res_autowah_in_studio_and_party_dj() {
        use crate::model::{apply_vibe_to_project, generate_melodic_tracks, PartyMixMode};

        let mut p = Project::default();
        apply_vibe_to_project(&mut p, Vibe::Eurobeat, None, 0xE0A0_8EA7);
        assert_eq!(p.voice_presets[1], 12, "Eurobeat should default to Eurobeat Octave");
        assert!(p.moog.glide == 0.0, "Eurobeat bass should have zero glide");
        assert!(
            p.moog.resonance <= 0.25,
            "Eurobeat bass resonance should stay clean (got {})",
            p.moog.resonance
        );

        for seed_i in 0..10u32 {
            let (bass, _, _, acid, _) =
                generate_melodic_tracks(Vibe::Eurobeat, 2, 20, 0xE0A0_0001 + seed_i);
            for (si, st) in bass.steps.iter().enumerate() {
                if st.active {
                    assert!(
                        !st.slide,
                        "Eurobeat bass step {si} (seed {seed_i}) should not slide"
                    );
                    if let Some(res) = st.motion.resonance {
                        assert!(
                            res <= 0.40,
                            "Eurobeat bass step {si} (seed {seed_i}) had high auto-wah resonance {res}"
                        );
                    }
                }
            }
            for (si, st) in acid.steps.iter().enumerate() {
                if st.active {
                    assert!(
                        !st.slide,
                        "Eurobeat 303 counter step {si} should not slide"
                    );
                }
            }
        }

        // Direct MoogSynth mono check: 12-semitone jump snaps instantaneously when glide == 0.0 and slide == false
        let mut moog = MoogSynth::new(44_100.0);
        for _ in 0..512 {
            moog.note_on(38, 0.9, false, false, &p.moog);
            let _ = moog.process_sample(&p.moog);
            break;
        }
        for _ in 0..512 {
            let _ = moog.process_sample(&p.moog);
        }
        moog.note_on(50, 0.9, false, false, &p.moog);
        let _ = moog.process_sample(&p.moog);
        assert!(
            (moog.current_mono_midi() - 50.0).abs() < 1e-4,
            "Expected instant octave snap in mono mode, got {}",
            moog.current_mono_midi()
        );

        // Render 4 bars of Eurobeat in Studio mode
        let mut studio_p = p.clone();
        studio_p.party_mode = false;
        let (studio_rms, studio_peak) = render_rms_and_peak(&studio_p, 4);
        assert!(studio_rms > 0.04, "Eurobeat Studio RMS too low: {studio_rms}");
        assert!(studio_peak <= 1.0, "Eurobeat Studio clipped at {studio_peak}");

        // Render 8 bars of Eurobeat in Party DJ mode (SingleVibe)
        let mut dj_p = p;
        dj_p.party_mode = true;
        dj_p.party_mix_mode = PartyMixMode::SingleVibe;
        dj_p.party_sound_morph = true;
        dj_p.party_pace = PartyPace::Fast;
        dj_p.party_energy = 0.85;
        let mut rt = Runtime::new(44_100.0);
        let mut tel = EngineTelemetry::default();
        let total_samples = (8.0 * 4.0 * (60.0 / dj_p.bpm) * 44_100.0).round() as usize;
        let mut sq = 0.0_f64;
        for i in 0..total_samples {
            let (l, r) = rt.process_sample(&dj_p, true, false, false, 0, &mut tel);
            assert!(l.is_finite() && r.is_finite(), "Non-finite Party DJ Eurobeat sample");
            assert!(l.abs() <= 1.0 && r.abs() <= 1.0, "Party DJ Eurobeat clipped");
            sq += (l as f64) * (l as f64) + (r as f64) * (r as f64);
            if i % 2048 == 0 {
                assert_eq!(rt.party_glitch_fx, 0.0, "Eurobeat should not inject bitcrush glitch FX");
                if let Some(rig) = &rt.party_rig {
                    assert!(
                        rig.voice_presets[1] != 7 && rig.voice_presets[1] != 9,
                        "Eurobeat DJ pool must not pick Acid Growl or Dubstep Wobble (got {})",
                        rig.voice_presets[1]
                    );
                }
            }
        }
        let dj_rms = (sq / (total_samples as f64 * 2.0)).sqrt() as f32;
        assert!(dj_rms > 0.04, "Eurobeat Party DJ RMS too low: {dj_rms}");
    }

    #[test]
    fn party_dj_reload_rig_and_set_instrument_swap_live_voices() {
        use crate::model::{apply_vibe_to_project, PartyMixMode};

        let mut p = Project::default();
        apply_vibe_to_project(&mut p, Vibe::Eurobeat, None, 0xBEEF_4242);
        p.party_mode = true;
        p.party_sound_morph = true;
        p.party_mix_mode = PartyMixMode::SingleVibe;

        let mut rt = Runtime::new(44_100.0);
        let mut tel = EngineTelemetry::default();

        // Warm up playback so party_rig initializes on party_cycle == 0
        for _ in 0..256 {
            let _ = rt.process_sample(&p, true, false, false, 0, &mut tel);
        }
        assert_eq!(rt.party_cycle, 0);
        assert!(rt.party_rig.is_some());

        // SetPartyInstrument immediately swaps Bass to OB-X Power Bass (4)
        rt.handle_live_event(
            LiveEvent::SetPartyInstrument {
                track: TrackId::Moog,
                preset_idx: 4,
            },
            &p,
        );
        let mut sq = 0.0_f64;
        for _ in 0..256 {
            let (l, r) = rt.process_sample(&p, true, false, false, 0, &mut tel);
            assert!(l.is_finite() && r.is_finite());
            sq += (l as f64) * (l as f64) + (r as f64) * (r as f64);
        }
        assert!(sq > 0.001, "Expected audible output after SetPartyInstrument");
        assert_eq!(rt.party_rig.as_ref().unwrap().voice_presets[1], 4);
        assert_eq!(tel.party_voice_presets[1], 4);
        assert!(
            tel.party_palette.contains("OB-X Power Bass"),
            "Expected telemetry palette to reflect OB-X Power Bass, got '{}'",
            tel.party_palette
        );

        // ReloadPartyRig swaps instruments immediately on party_cycle == 0 without advancing party_cycle
        let before_presets = rt.party_rig.as_ref().unwrap().voice_presets;
        rt.handle_live_event(LiveEvent::ReloadPartyRig, &p);
        for _ in 0..256 {
            let (l, r) = rt.process_sample(&p, true, false, false, 0, &mut tel);
            assert!(l.is_finite() && r.is_finite());
        }
        let after_presets = rt.party_rig.as_ref().unwrap().voice_presets;
        assert_eq!(rt.party_cycle, 0);
        assert_ne!(
            after_presets, before_presets,
            "Expected ReloadPartyRig to swap rig presets"
        );
        assert_eq!(tel.party_voice_presets, after_presets);
    }

    #[test]
    fn drum_tuning_uses_semitones_and_kicks_hit_club_sub_band() {
        use crate::model::{apply_voice_preset, DrumVoiceId};

        let sr = 44_100.0_f32;
        let measure_kick_hz = |preset_idx: usize| -> f32 {
            let mut p = Project::default();
            apply_voice_preset(&mut p, TrackId::Drums, preset_idx);
            let mut dm = DrumMachine::new(sr);
            dm.trigger(DrumVoiceId::Kick, 1.0, false, false, &p.drums);

            let start = (0.070 * sr) as usize;
            let end = (0.190 * sr) as usize;
            let mut prev = 0.0_f32;
            let mut rising_crossings = Vec::new();
            for i in 0..end {
                let (l, r) = dm.process_sample(&p.drums);
                let m = 0.5 * (l + r);
                if i >= start && prev <= 0.0 && m > 0.0 {
                    rising_crossings.push(i);
                }
                prev = m;
            }
            assert!(
                rising_crossings.len() >= 2,
                "Expected >=2 positive zero crossings in kick body for preset {preset_idx}, got {}",
                rising_crossings.len()
            );
            let span = (*rising_crossings.last().unwrap() - rising_crossings[0]) as f32;
            let cycles = (rising_crossings.len() - 1) as f32;
            cycles * sr / span
        };

        let hz_808_warm = measure_kick_hz(0); // tune = -2.0 st on 47 Hz -> ~41.9 Hz
        let hz_909_club = measure_kick_hz(1); // tune = +1.0 st on 52.5 Hz -> ~55.6 Hz
        let hz_trap_808 = measure_kick_hz(8); // tune = -3.0 st on 47 Hz -> ~39.5 Hz

        assert!(
            (36.0..=48.0).contains(&hz_808_warm),
            "808 Warm kick should hit 36..48 Hz club sub band, got {hz_808_warm:.2} Hz"
        );
        assert!(
            (49.0..=62.0).contains(&hz_909_club),
            "909 Club kick should hit 49..62 Hz club punch band, got {hz_909_club:.2} Hz"
        );
        assert!(
            (34.0..=45.0).contains(&hz_trap_808),
            "Trap 808 kick should hit 34..45 Hz sub band, got {hz_trap_808:.2} Hz"
        );
    }

    #[test]
    fn analog_filters_and_bbd_chorus_retain_bass_and_widen_cleanly() {
        use crate::dsp::{AnalogStereoChorus, DiodeLadder303Filter, MoogLadderFilter};

        let sr = 44_100.0_f32;
        let n = 16_384_usize;

        // 1. MoogLadderFilter retains >= 65% of 80 Hz passband energy at res=0.72
        let measure_moog_80hz = |res: f32| -> f32 {
            let mut f = MoogLadderFilter::new(sr);
            let mut sq = 0.0_f64;
            for i in 0..n {
                let t = i as f32 / sr;
                let s = (t * 80.0 * std::f32::consts::TAU).sin() * 0.5;
                let y = f.process(s, 650.0, res);
                assert!(y.is_finite());
                if i >= n / 2 {
                    sq += (y as f64) * (y as f64);
                }
            }
            (sq / (n / 2) as f64).sqrt() as f32
        };
        let rms_dry = measure_moog_80hz(0.0);
        let rms_res = measure_moog_80hz(0.72);
        assert!(
            rms_res >= rms_dry * 0.65,
            "MoogLadderFilter lost too much 80Hz bass at res=0.72: dry={rms_dry}, res={rms_res}"
        );

        // 2. DiodeLadder303Filter boosts resonant acid peak around cutoff while keeping fundamental
        let measure_303_cutoff_rms = |res: f32| -> f32 {
            let mut f = DiodeLadder303Filter::new(sr);
            let mut sq = 0.0_f64;
            for i in 0..n {
                let t = i as f32 / sr;
                let saw = (t * 110.0).fract() * 2.0 - 1.0;
                let y = f.process(saw * 0.45, 720.0, res);
                assert!(y.is_finite());
                if i >= n / 2 {
                    sq += (y as f64) * (y as f64);
                }
            }
            (sq / (n / 2) as f64).sqrt() as f32
        };
        let d_flat = measure_303_cutoff_rms(0.05);
        let d_squelch = measure_303_cutoff_rms(0.85);
        assert!(
            d_squelch > d_flat * 1.10,
            "DiodeLadder303Filter should deliver stronger resonant acid energy: flat={d_flat}, squelch={d_squelch}"
        );

        // 3. AnalogStereoChorus widens mono cleanly with negligible DC offset
        let mut chorus = AnalogStereoChorus::new(sr);
        let mut side_sq = 0.0_f64;
        let mut sum_l = 0.0_f64;
        let mut sum_r = 0.0_f64;
        for i in 0..n {
            let t = i as f32 / sr;
            let mono = (t * 220.0 * std::f32::consts::TAU).sin() * 0.45
                + (t * 440.0 * std::f32::consts::TAU).sin() * 0.25;
            let (l, r) = chorus.process_mono(mono, 0.70);
            assert!(l.is_finite() && r.is_finite());
            if i >= n / 4 {
                let side = 0.5 * (l - r);
                side_sq += (side as f64) * (side as f64);
                sum_l += l as f64;
                sum_r += r as f64;
            }
        }
        let count = (n - n / 4) as f64;
        let side_rms = (side_sq / count).sqrt() as f32;
        let dc_l = (sum_l / count).abs() as f32;
        let dc_r = (sum_r / count).abs() as f32;
        assert!(side_rms > 0.08, "AnalogStereoChorus side_rms too narrow: {side_rms}");
        assert!(dc_l < 0.01 && dc_r < 0.01, "AnalogStereoChorus DC offset too high: ({dc_l}, {dc_r})");
    }

    #[test]
    fn club_heft_mastering_delivers_fat_sub_stereo_width_and_loudness() {
        use crate::model::apply_vibe_to_project;

        let sr = 44_100.0_f32;
        let vibes = [
            Vibe::DeepHouse,
            Vibe::AcidClub,
            Vibe::BigRoomEdm,
            Vibe::Trap808,
            Vibe::ImperialBass,
            Vibe::ElectroFunk,
        ];
        for &vibe in &vibes {
            let mut p = Project::default();
            apply_vibe_to_project(&mut p, vibe, None, 0xC10B_BEEF);
            p.party_mode = false;

            let mut rt = Runtime::new(sr);
            let mut tel = EngineTelemetry::default();
            let total_secs = 4.0 * 4.0 * (60.0 / p.bpm.clamp(40.0, 240.0));
            let n = (total_secs * sr).round() as usize;

            let mut sub_lp = Svf12Filter::new(sr);
            let mut sub_hp = Svf12Filter::new(sr);
            let mut sum_sq = 0.0_f64;
            let mut sub_sq = 0.0_f64;
            let mut side_sq = 0.0_f64;
            let mut peak = 0.0_f32;

            for _ in 0..n {
                let (l, r) = rt.process_sample(&p, true, false, false, 0, &mut tel);
                assert!(l.is_finite() && r.is_finite(), "[{:?}] Non-finite sample", vibe);
                assert!(l.abs() <= 0.98 && r.abs() <= 0.98, "[{:?}] Clipping: ({l}, {r})", vibe);
                peak = peak.max(l.abs()).max(r.abs());
                sum_sq += (l as f64) * (l as f64) + (r as f64) * (r as f64);
                let mid = 0.5 * (l + r);
                let side = 0.5 * (l - r);
                side_sq += (side as f64) * (side as f64);
                let bp_sub = sub_hp.process_hp(sub_lp.process_lp(mid, 120.0, 0.05), 35.0, 0.05);
                sub_sq += (bp_sub as f64) * (bp_sub as f64);
            }
            let rms = (sum_sq / (n.max(1) as f64 * 2.0)).sqrt() as f32;
            let sub_rms = (sub_sq / n.max(1) as f64).sqrt() as f32;
            let side_rms = (side_sq / n.max(1) as f64).sqrt() as f32;

            assert!(rms >= 0.14, "[{:?}] Club loudness RMS too low: {rms}", vibe);
            assert!(sub_rms >= 0.055, "[{:?}] Club sub-bass RMS (35..120Hz) too thin: {sub_rms}", vibe);
            assert!(side_rms >= 0.03, "[{:?}] Stereo width side_rms too narrow: {side_rms}", vibe);
        }
    }

    #[test]
    fn studio_drums_and_analog_synths_have_wide_dynamic_and_spectral_range() {
        use crate::model::{apply_voice_preset, DrumVoiceId};

        let sr = 44_100.0_f32;

        // 1. 909/Linn Snare & Closed Hat have crisp broadband energy + punchy dynamic crest factor (>= 2.2)
        let mut p = Project::default();
        apply_voice_preset(&mut p, TrackId::Drums, 4); // 80s Linn Gate
        let mut dm = DrumMachine::new(sr);
        dm.trigger(DrumVoiceId::Snare, 1.0, false, false, &p.drums);

        let n_snare = (0.35 * sr) as usize;
        let mut sq = 0.0_f64;
        let mut hf_sq = 0.0_f64;
        let mut peak = 0.0_f32;
        let mut hp_lp = 0.0_f32;
        let hp_a = (std::f32::consts::TAU * 4_500.0 / sr).clamp(0.01, 0.8);
        for _ in 0..n_snare {
            let (l, r) = dm.process_sample(&p.drums);
            assert!(l.is_finite() && r.is_finite());
            let m = 0.5 * (l + r);
            peak = peak.max(l.abs()).max(r.abs());
            sq += (m as f64) * (m as f64);
            hp_lp += hp_a * (m - hp_lp);
            let hf = m - hp_lp;
            hf_sq += (hf as f64) * (hf as f64);
        }
        let snare_rms = (sq / n_snare as f64).sqrt() as f32;
        let snare_hf_rms = (hf_sq / n_snare as f64).sqrt() as f32;
        let crest = peak / snare_rms.max(1e-6);
        assert!(crest >= 2.2, "Expected punchy studio snare crest factor >= 2.2, got {crest:.2}");
        assert!(snare_hf_rms > snare_rms * 0.25, "Expected crisp broadband snare highs: hf={snare_hf_rms}, total={snare_rms}");

        let mut dm_hat = DrumMachine::new(sr);
        dm_hat.trigger(DrumVoiceId::ClosedHat, 1.0, false, false, &p.drums);
        let n_hat = (0.15 * sr) as usize;
        let mut hat_hf_sq = 0.0_f64;
        let mut hat_hp_lp = 0.0_f32;
        for _ in 0..n_hat {
            let (l, r) = dm_hat.process_sample(&p.drums);
            let m = 0.5 * (l + r);
            hat_hp_lp += hp_a * (m - hat_hp_lp);
            let hf = m - hat_hp_lp;
            hat_hf_sq += (hf as f64) * (hf as f64);
        }
        let hat_hf_rms = (hat_hf_sq / n_hat as f64).sqrt() as f32;
        assert!(hat_hf_rms > 0.015, "Expected shimmering broadband 909/Linn hat highs, got {hat_hf_rms}");

        // 2. Polyphonic OB-X 80s Power chord renders wide, unclipped, bright analog stereo harmonics
        apply_voice_preset(&mut p, TrackId::MoogPad, 5); // OB-X 80s Power
        let mut poly = MoogSynth::new(sr);
        for &note in &[48u8, 55, 60, 64, 67] {
            poly.note_on(note, 0.85, false, false, &p.moog_pad);
        }
        let n_pad = 16_384_usize;
        let mut pad_side_sq = 0.0_f64;
        let mut pad_hf_sq = 0.0_f64;
        let mut pad_peak = 0.0_f32;
        let mut pad_hp_lp = 0.0_f32;
        let pad_hp_a = (std::f32::consts::TAU * 2_500.0 / sr).clamp(0.01, 0.8);
        for _ in 0..n_pad {
            let (l, r) = poly.process_stereo(&p.moog_pad);
            assert!(l.is_finite() && r.is_finite());
            pad_peak = pad_peak.max(l.abs()).max(r.abs());
            let side = 0.5 * (l - r);
            pad_side_sq += (side as f64) * (side as f64);
            let mid = 0.5 * (l + r);
            pad_hp_lp += pad_hp_a * (mid - pad_hp_lp);
            let hf = mid - pad_hp_lp;
            pad_hf_sq += (hf as f64) * (hf as f64);
        }
        let pad_side_rms = (pad_side_sq / n_pad as f64).sqrt() as f32;
        let pad_hf_rms = (pad_hf_sq / n_pad as f64).sqrt() as f32;
        assert!(pad_peak <= 0.99, "OB-Xa poly chord clipped at {pad_peak}");
        assert!(pad_side_rms > 0.04, "Expected wide stereo OB-Xa unison spread, got {pad_side_rms}");
        assert!(pad_hf_rms > 0.015, "Expected open 12dB OB-Xa upper harmonics, got {pad_hf_rms}");

        // 3. 8-channel Hadamard FDN reverb generates lush decorrelated stereo tail after dry burst
        let mut fx = MasterFx::new(sr);
        let mut fxp = p.fx.clone();
        fxp.delay_enabled = false;
        fxp.reverb_enabled = true;
        fxp.reverb_mix = 0.85;
        fxp.reverb_size = 0.72;
        fxp.reverb_damp = 0.40;
        fxp.global_filter_enabled = false;
        fxp.analog_warmth = 0.0;
        fxp.master_crisp = 0.0;
        fxp.club_punch = 0.0;

        let n_rev = 22_050_usize;
        let mut rev_l_sq = 0.0_f64;
        let mut rev_r_sq = 0.0_f64;
        let mut rev_side_sq = 0.0_f64;
        let mut tail_count = 0usize;
        for i in 0..n_rev {
            let burst = if i < 441 {
                let t = i as f32 / sr;
                ((t * 880.0 * std::f32::consts::TAU).sin() * 0.55
                    + (t * 1320.0 * std::f32::consts::TAU).sin() * 0.35)
                    * 0.85
            } else {
                0.0
            };
            let (ol, or) = fx.process_stereo(0.0, 0.0, 0.0, 0.0, burst, burst * 0.7, &fxp, 120.0);
            assert!(ol.is_finite() && or.is_finite());
            if i >= 1_500 {
                rev_l_sq += (ol as f64) * (ol as f64);
                rev_r_sq += (or as f64) * (or as f64);
                let side = 0.5 * (ol - or);
                rev_side_sq += (side as f64) * (side as f64);
                tail_count += 1;
            }
        }
        let rev_l_rms = (rev_l_sq / tail_count as f64).sqrt() as f32;
        let rev_r_rms = (rev_r_sq / tail_count as f64).sqrt() as f32;
        let rev_side_rms = (rev_side_sq / tail_count as f64).sqrt() as f32;
        assert!(rev_l_rms > 0.02 && rev_r_rms > 0.02, "Expected lush FDN reverb tail: L={rev_l_rms}, R={rev_r_rms}");
        assert!(rev_side_rms > 0.015, "Expected decorrelated stereo FDN width, got {rev_side_rms}");
    }

    #[test]
    fn studio_mastering_preserves_dynamics_stereo_width_and_clean_highs() {
        use crate::model::{apply_vibe_to_project, FxParams};

        let vibes = [
            Vibe::Arena80s,
            Vibe::DeepHouse,
            Vibe::ElectroFunk,
            Vibe::ImperialBass,
            Vibe::Dubstep,
        ];
        let sr = 44_100.0_f32;
        for &vibe in &vibes {
            let mut p = Project::default();
            apply_vibe_to_project(&mut p, vibe, None, 20261124);
            p.party_mode = false;

            let mut rt = Runtime::new(sr);
            let mut tel = EngineTelemetry::default();
            let total_secs = 4.0 * 4.0 * (60.0 / p.bpm.clamp(40.0, 240.0));
            let n = (total_secs * sr).round() as usize;

            let mut sum_sq = 0.0_f64;
            let mut mid_sq = 0.0_f64;
            let mut side_sq = 0.0_f64;
            let mut peak = 0.0_f32;

            let mut sub_l_lp = Biquad::new();
            let mut sub_r_lp = Biquad::new();
            sub_l_lp.set_lowpass(sr, 120.0, 0.7071);
            sub_r_lp.set_lowpass(sr, 120.0, 0.7071);
            let mut sub_ll = 0.0_f64;
            let mut sub_rr = 0.0_f64;
            let mut sub_lr = 0.0_f64;

            let mut midband_lp = Biquad::new();
            let mut midband_hp = Biquad::new();
            midband_lp.set_lowpass(sr, 4_000.0, 0.7071);
            midband_hp.set_highpass(sr, 400.0, 0.7071);
            let mut ultra_hp = Biquad::new();
            ultra_hp.set_highpass(sr, 16_000.0, 0.7071);
            let mut midband_sq = 0.0_f64;
            let mut ultra_sq = 0.0_f64;

            for _ in 0..n {
                let (l, r) = rt.process_sample(&p, true, false, false, 0, &mut tel);
                assert!(l.is_finite() && r.is_finite(), "[{:?}] Non-finite sample", vibe);
                assert!(l.abs() <= 0.961 && r.abs() <= 0.961, "[{:?}] Exceeded ceiling: ({l}, {r})", vibe);
                peak = peak.max(l.abs()).max(r.abs());
                sum_sq += (l as f64) * (l as f64) + (r as f64) * (r as f64);

                let m = 0.5 * (l + r);
                let s = 0.5 * (l - r);
                mid_sq += (m as f64) * (m as f64);
                side_sq += (s as f64) * (s as f64);

                let sl = sub_l_lp.process(l) as f64;
                let sr_v = sub_r_lp.process(r) as f64;
                sub_ll += sl * sl;
                sub_rr += sr_v * sr_v;
                sub_lr += sl * sr_v;

                let mb = midband_hp.process(midband_lp.process(m)) as f64;
                let uh = ultra_hp.process(m) as f64;
                midband_sq += mb * mb;
                ultra_sq += uh * uh;
            }

            let rms = (sum_sq / (n.max(1) as f64 * 2.0)).sqrt() as f32;
            let crest_db = 20.0 * (peak.max(1e-6) / rms.max(1e-6)).log10();
            let sm_ratio = (side_sq / mid_sq.max(1e-12)).sqrt() as f32;
            let sub_corr = (sub_lr / (sub_ll * sub_rr).sqrt().max(1e-12)) as f32;
            let midband_rms = (midband_sq / n.max(1) as f64).sqrt() as f32;
            let ultra_rms = (ultra_sq / n.max(1) as f64).sqrt() as f32;

            assert!(
                crest_db >= 8.5,
                "[{:?}] Crest factor too squashed: {crest_db:.2} dB (peak={peak:.3}, rms={rms:.3})",
                vibe
            );
            assert!(
                sm_ratio >= 0.16,
                "[{:?}] Stereo width S/M too narrow: {sm_ratio:.3}",
                vibe
            );
            if vibe == Vibe::Arena80s {
                assert!(
                    sm_ratio >= 0.22,
                    "Arena80s should be wide >= 0.22, got {sm_ratio:.3}"
                );
            }
            assert!(
                sub_corr >= 0.97,
                "[{:?}] Sub <120Hz stereo correlation too loose: {sub_corr:.4}",
                vibe
            );
            assert!(
                ultra_rms < midband_rms * 0.65,
                "[{:?}] Ultrasonic >16kHz fizz too high: ultra={ultra_rms:.4}, midband={midband_rms:.4}",
                vibe
            );
        }

        // Sample-rate invariance sanity check for Biquad + MasterFx at 44.1 kHz & 48 kHz
        for &test_sr in &[44_100.0_f32, 48_000.0_f32] {
            let mut bq = Biquad::new();
            bq.set_high_shelf(test_sr, 10_500.0, 0.70, 2.5);
            let mut fx = MasterFx::new(test_sr);
            let fxp = FxParams::default();
            for i in 0..4096 {
                let t = i as f32 / test_sr;
                let s = (t * 440.0 * std::f32::consts::TAU).sin() * 0.4;
                let y = bq.process(s);
                let (ol, or) = fx.process_stereo(y, y, y * 0.2, y * 0.2, y * 0.2, y * 0.2, &fxp, 122.0);
                assert!(ol.is_finite() && or.is_finite() && ol.abs() <= 0.961 && or.abs() <= 0.961);
            }
        }
    }

    #[test]
    fn stereo_sampler_and_saved_sample_preserve_stereo_and_velocity_dynamics() {
        use crate::model::{SamplerMode, SamplerParams, SavedSample};

        let sr = 44_100u32;
        for id in 0..9u8 {
            let (l, r) = generate_builtin_sample_stereo(id, sr);
            assert!(!l.is_empty() && l.len() == r.len(), "Patch {id} empty or mismatched len");
            let mut side_sq = 0.0_f64;
            for (&sl, &sr_v) in l.iter().zip(r.iter()) {
                assert!(
                    sl.is_finite() && sr_v.is_finite() && sl.abs() <= 1.0 && sr_v.abs() <= 1.0,
                    "Patch {id} out of range: ({sl}, {sr_v})"
                );
                let side = 0.5 * (sl - sr_v);
                side_sq += (side as f64) * (side as f64);
            }
            if matches!(id, 0 | 3 | 5 | 6 | 7) {
                let side_rms = (side_sq / l.len() as f64).sqrt() as f32;
                assert!(
                    side_rms > 0.015,
                    "Expected true stereo width > 0.015 on built-in patch {id}, got {side_rms:.4}"
                );
            }
        }

        // Stereo round-trip + legacy mono decode backwards compatibility
        let left_in: Vec<f32> = (0..128).map(|i| (i as f32 * 0.05).sin() * 0.8).collect();
        let right_in: Vec<f32> = (0..128).map(|i| (i as f32 * 0.05).cos() * 0.6).collect();
        let saved_st = SavedSample::from_stereo_pcm("StereoTest", sr, &left_in, &right_in, None);
        assert_eq!(saved_st.channels, 2);
        let (dec_l, dec_r) = saved_st.decode_stereo_pcm().expect("decode stereo");
        assert_eq!(dec_l.len(), 128);
        assert_eq!(dec_r.len(), 128);
        let diff_lr: f32 = dec_l.iter().zip(dec_r.iter()).map(|(a, b)| (a - b).abs()).sum();
        assert!(diff_lr > 10.0, "Expected distinct L/R round-trip, got diff={diff_lr}");
        let downmix = saved_st.decode_f32_pcm().expect("downmix mono");
        assert_eq!(downmix.len(), 128);

        let mut legacy_mono = SavedSample::from_f32_pcm("Legacy", sr, &left_in, None);
        legacy_mono.channels = 0; // simulate old serialized project JSON without channels field
        let (ml, mr) = legacy_mono.decode_stereo_pcm().expect("decode legacy mono");
        assert_eq!(ml, mr, "Legacy mono sample should mirror to L/R identically");

        // Sk1Sampler velocity-to-brightness & output level dynamics
        let render_vel = |vel: f32| -> f32 {
            let mut samp = Sk1Sampler::new(sr as f32);
            let mut sp = SamplerParams::default();
            sp.mode = SamplerMode::HiFi;
            sp.cutoff = 4_800.0;
            samp.note_on(60, vel, &sp);
            let mut sum_sq = 0.0_f64;
            for _ in 0..4096 {
                let (l, r) = samp.process_stereo(&sp);
                assert!(l.is_finite() && r.is_finite());
                sum_sq += (l as f64) * (l as f64) + (r as f64) * (r as f64);
            }
            (sum_sq / (4096.0 * 2.0)).sqrt() as f32
        };
        let rms_soft = render_vel(0.25);
        let rms_loud = render_vel(0.95);
        assert!(
            rms_loud > rms_soft * 2.0,
            "Expected strong sampler velocity dynamics: loud={rms_loud:.4}, soft={rms_soft:.4}"
        );
    }

    #[test]
    fn expressive_groove_pocket_and_formant_growl_render_cleanly() {
        use crate::model::{
            apply_vibe_to_project, LfoTarget, MonologueOscWave, MonologueParams, Osc2ModMode,
        };

        let sr = 44_100.0_f32;
        // 1) Monologue delayed vibrato bloom + slow drift + silk smoother
        let mut vib_synth = MonologueSynth::new(sr);
        let mut vib_p = MonologueParams::default();
        vib_p.amp_sustain = true;
        vib_p.lfo_target = LfoTarget::Pitch;
        vib_p.lfo_rate = 0.56;
        vib_p.lfo_int = 0.020;
        vib_synth.note_on(76, 0.90, false, false, &vib_p);
        let mut vib_peak = 0.0_f32;
        for _ in 0..16_384 {
            let (l, r) = vib_synth.process_stereo(&vib_p);
            assert!(l.is_finite() && r.is_finite());
            vib_peak = vib_peak.max(l.abs()).max(r.abs());
        }
        assert!(vib_peak > 0.05 && vib_peak <= 1.4, "Unexpected vibrato lead peak: {vib_peak}");

        // 2) Monologue Osc2ModMode::Ring wavefolded talking formant growl
        let mut growl_synth = MonologueSynth::new(sr);
        let mut growl_p = MonologueParams::default();
        growl_p.vco1_wave = MonologueOscWave::Saw;
        growl_p.vco2_wave = MonologueOscWave::Square;
        growl_p.mod_mode = Osc2ModMode::Ring;
        growl_p.vco2_octave = -1;
        growl_p.vco2_pitch = 5.0;
        growl_p.vco1_shape = 0.65;
        growl_p.vco2_shape = 0.55;
        growl_p.cutoff = 1150.0;
        growl_p.resonance = 0.62;
        growl_p.drive = 0.84;
        growl_p.lfo_target = LfoTarget::Cutoff;
        growl_p.lfo_int = 0.55;
        growl_synth.note_on(40, 0.96, true, false, &growl_p);
        let mut growl_peak = 0.0_f32;
        for _ in 0..16_384 {
            let (l, r) = growl_synth.process_stereo(&growl_p);
            assert!(l.is_finite() && r.is_finite());
            growl_peak = growl_peak.max(l.abs()).max(r.abs());
        }
        assert!(
            growl_peak > 0.05 && growl_peak <= 1.8,
            "Unexpected formant growl peak: {growl_peak}"
        );

        // 3) Full Runtime groove pocket + console strips across 2 bars of key vibes
        for &vibe in &[Vibe::Dubstep, Vibe::Arena80s, Vibe::ElectroFunk, Vibe::DeepHouse] {
            let mut p = Project::default();
            apply_vibe_to_project(&mut p, vibe, None, 20261124);
            p.party_mode = false;
            let mut rt = Runtime::new(sr);
            let mut tel = EngineTelemetry::default();
            let total_secs = 2.0 * 4.0 * (60.0 / p.bpm.clamp(40.0, 240.0));
            let n = (total_secs * sr).round() as usize;
            let mut peak = 0.0_f32;
            let mut sum_sq = 0.0_f64;
            for _ in 0..n {
                let (l, r) = rt.process_sample(&p, true, false, false, 0, &mut tel);
                assert!(l.is_finite() && r.is_finite(), "[{:?}] Non-finite output", vibe);
                assert!(l.abs() <= 0.961 && r.abs() <= 0.961, "[{:?}] Clipped output: ({l}, {r})", vibe);
                peak = peak.max(l.abs()).max(r.abs());
                sum_sq += (l as f64) * (l as f64) + (r as f64) * (r as f64);
            }
            let rms = (sum_sq / (n.max(1) as f64 * 2.0)).sqrt() as f32;
            let crest_db = 20.0 * (peak.max(1e-6) / rms.max(1e-6)).log10();
            assert!(
                crest_db >= 8.5,
                "[{:?}] Crest dropped below 8.5 dB: {crest_db:.2} dB",
                vibe
            );
        }
    }

    #[test]
    fn halfband_oversampler_and_genre_purity_invariants_hold() {
        use crate::dsp::{AnalogDrift, HalfBandDecim2x, HalfBandUp2x};
        use crate::model::{apply_vibe_to_project, PROGRESSIONS};

        let sr = 48_000.0_f32;
        // 1) Half-band 2x oversampler preserves 15 kHz passband (> 0.90 gain, no boxcar -6 dB droop)
        let mut up = HalfBandUp2x::new();
        let mut decim = HalfBandDecim2x::new();
        let mut sum_sq = 0.0_f64;
        let n = 4096_usize;
        for i in 0..n {
            let x = (i as f32 * 15_000.0 / sr * std::f32::consts::TAU).sin();
            let u = up.process(x);
            let y = decim.process(u[0], u[1]);
            if i >= 256 {
                sum_sq += (y as f64) * (y as f64);
            }
        }
        let rms_15k = (sum_sq / (n - 256) as f64).sqrt() as f32;
        assert!(
            rms_15k > 0.64,
            "Expected flat 15 kHz passband RMS > 0.64 (~0.707 ideal), got {rms_15k:.4}"
        );

        // 2) Ornstein-Uhlenbeck AnalogDrift stays finite, bounded, and non-trivial
        let mut drift = AnalogDrift::new(0x1234_5678);
        let mut d_sq = 0.0_f64;
        for _ in 0..24_000 {
            let v = drift.next(sr);
            assert!(v.is_finite() && v.abs() <= 1.0);
            d_sq += (v as f64) * (v as f64);
        }
        let drift_rms = (d_sq / 24_000.0).sqrt() as f32;
        assert!(
            drift_rms > 0.05 && drift_rms <= 1.0,
            "Expected organic thermal drift RMS in (0.05, 1.0], got {drift_rms:.4}"
        );

        // 3) Lyrical/rock vibes have zero auto-wah lead hijacking & exact chord-root sampler pitch classes
        for &vibe in &[Vibe::IndieRock, Vibe::Synthwave, Vibe::LofiChill, Vibe::EuphoricTrance] {
            for seed in [42_u32, 1337, 20261124] {
                let mut p = Project::default();
                apply_vibe_to_project(&mut p, vibe, None, seed);
                let pat = &p.patterns[p.active_pattern];
                for st in &pat.monologue.steps {
                    if st.active {
                        assert!(
                            st.motion.cutoff.is_none() && st.motion.resonance.is_none(),
                            "[{:?}] Unexpected auto-wah motion lock on lead step",
                            vibe
                        );
                    }
                }
                let prog = &PROGRESSIONS[p.progression_idx % PROGRESSIONS.len()];
                for bar in 0..4 {
                    let expected_pc = (p.root_note + prog.slots[bar].offset) % 12;
                    for step_i in 0..16 {
                        let s = &pat.sampler.steps[bar * 16 + step_i];
                        if s.active {
                            assert_eq!(
                                s.note % 12,
                                expected_pc,
                                "[{:?}] Sampler note {} pitch class != chord root pc {}",
                                vibe,
                                s.note,
                                expected_pc
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn hardware_arpeggiator_modes_latch_and_philharmonia_layers_work() {
        use crate::dsp::{DrumMachine, Svf12Filter};
        use crate::model::{ArpMode, ArpRate, DrumModel};

        let sr = 44_100.0_f32;
        // 1) Legacy project JSON without `arp` deserializes cleanly with defaults
        let legacy_json = r#"{"version":2,"bpm":120.0}"#;
        let mut p: Project = serde_json::from_str(legacy_json).expect("legacy project json");
        p.normalize();
        assert!(!p.arp.enabled);
        assert_eq!(p.arp.mode, ArpMode::Up);
        assert_eq!(p.arp.rate, ArpRate::Sixteenth);
        assert_eq!(p.arp.octaves, 2);

        // 2) Exercise all ArpModes + Latch + disable cleanly leaving 0 active voices
        p.arp.enabled = true;
        p.arp.latch = true;
        p.arp.rate = ArpRate::Sixteenth;
        p.arp.octaves = 2;
        let mut rt = Runtime::new(sr);
        let mut tel = EngineTelemetry::default();
        for &note in &[60u8, 64, 67] {
            rt.handle_live_event(
                LiveEvent::PlayNote {
                    track: TrackId::Monologue,
                    note,
                    velocity: 0.88,
                },
                &p,
            );
        }
        // Release keys while HOLD/latch is active
        for &note in &[60u8, 64, 67] {
            rt.handle_live_event(
                LiveEvent::ReleaseNote {
                    track: TrackId::Monologue,
                    note,
                },
                &p,
            );
        }
        for mode in ArpMode::ALL {
            p.arp.mode = mode;
            let mut sq = 0.0_f64;
            for _ in 0..8_192 {
                let (l, r) = rt.process_sample(&p, false, false, false, 0, &mut tel);
                assert!(l.is_finite() && r.is_finite());
                assert!(l.abs() <= 0.961 && r.abs() <= 0.961);
                sq += (l as f64) * (l as f64) + (r as f64) * (r as f64);
            }
            let rms = (sq / (8_192.0 * 2.0)).sqrt() as f32;
            assert!(rms > 0.005, "Expected latched arp mode {:?} to produce audio, rms={rms}", mode);
        }
        p.arp.enabled = false;
        for _ in 0..24_000 {
            let _ = rt.process_sample(&p, false, false, false, 0, &mut tel);
        }
        assert!(rt.live_arp.sounding.is_none());
        assert_eq!(rt.mono.is_active(), false);

        // 3) Upgraded 909/Studio Crash Cymbal has wide stereo image and full mid-bronze body (500..3500 Hz)
        let mut p_cym = Project::default();
        p_cym.drums.voices[7].model = DrumModel::Tr909;
        let mut dm = DrumMachine::new(sr);
        dm.trigger(DrumVoiceId::Cymbal, 1.0, false, false, &p_cym.drums);
        let n_cym = (1.4 * sr) as usize;
        let mut side_sq = 0.0_f64;
        let mut body_sq = 0.0_f64;
        let mut bp_lp = Svf12Filter::new(sr);
        let mut bp_hp = Svf12Filter::new(sr);
        for _ in 0..n_cym {
            let (l, r) = dm.process_sample(&p_cym.drums);
            assert!(l.is_finite() && r.is_finite());
            let m = 0.5 * (l + r);
            let s = 0.5 * (l - r);
            side_sq += (s as f64) * (s as f64);
            let mid_band = bp_hp.process_hp(bp_lp.process_lp(m, 3_500.0, 0.05), 500.0, 0.05);
            body_sq += (mid_band as f64) * (mid_band as f64);
        }
        let side_rms = (side_sq / n_cym as f64).sqrt() as f32;
        let body_rms = (body_sq / n_cym as f64).sqrt() as f32;
        assert!(side_rms > 0.01, "Expected wide stereo crash cymbal side_rms > 0.01, got {side_rms:.4}");
        assert!(body_rms > 0.015, "Expected full mid-bronze crash body RMS > 0.015, got {body_rms:.4}");
    }

    #[test]
    fn multi_engine_fm_wavetable_philharmonia_shimmer_and_sf2_work() {
        use crate::dsp::{FmSynth, MasterFx, MultiSampleSynth, SoundFontBank, SoundFontSynth, WavetableSynth};

        let sr = 44_100.0_f32;
        // 1) 4-Op FM Engine across all 6 banks
        let mut fm = FmSynth::new(sr);
        for bank in 0..6u8 {
            fm.all_notes_off();
            fm.note_on(60, 0.9, false, false, true, 0.04);
            fm.note_on(67, 0.85, false, false, true, 0.04);
            let mut sq = 0.0_f64;
            for _ in 0..2048 {
                let (l, r) = fm.process_stereo(
                    bank, 0.58, 3600.0, 0.22, 0.20, 0.5, 0.004, 0.45, 0.55, 0.3, 0.04, true, 0.2, 0.0,
                );
                assert!(l.is_finite() && r.is_finite() && l.abs() <= 1.8 && r.abs() <= 1.8);
                sq += (l as f64) * (l as f64) + (r as f64) * (r as f64);
            }
            let rms = (sq / 4096.0).sqrt() as f32;
            assert!(rms > 0.01, "FM bank {bank} too quiet: {rms}");
        }

        // 2) Wavetable + 7-Voice HyperSaw Engine across all 6 banks (verifies wide HyperSaw stereo)
        let mut wt = WavetableSynth::new(sr);
        for bank in 0..6u8 {
            wt.all_notes_off();
            wt.note_on(60, 0.9, false, false, true, 0.04);
            wt.note_on(64, 0.88, false, false, true, 0.04);
            let mut side_sq = 0.0_f64;
            let mut mid_sq = 0.0_f64;
            for _ in 0..2048 {
                let (l, r) = wt.process_stereo(
                    bank, 0.65, 3600.0, 0.22, 0.18, 0.5, 0.01, 0.4, 0.5, 0.3, 0.01, 0.4, 0.7, 0.3,
                    0.04, true, 0.0, 0.0,
                );
                assert!(l.is_finite() && r.is_finite() && l.abs() <= 1.8 && r.abs() <= 1.8);
                let m = 0.5 * (l + r);
                let s = 0.5 * (l - r);
                mid_sq += (m as f64) * (m as f64);
                side_sq += (s as f64) * (s as f64);
            }
            let mid_rms = (mid_sq / 2048.0).sqrt() as f32;
            let side_rms = (side_sq / 2048.0).sqrt() as f32;
            assert!(mid_rms > 0.01, "Wavetable bank {bank} silent: {mid_rms}");
            if bank == 0 {
                assert!(side_rms > 0.008, "Virus 7x HyperSaw should have stereo width, got {side_rms}");
            }
        }

        // 3) Chromatic Multi-Sampled Philharmonia + D-50 Engine across all 6 banks
        let mut ms = MultiSampleSynth::new(sr);
        for bank in 0..6u8 {
            ms.all_notes_off();
            ms.note_on(57, 0.9, false, true, bank);
            ms.note_on(64, 0.88, false, true, bank);
            let mut sq = 0.0_f64;
            for _ in 0..3072 {
                let (l, r) = ms.process_stereo(
                    bank, 0.45, 4200.0, 0.18, 0.20, 0.01, 0.6, 0.7, 0.3, true, 0.15,
                    if bank == 2 { 0.75 } else { 0.0 },
                );
                assert!(l.is_finite() && r.is_finite() && l.abs() <= 1.8 && r.abs() <= 1.8);
                sq += (l as f64) * (l as f64) + (r as f64) * (r as f64);
            }
            let rms = (sq / 6144.0).sqrt() as f32;
            assert!(rms > 0.005, "Philharmonia bank {bank} silent: {rms}");
        }

        // 4) Pure-Rust SoundFont 2.0 (RIFF sfbk) parser + playback
        let mut sf2_bytes: Vec<u8> = Vec::new();
        let push_chunk = |dst: &mut Vec<u8>, id: &[u8; 4], data: &[u8]| {
            dst.extend_from_slice(id);
            dst.extend_from_slice(&(data.len() as u32).to_le_bytes());
            dst.extend_from_slice(data);
            if data.len() & 1 == 1 {
                dst.push(0);
            }
        };
        let mut info_list = Vec::new();
        info_list.extend_from_slice(b"INFO");
        push_chunk(&mut info_list, b"INAM", b"UnitTestBank\0\0");

        let mut smpl_raw = Vec::with_capacity(4096 * 2);
        for i in 0..4096 {
            let v = ((i as f32 * 440.0 / sr * std::f32::consts::TAU).sin() * 24000.0) as i16;
            smpl_raw.extend_from_slice(&v.to_le_bytes());
        }
        let mut sdta_list = Vec::new();
        sdta_list.extend_from_slice(b"sdta");
        push_chunk(&mut sdta_list, b"smpl", &smpl_raw);

        let mut phdr = Vec::new();
        let mut p_rec = [0u8; 38];
        p_rec[..9].copy_from_slice(b"TestPiano");
        phdr.extend_from_slice(&p_rec);
        let mut eop = [0u8; 38];
        eop[..3].copy_from_slice(b"EOP");
        eop[24..26].copy_from_slice(&1u16.to_le_bytes());
        phdr.extend_from_slice(&eop);

        let mut pbag = Vec::new();
        pbag.extend_from_slice(&[0, 0, 0, 0, 1, 0, 0, 0]);
        let mut pgen = Vec::new();
        // op 41 = instrument 0
        pgen.extend_from_slice(&41u16.to_le_bytes());
        pgen.extend_from_slice(&0u16.to_le_bytes());

        let mut inst = Vec::new();
        let mut i_rec = [0u8; 22];
        i_rec[..8].copy_from_slice(b"SineInst");
        inst.extend_from_slice(&i_rec);
        let mut eoi = [0u8; 22];
        eoi[..3].copy_from_slice(b"EOI");
        eoi[20..22].copy_from_slice(&1u16.to_le_bytes());
        inst.extend_from_slice(&eoi);

        let mut ibag = Vec::new();
        ibag.extend_from_slice(&[0, 0, 0, 0, 2, 0, 0, 0]);
        let mut igen = Vec::new();
        // op 54 = sampleModes (1 = loop), op 53 = sampleID 0
        igen.extend_from_slice(&54u16.to_le_bytes());
        igen.extend_from_slice(&1u16.to_le_bytes());
        igen.extend_from_slice(&53u16.to_le_bytes());
        igen.extend_from_slice(&0u16.to_le_bytes());

        let mut shdr = Vec::new();
        let mut s_rec = [0u8; 46];
        s_rec[..7].copy_from_slice(b"Sine440");
        s_rec[20..24].copy_from_slice(&0u32.to_le_bytes());
        s_rec[24..28].copy_from_slice(&4096u32.to_le_bytes());
        s_rec[28..32].copy_from_slice(&100u32.to_le_bytes());
        s_rec[32..36].copy_from_slice(&2100u32.to_le_bytes());
        s_rec[36..40].copy_from_slice(&(sr as u32).to_le_bytes());
        s_rec[40] = 69; // A4
        s_rec[44] = 1;  // monoSample
        shdr.extend_from_slice(&s_rec);
        let mut eos = [0u8; 46];
        eos[..3].copy_from_slice(b"EOS");
        shdr.extend_from_slice(&eos);

        let mut pdta_list = Vec::new();
        pdta_list.extend_from_slice(b"pdta");
        push_chunk(&mut pdta_list, b"phdr", &phdr);
        push_chunk(&mut pdta_list, b"pbag", &pbag);
        push_chunk(&mut pdta_list, b"pgen", &pgen);
        push_chunk(&mut pdta_list, b"inst", &inst);
        push_chunk(&mut pdta_list, b"ibag", &ibag);
        push_chunk(&mut pdta_list, b"igen", &igen);
        push_chunk(&mut pdta_list, b"shdr", &shdr);

        let mut sfbk_body = Vec::new();
        sfbk_body.extend_from_slice(b"sfbk");
        push_chunk(&mut sfbk_body, b"LIST", &info_list);
        push_chunk(&mut sfbk_body, b"LIST", &sdta_list);
        push_chunk(&mut sfbk_body, b"LIST", &pdta_list);
        push_chunk(&mut sf2_bytes, b"RIFF", &sfbk_body);

        let parsed = std::sync::Arc::new(SoundFontBank::from_bytes(&sf2_bytes).expect("parse sf2"));
        assert_eq!(parsed.name, "UnitTestBank");
        assert_eq!(parsed.presets.len(), 1);
        assert_eq!(parsed.presets[0].name, "TestPiano");
        let mut sf2 = SoundFontSynth::new(sr);
        let _ = sf2.set_bank(Some(parsed));
        sf2.note_on(64, 0.9, true, 0);
        let mut sf2_sq = 0.0_f64;
        for _ in 0..2048 {
            let (l, r) = sf2.process_stereo(6000.0, 0.1, 0.0, 0.0, 0.0);
            assert!(l.is_finite() && r.is_finite() && l.abs() <= 1.8 && r.abs() <= 1.8);
            sf2_sq += (l as f64) * (l as f64) + (r as f64) * (r as f64);
        }
        assert!((sf2_sq / 4096.0).sqrt() > 0.02);

        // 5) Shimmer Hall Reverb in MasterFx
        let mut fx = MasterFx::new(sr);
        let mut fxp = Project::default().fx;
        fxp.reverb_enabled = true;
        fxp.reverb_mix = 0.65;
        fxp.reverb_shimmer = 0.70;
        let mut wet_tail_sq = 0.0_f64;
        for i in 0..16_384 {
            let imp = if i < 256 { (i as f32 * 440.0 / sr * std::f32::consts::TAU).sin() * 0.6 } else { 0.0 };
            let (l, r) = fx.process_stereo(imp, imp, 0.0, 0.0, imp, imp, &fxp, 120.0);
            assert!(l.is_finite() && r.is_finite() && l.abs() <= 0.961 && r.abs() <= 0.961);
            if i > 4096 {
                wet_tail_sq += (l as f64) * (l as f64) + (r as f64) * (r as f64);
            }
        }
        assert!(wet_tail_sq > 0.05, "Expected lush shimmer tail energy, got {wet_tail_sq}");
    }

    #[test]
    fn voice_leading_and_melodic_contour_stay_smooth_and_in_key() {
        use crate::model::{
            generate_melodic_tracks, voice_led_chord_near, ChordSlot, ChordType, PROGRESSIONS,
        };

        // 1) Bar-to-bar voice_led_chord_near moves smoothly vs root-position jumps across i -> VI -> III -> VII
        let slots = [
            ChordSlot { offset: 0, chord: ChordType::Minor9 },
            ChordSlot { offset: 8, chord: ChordType::Major7 },
            ChordSlot { offset: 3, chord: ChordType::Major9 },
            ChordSlot { offset: 10, chord: ChordType::Add9 },
        ];
        let mut prev: Vec<u8> = Vec::new();
        for (bi, &sl) in slots.iter().enumerate() {
            let voiced = voice_led_chord_near(0, sl, &prev, 60);
            assert!(!voiced.is_empty());
            assert!(
                voiced.iter().all(|&n| (48..=74).contains(&n)),
                "Bar {bi} voiced chord out of warm pad zone: {:?}",
                voiced
            );
            if !prev.is_empty() {
                let total_move: i32 = voiced
                    .iter()
                    .map(|&n| prev.iter().map(|&p| (p as i32 - n as i32).abs()).min().unwrap_or(0))
                    .sum();
                let avg_move = total_move as f32 / voiced.len() as f32;
                assert!(
                    avg_move <= 3.5,
                    "Expected smooth bar-to-bar voice leading (avg <= 3.5st), got {avg_move:.2}st: {:?} -> {:?}",
                    prev,
                    voiced
                );
            }
            prev = voiced;
        }

        // 2) Generated melodies across core musical vibes avoid big leaps and land on chord tones on downbeats
        for &vibe in &[
            Vibe::ElectroFunk,
            Vibe::DeepHouse,
            Vibe::Synthwave,
            Vibe::LofiChill,
            Vibe::Arena80s,
        ] {
            let prog_idx = vibe.default_progression() % PROGRESSIONS.len();
            let prog = &PROGRESSIONS[prog_idx];
            let (_, _, arp, _, _) = generate_melodic_tracks(vibe, 0, prog_idx, 20261124);
            let mut prev_n: Option<u8> = None;
            for bar in 0..4 {
                let slot = prog.slots[bar];
                let ivs = slot.chord.intervals();
                let chord_pcs: Vec<u8> = ivs.iter().map(|&iv| (slot.offset + iv) % 12).collect();
                for st in 0..16 {
                    let s = &arp.steps[bar * 16 + st];
                    if !s.active {
                        continue;
                    }
                    if st % 4 == 0 {
                        assert!(
                            chord_pcs.contains(&(s.note % 12)),
                            "[{:?}] bar {bar} step {st} note {} (pc {}) missed chord tones {:?}",
                            vibe,
                            s.note,
                            s.note % 12,
                            chord_pcs
                        );
                    }
                    if let Some(p) = prev_n {
                        let leap = (s.note as i16 - p as i16).abs();
                        assert!(
                            leap <= 9,
                            "[{:?}] bar {bar} step {st} jumped {leap}st ({p} -> {})",
                            vibe,
                            s.note
                        );
                    }
                    prev_n = Some(s.note);
                }
            }
        }
    }

    #[test]
    fn philharmonia_orchestra_instrument_banks_and_percussion_kit_work() {
        use crate::dsp::{philharmonia_bank_loaded_count, MultiSampleSynth, NUM_PHILHARMONIA_BANKS};
        use crate::model::{apply_philharmonia_instrument, generate_philharmonia_track_seq, ExtraTrack, SynthEngine};

        let banks = SynthEngine::PhilharmoniaD50.bank_names();
        assert_eq!(banks.len(), NUM_PHILHARMONIA_BANKS);
        assert_eq!(banks[6], "Solo Violin");
        assert_eq!(banks[25], "Philharmonia Percussion Kit");

        let sr = 44_100.0_f32;
        let mut ms = MultiSampleSynth::new(sr);
        let disk_expected = std::path::Path::new("philharmonia-all-samples").is_dir();

        // Check representative solo, pizzicato, wind, brass, and percussion kit banks
        for &(bank_id, test_notes) in &[
            (6u8, &[64u8, 69, 76][..]),
            (8, &[48, 55, 60]),
            (9, &[36, 43]),
            (10, &[55, 62, 69]),
            (11, &[55, 62]),
            (13, &[60, 67]),
            (17, &[72, 79]),
            (19, &[60, 67]),
            (24, &[57, 64]),
            (25, &[48, 49, 55, 62]),
        ] {
            let (loaded, zone_cnt) = philharmonia_bank_loaded_count(bank_id);
            assert!(zone_cnt >= 1, "Bank {bank_id} returned 0 zones");
            if disk_expected {
                assert!(loaded, "Expected real disk Philharmonia recordings for bank {bank_id}");
                if bank_id == 25 {
                    assert!(zone_cnt >= 16, "Expected >=16 perc kit zones, got {zone_cnt}");
                }
            }
            ms.all_notes_off();
            for &n in test_notes {
                ms.note_on(n, 0.90, false, true, bank_id);
            }
            let mut sq = 0.0_f64;
            for _ in 0..4096 {
                let (l, r) = ms.process_stereo(bank_id, 0.0, 14_000.0, 0.1, 0.05, 0.01, 0.8, 0.8, 0.4, true, 0.0, 0.0);
                assert!(l.is_finite() && r.is_finite() && l.abs() <= 1.8 && r.abs() <= 1.8);
                sq += (l as f64) * (l as f64) + (r as f64) * (r as f64);
            }
            let rms = (sq / 8192.0).sqrt() as f32;
            assert!(rms > 0.005, "Philharmonia bank {bank_id} too quiet: rms={rms}");

            let mut tr = ExtraTrack::default();
            apply_philharmonia_instrument(&mut tr, bank_id);
            assert_eq!(tr.synth.engine, SynthEngine::PhilharmoniaD50);
            assert_eq!(tr.synth.engine_bank, bank_id);
            let seq = generate_philharmonia_track_seq(0, 0, bank_id, 42);
            assert!(seq.steps.iter().any(|s| s.active), "Empty starter seq for bank {bank_id}");
        }
    }

    #[test]
    fn sampler_filter_passthrough_bypasses_svf_cleanly() {
        use crate::model::{SamplePart, SamplerMode, SamplerParams};

        let sr = 44_100.0_f32;
        let len = 8192_usize;
        let tone_10k: Vec<f32> = (0..len)
            .map(|i| ((i as f32 * 10_000.0 / sr) * std::f32::consts::TAU).sin() * 0.75)
            .collect();
        let buf = Arc::new(tone_10k);

        let measure = |sp: &SamplerParams| -> f32 {
            let mut samp = Sk1Sampler::new(sr);
            let _ = samp.swap_stereo_buffer(Arc::clone(&buf), Arc::clone(&buf), sr as u32);
            samp.note_on(60, 0.95, sp);
            let mut sq = 0.0_f64;
            for i in 0..4096 {
                let (l, r) = samp.process_stereo(sp);
                assert!(l.is_finite() && r.is_finite());
                if i >= 512 {
                    sq += (l as f64) * (l as f64) + (r as f64) * (r as f64);
                }
            }
            (sq / (3584.0 * 2.0)).sqrt() as f32
        };

        let mut sp = SamplerParams::default();
        sp.mode = SamplerMode::HiFi;
        sp.bit_crush = 0.0;
        sp.filter_enabled = true;
        sp.cutoff = 300.0;
        sp.resonance = 0.0;
        let rms_filtered = measure(&sp);
        assert!(rms_filtered < 0.05, "Expected 300Hz LP to heavily attenuate 10kHz, got {rms_filtered}");

        // Even with cutoff=300 and HighPass mode set, filter_enabled=false must bypass transparently
        sp.filter_mode = GlobalFilterMode::HighPass;
        sp.filter_enabled = false;
        let rms_thru_mode = measure(&sp);
        assert!(
            rms_thru_mode > rms_filtered * 4.0 && rms_thru_mode > 0.15,
            "Expected filter_enabled=false passthrough for 10kHz: filtered={rms_filtered}, thru={rms_thru_mode}"
        );

        sp.set_filter_passthrough();
        assert!(!sp.filter_enabled && sp.cutoff >= 17_999.0 && sp.resonance == 0.0);
        let mut part = SamplePart::default();
        assert!(part.filter_enabled);
        part.set_filter_passthrough();
        assert!(!part.filter_enabled && !part.to_sampler_params().filter_enabled);

        // Legacy JSON missing filter_enabled keeps backwards-compatible default true
        let legacy: SamplerParams = serde_json::from_str(r#"{"cutoff":4200.0}"#).expect("legacy json");
        assert!(legacy.filter_enabled);
    }

    #[test]
    fn volca_lane_polymeter_shift_and_euclidean_work() {
        use crate::model::DrumLane;

        let mut lane = DrumLane::default();
        assert_eq!(lane.effective_len(), 16);
        lane.active_steps = 12;
        assert_eq!(lane.effective_len(), 12);

        lane.fill_euclidean(5, 0);
        let hits: Vec<usize> = (0..12).filter(|&i| lane.steps[i].active).collect();
        assert_eq!(hits.len(), 5);
        assert!(lane.steps[12..16].iter().all(|s| !s.active));

        lane.shift(1);
        let shifted_hits: Vec<usize> = (0..12).filter(|&i| lane.steps[i].active).collect();
        let expected: Vec<usize> = hits.iter().map(|&h| (h + 1) % 12).collect();
        for h in &shifted_hits {
            assert!(expected.contains(h));
        }

        lane.steps[0].reverse = true;
        lane.steps[0].pitch_offset = 5;
        lane.steps[0].cutoff_lock = Some(0.5);
        assert!(lane.steps[0].has_step_fx());
        lane.clear_motion();
        assert!(!lane.steps[0].has_step_fx());
    }

    #[test]
    fn volca_philharmonia_kit_loads_clean_passthrough_pads() {
        use crate::dsp::sampler::{load_volca_kit, VOLCA_KIT_NAMES};
        use crate::model::SamplePart;

        let mut parts = SamplePart::default_bank();
        for (kit_id, _) in VOLCA_KIT_NAMES.iter().enumerate() {
            let prev_gens: Vec<u64> = parts.iter().map(|p| p.generation).collect();
            let msg = load_volca_kit(&mut parts, kit_id, 44_100);
            assert!(!msg.is_empty());
            for (i, part) in parts.iter().enumerate() {
                assert!(!part.name.is_empty());
                assert!(!part.filter_enabled, "Kit {kit_id} pad {i} should be passthrough");
                assert_ne!(part.generation, prev_gens[i], "Kit {kit_id} pad {i} must bump generation");
            }
        }
    }

    #[test]
    fn volca_active_parts_mask_tracks_triggered_pads() {
        use crate::model::SamplePart;

        let sr = 44_100.0_f32;
        let parts = SamplePart::default_bank();
        let mut bank = VolcaSamplerBank::new(sr);
        bank.sync_parts(&parts);
        assert_eq!(bank.active_parts_mask(), 0);

        bank.trigger_part(2, 0.9, &parts);
        assert_ne!(bank.active_parts_mask() & (1 << 2), 0);
        let (l, r, _, _) = bank.process_stereo(&parts, 1.0, 1.0);
        assert!(l.is_finite() && r.is_finite());

        bank.all_notes_off();
        for _ in 0..((sr * 1.2) as usize) {
            let (sl, sr_v, _, _) = bank.process_stereo(&parts, 1.0, 1.0);
            assert!(sl.is_finite() && sr_v.is_finite());
        }
        assert_eq!(bank.active_parts_mask() & (1 << 2), 0);
    }

    #[test]
    fn auto_arrange_capture_dj_song_and_extra_mask_work() {
        use crate::model::{
            auto_arrange_project_song, capture_party_song_to_project, ExtraTrack,
        };

        let mut p = Project::default();
        p.extra_tracks.push(ExtraTrack::default());
        let seq = MelodicTrackSeq::with_default_note(64, crate::model::StepCount::Steps16);
        for pat in &mut p.patterns {
            pat.extra_seqs.push(seq.clone());
        }

        // 1. Auto-Arrange Loop -> Song
        auto_arrange_project_song(&mut p, 0xCAFE_BABE);
        assert!(p.song_mode);
        assert!(!p.party_mode);
        assert_eq!(p.song_sections.len(), 7);
        assert_eq!(p.song_sections[0].extra_mask, 0);
        assert_eq!(p.song_sections[1].extra_mask, 0x03FF);

        // Render Intro (extra masked out) then Groove (extra enabled)
        let mut rt = Runtime::new(44_100.0);
        let mut tel = EngineTelemetry::default();
        let mut peak = 0.0_f32;
        for _ in 0..22_050 {
            let (l, r) = rt.process_sample(&p, true, false, false, 0, &mut tel);
            assert!(l.is_finite() && r.is_finite() && l.abs() <= 1.0 && r.abs() <= 1.0);
            peak = peak.max(l.abs()).max(r.abs());
        }
        assert!(peak > 0.05, "Auto-arranged song rendered silent");
        assert_eq!(tel.active_song_section, Some(0));

        // 2. Capture active Party DJ song into Studio & Song Arranger
        p.party_mode = true;
        p.song_mode = false;
        let mut dj_rt = Runtime::new(44_100.0);
        let mut dj_tel = EngineTelemetry::default();
        for _ in 0..8_192 {
            let _ = dj_rt.process_sample(&p, true, false, false, 0, &mut dj_tel);
        }
        let mut captured = p.clone();
        let title = capture_party_song_to_project(&mut captured, &dj_tel);
        assert!(!title.is_empty());
        assert!(!captured.party_mode && captured.song_mode && !captured.auto_beat_mode);
        assert!(!captured.song_sections.is_empty());
        assert!(captured.patterns[1].drums.lanes[0].steps.iter().any(|s| s.active));
    }

    #[test]
    fn dj_crates_tempo_bridge_and_up_next_queue_work() {
        use crate::model::{party_crate_vibes, pick_party_vibe_crate, PartyMixMode};

        let club_pool = party_crate_vibes(PartyMixMode::ClubCrate, Vibe::DeepHouse, 0x00FF_FFFF);
        assert!(club_pool.contains(&Vibe::DetroitTechno));

        let mut prev = Vibe::DeepHouse;
        for cycle in 1..=12 {
            let v = pick_party_vibe_crate(
                Vibe::DeepHouse,
                PartyMixMode::AllVibes,
                0x00FF_FFFF,
                cycle,
                0x1357_9BDF,
            );
            assert_ne!(v, prev, "Must never repeat back-to-back on cycle {cycle}");
            let b1 = prev.default_bpm();
            let b2 = v.default_bpm();
            let bridged = (b2 - b1).abs() <= 28.0
                || (b2 - b1 * 0.5).abs() <= 16.0
                || (b2 - b1 * 2.0).abs() <= 16.0;
            assert!(bridged, "Tempo jump {prev:?} ({b1}) -> {v:?} ({b2}) exceeded bridge");
            prev = v;
        }

        // Custom crate bitmask (only Synthwave + Eurobeat)
        let mask = (1u32 << (Vibe::Synthwave as usize)) | (1u32 << (Vibe::Eurobeat as usize));
        let custom = party_crate_vibes(PartyMixMode::CustomCrate, Vibe::DeepHouse, mask);
        assert_eq!(custom, vec![Vibe::Synthwave, Vibe::Eurobeat]);

        // Up-Next queue forces next song vibe & populates telemetry
        let mut p = Project::default();
        p.party_mode = true;
        let mut rt = Runtime::new(44_100.0);
        let mut tel = EngineTelemetry::default();
        rt.handle_live_event(LiveEvent::QueueNextPartyVibe(Some(Vibe::Eurobeat)), &p);
        let _ = rt.process_sample(&p, true, false, false, 0, &mut tel);
        assert_eq!(tel.party_queued_vibe, Some(Vibe::Eurobeat));
        assert_eq!(tel.party_next_vibe, Vibe::Eurobeat.name());
        assert!((tel.party_next_bpm - Vibe::Eurobeat.default_bpm()).abs() < 0.1);

        rt.handle_live_event(LiveEvent::NextPartySong, &p);
        for _ in 0..8_192 {
            let _ = rt.process_sample(&p, true, false, false, 0, &mut tel);
        }
        assert_eq!(tel.party_vibe, Vibe::Eurobeat);
    }

    #[test]
    fn all_156_studio_presets_and_user_patch_round_trip_work() {
        use crate::model::voice_preset_names;
        use crate::settings::{UserPatch, UserPatchData};

        let mut total = 0usize;
        for &track in &TrackId::STUDIO_TRACKS {
            let names = voice_preset_names(track);
            total += names.len();
            for (pi, &pname) in names.iter().enumerate() {
                let mut p = Project::default();
                apply_voice_preset(&mut p, track, pi);
                let mut rt = Runtime::new(44_100.0);
                let mut tel = EngineTelemetry::default();
                for _ in 0..4096 {
                    let (l, r) = rt.process_sample(&p, true, false, false, 0, &mut tel);
                    assert!(
                        l.is_finite() && r.is_finite() && l.abs() <= 1.0 && r.abs() <= 1.0,
                        "Preset {track:?}[{pi}] '{pname}' clipped/NaN"
                    );
                }
            }
        }
        assert_eq!(total, 156, "Expected 156 Studio presets across 6 tracks");

        let p = Project::default();
        let patch = UserPatch {
            name: "Test Patch".into(),
            track_role: "BASS".into(),
            data: UserPatchData::Moog(p.moog.clone()),
        };
        let json = serde_json::to_string(&patch).expect("serialize user patch");
        let back: UserPatch = serde_json::from_str(&json).expect("deserialize user patch");
        assert_eq!(patch, back);
    }

    #[test]
    fn wavetable_mipmaps_and_acoustic_linn_drums_render_cleanly() {
        use crate::dsp::WavetableSynth;
        use crate::model::{DrumKitParams, DrumModel};

        let sr = 44_100.0_f32;
        let mut wt = WavetableSynth::new(sr);
        for bank in 0..6u8 {
            for &note in &[84u8, 96, 104] {
                wt.all_notes_off();
                wt.note_on(note, 0.9, false, false, true, 0.0);
                let mut energy = 0.0_f32;
                for _ in 0..2048 {
                    let (l, r) = wt.process_stereo(
                        bank, 0.65, 16_000.0, 0.2, 0.15, 0.2, 0.005, 0.2, 0.7, 0.2, 0.004, 0.3,
                        0.8, 0.2, 0.0, true, 0.0, 0.0,
                    );
                    assert!(l.is_finite() && r.is_finite() && l.abs() <= 1.5 && r.abs() <= 1.5);
                    energy += l * l + r * r;
                }
                assert!(energy > 0.01, "Wavetable bank {bank} note {note} silent");
            }
        }

        let mut dm = DrumMachine::new(sr);
        for model in [DrumModel::Acoustic, DrumModel::Linn80s] {
            let mut kit = DrumKitParams::default();
            kit.set_all_model(model);
            for lane in 0..8 {
                let v = DrumVoiceId::from_index(lane).unwrap();
                dm.trigger(v, 0.92, true, false, &kit);
                let mut peak = 0.0_f32;
                for _ in 0..4096 {
                    let (l, r) = dm.process_sample(&kit);
                    assert!(l.is_finite() && r.is_finite() && l.abs() <= 1.5 && r.abs() <= 1.5);
                    peak = peak.max(l.abs()).max(r.abs());
                }
                assert!(peak > 0.04, "Drum model {model:?} voice {v:?} too quiet: {peak}");
            }
        }
    }

    #[test]
    fn clock_div_and_melodic_probability_shape_step_timing() {
        let mut base = Project::default();
        base.bpm = 120.0;
        base.auto_beat_mode = false;
        base.party_mode = false;
        base.song_mode = false;
        base.current_pattern_mut().moog.clear_active();
        for ch in &mut base.mixer.channels {
            ch.mute = true;
            ch.solo = false;
        }
        base.mixer.moog_pad_mute = true;
        base.mixer.moog_pad_solo = false;
        // Monitor TB-303 only
        base.mixer.channels[TrackId::Tb303.index()].mute = false;
        base.mixer.channels[TrackId::Tb303.index()].solo = true;

        // 1) Eighth clock_div stretches notes at half rate vs ThirtySecond
        let mut p_eighth = base.clone();
        p_eighth.current_pattern_mut().tb303.clock_div = ClockDiv::Eighth;
        for i in 0..16 {
            p_eighth.current_pattern_mut().tb303.steps[i] = MelodicStep {
                active: i == 0,
                note: 48,
                velocity: 0.9,
                gate: 0.5,
                ..Default::default()
            };
        }
        let mut rt_8 = Runtime::new(44_100.0);
        let mut tel_8 = EngineTelemetry::default();
        // Global tick 0 triggers step 0; global tick 1 should NOT re-trigger even if step 1 were active
        p_eighth.current_pattern_mut().tb303.steps[1].active = true;
        rt_8.trigger_step(&p_eighth, false, false, &mut tel_8);
        assert!(rt_8.last_acid_note == Some(48));
        rt_8.last_acid_note = None;
        rt_8.step = 1;
        rt_8.trigger_step(&p_eighth, false, false, &mut tel_8);
        assert_eq!(rt_8.last_acid_note, None, "Odd global tick under 1/8 div must skip");

        // 2) ThirtySecond clock_div queues sub-step trigger
        let mut p_32 = base.clone();
        p_32.current_pattern_mut().tb303.clock_div = ClockDiv::ThirtySecond;
        p_32.current_pattern_mut().tb303.steps[0].active = true;
        p_32.current_pattern_mut().tb303.steps[1] = MelodicStep {
            active: true,
            note: 55,
            velocity: 0.92,
            gate: 0.5,
            ..Default::default()
        };
        let mut rt_32 = Runtime::new(44_100.0);
        let mut tel_32 = EngineTelemetry::default();
        rt_32.trigger_step(&p_32, false, false, &mut tel_32);
        assert!(
            !rt_32.pending_triggers.is_empty(),
            "Expected 1/32 division to schedule second half-step trigger"
        );

        // 3) MelodicStep probability = 0.0 suppresses firing
        let mut p_zero = base;
        p_zero.current_pattern_mut().tb303.steps[0] = MelodicStep {
            active: true,
            note: 48,
            velocity: 0.9,
            gate: 0.5,
            probability: 0.0,
            ..Default::default()
        };
        let mut rt_z = Runtime::new(44_100.0);
        let mut tel_z = EngineTelemetry::default();
        rt_z.trigger_step(&p_zero, false, false, &mut tel_z);
        assert_eq!(rt_z.last_acid_note, None, "Zero probability step must not trigger");
    }

    #[test]
    fn standard_midi_export_and_stereo_width_mono_sub_track_peaks_work() {
        use crate::midi::export_project_midi_bytes;
        use crate::model::{Scale, SongSection};

        // 1) Standard Format-1 MIDI export for both pattern and arranged song
        let mut p = Project::default();
        p.song_mode = true;
        p.song_sections = vec![
            SongSection {
                name: "Intro".into(),
                pattern: 0,
                bars: 2,
                ..Default::default()
            },
            SongSection {
                name: "Drop".into(),
                pattern: 1,
                bars: 2,
                ..Default::default()
            },
        ];
        let root = p.root_note;
        p.current_pattern_mut().moog.fill_euclidean(5, 1, root, Scale::Minor, 2, 77);
        p.current_pattern_mut().moog.transpose(2, root, Scale::Minor, true);
        p.current_pattern_mut().moog.humanize_velocities(88, 0.12);
        let bytes = export_project_midi_bytes(&p).expect("export SMF midi bytes");
        assert!(bytes.len() > 64);
        assert_eq!(&bytes[0..4], b"MThd");
        assert_eq!(u16::from_be_bytes([bytes[8], bytes[9]]), 1, "Format-1 MIDI expected");
        assert!(u16::from_be_bytes([bytes[10], bytes[11]]) >= 2, "Expected conductor + instrument tracks");
        assert_eq!(u16::from_be_bytes([bytes[12], bytes[13]]), 480, "480 PPQ expected");

        // 2) Width = 0.0 folds stereo channel cleanly to mono & populates track_peaks
        let mut pm = Project::default();
        pm.current_pattern_mut().moog_pad.steps[0] = MelodicStep {
            active: true,
            note: 60,
            velocity: 0.9,
            gate: 4.0,
            chord: crate::model::ChordType::Minor,
            ..Default::default()
        };
        for ch in &mut pm.mixer.channels {
            ch.solo = false;
            ch.mute = true;
        }
        pm.mixer.moog_pad_mute = false;
        pm.mixer.moog_pad_solo = true;
        pm.mixer.moog_pad_width = 0.0;
        pm.mixer.moog_pad_delay_enabled = false;
        pm.mixer.moog_pad_reverb_enabled = false;
        pm.fx.delay_enabled = false;
        pm.fx.reverb_enabled = false;
        pm.fx.stereo_chorus = 0.0;
        pm.mixer.mono_sub = true;
        let mut rt = Runtime::new(44_100.0);
        let mut tel = EngineTelemetry::default();
        let mut max_side = 0.0_f32;
        for _ in 0..4096 {
            let (l, r) = rt.process_sample(&pm, true, false, false, 0, &mut tel);
            max_side = max_side.max((l - r).abs());
        }
        assert!(max_side < 0.01, "Width 0.0 should collapse stereo side diff, got {max_side}");
        assert!(tel.track_peaks[5] > 0.01, "Expected track_peaks[5] (MoogPad) > 0, got {}", tel.track_peaks[5]);
    }
}
