use crate::{
    dsp::{
        sampler::generate_builtin_sample,
        DrumMachine, MasterFx, MonologueSynth, MoogSynth, Sk1Sampler, Tb303Synth,
    },
    model::{
        arp_tones_for, bass_midi_for, chord_symbol, generate_auto_beat, progression_at,
        voice_led_chord, ChordSlot, DrumVoiceId, MelodicStep, Project, TrackId,
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
}

#[derive(Debug, Clone, Default)]
pub struct EngineTelemetry {
    pub active_chord: String,
    pub party_section: String,
    pub party_bar: usize,
    pub party_section_bars: usize,
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

struct PartySectionDef {
    name: &'static str,
    bars: usize,
    /// bit 0=Drums, 1=Bass, 2=Chords, 3=Arp, 4=303, 5=Sampler
    mask: u8,
    filter_sweep: bool,
    snare_build_last_bar: bool,
    drop_on_start: bool,
}

const PARTY_SECTIONS: [PartySectionDef; 6] = [
    PartySectionDef {
        name: "INTRO",
        bars: 4,
        mask: 0b001101,
        filter_sweep: false,
        snare_build_last_bar: false,
        drop_on_start: false,
    },
    PartySectionDef {
        name: "GROOVE",
        bars: 8,
        mask: 0b101111,
        filter_sweep: false,
        snare_build_last_bar: false,
        drop_on_start: false,
    },
    PartySectionDef {
        name: "BUILD",
        bars: 4,
        mask: 0b011101,
        filter_sweep: true,
        snare_build_last_bar: true,
        drop_on_start: false,
    },
    PartySectionDef {
        name: "DROP 1",
        bars: 8,
        mask: 0b111111,
        filter_sweep: false,
        snare_build_last_bar: false,
        drop_on_start: true,
    },
    PartySectionDef {
        name: "BREAK",
        bars: 4,
        mask: 0b101100,
        filter_sweep: false,
        snare_build_last_bar: true,
        drop_on_start: false,
    },
    PartySectionDef {
        name: "DROP 2",
        bars: 8,
        mask: 0b111111,
        filter_sweep: false,
        snare_build_last_bar: false,
        drop_on_start: true,
    },
];

struct Runtime {
    sr: f32,
    moog: MoogSynth,
    pad_moog: MoogSynth,
    mono: MonologueSynth,
    acid: Tb303Synth,
    drums: DrumMachine,
    sampler: Sk1Sampler,
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
    // Drum ratchets & fills
    ratchets: Vec<RatchetEvent>,
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
    party_filter_mod: f32,
    // Auto-beat cache
    auto_beat_drums: crate::model::DrumTrackSeq,
    auto_beat_sig: (u8, u32, u32),
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
            force_fill_steps: 0,
            live_chord_held: false,
            live_slot: None,
            loaded_sample_gen: 0,
            party_section_idx: 0,
            party_bar_in_sec: 0,
            party_cycle: 0,
            party_filter_mod: 1.0,
            auto_beat_drums: crate::model::DrumTrackSeq::default(),
            auto_beat_sig: (255, 0, 0),
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
            LiveEvent::TriggerBassDrop => self.trigger_bass_drop(p.root_note),
            LiveEvent::TriggerFill => self.force_fill_steps = 8,
            LiveEvent::NextPartySection => {
                self.party_section_idx = (self.party_section_idx + 1) % PARTY_SECTIONS.len();
                self.party_bar_in_sec = 0;
                if PARTY_SECTIONS[self.party_section_idx].drop_on_start {
                    self.trigger_bass_drop(p.root_note);
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
        self.gate_moog = 0.0;
        self.gate_pad = 0.0;
        self.gate_mono = 0.0;
        self.gate_acid = 0.0;
        self.gate_sampler = 0.0;
        self.ratchets.clear();
    }

    fn step_duration_samples(&self, p: &Project, step_index: usize) -> f32 {
        let base_step = (60.0 / p.bpm.clamp(40.0, 240.0) / 4.0) * self.sr;
        let swing_ratio = (p.swing * 0.64).clamp(0.0, 0.30);
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
        let base_step_samples = (60.0 / p.bpm.clamp(40.0, 240.0) / 4.0) * self.sr;

        // Determine active section mask, pattern index, fill, and filter sweep
        let mut pat_idx = p.active_pattern.min(p.patterns.len().saturating_sub(1));
        let mut section_mask: u8 = 0b111111;
        let mut section_fill = false;
        let mut party_filter = 1.0_f32;
        let mut party_snare_roll = false;

        if p.party_mode {
            let sec = &PARTY_SECTIONS[self.party_section_idx];
            if global > 0 && global % 16 == 0 {
                self.party_bar_in_sec += 1;
                if self.party_bar_in_sec >= sec.bars {
                    self.party_bar_in_sec = 0;
                    self.party_section_idx = (self.party_section_idx + 1) % PARTY_SECTIONS.len();
                    if self.party_section_idx == 0 {
                        self.party_cycle = self.party_cycle.wrapping_add(1);
                    }
                    if PARTY_SECTIONS[self.party_section_idx].drop_on_start {
                        self.trigger_bass_drop(p.root_note);
                    }
                }
            }
            let sec = &PARTY_SECTIONS[self.party_section_idx];
            section_mask = sec.mask;
            // Energy scaling in Party Mode
            if p.party_energy < 0.35 {
                section_mask &= !0b010000; // pull 303 on chill energy
            }
            let bar_progress =
                (self.party_bar_in_sec * 16 + (global % 16)) as f32 / (sec.bars * 16).max(1) as f32;
            if sec.filter_sweep {
                // Smooth DJ high-energy sweep during Build
                party_filter = 0.25 + bar_progress * 0.75;
            }
            if sec.snare_build_last_bar && self.party_bar_in_sec + 1 == sec.bars {
                party_snare_roll = true;
            }
            if self.party_bar_in_sec + 1 == sec.bars && global % 16 >= 12 {
                section_fill = true;
            }
            telemetry.party_section = sec.name.to_string();
            telemetry.party_bar = self.party_bar_in_sec + 1;
            telemetry.party_section_bars = sec.bars;
        } else if p.song_mode && !p.song_sections.is_empty() {
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
            telemetry.party_section.clear();
            telemetry.party_bar = (global / 16) % 4 + 1;
            telemetry.party_section_bars = 4;
        }
        self.party_filter_mod = party_filter;

        // Active chord slot for telemetry & Party/Auto-Chord harmonic lock
        let prog_offset = if p.party_mode && p.party_auto_evolve {
            (p.progression_idx + (self.party_cycle as usize)) % crate::model::PROGRESSIONS.len()
        } else {
            p.progression_idx
        };
        let prog = progression_at(prog_offset);
        let bpc = p.bars_per_chord.clamp(1, 4) as usize;
        let chord_idx = (global / (16 * bpc)) % 4;
        let active_slot = if p.auto_chord_follow && self.live_chord_held {
            self.live_slot.unwrap_or(prog.slots[chord_idx])
        } else {
            prog.slots[chord_idx]
        };
        telemetry.active_chord = chord_symbol(p.root_note, active_slot);

        let pat = &p.patterns[pat_idx];
        let seed = p.party_seed.wrapping_add(self.party_cycle.wrapping_mul(7919));

        // --- 1. DRUMS (bit 0) ---
        if !drum_cut && (section_mask & 0b000001) != 0 {
            // Refresh Auto-Beat cache if needed
            let use_auto_beat = p.auto_beat_mode || p.party_mode;
            if use_auto_beat {
                let energy = if p.party_mode {
                    (p.party_energy * 0.85 + 0.15).clamp(0.15, 1.0)
                } else {
                    p.beat_energy
                };
                let sig = (p.vibe as u8, (energy * 100.0) as u32, seed);
                if self.auto_beat_sig != sig {
                    self.auto_beat_drums = generate_auto_beat(p.vibe, energy, seed);
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
                        // Subtle velocity humanization for groove
                        let human = 0.94 + hash_f32(seed, global, lane as u32 + 99) * 0.12;
                        let vel = (st.velocity * human).clamp(0.1, 1.0);
                        self.drums.trigger(voice, vel, st.accent, st.flam, &p.drums);
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
                    self.drums
                        .trigger(DrumVoiceId::Snare, vel, phase >= 12, false, &p.drums);
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
                    self.drums
                        .trigger(voice, vel, phase == 15, false, &p.drums);
                }
            }
        }

        // Helper: trigger a melodic step with proper gate/slide/motion
        let bass_root = bass_midi_for(p.root_note, active_slot.offset);
        let pad_notes = voice_led_chord(p.root_note, active_slot, 60);
        let arp_tones = arp_tones_for(p.root_note, active_slot);
        let follow_override = p.auto_chord_follow && self.live_chord_held;

        // --- 2. BASS / MOOG (bit 1) ---
        if (section_mask & 0b000010) != 0 && !follow_override {
            let seq = &pat.moog;
            let mut st = seq.steps[global % seq.step_count.as_usize()];
            // Party Mode subtle scale-safe bass variation
            if p.party_mode && !st.active && global % 16 == 14 && hash_f32(seed, global, 11) < 0.30 {
                st = MelodicStep {
                    active: true,
                    note: bass_root + 12,
                    velocity: 0.72,
                    gate: 1.2,
                    slide: true,
                    ..Default::default()
                };
            }
            if st.active {
                let note = if p.party_mode && p.party_auto_evolve && self.party_cycle > 0 {
                    // Keep bass locked to evolving progression!
                    let iv = (st.note as i32 - bass_midi_for(p.root_note, progression_at(p.progression_idx).slots[(global / 16) % 4].offset) as i32).clamp(-12, 12);
                    (bass_root as i32 + iv).clamp(28, 60) as u8
                } else {
                    st.note
                };
                if st.motion.any_locked() {
                    self.moog.set_motion_locks(st.motion);
                } else {
                    self.moog.clear_motion_locks();
                }
                if !st.slide {
                    if let Some(prev) = self.last_moog_note.take() {
                        self.moog.note_off(prev);
                    }
                }
                self.moog
                    .note_on(note, st.velocity, st.accent, st.slide, &p.moog);
                self.last_moog_note = Some(note);
                self.gate_moog = base_step_samples * st.gate.clamp(0.15, 16.0);
            }
        }

        // --- 3. CHORDS / MOOG PAD (bit 2) ---
        if (section_mask & 0b000100) != 0 && !follow_override {
            let seq = &pat.moog_pad;
            let st = seq.steps[global % seq.step_count.as_usize()];
            if st.active {
                if st.motion.any_locked() {
                    self.pad_moog.set_motion_locks(st.motion);
                } else {
                    self.pad_moog.clear_motion_locks();
                }
                self.pad_moog.all_notes_off();
                if st.chord != crate::model::ChordType::Single {
                    // Voice-led lush chord!
                    let notes_to_play = if p.party_mode && p.party_auto_evolve && self.party_cycle > 0 {
                        pad_notes.clone()
                    } else {
                        let slot = ChordSlot {
                            offset: (st.note + 12 - (p.root_note % 12)) % 12,
                            chord: st.chord,
                        };
                        voice_led_chord(p.root_note, slot, 60)
                    };
                    for n in notes_to_play {
                        self.pad_moog
                            .note_on(n, st.velocity, st.accent, false, &p.moog_pad);
                    }
                } else {
                    self.pad_moog
                        .note_on(st.note, st.velocity, st.accent, false, &p.moog_pad);
                }
                self.gate_pad = base_step_samples * st.gate.clamp(0.25, 16.0);
            }
        }

        // --- 4. ARP / MONOLOGUE (bit 3) ---
        if (section_mask & 0b001000) != 0 {
            let seq = &pat.monologue;
            let mut st = seq.steps[global % seq.step_count.as_usize()];
            // Party Mode sparkling chord-tone arp variation!
            if p.party_mode && !st.active && global % 2 == 0 && hash_f32(seed, global, 23) < p.party_energy * 0.25 {
                st = MelodicStep {
                    active: true,
                    note: arp_tones[(global / 2) % arp_tones.len().max(1)],
                    velocity: 0.62,
                    gate: 0.9,
                    ..Default::default()
                };
            }
            if st.active {
                let note = if follow_override || (p.party_mode && p.party_auto_evolve && self.party_cycle > 0) {
                    arp_tones[(global / 2) % arp_tones.len().max(1)]
                } else {
                    st.note
                };
                if st.motion.any_locked() {
                    self.mono.set_motion_locks(st.motion, true);
                } else {
                    self.mono.clear_motion_locks();
                }
                if !st.slide {
                    if let Some(prev) = self.last_mono_note.take() {
                        self.mono.note_off(prev);
                    }
                }
                self.mono
                    .note_on(note, st.velocity, st.accent, st.slide, &p.monologue);
                self.last_mono_note = Some(note);
                self.gate_mono = base_step_samples * st.gate.clamp(0.15, 8.0);
            }
        }

        // --- 5. LEAD / TB-303 (bit 4) ---
        if (section_mask & 0b010000) != 0 {
            let seq = &pat.tb303;
            let st = seq.steps[global % seq.step_count.as_usize()];
            if st.active {
                let note = if follow_override {
                    bass_root + (st.note % 12)
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

        // --- 6. SAMPLER / TEXTURE (bit 5) ---
        if (section_mask & 0b100000) != 0 {
            let seq = &pat.sampler;
            let st = seq.steps[global % seq.step_count.as_usize()];
            if st.active {
                let note = if follow_override {
                    (48 + (p.root_note + active_slot.offset) % 12).clamp(48, 66)
                } else {
                    st.note
                };
                if st.motion.any_locked() {
                    self.sampler.set_motion_locks(st.motion);
                } else {
                    self.sampler.clear_motion_locks();
                }
                self.sampler.note_on(note, st.velocity, &p.sampler);
                self.gate_sampler = base_step_samples * st.gate.clamp(0.25, 16.0);
            }
        }
    }

    fn process_sample(
        &mut self,
        p: &Project,
        playing: bool,
        drum_cut: bool,
        snare_build: bool,
        telemetry: &mut EngineTelemetry,
    ) -> (f32, f32) {
        self.sync_sampler_if_needed(p);

        if playing && !self.was_playing {
            self.step = 0;
            self.tick_left = 0.0;
            self.party_section_idx = 0;
            self.party_bar_in_sec = 0;
            self.party_cycle = 0;
            self.party_filter_mod = 1.0;
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
                        self.drums.trigger(v, ev.vel, ev.accent, false, &p.drums);
                    }
                } else {
                    i += 1;
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

        // Sidechain ducking envelope decay
        self.sidechain_env *= self.sidechain_decay;
        let duck = (1.0 - self.sidechain_env * p.sidechain_amount.clamp(0.0, 1.0) * 0.78).clamp(0.18, 1.0);
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

        // Render all 6 studio voices
        let any_solo = p.mixer.channels.iter().any(|c| c.solo) || p.mixer.moog_pad_solo;
        let audible = |mute: bool, solo: bool| !mute && (!any_solo || solo);

        let ch = &p.mixer.channels;
        let mut mix_l = drop_sig;
        let mut mix_r = drop_sig;
        let mut del_l = 0.0_f32;
        let mut del_r = 0.0_f32;
        let mut rev_l = 0.0_f32;
        let mut rev_r = 0.0_f32;

        // 0: Bass (Moog) — center punch + sidechain
        let moog_s = self.moog.process_sample(&p.moog) * duck;
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
        let mono_s = self.mono.process_sample(&p.monologue) * (0.45 + 0.55 * duck);
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
        let acid_s = self.acid.process_sample(&p.tb303) * duck;
        if audible(ch[2].mute, ch[2].solo) {
            let (gl, gr) = pan_gains(ch[2].pan);
            let sl = acid_s * ch[2].volume * gl * 0.55;
            let sr_v = acid_s * ch[2].volume * gr * 0.55;
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
        let (dl, dr) = self.drums.process_sample(&p.drums);
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

        // 4: Sampler / Texture — stereo widened + gentle sidechain
        let samp_s = self.sampler.process_sample(&p.sampler) * (0.35 + 0.65 * duck);
        if audible(ch[4].mute, ch[4].solo) {
            let (wl, wr) = self.sampler_widener.process(samp_s, 0.58);
            let (gl, gr) = pan_gains(ch[4].pan);
            let sl = wl * ch[4].volume * gl * 0.52;
            let sr_v = wr * ch[4].volume * gr * 0.52;
            mix_l += sl;
            mix_r += sr_v;
            if ch[4].delay_enabled {
                del_l += sl * ch[4].delay_send;
                del_r += sr_v * ch[4].delay_send;
            }
            if ch[4].reverb_enabled {
                rev_l += sl * ch[4].reverb_send;
                rev_r += sr_v * ch[4].reverb_send;
            }
        }

        // 5: Chords / MoogPad — lush stereo Haas width + sidechain bounce!
        let pad_s = self.pad_moog.process_sample(&p.moog_pad) * (0.25 + 0.75 * duck);
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

        // Apply Party Build filter modulation cleanly on a stack copy of FxParams
        let mut fx_params = p.fx.clone();
        if p.party_mode && self.party_filter_mod < 0.99 {
            fx_params.global_filter_enabled = true;
            fx_params.global_filter_cutoff =
                (fx_params.global_filter_cutoff * self.party_filter_mod).clamp(260.0, 18_000.0);
            fx_params.global_filter_resonance = (fx_params.global_filter_resonance + 0.22).min(0.75);
        }

        let (fx_l, fx_r) = self.fx.process_stereo(
            mix_l,
            mix_r,
            del_l,
            del_r,
            rev_l,
            rev_r,
            &fx_params,
            p.bpm,
        );

        let master = p.mixer.master_volume.clamp(0.0, 1.3);
        let out_l = (fx_l * master).clamp(-0.98, 0.98);
        let out_r = (fx_r * master).clamp(-0.98, 0.98);
        self.peak_l = (self.peak_l * 0.9992).max(out_l.abs());
        self.peak_r = (self.peak_r * 0.9992).max(out_r.abs());
        telemetry.peak_l = self.peak_l;
        telemetry.peak_r = self.peak_r;
        telemetry.sampler_playhead = self.sampler.latest_playhead_norm();
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
    live_queue: Arc<Mutex<VecDeque<LiveEvent>>>,
    telemetry: Arc<Mutex<EngineTelemetry>>,
}

impl AudioEngine {
    pub fn start(project: Project) -> Self {
        let project = Arc::new(Mutex::new(project));
        let playing = Arc::new(AtomicBool::new(false));
        let current_step = Arc::new(AtomicUsize::new(0));
        let scope = Arc::new(OutputScope::new(256));
        let drum_cut = Arc::new(AtomicBool::new(false));
        let snare_build = Arc::new(AtomicBool::new(false));
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

                for frame in data.chunks_mut(channels.max(1)) {
                    let (out_l, out_r) =
                        rt.process_sample(&p, is_playing, is_cut, is_build, &mut local_tel);
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
        let (l, r) = rt.process_sample(project, true, false, false, &mut tel);
        let il = (l.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        let ir = (r.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        writer.write_sample(il).map_err(|e| e.to_string())?;
        writer.write_sample(ir).map_err(|e| e.to_string())?;
    }
    writer.finalize().map_err(|e| e.to_string())
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
                let (l, r) = rt.process_sample(&p, true, false, false, &mut tel);
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
        let mut p = Project::default();
        apply_vibe_to_project(&mut p, Vibe::ElectroFunk, None, 0x9999);
        p.party_mode = true;
        let mut rt = Runtime::new(44_100.0);
        let mut tel = EngineTelemetry::default();
        let mut max_amp = 0.0_f32;
        for _ in 0..44_100 {
            let (l, r) = rt.process_sample(&p, true, false, false, &mut tel);
            assert!(l.is_finite() && r.is_finite());
            max_amp = max_amp.max(l.abs()).max(r.abs());
        }
        assert!(max_amp > 0.05);
        assert!(!tel.active_chord.is_empty());
        assert!(!tel.party_section.is_empty());
    }
}
