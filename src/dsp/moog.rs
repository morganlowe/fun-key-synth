use std::f32::consts::TAU;

use crate::model::{midi_to_hz, MoogParams, MoogWave, MotionLocks};

use super::{osc_pulse, osc_saw, osc_tri, osc_trisaw, soft_tanh, AdsrEnv, MoogLadderFilter, Xorshift32};

const MAX_VOICES: usize = 8;

#[derive(Debug, Clone)]
struct MoogVoice {
    active: bool,
    held: bool,
    slide_active: bool,
    note: u8,
    current_midi: f32,
    target_midi: f32,
    velocity: f32,
    accent: f32,
    age: u64,
    phase1: f32,
    phase2: f32,
    phase3: f32,
    filter: MoogLadderFilter,
    filt_env: AdsrEnv,
    amp_env: AdsrEnv,
}

impl MoogVoice {
    fn new(sr: f32) -> Self {
        Self {
            active: false,
            held: false,
            slide_active: false,
            note: 48,
            current_midi: 48.0,
            target_midi: 48.0,
            velocity: 0.8,
            accent: 0.0,
            age: 0,
            phase1: 0.0,
            phase2: 0.0,
            phase3: 0.0,
            filter: MoogLadderFilter::new(sr),
            filt_env: AdsrEnv::new(sr),
            amp_env: AdsrEnv::new(sr),
        }
    }

    fn set_sample_rate(&mut self, sr: f32) {
        self.filter.set_sample_rate(sr);
        self.filt_env.set_sample_rate(sr);
        self.amp_env.set_sample_rate(sr);
    }
}

#[derive(Debug, Clone)]
pub struct MoogSynth {
    voices: [MoogVoice; MAX_VOICES],
    lfo_phase: f32,
    rng: Xorshift32,
    age_counter: u64,
    sample_rate: f32,
    motion_cutoff: Option<f32>,
    motion_res: Option<f32>,
    motion_drive: Option<f32>,
    motion_detune: Option<f32>,
    slew_cutoff: f32,
    slew_res: f32,
    slew_drive: f32,
}

impl MoogSynth {
    pub fn new(sample_rate: f32) -> Self {
        let sr = sample_rate.max(8000.0);
        Self {
            voices: std::array::from_fn(|_| MoogVoice::new(sr)),
            lfo_phase: 0.0,
            rng: Xorshift32::new(0xA11CE5),
            age_counter: 0,
            sample_rate: sr,
            motion_cutoff: None,
            motion_res: None,
            motion_drive: None,
            motion_detune: None,
            slew_cutoff: 1350.0,
            slew_res: 0.36,
            slew_drive: 0.30,
        }
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sample_rate = sr.max(8000.0);
        for v in &mut self.voices {
            v.set_sample_rate(self.sample_rate);
        }
    }

    pub fn set_motion_locks(&mut self, motion: MotionLocks) {
        self.motion_cutoff = motion.cutoff;
        self.motion_res = motion.resonance;
        self.motion_drive = motion.drive;
        self.motion_detune = motion.shape;
    }

    pub fn clear_motion_locks(&mut self) {
        self.motion_cutoff = None;
        self.motion_res = None;
        self.motion_drive = None;
        self.motion_detune = None;
    }

