use crate::{
    dsp::{
        sampler::{generate_builtin_sample, VolcaSamplerBank},
        DrumMachine, MasterFx, MonologueSynth, MoogSynth, Sk1Sampler, Tb303Synth,
    },
    model::{
        arp_tones_for, bass_midi_for, build_party_song_rig, chord_symbol, generate_auto_beat,
        generate_melodic_tracks, party_song_title, pick_party_vibe, progression_at,
        vibe_progressions, voice_led_chord, ChordSlot, DrumVoiceId, GlobalFilterMode,
        MelodicStep, MelodicTrackSeq, MotionLocks, PartyPace, PartySongRig, Project, TrackId, Vibe,
        MAX_EXTRA_TRACKS,
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
    TriggerFill,
    NextPartySection,
    NextPartySong,
    TriggerSamplePart {
        part: usize,
        velocity: f32,
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
    pub party_bpm: f32,
    pub party_vibe_name: String,
    pub peak_l: f32,
    pub peak_r: f32,
    pub sidechain_duck: f32,
    pub sampler_playhead: Option<f32>,
}

#[derive(Debug, Clone, Copy)]
struct RatchetEvent {
    lane: usize,
    samples_left: f32,
    vel: f32,
    accent: bool,
}

/// Simple stereo Haas micro-delay widener so pads and arps bloom wide while
/// kick/snare/bass stay focused in the center.
struct StereoWidener {
    buf: Vec<f32>,
    pos: usize,
    delay_samples: usize,
}

impl StereoWidener {
    fn new(sr: f32, ms: f32) -> Self {
        let d = ((sr * ms * 0.001).round() as usize).clamp(1, 2048);
        Self {
            buf: vec![0.0; d + 2],
            pos: 0,
            delay_samples: d,
        }
    }

    #[inline]
    fn process(&mut self, mono: f32, width: f32) -> (f32, f32) {
        let idx = (self.pos + self.buf.len() - self.delay_samples) % self.buf.len();
        let delayed = self.buf[idx];
        self.buf[self.pos] = mono;
        self.pos = (self.pos + 1) % self.buf.len();
        let l = mono;
        let r = mono * (1.0 - width) + delayed * width;
        (l, r)
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

// Template 0: Club Journey (20 bars at Normal pace)
const PARTY_TMPL_CLUB: [PartySectionDef; 7] = [
    PartySectionDef {
        name: "INTRO",
        bars: 2,
        mask: 0b001101,
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
        mask: 0b101111,
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
        mask: 0b111111,
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
        mask: 0b101011,
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

// Template 1: Instant Groove & Acid Solo (20 bars at Normal pace)
const PARTY_TMPL_GROOVE_SOLO: [PartySectionDef; 6] = [
    PartySectionDef {
        name: "LOCK IN",
        bars: 4,
        mask: 0b100011,
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
        mask: 0b011011,
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
        mask: 0b110100,
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
        mask: 0b111111,
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
        mask: 0b001011,
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
        mask: 0b101100,
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
        mask: 0b001111,
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
        mask: 0b111111,
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
        mask: 0b110001,
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
        mask: 0b111111,
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
        mask: 0b101111,
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
        mask: 0b111111,
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
        mask: 0b111100,
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
        mask: 0b111111,
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
    pad_widener: StereoWidener,
    arp_widener: StereoWidener,
    sampler_widener: StereoWidener,
    // Timing & sequencer
    tick_left: f32,
    step: usize,
    was_playing: bool,
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
    // Sidechain ducking envelope
    sidechain_env: f32,
    sidechain_decay: f32,
    // Sub bass dive (key-locked!)
    drop_phase: f32,
    drop_env: f32,
    drop_pitch: f32,
    drop_target_hz: f32,
    // Drum & Volca ratchets & fills
    ratchets: Vec<RatchetEvent>,
    volca_ratchets: Vec<RatchetEvent>,
    force_fill_steps: usize,
    // Live chord override
    live_chord_held: bool,
    live_slot: Option<ChordSlot>,
    // Sampler sync
    loaded_sample_gen: u64,
    // Party mode state
    party_section_idx: usize,
    party_bar_in_sec: usize,
    party_cycle: u32,
    party_evolution: u32,
    party_template_idx: usize,
    party_rig: Option<PartySongRig>,
    party_rig_sig: (u8, u8, u32, u32, bool),
    party_live_bpm: f32,
    last_base_vibe: Option<Vibe>,
    was_party_mode: bool,
    builtin_sample_cache: [Option<Arc<Vec<f32>>>; 8],
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
            pad_widener: StereoWidener::new(sr, 11.5),
            arp_widener: StereoWidener::new(sr, 7.2),
            sampler_widener: StereoWidener::new(sr, 9.0),
            tick_left: 0.0,
            step: 0,
            was_playing: false,
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
            sidechain_env: 0.0,
            sidechain_decay,
            drop_phase: 0.0,
            drop_env: 0.0,
            drop_pitch: 65.0,
            drop_target_hz: 32.7,
            ratchets: Vec::with_capacity(16),
            volca_ratchets: Vec::with_capacity(16),
            force_fill_steps: 0,
            live_chord_held: false,
            live_slot: None,
            loaded_sample_gen: 0,
            party_section_idx: 0,
            party_bar_in_sec: 0,
            party_cycle: 0,
            party_evolution: 0,
            party_template_idx: 0,
            party_rig: None,
            party_rig_sig: (255, 255, u32::MAX, 0, false),
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
        }
    }

    fn sync_sampler_if_needed(&mut self, p: &Project) {
        if self.loaded_sample_gen == p.sample_generation {
            return;
        }
        self.loaded_sample_gen = p.sample_generation;
        if let Some(samples) = p.sample_data.decode_f32_pcm() {
            if !samples.is_empty() {
                let _ = self
                    .sampler
                    .swap_buffer(Arc::new(samples), p.sample_data.sample_rate.max(8_000));
                return;
            }
        }
        let builtin = p.sample_data.builtin_id.unwrap_or(3);
        let buf = generate_builtin_sample(builtin, self.sr as u32);
        let _ = self.sampler.swap_buffer(Arc::new(buf), self.sr as u32);
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

    fn ensure_party_rig(&mut self, p: &Project) {
        let sig = (
            p.vibe as u8,
            p.party_mix_mode as u8,
            self.party_cycle,
            p.party_seed,
            p.party_sound_morph,
        );
        if self.party_rig_sig == sig && self.party_rig.is_some() {
            return;
        }
        let song_vibe = pick_party_vibe(p.vibe, p.party_mix_mode, self.party_cycle, p.party_seed);
        let rig = build_party_song_rig(p, song_vibe, self.party_cycle, p.party_seed, p.party_sound_morph);

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

        if p.party_sound_morph && p.sample_data.builtin_id.is_some() {
            if let Some(id) = rig.sampler_builtin_id {
                if self.active_party_sample_id != Some(id) {
                    let slot = (id as usize) % self.builtin_sample_cache.len();
                    if self.builtin_sample_cache[slot].is_none() {
                        let pcm = generate_builtin_sample(id, self.sr as u32);
                        self.builtin_sample_cache[slot] = Some(Arc::new(pcm));
                    }
                    if let Some(buf) = &self.builtin_sample_cache[slot] {
                        let _ = self.sampler.swap_buffer(Arc::clone(buf), self.sr as u32);
                        self.active_party_sample_id = Some(id);
                    }
                }
            }
        }

        if self.party_live_bpm < 40.0 {
            self.party_live_bpm = p.bpm;
        }
        self.party_rig = Some(rig);
        self.party_rig_sig = sig;
    }

    fn trigger_bass_drop(&mut self, root_note: u8) {
        let low_midi = (24 + (root_note % 12)) as f32;
        let high_hz = crate::model::midi_to_hz(low_midi + 24.0);
        let low_hz = crate::model::midi_to_hz(low_midi);
        self.drop_pitch = high_hz;
        self.drop_target_hz = low_hz;
        self.drop_env = 1.0;
        self.drop_phase = 0.0;
    }

    fn handle_live_event(&mut self, ev: LiveEvent, p: &Project) {
        match ev {
            LiveEvent::PlayChord {
                notes,
                bass,
                velocity,
                slot,
            } => {
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
            LiveEvent::ReleaseChord => {
                self.pad_moog.all_notes_off();
                if let Some(b) = self.last_moog_note.take() {
                    self.moog.note_off(b);
                }
                self.live_chord_held = false;
                self.gate_pad = 0.0;
                self.gate_moog = 0.0;
            }
            LiveEvent::PlayNote {
                track,
                note,
                velocity,
            } => match track {
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
                            self.sidechain_env = velocity;
                        }
                    }
                }
                TrackId::MidiOut => {}
            },
            LiveEvent::ReleaseNote { track, note } => match track {
                TrackId::Moog => self.moog.note_off(note),
                TrackId::MoogPad => self.pad_moog.note_off(note),
                TrackId::Monologue => self.mono.note_off(note),
                TrackId::Tb303 => self.acid.note_off(note),
                TrackId::Sampler => self.sampler.note_off(note),
                TrackId::Drums | TrackId::MidiOut => {}
            },
            LiveEvent::TriggerBassDrop => {
                let root = if p.party_mode { self.party_dj_root(p) } else { p.root_note };
                self.trigger_bass_drop(root);
            }
            LiveEvent::TriggerFill => self.force_fill_steps = 8,
            LiveEvent::NextPartySection => {
                let tmpl = PARTY_TEMPLATES[self.party_template_idx % PARTY_TEMPLATES.len()];
                self.party_section_idx = (self.party_section_idx + 1) % tmpl.len();
                self.party_bar_in_sec = 0;
                if self.party_section_idx == 0 {
                    self.party_cycle = self.party_cycle.wrapping_add(1);
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
                    self.trigger_bass_drop(song_root);
                }
            }
            LiveEvent::NextPartySong => {
                // DJ mix straight into a brand-new song with fresh vibe, template, and voice palette!
                self.party_cycle = self.party_cycle.wrapping_add(1);
                self.party_evolution = self.party_evolution.wrapping_add(1);
                self.party_section_idx = 0;
                self.party_bar_in_sec = 0;
                self.force_fill_steps = 8;
                self.party_rig = None;
                self.ensure_party_rig(p);
                let song_root = self.party_dj_root(p);
                self.trigger_bass_drop(song_root);
            }
            LiveEvent::TriggerSamplePart { part, velocity } => {
                self.volca.sync_parts(&p.sample_parts);
                self.volca.trigger_part(part, velocity, &p.sample_parts);
            }
            LiveEvent::ReleaseSamplePart { part } => {
                self.volca.release_part(part);
            }
            LiveEvent::PlayExtraNote { idx, note, velocity } => {
                if let (Some(synth), Some(tr)) = (self.extra_synths.get_mut(idx), p.extra_tracks.get(idx)) {
                    synth.note_on(note, velocity, false, false, &tr.synth);
                }
            }
            LiveEvent::ReleaseExtraNote { idx, note } => {
                if let Some(synth) = self.extra_synths.get_mut(idx) {
                    synth.note_off(note);
                }
            }
        }
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
        let mut section_fill = false;
        let mut party_snare_roll = false;
        let mut party_section_energy = p.party_energy;
        let mut song_vibe = p.vibe;
        let mut active_bpm = p.bpm;
        let mut sec_lead_bright = 0.75_f32;
        let mut sec_acid_int = 0.5_f32;

        if p.party_mode {
            if self.last_base_vibe != Some(p.vibe) {
                self.party_cycle = 0;
                self.party_section_idx = 0;
                self.party_bar_in_sec = 0;
                self.party_evolution = 0;
                self.party_live_bpm = p.bpm;
                self.party_rig = None;
                self.last_base_vibe = Some(p.vibe);
            }
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
                        self.force_fill_steps = 8; // DJ song mix transition fill!
                        self.party_rig = None;
                        self.ensure_party_rig(p);
                        let new_root = self.party_dj_root(p);
                        self.trigger_bass_drop(new_root);
                    }
                    let next_tmpl = PARTY_TEMPLATES[self.party_template_idx % PARTY_TEMPLATES.len()];
                    let next_sec = &next_tmpl[self.party_section_idx % next_tmpl.len()];
                    if next_sec.evolve_progression {
                        self.party_evolution = self.party_evolution.wrapping_add(1);
                    }
                    if next_sec.drop_on_start {
                        let new_root = self.party_dj_root(p);
                        self.trigger_bass_drop(new_root);
                    }
                }
            }

            let (rig_bpm, rig_vibe, rig_palette) = if let Some(rig) = &self.party_rig {
                (rig.bpm, rig.vibe, rig.palette_label.clone())
            } else {
                (p.bpm, p.vibe, String::new())
            };
            song_vibe = rig_vibe;
            if self.party_live_bpm < 40.0 {
                self.party_live_bpm = rig_bpm;
            } else {
                self.party_live_bpm += (rig_bpm - self.party_live_bpm) * 0.08;
            }
            active_bpm = self.party_live_bpm.clamp(40.0, 240.0);

            let tmpl = PARTY_TEMPLATES[self.party_template_idx % PARTY_TEMPLATES.len()];
            let sec_idx = self.party_section_idx % tmpl.len();
            let sec = &tmpl[sec_idx];
            let sec_bars = scaled_section_bars(sec.bars, p.party_pace);
            section_mask = sec.mask;
            party_section_energy = (p.party_energy * sec.energy_mul).clamp(0.18, 1.0);
            sec_lead_bright = sec.lead_brightness;
            sec_acid_int = sec.acid_intensity;
            if p.party_energy < 0.35 {
                section_mask &= !0b010000; // pull 303 on chill energy
            }

            let bar_progress =
                (self.party_bar_in_sec * 16 + (global % 16)) as f32 / (sec_bars * 16).max(1) as f32;
            match sec.filter_sweep {
                PartyFilterSweep::LowPassOpen => {
                    self.party_filter_target_hz = 260.0 * (18_000.0_f32 / 260.0).powf(bar_progress);
                    self.party_filter_mode = GlobalFilterMode::LowPass;
                    self.party_filter_res_boost = 0.24;
                }
                PartyFilterSweep::LowPassClose => {
                    self.party_filter_target_hz = 16_000.0 * (380.0_f32 / 16_000.0).powf(bar_progress);
                    self.party_filter_mode = GlobalFilterMode::LowPass;
                    self.party_filter_res_boost = 0.26;
                }
                PartyFilterSweep::HighPassRise => {
                    self.party_filter_target_hz = 55.0 * (950.0_f32 / 55.0).powf(bar_progress);
                    self.party_filter_mode = GlobalFilterMode::HighPass;
                    self.party_filter_res_boost = 0.30;
                }
                PartyFilterSweep::AtmosBreath => {
                    self.party_filter_target_hz = 520.0 * (10_500.0_f32 / 520.0).powf(bar_progress);
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
            self.party_bass_cut_target = (0.78 + party_section_energy * 0.48 + wave * 0.22).clamp(0.55, 1.75);
            self.party_pad_cut_target = (0.68 + sec.lead_brightness * 0.45 + wave * 0.28).clamp(0.50, 1.95);
            self.party_lead_cut_target = (0.70 + sec.lead_brightness * 0.55 - wave * 0.24).clamp(0.55, 2.10);
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
                if fx_roll < 0.45 {
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
            telemetry.party_bpm = active_bpm;
            telemetry.party_vibe_name = song_vibe.name().to_string();
        } else if p.song_mode && !p.song_sections.is_empty() {
            self.party_filter_target_hz = 18_000.0;
            self.party_filter_mode = GlobalFilterMode::LowPass;
            self.party_filter_res_boost = 0.0;
            self.party_space_target = 0.0;
            self.party_repeat_fx = 0.0;
            self.party_glitch_fx = 0.0;
            telemetry.party_palette.clear();
            telemetry.party_vibe_name.clear();
            telemetry.party_bpm = 0.0;

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
            for sec in &p.song_sections {
                let len = sec.bars.max(1) as usize * 16;
                if rem < len {
                    pat_idx = sec.pattern.min(p.patterns.len().saturating_sub(1));
                    section_mask = sec.track_mask;
                    if sec.bass_drop && rem == 0 {
                        self.trigger_bass_drop(p.root_note);
                    }
                    section_fill = sec.fill && rem + 8 >= len;
                    telemetry.party_section = sec.name.clone();
                    telemetry.party_bar = rem / 16 + 1;
                    telemetry.party_section_bars = sec.bars.max(1) as usize;
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
            telemetry.party_vibe_name.clear();
            telemetry.party_bpm = 0.0;
            telemetry.party_section.clear();
            telemetry.party_bar = (global / 16) % 4 + 1;
            telemetry.party_section_bars = 4;
        }

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

        // --- 0. MOTION AUTOMATION PLAYBACK ---
        let m_step = global % 64;
        self.auto_filter = pat.motion.filter.get(m_step).copied().flatten();
        self.auto_space = pat.motion.space.get(m_step).copied().flatten();
        self.auto_sidechain = pat.motion.sidechain.get(m_step).copied().flatten();
        self.auto_glitch = pat.motion.glitch.get(m_step).copied().flatten();
        self.auto_iso_bass = pat.motion.iso_bass.get(m_step).copied().flatten();
        self.auto_iso_treble = pat.motion.iso_treble.get(m_step).copied().flatten();

        // --- 1. DRUMS (bit 0) ---
        if !drum_cut && (section_mask & 0b000001) != 0 {
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

            let phase = global % 16;
            for lane in 0..8 {
                let d_lane = &drum_seq.lanes[lane];
                let d_len = d_lane.step_count.as_usize();
                let st = d_lane.steps[global % d_len];
                if st.active && (st.probability >= 0.999 || hash_f32(seed, global, lane as u32) <= st.probability) {
                    if let Some(voice) = DrumVoiceId::from_index(lane) {
                        // Subtle velocity humanization + per-step Pitch/Pan P-Locks!
                        let human = 0.94 + hash_f32(seed, global, lane as u32 + 99) * 0.12;
                        let vel = (st.velocity * human).clamp(0.1, 1.0);
                        let drums_p = if p.party_mode {
                            self.party_rig.as_ref().map(|r| &r.drums).unwrap_or(&p.drums)
                        } else {
                            &p.drums
                        };
                        if st.pitch_offset != 0 || st.pan_lock.is_some() {
                            let mut dk = drums_p.clone();
                            dk.voices[lane].tune = (dk.voices[lane].tune + st.pitch_offset as f32).clamp(-12.0, 12.0);
                            if let Some(pan) = st.pan_lock {
                                dk.voices[lane].pan = pan.clamp(-1.0, 1.0);
                            }
                            self.drums.trigger(voice, vel, st.accent, st.flam, &dk);
                        } else {
                            self.drums.trigger(voice, vel, st.accent, st.flam, drums_p);
                        }
                        if voice == DrumVoiceId::Kick {
                            self.sidechain_env = vel;
                        }
                        if st.ratchet > 1 {
                            let n = st.ratchet.min(4) as usize;
                            let sub_len = base_step_samples / n as f32;
                            for r in 1..n {
                                self.ratchets.push(RatchetEvent {
                                    lane,
                                    samples_left: sub_len * r as f32,
                                    vel: vel * (0.85 - r as f32 * 0.12).max(0.35),
                                    accent: false,
                                });
                            }
                        }
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
                    let drums_p = if p.party_mode {
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
                    let drums_p = if p.party_mode {
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
            let mut st = seq.steps[global % seq.step_count.as_usize()];
            if p.party_mode {
                let step16 = global % 16;
                let bass_style = (party_phrase + self.party_section_idx as u32) % 4;
                // Evolve rhythmic bass feel per 4-bar phrase & section with funk slaps & chromatic walkups
                if !st.active && (step16 == 3 || step16 == 7 || step16 == 11) && party_section_energy >= 0.55 && hash_f32(seed, global, 17) < 0.42 {
                    // Syncopated 16th-note thumb slap with snappy auto-wah quack
                    st = MelodicStep {
                        active: true,
                        note: (bass_root + if step16 == 11 { 10 } else { 12 }).min(60),
                        velocity: 0.82,
                        gate: 0.48,
                        accent: true,
                        motion: MotionLocks {
                            cutoff: Some(0.80),
                            resonance: Some(0.70),
                            drive: Some(0.64),
                            shape: Some(0.65),
                        },
                        ..Default::default()
                    };
                } else if !st.active && step16 == 15 && hash_f32(seed, global, 18) < 0.55 {
                    // Chromatic half-step walkup slide into the next bar's "One"
                    let next_slot = prog.slots[(chord_idx + 1) % 4];
                    let next_bass = bass_midi_for(dj_root, next_slot.offset);
                    st = MelodicStep {
                        active: true,
                        note: next_bass.saturating_sub(1).max(33),
                        velocity: 0.78,
                        gate: 0.90,
                        slide: true,
                        motion: MotionLocks {
                            cutoff: Some(0.76),
                            resonance: Some(0.66),
                            drive: Some(0.62),
                            shape: Some(0.60),
                        },
                        ..Default::default()
                    };
                } else if bass_style == 1 && (step16 == 6 || step16 == 14) && !st.active {
                    // Syncopated octave bounce
                    st = MelodicStep {
                        active: true,
                        note: (bass_root + 12).min(60),
                        velocity: 0.78,
                        gate: 0.9,
                        slide: step16 == 14,
                        motion: MotionLocks {
                            cutoff: Some(0.76),
                            resonance: Some(0.64),
                            drive: Some(0.60),
                            shape: Some(0.60),
                        },
                        ..Default::default()
                    };
                } else if bass_style == 2 && party_section_energy >= 0.75 && step16 % 2 == 0 && !st.active {
                    // Driving offbeat 8th-note pump during high-energy Groove/Drops
                    let oct = if step16 % 4 == 2 { 12 } else { 0 };
                    st = MelodicStep {
                        active: true,
                        note: (bass_root + oct).min(60),
                        velocity: 0.74,
                        gate: 0.65,
                        ..Default::default()
                    };
                } else if bass_style == 3 && step16 == 10 && !st.active && hash_f32(seed, global, 13) < 0.65 {
                    // Fifth / octave turnaround pickup
                    let jump = if hash_f32(seed, global, 14) < 0.5 { 7 } else { 12 };
                    st = MelodicStep {
                        active: true,
                        note: (bass_root + jump).min(60),
                        velocity: 0.76,
                        gate: 1.1,
                        slide: true,
                        motion: MotionLocks {
                            cutoff: Some(0.74),
                            resonance: Some(0.60),
                            drive: Some(0.56),
                            shape: None,
                        },
                        ..Default::default()
                    };
                } else if !st.active && step16 == 14 && hash_f32(seed, global, 11) < 0.45 {
                    st = MelodicStep {
                        active: true,
                        note: (bass_root + 12).min(60),
                        velocity: 0.72,
                        gate: 1.2,
                        slide: true,
                        motion: MotionLocks {
                            cutoff: Some(0.78),
                            resonance: Some(0.66),
                            drive: Some(0.60),
                            shape: None,
                        },
                        ..Default::default()
                    };
                }
            }
            if st.active {
                let note = st.note;
                if st.motion.any_locked() {
                    let mut m = st.motion;
                    if p.party_mode && p.party_sound_morph {
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
                let moog_p = if p.party_mode {
                    self.party_rig.as_ref().map(|r| &r.moog).unwrap_or(&p.moog)
                } else {
                    &p.moog
                };
                self.moog
                    .note_on(note, st.velocity, st.accent, st.slide, moog_p);
                self.last_moog_note = Some(note);
                self.gate_moog = base_step_samples * st.gate.clamp(0.15, 16.0);
            }
        }

        // --- 3. CHORDS / MOOG PAD (bit 2) ---
        if (section_mask & 0b000100) != 0 && !follow_override {
            let seq = if p.party_mode { &self.party_melodic.1 } else { &pat.moog_pad };
            let mut st = seq.steps[global % seq.step_count.as_usize()];
            if p.party_mode {
                let step16 = global % 16;
                let chord_rhythm = (party_phrase + self.party_section_idx as u32) % 3;
                if !st.active && (step16 == 3 || step16 == 7 || step16 == 15) && party_section_energy >= 0.55 && hash_f32(seed, global, 21) < 0.48 {
                    // Tight 16th-note clav / brass funk chuck
                    st = MelodicStep {
                        active: true,
                        note: dj_root,
                        velocity: 0.72,
                        gate: 0.65,
                        accent: step16 == 15,
                        chord: active_slot.chord,
                        motion: MotionLocks {
                            cutoff: Some(0.78),
                            resonance: Some(0.48),
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
                        velocity: 0.68,
                        gate: 1.5,
                        chord: active_slot.chord,
                        motion: MotionLocks {
                            cutoff: Some(0.74),
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
                        velocity: 0.64,
                        gate: 3.0,
                        chord: active_slot.chord,
                        ..Default::default()
                    };
                }
            }
            if st.active {
                if st.motion.any_locked() {
                    let mut m = st.motion;
                    if p.party_mode && p.party_sound_morph {
                        if let Some(c) = m.cutoff {
                            m.cutoff = Some((c + (self.party_pad_cut_mul - 1.0) * 0.16).clamp(0.28, 0.94));
                        }
                    }
                    self.pad_moog.set_motion_locks(m);
                } else {
                    self.pad_moog.clear_motion_locks();
                }
                self.pad_moog.all_notes_off();
                let pad_p = if p.party_mode {
                    self.party_rig.as_ref().map(|r| &r.moog_pad).unwrap_or(&p.moog_pad)
                } else {
                    &p.moog_pad
                };
                if st.chord != crate::model::ChordType::Single {
                    // Voice-led lush chord!
                    let mut notes_to_play = if party_evolve_active || p.party_mode {
                        pad_notes.clone()
                    } else {
                        let slot = ChordSlot {
                            offset: (st.note + 12 - (dj_root % 12)) % 12,
                            chord: st.chord,
                        };
                        voice_led_chord(dj_root, slot, 60)
                    };
                    // Subtle octave lift on Drop 2 / Climax / odd phrases for harmonic bloom
                    if p.party_mode && sec_lead_bright >= 0.96 && (party_phrase % 2 == 1) {
                        for n in &mut notes_to_play {
                            if *n <= 72 {
                                *n += 12;
                            }
                        }
                    }
                    for n in notes_to_play {
                        self.pad_moog
                            .note_on(n, st.velocity, st.accent, false, pad_p);
                    }
                } else {
                    self.pad_moog
                        .note_on(st.note, st.velocity, st.accent, false, pad_p);
                }
                self.gate_pad = base_step_samples * st.gate.clamp(0.25, 16.0);
            }
        }

        // --- 3b. EXTRA DYNAMIC SYNTH TRACKS (tracks 7..=16) ---
        if (section_mask & 0b000100) != 0 && !follow_override {
            let extra_n = p.extra_tracks.len().min(MAX_EXTRA_TRACKS).min(pat.extra_seqs.len());
            for ei in 0..extra_n {
                let seq = &pat.extra_seqs[ei];
                let st = seq.steps[global % seq.step_count.as_usize()];
                if st.active {
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
                    self.extra_gates[ei] = base_step_samples * st.gate.clamp(0.25, 16.0);
                }
            }
        }

        // --- 4. ARP / MONOLOGUE (bit 3) ---
        if (section_mask & 0b001000) != 0 {
            let seq = if p.party_mode { &self.party_melodic.2 } else { &pat.monologue };
            let mut st = seq.steps[global % seq.step_count.as_usize()];
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
            // Preserve the song's generated melodic hook; add sparkle & sparse funky fills on peak sections
            if p.party_mode {
                if !st.active && sec_lead_bright >= 0.85 && global % 2 == 0 && hash_f32(seed, global, 23) < party_section_energy * 0.30 {
                    st = MelodicStep {
                        active: true,
                        note: pick_arp_note(global),
                        velocity: 0.64,
                        gate: 0.85,
                        motion: MotionLocks {
                            cutoff: Some(0.82),
                            resonance: Some(0.62),
                            drive: Some(0.46),
                            shape: Some(0.75),
                        },
                        ..Default::default()
                    };
                } else if st.active && sec_lead_bright >= 0.90 && global % 8 == 6 && st.note <= 74 {
                    st.note += 12;
                }
            }
            if st.active {
                let note = if follow_override {
                    pick_arp_note(global)
                } else {
                    st.note
                };
                if st.motion.any_locked() {
                    let mut m = st.motion;
                    if p.party_mode && p.party_sound_morph {
                        if let Some(c) = m.cutoff {
                            m.cutoff = Some((c + (self.party_lead_cut_mul - 1.0) * 0.16).clamp(0.28, 0.94));
                        }
                    }
                    self.mono.set_motion_locks(m, true);
                } else {
                    self.mono.clear_motion_locks();
                }
                if !st.slide {
                    if let Some(prev) = self.last_mono_note.take() {
                        self.mono.note_off(prev);
                    }
                }
                let mono_p = if p.party_mode {
                    self.party_rig.as_ref().map(|r| &r.monologue).unwrap_or(&p.monologue)
                } else {
                    &p.monologue
                };
                self.mono
                    .note_on(note, st.velocity, st.accent, st.slide, mono_p);
                self.last_mono_note = Some(note);
                self.gate_mono = base_step_samples * st.gate.clamp(0.15, 8.0);
            }
        }

        // --- 5. LEAD / TB-303 (bit 4) ---
        if (section_mask & 0b010000) != 0 {
            let seq = if p.party_mode { &self.party_melodic.3 } else { &pat.tb303 };
            let mut st = seq.steps[global % seq.step_count.as_usize()];
            // Use the song's generated 303 riff with section-aware octave lift & acid accents
            if p.party_mode {
                if st.active && sec_acid_int >= 0.80 {
                    if (global % 16) % 6 == 2 && st.note <= 64 {
                        st.note += 12;
                    }
                    if global % 4 == 2 {
                        st.accent = true;
                    }
                } else if !st.active && sec_acid_int >= 0.92 && (global % 4 == 2) && hash_f32(seed, global, 41) < 0.42 {
                    let tone = arp_tones[(global + self.party_section_idx) % arp_tones.len().max(1)];
                    st = MelodicStep {
                        active: true,
                        note: (48 + (tone % 12)).clamp(36, 72),
                        velocity: 0.78,
                        gate: 0.75,
                        accent: true,
                        slide: hash_f32(seed, global, 44) < 0.40,
                        ..Default::default()
                    };
                }
            }
            if st.active {
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
                self.acid
                    .note_on(note, st.velocity, st.accent, st.slide);
                self.last_acid_note = Some(note);
                self.gate_acid = base_step_samples * st.gate.clamp(0.15, 4.0);
            }
        }

        // --- 6. SAMPLER / 8-PART VOLCA BANK (bit 5) ---
        if (section_mask & 0b100000) != 0 {
            let seq = if p.party_mode { &self.party_melodic.4 } else { &pat.sampler };
            let st = seq.steps[global % seq.step_count.as_usize()];
            if st.active {
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
                let samp_p = if p.party_mode {
                    self.party_rig.as_ref().map(|r| &r.sampler).unwrap_or(&p.sampler)
                } else {
                    &p.sampler
                };
                self.sampler.note_on(note, st.velocity, samp_p);
                self.gate_sampler = base_step_samples * st.gate.clamp(0.25, 16.0);
            }

            // 8-Part Volca Sample Lanes!
            for (part_idx, lane) in pat.sample_lanes.iter().take(8).enumerate() {
                let len = lane.step_count.as_usize();
                let dst = lane.steps[global % len];
                if dst.active
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
                            });
                        }
                    }
                }
            }
        }
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
            self.party_rig = None;
            self.party_live_bpm = 0.0;
            self.party_repeat_fx = 0.0;
            self.party_glitch_fx = 0.0;
        } else if !self.was_party_mode && p.party_mode {
            self.party_section_idx = 0;
            self.party_bar_in_sec = 0;
            self.party_cycle = 0;
            self.party_evolution = 0;
            self.party_rig = None;
            self.party_live_bpm = p.bpm;
        }
        self.was_party_mode = p.party_mode;

        self.sync_sampler_if_needed(p);
        self.volca.sync_parts(&p.sample_parts);

        if playing && !self.was_playing {
            self.step = 0;
            self.tick_left = 0.0;
            self.party_section_idx = 0;
            self.party_bar_in_sec = 0;
            self.party_cycle = 0;
            self.party_evolution = 0;
            self.party_template_idx = 0;
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
        }
        self.was_playing = playing;

        // Advance step sequencer with real swing
        if playing {
            if self.tick_left <= 0.0 {
                self.trigger_step(p, drum_cut, snare_build, telemetry);
                self.tick_left = self.step_duration_samples(p, self.step);
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
                        let drums_p = if p.party_mode {
                            self.party_rig.as_ref().map(|r| &r.drums).unwrap_or(&p.drums)
                        } else {
                            &p.drums
                        };
                        self.drums.trigger(v, ev.vel, ev.accent, false, drums_p);
                    }
                } else {
                    i += 1;
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
        let use_morph = p.party_mode && p.party_sound_morph;
        let mut moog_p;
        let mut pad_p;
        let mut mono_p;
        let mut acid_p;
        let drums_p;
        let samp_p;
        let mut sc_base = p.sidechain_amount;
        let mut enable_303_override = false;
        let mut enable_samp_override = false;
        let mut vol_303 = p.mixer.channels[2].volume;
        let mut vol_samp = p.mixer.channels[4].volume;

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
                vol_303 = rig.tb303_vol;
                vol_samp = rig.sampler_vol;
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

        // Sidechain ducking envelope decay
        self.sidechain_env *= self.sidechain_decay;
        let sc_amt = self.auto_sidechain.unwrap_or(sc_base).clamp(0.0, 1.0);
        let duck = (1.0 - self.sidechain_env * sc_amt * 0.78).clamp(0.18, 1.0);
        telemetry.sidechain_duck = 1.0 - duck;

        // Key-locked sub bass dive (DJ Bass Drop)
        let mut drop_sig = 0.0;
        if self.drop_env > 0.0008 {
            self.drop_pitch += (self.drop_target_hz - self.drop_pitch) * (4.2 / self.sr);
            self.drop_phase = (self.drop_phase + self.drop_pitch / self.sr).fract();
            let sub = (self.drop_phase * std::f32::consts::TAU).sin();
            drop_sig = (sub * 1.4).tanh() * self.drop_env * 0.65;
            self.drop_env *= (-1.0 / (0.85 * self.sr)).exp();
        }

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
        let mut rev_l = 0.0_f32;
        let mut rev_r = 0.0_f32;

        // 0: Bass (Moog) — center punch + sidechain
        let moog_s = self.moog.process_sample(&moog_p) * duck;
        if audible(ch[0].mute, ch[0].solo) {
            let (gl, gr) = pan_gains(ch[0].pan);
            let sl = moog_s * ch[0].volume * gl * 0.62;
            let sr_v = moog_s * ch[0].volume * gr * 0.62;
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

        // 1: Arp / Lead (Monologue) — stereo widened + sidechain
        let mono_s = self.mono.process_sample(&mono_p) * (0.45 + 0.55 * duck);
        if audible(ch[1].mute, ch[1].solo) {
            let (wl, wr) = self.arp_widener.process(mono_s, 0.55);
            let (gl, gr) = pan_gains(ch[1].pan);
            let sl = wl * ch[1].volume * gl * 0.52;
            let sr_v = wr * ch[1].volume * gr * 0.52;
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

        // 2: Acid / 303 — sidechained
        let acid_s = self.acid.process_sample(&acid_p) * duck;
        let acid_mute = ch[2].mute && !enable_303_override;
        if audible(acid_mute, ch[2].solo) {
            let (gl, gr) = pan_gains(ch[2].pan);
            let sl = acid_s * vol_303 * gl * 0.55;
            let sr_v = acid_s * vol_303 * gr * 0.55;
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

        // 3: Drums — TRUE STEREO preserved & punchy on top!
        let (dl, dr) = self.drums.process_sample(&drums_p);
        if audible(ch[3].mute, ch[3].solo) {
            let sl = dl * ch[3].volume * 0.85;
            let sr_v = dr * ch[3].volume * 0.85;
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

        // 4: Sampler + 8-Part Volca Bank — true stereo + Isolator EQ + gentle sidechain
        let samp_s = self.sampler.process_sample(&samp_p) * (0.35 + 0.65 * duck);
        let iso_b = self.auto_iso_bass.unwrap_or(p.sampler_iso_bass);
        let iso_t = self.auto_iso_treble.unwrap_or(p.sampler_iso_treble);
        let (vl, vr, vrev_l, vrev_r) =
            self.volca
                .process_stereo(&p.sample_parts, iso_b, iso_t);
        let v_duck = 0.40 + 0.60 * duck;
        let samp_mute = ch[4].mute && !enable_samp_override;
        if audible(samp_mute, ch[4].solo) {
            let (wl, wr) = self.sampler_widener.process(samp_s, 0.58);
            let (gl, gr) = pan_gains(ch[4].pan);
            let sl = (wl * gl * 0.52 + vl * v_duck * 0.72) * vol_samp;
            let sr_v = (wr * gr * 0.52 + vr * v_duck * 0.72) * vol_samp;
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

        // 5: Chords / MoogPad — lush stereo Haas width + sidechain bounce!
        let pad_s = self.pad_moog.process_sample(&pad_p) * (0.25 + 0.75 * duck);
        if audible(p.mixer.moog_pad_mute, p.mixer.moog_pad_solo) {
            let (wl, wr) = self.pad_widener.process(pad_s, 0.72);
            let sl = wl * p.mixer.moog_pad_volume * 0.54;
            let sr_v = wr * p.mixer.moog_pad_volume * 0.54;
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

        // 6+: Extra Dynamic Synth Tracks (tracks 7..=16)
        let extra_duck = 0.30 + 0.70 * duck;
        for (ei, tr) in p.extra_tracks.iter().take(MAX_EXTRA_TRACKS).enumerate() {
            let s = self.extra_synths[ei].process_sample(&tr.synth) * extra_duck;
            if audible(tr.mute, tr.solo) {
                let (gl, gr) = pan_gains(tr.pan);
                let sl = s * tr.volume * gl * 0.50;
                let sr_v = s * tr.volume * gr * 0.50;
                mix_l += sl;
                mix_r += sr_v;
                del_l += sl * tr.delay_send;
                del_r += sr_v * tr.delay_send;
                rev_l += sl * tr.reverb_send;
                rev_r += sr_v * tr.reverb_send;
            }
        }

        // Apply Motion Automation & Party DJ FX modulation cleanly on a stack copy of FxParams
        let mut fx_params = p.fx.clone();
        if use_morph {
            if let Some(rig) = &self.party_rig {
                fx_params.delay_div = rig.delay_div;
                fx_params.reverb_size = rig.reverb_size;
                fx_params.analog_warmth = rig.analog_warmth;
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
        telemetry.sampler_playhead = self
            .volca
            .playhead_norm(selected_part)
            .or_else(|| self.sampler.latest_playhead_norm());
        (out_l, out_r)
    }
}

pub struct AudioEngine {
    _stream: Option<cpal::Stream>,
    pub project: Arc<Mutex<Project>>,
    pub playing: Arc<AtomicBool>,
    pub current_step: Arc<AtomicUsize>,
    pub scope: Arc<OutputScope>,
    pub status: String,
    pub drum_cut: Arc<AtomicBool>,
    pub snare_build: Arc<AtomicBool>,
    pub selected_sample_part: Arc<AtomicUsize>,
    live_queue: Arc<Mutex<VecDeque<LiveEvent>>>,
    telemetry: Arc<Mutex<EngineTelemetry>>,
}

impl AudioEngine {
    #[cfg(test)]
    pub fn start_silent(project: Project) -> Self {
        Self {
            _stream: None,
            project: Arc::new(Mutex::new(project)),
            playing: Arc::new(AtomicBool::new(false)),
            current_step: Arc::new(AtomicUsize::new(0)),
            scope: Arc::new(OutputScope::new(256)),
            status: "Stereo Audio Live @ 44100 Hz".into(),
            drum_cut: Arc::new(AtomicBool::new(false)),
            snare_build: Arc::new(AtomicBool::new(false)),
            selected_sample_part: Arc::new(AtomicUsize::new(0)),
            live_queue: Arc::new(Mutex::new(VecDeque::new())),
            telemetry: Arc::new(Mutex::new(EngineTelemetry::default())),
        }
    }

    #[cfg(test)]
    pub fn set_telemetry_for_test(&self, t: EngineTelemetry) {
        if let Ok(mut guard) = self.telemetry.lock() {
            *guard = t;
        }
    }

    pub fn start(project: Project) -> Self {
        let project = Arc::new(Mutex::new(project));
        let playing = Arc::new(AtomicBool::new(false));
        let current_step = Arc::new(AtomicUsize::new(0));
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
                scope,
                status: "No audio output device".into(),
                drum_cut,
                snare_build,
                selected_sample_part,
                live_queue,
                telemetry,
            };
        };
        let Ok(cfg) = device.default_output_config() else {
            return Self {
                _stream: None,
                project,
                playing,
                current_step,
                scope,
                status: "No default output config".into(),
                drum_cut,
                snare_build,
                selected_sample_part,
                live_queue,
                telemetry,
            };
        };

        let channels = cfg.channels() as usize;
        let sr = cfg.sample_rate().0 as f32;
        let p_ref = project.clone();
        let play_ref = playing.clone();
        let step_ref = current_step.clone();
        let scope_ref = scope.clone();
        let cut_ref = drum_cut.clone();
        let build_ref = snare_build.clone();
        let part_ref = selected_sample_part.clone();
        let q_ref = live_queue.clone();
        let tel_ref = telemetry.clone();

        let mut rt = Runtime::new(sr);
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
                    scope,
                    status: format!("Stereo Audio Live @ {} Hz", sr as u32),
                    drum_cut,
                    snare_build,
                    selected_sample_part,
                    live_queue,
                    telemetry,
                },
                Err(e) => Self {
                    _stream: None,
                    project,
                    playing,
                    current_step,
                    scope,
                    status: format!("Audio play error: {e}"),
                    drum_cut,
                    snare_build,
                    selected_sample_part,
                    live_queue,
                    telemetry,
                },
            },
            Err(e) => Self {
                _stream: None,
                project,
                playing,
                current_step,
                scope,
                status: format!("Audio stream error: {e}"),
                drum_cut,
                snare_build,
                selected_sample_part,
                live_queue,
                telemetry,
            },
        }
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

    for _ in 0..total_samples {
        let (l, r) = rt.process_sample(project, true, false, false, 0, &mut tel);
        let il = (l.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        let ir = (r.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
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
        assert_eq!(PROGRESSIONS.len(), 20);

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
            fbass.steps.iter().any(|s| s.active && s.motion.any_locked()),
            "Expected Bass auto-wah MotionLocks"
        );
        assert!(
            fpad.steps.iter().any(|s| s.active && s.motion.any_locked()),
            "Expected Chords wah/comping MotionLocks"
        );
        assert!(
            farp.steps.iter().any(|s| s.active && s.motion.any_locked()),
            "Expected Arp/Clav MotionLocks"
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
}