    pub fn note_on(&mut self, note: u8, velocity: f32, accent: bool, slide: bool, params: &MoogParams) {
        self.age_counter += 1;
        let vel = velocity.clamp(0.05, 1.0);
        let acc = if accent { 0.35 } else { 0.0 };

        if !params.poly_mode {
            // Mono mode: use voice 0, glide from previous pitch
            for v in &mut self.voices[1..] {
                v.active = false;
                v.held = false;
                v.slide_active = false;
                v.amp_env.reset();
                v.filt_env.reset();
            }
            let v = &mut self.voices[0];
            let was_active = v.active && v.amp_env.is_active();
            v.note = note;
            v.target_midi = note as f32;
            v.slide_active = slide && was_active;
            if !v.slide_active && params.glide <= 0.005 {
                v.current_midi = note as f32;
            }
            v.velocity = vel;
            v.accent = acc;
            v.held = true;
            v.active = true;
            v.age = self.age_counter;
            if !(slide && was_active) {
                v.filt_env.trigger();
                v.amp_env.trigger();
            }
            return;
        }

        // Poly mode: reuse same note if active, else idle voice, else oldest voice
        let idx = self
            .voices
            .iter()
            .position(|v| v.active && v.note == note)
            .or_else(|| self.voices.iter().position(|v| !v.active || !v.amp_env.is_active()))
            .unwrap_or_else(|| {
                self.voices
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, v)| (v.held as u64 * 1_000_000_000) + v.age)
                    .map(|(i, _)| i)
                    .unwrap_or(0)
            });

        let v = &mut self.voices[idx];
        let was_active = v.active && v.amp_env.is_active();
        v.note = note;
        v.target_midi = note as f32;
        v.slide_active = slide && was_active;
        if !v.slide_active && !slide {
            v.current_midi = note as f32;
        }
        v.velocity = vel;
        v.accent = acc;
        v.held = true;
        v.active = true;
        v.age = self.age_counter;
        if !was_active {
            v.phase1 = 0.0;
            v.phase2 = 0.23;
            v.phase3 = 0.57;
        }
        if !(slide && was_active) {
            v.filt_env.trigger();
            v.amp_env.trigger();
        }
    }

    pub fn note_off(&mut self, note: u8) {
        for v in &mut self.voices {
            if v.active && v.held && v.note == note {
                v.held = false;
                v.filt_env.release();
                v.amp_env.release();
            }
        }
    }

    pub fn all_notes_off(&mut self) {
        for v in &mut self.voices {
            v.held = false;
            v.filt_env.release();
            v.amp_env.release();
        }
    }

    pub fn active_voice_count(&self) -> usize {
        self.voices.iter().filter(|v| v.active && v.amp_env.is_active()).count()
    }

    #[cfg(test)]
    pub fn current_mono_midi(&self) -> f32 {
        self.voices[0].current_midi
    }

    #[inline]
    pub fn process_sample(&mut self, params: &MoogParams) -> f32 {
        let sr = self.sample_rate;
        let target_cutoff = self
            .motion_cutoff
            .map(|n| 35.0 * (16000.0_f32 / 35.0).powf(n.clamp(0.0, 1.0)))
            .unwrap_or(params.cutoff);
        let target_res = self.motion_res.unwrap_or(params.resonance);
        let target_drive = self.motion_drive.unwrap_or(params.drive);
        let detune_scale = self.motion_detune.map(|s| 0.3 + s * 2.0).unwrap_or(1.0);

        self.slew_cutoff += 0.008 * (target_cutoff - self.slew_cutoff);
        self.slew_res += 0.008 * (target_res - self.slew_res);
        self.slew_drive += 0.008 * (target_drive - self.slew_drive);

        self.lfo_phase = (self.lfo_phase + params.lfo_rate.clamp(0.05, 30.0) / sr).fract();
        let lfo = (self.lfo_phase * TAU).sin();
        let pitch_mod_semi = lfo * params.lfo_to_pitch * 1.5;
        let filt_lfo_oct = lfo * params.lfo_to_filter * 2.2;

        let mut mix = 0.0_f32;
        for v in &mut self.voices {
            if !v.active {
                continue;
            }
            let eff_glide = if v.slide_active {
                params.glide.max(0.045)
            } else {
                params.glide
            };
            if eff_glide <= 0.005 {
                v.current_midi = v.target_midi;
            } else {
                let glide_coeff = 1.0 - (-1.0 / (eff_glide * 0.35 * sr)).exp();
                v.current_midi += glide_coeff * (v.target_midi - v.current_midi);
            }
            let f_env = v.filt_env.next(
                params.filt_attack,
                params.filt_decay,
                params.filt_sustain,
                params.filt_release,
            );
            let a_env = v.amp_env.next(
                params.amp_attack,
                params.amp_decay,
                params.amp_sustain,
                params.amp_release,
            );

            if !v.amp_env.is_active() {
                v.active = false;
                continue;
            }

            let base_midi = v.current_midi + pitch_mod_semi;
            let f1 = midi_to_hz(base_midi).clamp(16.0, sr * 0.42);
            let f2 = midi_to_hz(
                base_midi + params.osc2_octave as f32 * 12.0 + (params.osc2_detune * detune_scale) / 100.0,
            )
            .clamp(16.0, sr * 0.42);
            let f3 = midi_to_hz(
                base_midi + params.osc3_octave as f32 * 12.0 + (params.osc3_detune * detune_scale) / 100.0,
            )
            .clamp(16.0, sr * 0.42);

            let dt1 = f1 / sr;
            let dt2 = f2 / sr;
            let dt3 = f3 / sr;
            v.phase1 = (v.phase1 + dt1).fract();
            v.phase2 = (v.phase2 + dt2).fract();
            v.phase3 = (v.phase3 + dt3).fract();

            let o1 = sample_moog_wave(params.osc1_wave, v.phase1, dt1) * params.osc1_level;
            let o2 = sample_moog_wave(params.osc2_wave, v.phase2, dt2) * params.osc2_level;
            let o3 = sample_moog_wave(params.osc3_wave, v.phase3, dt3) * params.osc3_level;
            let noise = self.rng.next_bipolar() * params.noise_level;

            // Classic CP3 mixer overdrive into 24dB Moog ladder
            let pre_gain = 0.55 + self.slew_drive * 1.9;
            let mixer_sum = soft_tanh((o1 + o2 + o3 + noise) * pre_gain * 0.48);

            let key_oct = ((v.current_midi - 48.0) / 12.0) * params.key_track;
            let env_oct = (params.env_amount + v.accent * 0.5) * f_env * 4.5;
            let cutoff_hz = self.slew_cutoff * 2.0_f32.powf(key_oct + env_oct + filt_lfo_oct);
            let filtered = v.filter.process(mixer_sum, cutoff_hz, self.slew_res);

            let voice_gain = if params.poly_mode { 0.46 } else { 0.70 };
            mix += filtered * a_env * (v.velocity + v.accent * 0.25) * voice_gain;
        }

        soft_tanh(mix)
    }
}

#[inline]
fn sample_moog_wave(wave: MoogWave, phase: f32, dt: f32) -> f32 {
    match wave {
        MoogWave::Triangle => osc_tri(phase),
        MoogWave::TriSaw => osc_trisaw(phase, dt),
        MoogWave::Saw => osc_saw(phase, dt),
        MoogWave::WidePulse => osc_pulse(phase, dt, 0.35),
        MoogWave::Square => osc_pulse(phase, dt, 0.50),
        MoogWave::NarrowPulse => osc_pulse(phase, dt, 0.15),
    }
}
